//! Opt-in integration checks. Writes only to ORBIT_TEST_ROOT.
use crate::*;
use anyhow::{ensure, Context, Result};
use serde_json::json;
fn test_state(root: PathBuf) -> Result<AppState> {
    std::fs::create_dir_all(&root)?;
    Ok(AppState {
        db: Mutex::new(db::init(&root)?),
        root,
        net: net::Network::new()?,
        install_lock: tokio::sync::Mutex::new(()),
        auth_lock: tokio::sync::Mutex::new(()),
        social: social::Runtime::default(),
        transfers: media::Transfers::new(),
        voice: crate::voice::Runtime::default(),
    })
}

#[tokio::test]
#[ignore = "Imports a real Windows shortcut and launches the harmless where.exe utility"]
async fn live_program_import_persistence_and_launch() -> Result<()> {
    let root = PathBuf::from(std::env::var("ORBIT_TEST_ROOT")?).join("programs");
    let s = test_state(root.clone())?;
    let link = root.join("Skrót testowy.lnk");
    let script = r#"$s=(New-Object -ComObject WScript.Shell).CreateShortcut($env:ORBIT_LINK);$s.TargetPath='C:\Windows\System32\where.exe';$s.Arguments='notepad.exe';$s.WorkingDirectory='C:\Windows';$s.Save()"#;
    let status = tokio::process::Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-Command", script])
        .env("ORBIT_LINK", &link)
        .status()
        .await?;
    ensure!(status.success(), "Cannot create test shortcut");
    let mut entry = programs::import(&s, &json!({"path":link})).await?;
    ensure!(
        entry["args"] == json!(["notepad.exe"]),
        "Shortcut arguments were lost"
    );
    ensure!(
        entry["image"]
            .as_str()
            .is_some_and(|v| v.starts_with("data:image/png")),
        "Icon extraction failed"
    );
    entry["favorite"] = json!(true);
    db::save_item(&s, &entry)?;
    let collection = db::save_collection(&s, &json!({"name":"QA collection"}))?;
    db::collect(&s, &json!({"id":entry["id"],"collection":collection}))?;
    programs::launch(None, &s, &json!({"id":entry["id"]})).await?;
    let lib = db::library(&s)?;
    ensure!(
        lib[0]["lastLaunch"].as_i64().is_some(),
        "Launch history missing"
    );
    drop(s);
    let s = test_state(root)?;
    let lib = db::library(&s)?;
    ensure!(lib[0]["favorite"] == true, "Favorite did not persist");
    ensure!(
        db::collections(&s)?[0]["items"][0] == entry["id"],
        "Collection did not persist"
    );
    let mut missing = entry.clone();
    missing["id"] = json!(uuid::Uuid::new_v4().to_string());
    missing["path"] = json!(s.root.join("missing.exe"));
    db::save_item(&s, &missing)?;
    let error = programs::launch(None, &s, &json!({"id":missing["id"]}))
        .await
        .unwrap_err();
    ensure!(
        error.to_string().contains("missing or has moved"),
        "Missing file error was not readable"
    );
    db::remove_item(&s, &json!({"id":entry["id"]}))?;
    ensure!(
        db::collections(&s)?[0]["items"]
            .as_array()
            .unwrap()
            .is_empty(),
        "Collection deletion cascade failed"
    );
    eprintln!("Shortcut, icon, arguments, launch, history, persistence, collection and missing-path checks passed");
    Ok(())
}

