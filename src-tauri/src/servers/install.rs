use super::*;
use crate::{minecraft, net};
pub async fn prepare(s: &AppState, p: &Value, app: Option<&tauri::AppHandle>) -> Result<PathBuf> {
    validate(p)?;
    ensure!(
        p["eulaAcceptedAt"].as_i64().is_some_and(|x| x > 0),
        "Review and accept the Minecraft server EULA before hosting"
    );
    let folder = root(s, p["id"].as_str().unwrap())?;
    tokio::fs::create_dir_all(&folder).await?;
    if folder.join("world/level.dat").exists() {
        ensure!(worlds::world_version(&folder.join("world"))?==3955,"This world was changed to another Minecraft version. Hosting is blocked to prevent a silent downgrade. Restore a compatible backup or use a separate server.");
    }
    let manifest = net::cached(
        s,
        "mc:manifest",
        "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json",
        3600,
    )
    .await?;
    let v = manifest["versions"]
        .as_array()
        .context("Minecraft manifest missing")?
        .iter()
        .find(|v| v["id"] == VERSION)
        .context("Supported Minecraft version missing")?;
    let meta_path = folder.join("version.json");
    s.net
        .download(
            v["url"].as_str().context("Metadata URL missing")?,
            &meta_path,
            v["sha1"].as_str().context("Metadata hash missing")?,
        )
        .await?;
    let meta: Value = serde_json::from_slice(&tokio::fs::read(meta_path).await?)?;
    let runtime = minecraft::ensure_runtime(s, &meta, app).await?;
    let d = &meta["downloads"]["server"];
    if let Some(a) = app {
        crate::progress(a, "Installing verified Minecraft server", 0, 1);
    }
    s.net
        .download(
            d["url"].as_str().context("Server URL missing")?,
            &folder.join("server.jar"),
            d["sha1"].as_str().context("Server hash missing")?,
        )
        .await?;
    if p["loader"] == "fabric" {
        // Official generated server metadata supplies the launcher class and Maven library set.
        // Every executable library is checked against its upstream Maven SHA-1 before launch.
        let metadata = net::cached(
            s,
            "server:fabric:1.21.1:0.16.14",
            &format!("https://meta.fabricmc.net/v2/versions/loader/{VERSION}/{FABRIC}/server/json"),
            86400,
        )
        .await?;
        let libs = metadata["libraries"]
            .as_array()
            .context("Fabric server metadata missing libraries")?;
        let mut cp = vec![];
        for lib in libs {
            let coordinate = lib["name"].as_str().context("Fabric coordinate missing")?;
            let parts = coordinate.split(':').collect::<Vec<_>>();
            ensure!(
                parts.len() == 3
                    && parts.iter().all(|s| s
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b".-_+".contains(&b))),
                "Invalid Fabric coordinate"
            );
            let relative = format!(
                "{}/{}/{}/{}-{}.jar",
                parts[0].replace('.', "/"),
                parts[1],
                parts[2],
                parts[1],
                parts[2]
            );
            let base = lib["url"]
                .as_str()
                .context("Fabric library repository missing")?;
            let url = format!("{}{relative}", base);
            let sha = s
                .net
                .request(reqwest::Method::GET, &format!("{url}.sha1"), None)
                .await?
                .text()
                .await?;
            let sha = sha
                .split_whitespace()
                .next()
                .context("Fabric hash missing")?;
            let path = net::safe_join(&folder.join("libraries"), &relative)?;
            s.net.download(&url, &path, sha).await?;
            cp.push(path);
        }
        tokio::fs::write(
            folder.join("fabric-server-launcher.properties"),
            "serverJar=server.jar\n",
        )
        .await?;
        tokio::fs::write(
            folder.join("fabric-plan.json"),
            serde_json::to_vec(&json!({"classpath":cp,"mainClass":"net.fabricmc.loader.impl.launch.server.FabricServerLauncher"}))?,
        )
        .await?;
    }
    // No unsupported arbitrary server mods are silently executed by this release.
    let mods = folder.join("mods");
    if mods.exists() {
        ensure!(std::fs::read_dir(mods)?.next().is_none(),"This release supports bare Fabric only. Use a separate server for modpacks; Orbit does not verify arbitrary server mods.");
    }
    tokio::fs::write(
        folder.join("eula.txt"),
        format!(
            "# Accepted in Orbit at {}\n# https://www.minecraft.net/eula\neula=true\n",
            p["eulaAcceptedAt"]
        ),
    )
    .await?;
    write_properties(s, p)?;
    if let Some(a) = app {
        crate::progress(a, "Server files ready", 1, 1);
    }
    let java = runtime.join("bin/java.exe");
    ensure!(
        java.is_file(),
        "Managed Java runtime missing. Retry Install / Repair."
    );
    Ok(java)
}
pub fn write_properties(s: &AppState, p: &Value) -> Result<()> {
    let folder = root(s, p["id"].as_str().unwrap())?;
    let bind = if p["access"] == "playit" {
        "127.0.0.1"
    } else {
        "0.0.0.0"
    };
    let authenticated = !offline(p);
    let text=format!("# Managed by Orbit. Settings are edited while stopped.\nserver-ip={bind}\nserver-port={}\nonline-mode={authenticated}\nenforce-secure-profile={authenticated}\nwhite-list=true\nenforce-whitelist=true\nop-permission-level=1\nenable-rcon=false\nenable-query=false\nenable-jmx-monitoring=false\nlevel-name=world\nmax-players={}\ndifficulty={}\ngamemode={}\nlevel-seed={}\nview-distance={}\nsimulation-distance={}\nmotd=Orbit private server\nspawn-protection=16\n",p["port"],p["players"],p["difficulty"].as_str().unwrap(),p["gameMode"].as_str().unwrap(),p["seed"].as_str().unwrap(),p["viewDistance"],p["simulationDistance"]);
    std::fs::create_dir_all(&folder)?;
    std::fs::write(folder.join("server.properties"), text)?;
    Ok(())
}
