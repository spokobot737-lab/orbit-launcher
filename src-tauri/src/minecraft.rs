use crate::{auth, db, net, post_launch, AppState};
use anyhow::{bail, ensure, Context, Result};
use futures_util::{stream, StreamExt, TryStreamExt};
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};
const MANIFEST: &str = "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json";
const RUNTIMES:&str="https://piston-meta.mojang.com/v1/products/java-runtime/2ec0cc96c44e5a76b9c8b7c39df7210883d12871/all.json";
fn text<'a>(v: &'a Value, key: &str) -> Result<&'a str> {
    v[key]
        .as_str()
        .with_context(|| format!("Metadata is missing {key}"))
}
pub fn validate_profile(p: &Value) -> Result<()> {
    uuid::Uuid::parse_str(text(p, "id")?).context("Invalid profile ID")?;
    let version = text(p, "version")?;
    ensure!(
        version.len() < 50
            && version
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b".-_".contains(&b)),
        "Invalid Minecraft version"
    );
    ensure!(
        matches!(p["loader"].as_str(), Some("vanilla" | "fabric")),
        "Choose Vanilla or Fabric"
    );
    if p["performance"] == true {
        ensure!(
            p["loader"] == "fabric" && p["version"] == "1.21.1",
            "Performance preset requires Fabric for Minecraft 1.21.1"
        );
    }
    let mem = p["memory"].as_u64().unwrap_or(0);
    ensure!(
        (1024..=32768).contains(&mem),
        "Memory must be between 1024 and 32768 MB"
    );
    for key in ["width", "height"] {
        ensure!(
            p[key].as_u64().is_some_and(|v| (480..=7680).contains(&v)),
            "Resolution must be between 480 and 7680 pixels"
        );
    }
    ensure!(
        p["javaArgs"].as_array().is_some_and(|a| a.len() <= 30
            && a.iter().all(|x| x
                .as_str()
                .is_some_and(|x| x.starts_with('-') && x.len() < 1024 && !x.contains('\n')))),
        "Java arguments must be a JSON array of JVM options"
    );
    ensure!(
        matches!(p["account"].as_str(), Some("offline" | "microsoft")),
        "Choose an account type"
    );
    if p["account"] == "offline" {
        let name = text(p, "offlineName")?;
        ensure!(
            (3..=16).contains(&name.len())
                && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_'),
            "Offline name needs 3–16 letters, digits, or underscores"
        );
    }
    Ok(())
}
fn profile_root(s: &AppState, p: &Value) -> Result<PathBuf> {
    let id = text(p, "id")?;
    uuid::Uuid::parse_str(id)?;
    Ok(s.root.join("minecraft/profiles").join(id))
}
pub async fn versions(s: &AppState) -> Result<Value> {
    let m = net::cached(s, "mc:manifest", MANIFEST, 3600).await?;
    let list = m["versions"]
        .as_array()
        .context("Invalid version manifest")?
        .iter()
        .filter(|v| {
            v["type"] == "release" && v["releaseTime"].as_str().unwrap_or("") >= "2024-04-23"
        })
        .map(|v| json!({"id":v["id"],"type":v["type"]}))
        .collect::<Vec<_>>();
    Ok(json!(list))
}
#[derive(Clone, serde::Serialize, serde::Deserialize)]
struct Download {
    url: String,
    path: PathBuf,
    sha1: String,
    native: bool,
}
fn task(v: &Value, path: PathBuf) -> Result<Download> {
    Ok(Download {
        url: text(v, "url")?.to_owned(),
        sha1: text(v, "sha1")?.to_owned(),
        path,
        native: false,
    })
}
fn report(app: Option<&tauri::AppHandle>, stage: &str, done: usize, total: usize) {
    if let Some(a) = app {
        crate::progress(a, stage, done, total)
    }
}
async fn batch(
    s: &AppState,
    app: Option<&tauri::AppHandle>,
    label: &str,
    jobs: Vec<Download>,
) -> Result<()> {
    let count = jobs.len();
    let done = std::sync::atomic::AtomicUsize::new(0);
    report(app, label, 0, count);
    stream::iter(jobs)
        .map(|d| {
            let done = &done;
            async move {
                s.net.download(&d.url, &d.path, &d.sha1).await?;
                let n = done.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
                if n % 10 == 0 || n == count {
                    report(app, label, n, count)
                }
                Ok::<(), anyhow::Error>(())
            }
        })
        .buffer_unordered(4)
        .try_collect::<Vec<_>>()
        .await?;
    Ok(())
}
pub fn rules(v: &Value, features: &HashMap<String, bool>) -> bool {
    let Some(rs) = v["rules"].as_array() else {
        return true;
    };
    let mut allow = false;
    for r in rs {
        let os = &r["os"];
        let os_ok = os["name"].as_str().is_none_or(|n| n == "windows")
            && os["arch"]
                .as_str()
                .is_none_or(|n| n == "x86_64" || n == "amd64")
            && os["version"]
                .as_str()
                .is_none_or(|pattern| regex::Regex::new(pattern).is_ok_and(|r| r.is_match("10.0")));
        let feature_ok = r["features"].as_object().is_none_or(|m| {
            m.iter().all(|(k, v)| {
                features.get(k).copied().unwrap_or(false) == v.as_bool().unwrap_or(false)
            })
        });
        if os_ok && feature_ok {
            allow = r["action"] == "allow";
        }
    }
    allow
}
fn maven(name: &str) -> Result<String> {
    let parts = name.split(':').collect::<Vec<_>>();
    ensure!(parts.len() == 3, "Unsupported Maven coordinate");
    let group = parts[0].replace('.', "/");
    Ok(format!(
        "{group}/{}/{}/{}-{}.jar",
        parts[1], parts[2], parts[1], parts[2]
    ))
}
pub async fn install(app: &tauri::AppHandle, s: &AppState, v: &Value) -> Result<Value> {
    let p = db::item(s, text(v, "id")?)?;
    install_core(Some(app), s, &p).await
}
pub async fn install_core(
    app: Option<&tauri::AppHandle>,
    s: &AppState,
    p: &Value,
) -> Result<Value> {
    validate_profile(p)?;
    let root = profile_root(s, p)?;
    let shared = s.root.join("minecraft/shared");
    let version = text(p, "version")?;
    let m = net::cached(s, "mc:manifest", MANIFEST, 3600).await?;
    let entry = m["versions"]
        .as_array()
        .context("Version manifest missing")?
        .iter()
        .find(|v| v["id"] == version)
        .context("Version not found in Mojang manifest")?;
    report(app, "Fetching version metadata", 0, 1);
    let meta_path = root.join("version.json");
    let d = task(entry, meta_path.clone())?;
    s.net.download(&d.url, &d.path, &d.sha1).await?;
    let mut meta: Value = serde_json::from_slice(&tokio::fs::read(meta_path).await?)?;
    ensure!(
        meta["arguments"]["game"].is_array(),
        "This release supports modern Java Edition versions only (1.20.5+)"
    );
    let runtime_root = ensure_runtime(s, &meta, app).await?;
    let component = text(&meta["javaVersion"], "component")?.to_owned();
    let mut jobs = vec![task(
        &meta["downloads"]["client"],
        shared
            .join("versions")
            .join(version)
            .join(format!("{version}.jar")),
    )?];
    let mut cp = vec![];
    let mut natives = vec![];
    let features = HashMap::new();
    for lib in meta["libraries"]
        .as_array()
        .context("Version has no libraries")?
    {
        if !rules(lib, &features) {
            continue;
        }
        if lib["downloads"]["artifact"].is_object() {
            let a = &lib["downloads"]["artifact"];
            let path = net::safe_join(&shared.join("libraries"), text(a, "path")?)?;
            cp.push(path.clone());
            jobs.push(task(a, path)?);
        }
        if let Some(class) = lib["natives"]["windows"].as_str() {
            let class = class.replace("${arch}", "64");
            let a = &lib["downloads"]["classifiers"][class];
            let path = net::safe_join(&shared.join("libraries"), text(a, "path")?)?;
            let mut d = task(a, path.clone())?;
            d.native = true;
            natives.push(path);
            jobs.push(d);
        }
    }
    if p["loader"] == "fabric" {
        report(app, "Resolving Fabric Loader", 0, 1);
        let loaders = net::cached(
            s,
            &format!("fabric:{version}"),
            &format!("https://meta.fabricmc.net/v2/versions/loader/{version}"),
            3600,
        )
        .await?;
        let preset: Value =
            serde_json::from_str(include_str!("../../docs/performance-profile.json"))?;
        let loader = loaders
            .as_array()
            .and_then(|a| {
                a.iter().find(|x| {
                    if p["performance"] == true {
                        x["loader"]["version"] == preset["fabricLoader"]
                    } else {
                        x["loader"]["stable"] == true
                    }
                })
            })
            .context("No stable Fabric loader for this version")?;
        let lv = text(&loader["loader"], "version")?;
        let fabric = net::cached(
            s,
            &format!("fabric:{version}:{lv}"),
            &format!("https://meta.fabricmc.net/v2/versions/loader/{version}/{lv}/profile/json"),
            86400,
        )
        .await?;
        meta["mainClass"] = fabric["mainClass"].clone();
        for lib in fabric["libraries"]
            .as_array()
            .context("Fabric libraries missing")?
        {
            let rel = maven(text(lib, "name")?)?;
            let url = format!(
                "{}{}",
                lib["url"].as_str().unwrap_or("https://maven.fabricmc.net/"),
                rel
            );
            let sha = s
                .net
                .request(reqwest::Method::GET, &format!("{url}.sha1"), None)
                .await?
                .text()
                .await?;
            let path = net::safe_join(&shared.join("libraries"), &rel)?;
            cp.push(path.clone());
            jobs.push(Download {
                url,
                path,
                sha1: sha.trim().to_owned(),
                native: false,
            });
        }
        if let Some(game) = fabric["arguments"]["game"].as_array() {
            meta["arguments"]["game"]
                .as_array_mut()
                .unwrap()
                .extend(game.iter().cloned());
        }
    }
    let client_path = shared
        .join("versions")
        .join(version)
        .join(format!("{version}.jar"));
    cp.push(client_path);
    let log = &meta["logging"]["client"]["file"];
    let mut log_path = None;
    if log.is_object() {
        let path = net::safe_join(&shared.join("logging"), text(log, "id")?)?;
        jobs.push(task(log, path.clone())?);
        log_path = Some(path);
    }
    batch(s, app, "Installing game and libraries", jobs.clone()).await?;
    let native_dir = root.join("natives");
    tokio::fs::create_dir_all(&native_dir).await?;
    for archive in natives {
        let dest = native_dir.clone();
        tokio::task::spawn_blocking(move || -> Result<()> {
            let mut zip = zip::ZipArchive::new(std::fs::File::open(archive)?)?;
            for i in 0..zip.len() {
                let mut f = zip.by_index(i)?;
                let Some(name) = f.enclosed_name() else {
                    bail!("Unsafe path in native archive")
                };
                if name
                    .extension()
                    .is_some_and(|e| e.eq_ignore_ascii_case("dll"))
                {
                    let target = dest.join(name.file_name().context("Invalid native file")?);
                    let mut out = std::fs::File::create(target)?;
                    std::io::copy(&mut f, &mut out)?;
                }
            }
            Ok(())
        })
        .await??;
    }
    let ai = &meta["assetIndex"];
    let asset_id = text(ai, "id")?;
    let index_path = net::safe_join(&shared.join("assets/indexes"), &format!("{asset_id}.json"))?;
    let d = task(ai, index_path.clone())?;
    s.net.download(&d.url, &d.path, &d.sha1).await?;
    let index: Value = serde_json::from_slice(&tokio::fs::read(index_path).await?)?;
    let mut assets = vec![];
    for o in index["objects"]
        .as_object()
        .context("Asset index missing objects")?
        .values()
    {
        let hash = text(o, "hash")?;
        ensure!(
            hash.len() == 40 && hash.bytes().all(|b| b.is_ascii_hexdigit()),
            "Invalid asset hash"
        );
        let rel = format!("{}/{}", &hash[..2], hash);
        assets.push(Download {
            url: format!("https://resources.download.minecraft.net/{rel}"),
            path: shared.join("assets/objects").join(rel),
            sha1: hash.to_owned(),
            native: false,
        });
    }
    batch(s, app, "Downloading assets", assets).await?;
    tokio::fs::create_dir_all(root.join("game")).await?;
    let mods = if p["performance"] == true {
        crate::mods::install(s, &root, version, app).await?
    } else {
        json!([])
    };
    let plan = json!({"meta":meta,"classpath":cp,"nativeDir":native_dir,"java":runtime_root.join("bin/javaw.exe"),"assets":shared.join("assets"),"logPath":log_path,"files":jobs,"runtimeComponent":component,"profileVersion":version,"loader":p["loader"],"performance":p["performance"],"mods":mods});
    tokio::fs::write(
        root.join("installed.json"),
        serde_json::to_vec_pretty(&plan)?,
    )
    .await?;
    report(app, "Installation complete", 1, 1);
    Ok(json!({"installed":true,"id":p["id"]}))
}
pub async fn ensure_runtime(
    s: &AppState,
    meta: &Value,
    app: Option<&tauri::AppHandle>,
) -> Result<PathBuf> {
    let shared = s.root.join("minecraft/shared");
    let java = meta["javaVersion"].clone();
    let component = text(&java, "component")?;
    let runtime_index = net::cached(s, "mc:runtimes", RUNTIMES, 86400).await?;
    let runtime = &runtime_index["windows-x64"][component][0];
    ensure!(
        runtime.is_object(),
        "No Windows x64 Java runtime is available for this version"
    );
    let runtime_root = shared.join("runtimes").join(component);
    let runtime_meta = runtime_root.join("manifest.json");
    let d = task(&runtime["manifest"], runtime_meta.clone())?;
    s.net.download(&d.url, &d.path, &d.sha1).await?;
    let rm: Value = serde_json::from_slice(&tokio::fs::read(runtime_meta).await?)?;
    let mut runtime_jobs = vec![];
    for (name, f) in rm["files"]
        .as_object()
        .context("Runtime manifest missing files")?
    {
        let dest = net::safe_join(&runtime_root, name)?;
        match f["type"].as_str() {
            Some("file") => runtime_jobs.push(task(&f["downloads"]["raw"], dest)?),
            Some("directory") => tokio::fs::create_dir_all(dest).await?,
            Some("link") => bail!("Runtime contains an unsupported symbolic link"),
            _ => bail!("Unknown runtime file type"),
        }
    }
    batch(s, app, "Installing Java runtime", runtime_jobs).await?;
    Ok(runtime_root)
}
fn expand(s: &str, map: &HashMap<String, String>) -> Result<String> {
    let mut out = s.to_owned();
    for (k, v) in map {
        out = out.replace(&format!("${{{k}}}"), v);
    }
    ensure!(
        !out.contains("${"),
        "This version needs an unsupported launch argument: {out}"
    );
    Ok(out)
}
fn args(
    values: &Value,
    map: &HashMap<String, String>,
    features: &HashMap<String, bool>,
) -> Result<Vec<String>> {
    let mut out = vec![];
    for a in values.as_array().context("Missing launch arguments")? {
        if let Some(s) = a.as_str() {
            out.push(expand(s, map)?)
        } else if rules(a, features) {
            match &a["value"] {
                Value::String(s) => out.push(expand(s, map)?),
                Value::Array(xs) => {
                    for x in xs {
                        out.push(expand(x.as_str().context("Invalid launch argument")?, map)?)
                    }
                }
                _ => bail!("Invalid conditional argument"),
            }
        }
    }
    Ok(out)
}
pub async fn launch(app: &tauri::AppHandle, s: &AppState, v: &Value) -> Result<Value> {
    let id = text(v, "id")?;
    let p = db::item(s, id)?;
    validate_profile(&p)?;
    crate::servers::client_memory(s, p["memory"].as_u64().unwrap_or(2048))?;
    let root = profile_root(s, &p)?;
    let installed: Value = serde_json::from_slice(
        &tokio::fs::read(root.join("installed.json"))
            .await
            .context("Install this profile before playing")?,
    )?;
    ensure!(
        installed["profileVersion"] == p["version"] && installed["loader"] == p["loader"],
        "Profile version or loader changed. Run Install / Repair first."
    );
    let session = if p["account"] == "microsoft" {
        auth::session(s).await?
    } else {
        offline_session(text(&p, "offlineName")?)
    };
    let mut command = launch_command(&p, &root, &installed, &session)?;
    if let Some(address) = v["join"].as_str() {
        ensure!(
            p["account"] == "microsoft" && p["version"] == crate::servers::VERSION,
            "Server Quick Play requires an authenticated supported profile"
        );
        crate::servers::sessions::address(address)?;
        command.arg("--quickPlayMultiplayer").arg(address);
    }
    let secret = session["accessToken"].as_str().unwrap_or("").to_owned();
    spawn(app, s, &p, command, secret).await
}
pub fn offline_session(name: &str) -> Value {
    let digest = md5::compute(format!("OfflinePlayer:{name}"));
    let mut bytes = digest.0;
    bytes[6] = (bytes[6] & 0x0f) | 0x30;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    json!({"name":name,"id":uuid::Uuid::from_bytes(bytes).simple().to_string(),"accessToken":"0"})
}
pub fn launch_command(
    p: &Value,
    root: &Path,
    installed: &Value,
    session: &Value,
) -> Result<tokio::process::Command> {
    let meta = &installed["meta"];
    let cp = installed["classpath"]
        .as_array()
        .context("Classpath missing")?
        .iter()
        .map(|x| x.as_str().context("Invalid classpath"))
        .collect::<Result<Vec<_>>>()?
        .join(";");
    let game = root.join("game");
    let mut map: HashMap<String, String> = [
        ("auth_player_name", text(session, "name")?.to_owned()),
        ("auth_uuid", text(session, "id")?.to_owned()),
        (
            "auth_access_token",
            text(session, "accessToken")?.to_owned(),
        ),
        ("auth_session", text(session, "accessToken")?.to_owned()),
        ("version_name", text(p, "version")?.to_owned()),
        ("version_type", "release".to_owned()),
        ("game_directory", game.to_string_lossy().into_owned()),
        ("assets_root", text(installed, "assets")?.to_owned()),
        (
            "assets_index_name",
            text(&meta["assetIndex"], "id")?.to_owned(),
        ),
        ("user_type", "msa".to_owned()),
        ("user_properties", "{}".to_owned()),
        (
            "natives_directory",
            text(installed, "nativeDir")?.to_owned(),
        ),
        ("launcher_name", "Orbit".to_owned()),
        ("launcher_version", env!("CARGO_PKG_VERSION").to_owned()),
        ("classpath", cp),
        ("classpath_separator", ";".to_owned()),
        (
            "library_directory",
            root.parent()
                .unwrap()
                .parent()
                .unwrap()
                .join("shared/libraries")
                .to_string_lossy()
                .into_owned(),
        ),
        ("resolution_width", p["width"].to_string()),
        ("resolution_height", p["height"].to_string()),
        (
            "clientid",
            session["clientId"].as_str().unwrap_or("").to_owned(),
        ),
        (
            "auth_xuid",
            session["xuid"].as_str().unwrap_or("").to_owned(),
        ),
        (
            "quickPlayPath",
            root.join("quickplay.json").to_string_lossy().into_owned(),
        ),
    ]
    .into_iter()
    .map(|(k, v)| (k.to_owned(), v))
    .collect();
    let features = HashMap::from([
        ("has_custom_resolution".to_owned(), true),
        ("is_demo_user".to_owned(), false),
    ]);
    let java = Path::new(text(installed, "java")?);
    ensure!(
        java.is_file(),
        "Java runtime is missing. Run Install / Repair."
    );
    for f in installed["classpath"].as_array().unwrap() {
        ensure!(
            Path::new(f.as_str().unwrap()).is_file(),
            "A library is missing. Run Install / Repair."
        );
    }
    let mut cmd = tokio::process::Command::new(java);
    cmd.args(args(&meta["arguments"]["jvm"], &map, &features)?);
    cmd.arg(format!("-Xmx{}M", p["memory"].as_u64().unwrap()));
    cmd.arg("-Xms512M");
    for a in p["javaArgs"].as_array().unwrap() {
        cmd.arg(a.as_str().unwrap());
    }
    if let Some(path) = installed["logPath"].as_str() {
        map.insert("path".into(), path.into());
        cmd.arg(expand(text(&meta["logging"]["client"], "argument")?, &map)?);
    }
    cmd.arg(text(meta, "mainClass")?);
    cmd.args(args(&meta["arguments"]["game"], &map, &features)?);
    if p["fullscreen"] == true {
        cmd.arg("--fullscreen");
    }
    cmd.current_dir(game);
    #[cfg(windows)]
    cmd.creation_flags(0x08000000);
    Ok(cmd)
}
async fn spawn(
    app: &tauri::AppHandle,
    s: &AppState,
    p: &Value,
    mut cmd: tokio::process::Command,
    secret: String,
) -> Result<Value> {
    use tauri::Emitter;
    use tokio::io::{AsyncBufReadExt, BufReader};
    let root = profile_root(s, p)?;
    let log = root.join("orbit.log");
    let _ = tokio::fs::remove_file(&log).await;
    cmd.stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    let mut child = cmd
        .spawn()
        .context("Java could not start. Repair the installation and retry.")?;
    let pid = child.id();
    let _ = app.emit("process", json!({"id":p["id"],"running":true,"pid":pid}));
    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();
    let writer = std::sync::Arc::new(tokio::sync::Mutex::new(
        tokio::fs::File::create(&log).await?,
    ));
    for stream in [
        Box::new(stdout) as Box<dyn tokio::io::AsyncRead + Unpin + Send>,
        Box::new(stderr),
    ] {
        let writer = writer.clone();
        let secret = secret.clone();
        let a = app.clone();
        tauri::async_runtime::spawn(async move {
            let mut lines = BufReader::new(stream).lines();
            let mut size = 0;
            while let Ok(Some(line)) = lines.next_line().await {
                let line = if secret.len() > 8 {
                    line.replace(&secret, "[REDACTED]")
                } else {
                    line
                };
                let clean = redact(&line);
                if size < 5_000_000 {
                    use tokio::io::AsyncWriteExt;
                    let _ = writer
                        .lock()
                        .await
                        .write_all(format!("{clean}\n").as_bytes())
                        .await;
                    size += clean.len();
                }
                let _ = a.emit("game-log", json!(clean));
            }
        });
    }
    db::record(s, text(p, "id")?)?;
    crate::activity::game_started(
        s,
        &json!({"id":p["id"],"name":p["name"],"kind":"minecraft","version":p["version"],"loader":p["loader"]}),
        pid,
    );
    let a = app.clone();
    let id = p["id"].clone();
    tauri::async_runtime::spawn(async move {
        let exit = child.wait().await;
        use tauri::Manager;
        crate::activity::game_stopped(&a.state::<crate::Shared>(), pid);
        let _ = a.emit(
            "process",
            json!({"id":id,"running":false,"exitCode":exit.ok().and_then(|s|s.code())}),
        );
    });
    post_launch(app, s);
    Ok(json!({"pid":pid,"running":true}))
}
fn redact(line: &str) -> String {
    let lower = line.to_ascii_lowercase();
    if [
        "access_token",
        "accesstoken",
        "refresh_token",
        "refreshtoken",
        "authorization:",
        "identitytoken",
        "--accesstoken",
    ]
    .iter()
    .any(|x| lower.contains(x))
    {
        "[Credential-bearing log line omitted]".into()
    } else {
        line.chars().take(8192).collect()
    }
}
pub fn logs(s: &AppState, v: &Value) -> Result<Value> {
    use std::io::{Read, Seek, SeekFrom};
    let p = db::item(s, text(v, "id")?)?;
    let root = profile_root(s, &p)?;
    let path = root.join("orbit.log");
    if !path.exists() {
        return Ok(json!("No launch logs yet."));
    }
    let mut f = std::fs::File::open(path)?;
    let len = f.metadata()?.len();
    f.seek(SeekFrom::Start(len.saturating_sub(128_000)))?;
    let mut buf = vec![];
    f.read_to_end(&mut buf)?;
    Ok(json!(String::from_utf8_lossy(&buf)))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn windows_rules() {
        let f = HashMap::new();
        assert!(rules(&json!({}), &f));
        assert!(!rules(
            &json!({"rules":[{"action":"allow","os":{"name":"linux"}}]}),
            &f
        ));
        assert!(rules(
            &json!({"rules":[{"action":"allow"},{"action":"disallow","os":{"name":"osx"}}]}),
            &f
        ));
        assert!(!rules(
            &json!({"rules":[{"action":"allow","features":{"is_demo_user":true}}]}),
            &f
        ));
    }
    #[test]
    fn offline_identity() {
        assert_eq!(
            offline_session("Notch")["id"],
            "b50ad385829d3141a2167e7d7539ba7f"
        );
    }
    #[test]
    fn no_unknown_substitutions() {
        assert!(expand("${missing}", &HashMap::new()).is_err());
    }
    #[test]
    fn credentials_redacted() {
        assert!(!redact("--accessToken secret").contains("secret"));
    }
}