#[tokio::test]
#[ignore = "Queries live Roblox and optionally dispatches a selected free experience"]
async fn live_roblox_catalog_and_dispatch() -> Result<()> {
    let root = PathBuf::from(std::env::var("ORBIT_TEST_ROOT")?).join("roblox");
    let s = test_state(root)?;
    let home = roblox::home(&s).await;
    ensure!(
        home.is_err(),
        "Home access changed; inspect its schema before shipping"
    );
    eprintln!("Home limitation: {}", home.unwrap_err());
    let explore = roblox::explore(&s).await?;
    let sorts = explore["data"]["sorts"]
        .as_array()
        .context("Explore has no sorts")?;
    let sort = sorts
        .iter()
        .find(|x| x["contentType"] == "Games")
        .context("No experience sections")?;
    let id = sort["games"][0]["universeId"]
        .as_u64()
        .context("No universe ID")?;
    let d = roblox::details(&s, &json!({"ids":[id]})).await?;
    ensure!(
        d["games"][0]["creator"]["name"].is_string(),
        "Creator metadata missing"
    );
    let image = d["thumbnails"][0]["imageUrl"]
        .as_str()
        .context("Thumbnail missing")?;
    let cached = net::image(&s, &json!({"url":image})).await?;
    ensure!(
        PathBuf::from(cached.as_str().unwrap()).is_file(),
        "Thumbnail not cached"
    );
    let search = roblox::search(&s, &json!({"query":"obby"})).await?;
    ensure!(
        search["searchResults"]
            .as_array()
            .is_some_and(|a| !a.is_empty()),
        "Search returned no results"
    );
    eprintln!(
        "Live section: {} / selected experience: {} ({id})",
        sort["sortDisplayName"], d["games"][0]["name"]
    );
    if std::env::var("ORBIT_TEST_ROBLOX_LAUNCH").as_deref() == Ok("1") {
        ensure!(
            d["games"][0]["price"].is_null() || d["games"][0]["price"] == 0,
            "Skipping paid experience"
        );
        let r = roblox::launch(None, &s, &json!({"universeId":id})).await?;
        ensure!(r["dispatched"] == true, "Protocol not dispatched");
        eprintln!("Selected Roblox place dispatched; server entry is not observable via this API");
    }
    Ok(())
}
#[tokio::test]
#[ignore = "Downloads Minecraft and optionally opens the game; set ORBIT_TEST_ROOT"]
async fn live_minecraft_install_repair_and_launch() -> Result<()> {
    let root = PathBuf::from(std::env::var("ORBIT_TEST_ROOT")?);
    ensure!(root.is_absolute(), "Use an absolute test root");
    std::fs::create_dir_all(&root)?;
    let state = AppState {
        db: Mutex::new(db::init(&root)?),
        root: root.clone(),
        net: net::Network::new()?,
        install_lock: tokio::sync::Mutex::new(()),
        auth_lock: tokio::sync::Mutex::new(()),
        social: social::Runtime::default(),
        transfers: media::Transfers::new(),
        voice: crate::voice::Runtime::default(),
    };
    let loader = std::env::var("ORBIT_TEST_LOADER").unwrap_or("vanilla".into());
    let performance = std::env::var("ORBIT_TEST_PERFORMANCE").as_deref() == Ok("1");
    let profile_id = uuid::Uuid::new_v3(
        &uuid::Uuid::NAMESPACE_OID,
        format!("orbit-test-{loader}-{performance}").as_bytes(),
    )
    .to_string();
    let p = json!({"id":profile_id,"kind":"minecraft","name":"Integration test","version":"1.21.1","loader":loader,"memory":2048,"width":960,"height":540,"javaArgs":[],"account":"offline","offlineName":"OrbitTest","fullscreen":false,"performance":performance});
    db::save_item(&state, &p)?;
    eprintln!("Installing Minecraft 1.21.1 + managed Java; loader={loader}");
    minecraft::install_core(None, &state, &p).await?;
    let profile = root.join("minecraft/profiles").join(&profile_id);
    let installed: Value = serde_json::from_slice(&std::fs::read(profile.join("installed.json"))?)?;
    let client = root.join("minecraft/shared/versions/1.21.1/1.21.1.jar");
    let bytes = std::fs::read(&client)?;
    let part = client.with_extension("jar.part");
    std::fs::write(&part, &bytes[..bytes.len() / 2])?;
    std::fs::write(&client, b"corrupt")?;
    eprintln!("Repairing corrupt client from a half-complete .part file");
    minecraft::install_core(None, &state, &p).await?;
    ensure!(std::fs::read(&client)? == bytes, "Client was not repaired");
    ensure!(!part.exists(), "Partial file was not promoted");
    let java = PathBuf::from(installed["java"].as_str().unwrap()).with_file_name("java.exe");
    let out = tokio::process::Command::new(java)
        .arg("-version")
        .output()
        .await?;
    ensure!(out.status.success(), "Managed Java cannot run");
    eprintln!("{}", String::from_utf8_lossy(&out.stderr));
    if std::env::var("ORBIT_TEST_LAUNCH").as_deref() == Ok("1") {
        let mut command = minecraft::launch_command(
            &p,
            &profile,
            &installed,
            &minecraft::offline_session("OrbitTest"),
        )?;
        let log = std::fs::File::create(root.join(format!("{loader}-launch.log")))?;
        command.stdout(log.try_clone()?).stderr(log);
        let mut child = command.spawn()?;
        eprintln!("Direct game PID {:?}; observing for 35 seconds", child.id());
        tokio::time::sleep(std::time::Duration::from_secs(35)).await;
        let status = child.try_wait()?;
        if status.is_none() {
            child.kill().await?;
        }
        ensure!(
            status.is_none(),
            "Minecraft exited early: {status:?}. Inspect the launch log."
        );
        let log = std::fs::read_to_string(root.join(format!("{loader}-launch.log")))?;
        ensure!(
            log.contains("Backend library"),
            "Game did not initialize rendering. Inspect launch log."
        );
    }
    eprintln!("Installation, verification, recovery, and managed Java passed");
    Ok(())
}
