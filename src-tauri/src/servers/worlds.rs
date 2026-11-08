use super::*;
use fs2::FileExt;
use std::{io::Read, path::Path};
fn stopped(s: &AppState, id: &str) -> Result<()> {
    load(s, id)?;
    ensure!(
        !s.servers
            .jobs
            .lock()
            .unwrap()
            .get(id)
            .is_some_and(|j| !j.stopped.load(std::sync::atomic::Ordering::Acquire)),
        "Save and stop this server first. Copying an active world is not a consistent backup."
    );
    Ok(())
}
pub fn host_lock(s: &AppState, id: &str) -> Result<std::fs::File> {
    let base = root(s, id)?;
    std::fs::create_dir_all(&base)?;
    let file = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(base.join("orbit-host.lock"))?;
    FileExt::try_lock_exclusive(&file)
        .context("Another Orbit instance is hosting or managing this server")?;
    Ok(file)
}
fn world_lock(path: &Path) -> Result<Option<std::fs::File>> {
    let path = path.join("session.lock");
    if !path.exists() {
        return Ok(None);
    }
    let f = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(path)
        .context("World is open in another Minecraft process")?;
    FileExt::try_lock_exclusive(&f).context("World is open in another Minecraft process")?;
    Ok(Some(f))
}
fn ordinary(path: &Path) -> Result<()> {
    let m = std::fs::symlink_metadata(path)?;
    ensure!(
        !m.file_type().is_symlink(),
        "Symbolic links are not allowed in worlds or backups"
    );
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        ensure!(
            m.file_attributes() & 0x400 == 0,
            "Reparse points are not allowed in worlds or backups"
        );
    }
    Ok(())
}
pub fn copy_tree(from: &Path, to: &Path) -> Result<u64> {
    fn copy(from: &Path, to: &Path, depth: u32, files: &mut u64, bytes: &mut u64) -> Result<()> {
        ensure!(
            depth < 64 && *files < 200_000,
            "World has too many files or nested folders"
        );
        ordinary(from)?;
        std::fs::create_dir_all(to)?;
        for entry in std::fs::read_dir(from)? {
            let entry = entry?;
            let path = entry.path();
            ordinary(&path)?;
            let target = to.join(entry.file_name());
            if path.is_dir() {
                copy(&path, &target, depth + 1, files, bytes)?;
            } else {
                *files += 1;
                if entry.file_name() == "session.lock" {
                    continue;
                }
                *bytes += std::fs::metadata(&path)?.len();
                ensure!(
                    *bytes <= 64 * 1024 * 1024 * 1024,
                    "World copy exceeds the 64 GiB safety bound"
                );
                std::fs::copy(path, target)?;
            }
        }
        Ok(())
    }
    let mut files = 0;
    let mut bytes = 0;
    copy(from, to, 0, &mut files, &mut bytes)?;
    Ok(bytes)
}
fn world_info(path: &Path) -> Result<Value> {
    ordinary(path)?;
    let file = std::fs::File::open(path.join("level.dat"))
        .context("Choose the world folder containing level.dat")?;
    ensure!(
        file.metadata()?.len() <= 16 * 1024 * 1024,
        "World metadata is too large"
    );
    let mut decoded = vec![];
    flate2::read::GzDecoder::new(file)
        .take(16 * 1024 * 1024 + 1)
        .read_to_end(&mut decoded)?;
    ensure!(
        decoded.len() <= 16 * 1024 * 1024,
        "World metadata exceeds the safe decoding bound"
    );
    let nbt: fastnbt::Value =
        fastnbt::from_bytes(&decoded).context("Invalid Java world metadata")?;
    Ok(serde_json::to_value(nbt)?)
}
pub fn world_version(path: &Path) -> Result<i64> {
    world_info(path)?["Data"]["DataVersion"]
        .as_i64()
        .context("World has no DataVersion; compatibility cannot be verified")
}
pub fn import(s: &AppState, v: &Value) -> Result<Value> {
    let id = v["id"].as_str().context("Choose a server")?;
    stopped(s, id)?;
    let _host = host_lock(s, id)?;
    ensure!(
        v["confirmed"] == true,
        "Confirm importing a copy; never run a dedicated server on the original client world"
    );
    let source = dunce::canonicalize(v["path"].as_str().context("Choose a world folder")?)?;
    let dest = root(s, id)?;
    std::fs::create_dir_all(&dest)?;
    let dest = dunce::canonicalize(dest)?;
    ensure!(
        !dest.starts_with(&source) && !source.starts_with(&dest),
        "Choose a separate original world, not this server directory"
    );
    ensure!(world_version(&source)?==3955,"Only worlds saved by Minecraft Java 1.21.1 (DataVersion 3955) can be imported. Orbit never upgrades or downgrades a world silently.");
    let metadata = world_info(&source)?;
    ensure!(metadata["Data"]["WasModded"].as_i64().unwrap_or(0)==0 && metadata["Data"]["WasModded"]!=true && metadata["Data"]["FML"].is_null(),"This world was marked as modded. Its compatibility cannot be established from the Minecraft version alone; this release imports unmodified Vanilla 1.21.1 worlds only.");
    let lock = if source.join("session.lock").exists() {
        Some(
            std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .open(source.join("session.lock"))
                .context(
                    "Cannot lock the original world. Close Minecraft and check folder access.",
                )?,
        )
    } else {
        None
    };
    if let Some(ref file) = lock {
        FileExt::try_lock_exclusive(file).context(
            "The original world is open in Minecraft. Close it before importing a copy.",
        )?;
    }
    let staging = dest.join(format!("world-import-{}", uuid::Uuid::new_v4()));
    let bytes = copy_tree(&source, &staging)?;
    ensure!(
        world_version(&staging)? == 3955,
        "Copied world metadata mismatch"
    );
    if dest.join("world").exists() {
        backup_inner(s, id)?;
        std::fs::rename(
            dest.join("world"),
            dest.join(format!("world-before-import-{}", uuid::Uuid::new_v4())),
        )?;
    }
    std::fs::rename(staging, dest.join("world"))?;
    drop(lock);
    Ok(json!({"copiedBytes":bytes,"originalPreserved":true}))
}
pub fn backup(s: &AppState, id: &str) -> Result<Value> {
    stopped(s, id)?;
    let _host = host_lock(s, id)?;
    backup_inner(s, id)
}
fn backup_inner(s: &AppState, id: &str) -> Result<Value> {
    stopped(s, id)?;
    let _world = world_lock(&root(s, id)?.join("world"))?;
    let server = load(s, id)?;
    let base = root(s, id)?;
    ensure!(
        base.join("world/level.dat").is_file(),
        "Start and gracefully stop the server to create a world before backing up"
    );
    let key = uuid::Uuid::new_v4().to_string();
    let folder = base.join("backups").join(&key);
    let bytes = copy_tree(&base.join("world"), &folder.join("world"))?;
    for file in ["server.properties", "whitelist.json"] {
        if base.join(file).is_file() {
            std::fs::copy(base.join(file), folder.join(file))?;
        }
    }
    let manifest = json!({"id":key,"created":db::now(),"bytes":bytes,"version":server["version"],"loader":server["loader"],"config":server,"path":folder});
    std::fs::write(
        folder.join("backup.json"),
        serde_json::to_vec_pretty(&manifest)?,
    )?;
    Ok(manifest)
}
pub fn backups(s: &AppState, id: &str) -> Result<Value> {
    load(s, id)?;
    let folder = root(s, id)?.join("backups");
    let mut out = vec![];
    if folder.exists() {
        for entry in std::fs::read_dir(folder)? {
            let path = entry?.path();
            ordinary(&path)?;
            if path.join("backup.json").is_file() {
                if let Ok(m) =
                    serde_json::from_slice::<Value>(&std::fs::read(path.join("backup.json"))?)
                {
                    out.push(m);
                }
            }
        }
    }
    out.sort_by_key(|v| std::cmp::Reverse(v["created"].as_i64().unwrap_or(0)));
    Ok(json!(out))
}
pub fn restore(s: &AppState, v: &Value) -> Result<Value> {
    let id = v["id"].as_str().context("Choose a server")?;
    stopped(s, id)?;
    let _host = host_lock(s, id)?;
    let key = v["backup"].as_str().context("Choose a backup")?;
    uuid::Uuid::parse_str(key)?;
    let base = root(s, id)?;
    let folder = base.join("backups").join(key);
    ordinary(&folder)?;
    let manifest: Value = serde_json::from_slice(&std::fs::read(folder.join("backup.json"))?)?;
    let server = load(s, id)?;
    ensure!(
        manifest["version"] == server["version"] && manifest["loader"] == server["loader"],
        "Backup version or loader differs; restoration is blocked"
    );
    ensure!(
        world_version(&folder.join("world"))? == 3955,
        "Backup world is incompatible"
    );
    let recovery = if base.join("world").exists() {
        Some(backup_inner(s, id)?)
    } else {
        None
    };
    let stage = base.join(format!("world-restore-{}", uuid::Uuid::new_v4()));
    copy_tree(&folder.join("world"), &stage)?;
    if base.join("world").exists() {
        std::fs::rename(
            base.join("world"),
            base.join(format!("world-before-restore-{}", uuid::Uuid::new_v4())),
        )?;
    }
    std::fs::rename(stage, base.join("world"))?;
    Ok(
        json!({"restored":true,"safetyBackup":recovery,"note":"Restored the world only. Current authentication, allowlist and network settings remain enforced. Previous world directory is retained for recovery."}),
    )
}
pub fn delete_backup(s: &AppState, v: &Value) -> Result<Value> {
    let id = v["id"].as_str().context("Choose a server")?;
    stopped(s, id)?;
    let _host = host_lock(s, id)?;
    let key = v["backup"].as_str().context("Choose a backup")?;
    uuid::Uuid::parse_str(key)?;
    let base = dunce::canonicalize(root(s, id)?.join("backups"))?;
    let target = dunce::canonicalize(base.join(key))?;
    ensure!(
        target.parent() == Some(base.as_path()),
        "Invalid backup path"
    );
    ordinary(&target)?; // Only validated, explicitly confirmed backup directories are deleted.
    fn check(p: &Path) -> Result<()> {
        ordinary(p)?;
        if p.is_dir() {
            for e in std::fs::read_dir(p)? {
                check(&e?.path())?;
            }
        }
        Ok(())
    }
    check(&target)?;
    std::fs::remove_dir_all(&target)?;
    Ok(json!(true))
}
