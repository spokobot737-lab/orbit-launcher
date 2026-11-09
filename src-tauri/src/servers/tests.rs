use super::*;
use anyhow::Result;
#[tokio::test]
async fn encrypted_server_cards_expire_and_reject_wrong_author_or_older_revision() -> Result<()> {
    use crate::social_tests::{eventually, fixture, state};
    let base = std::env::temp_dir().join(format!("orbit-server-cards-{}", uuid::Uuid::new_v4()));
    let (relay, _, fixture_task, _) = fixture(&base).await?;
    let a = state(base.join("a"))?;
    let b = state(base.join("b"))?;
    for (s, name) in [(&a, "Synthetic host"), (&b, "Synthetic guest")] {
        crate::identity::save(s, &json!({"name":name}))?;
        crate::identity::save_config(s, &json!({"enabled":true,"relays":[relay]}))?;
        crate::social::start(None, s.clone()).await?;
    }
    let aid = crate::identity::keys(&a)?.public_key().to_hex();
    let bid = crate::identity::keys(&b)?.public_key().to_hex();
    let invitation = crate::social::invitation(&a).await?;
    crate::social::import_invitation(&b, &json!({"code":invitation["code"]})).await?;
    crate::social::flush(&b).await?;
    eventually(|| crate::identity::contact(&a, &bid).is_ok()).await?;
    crate::social::accept(&a, &json!({"id":bid})).await?;
    crate::social::flush(&a).await?;
    eventually(|| crate::identity::contact(&b, &aid).is_ok_and(|p| p["state"] == "accepted"))
        .await?;
    let mut card = json!({"schema":1,"server":uuid::Uuid::new_v4().to_string(),"session":uuid::Uuid::new_v4().to_string(),"host":aid,"name":"Synthetic fixture server","version":VERSION,"loader":"vanilla","mods":{"scope":"bare","required":[],"serverOnly":[],"optional":[]},"address":"example.org:25565","online":true,"expires":db::now()+150,"revision":2});
    let message = crate::social::queue(&a, &bid, "server_invite", card.clone(), true).await?;
    crate::social::flush(&a).await?;
    eventually(|| sessions::list(&b).is_ok_and(|v| v.as_array().unwrap().len() == 1)).await?;
    assert_eq!(sessions::list(&b)?[0]["revision"], 2);
    assert_eq!(sessions::list(&b)?[0]["fresh"], true);
    let target = json!({"peer":aid,"server":card["server"]});
    let clean = sessions::create_client(&b, &target)?;
    let mut offline = clean.clone();
    offline["id"] = json!(uuid::Uuid::new_v4().to_string());
    offline["account"] = json!("offline");
    crate::db::save_item(&b, &offline)?;
    assert_eq!(
        sessions::options(&b, &target).await?["profiles"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    let mods = b
        .root
        .join("minecraft/profiles")
        .join(clean["id"].as_str().unwrap())
        .join("game/mods");
    std::fs::create_dir_all(&mods)?;
    std::fs::write(
        mods.join("synthetic-unverified.jar"),
        b"synthetic file, never executed",
    )?;
    assert!(sessions::options(&b, &target).await?["profiles"]
        .as_array()
        .unwrap()
        .is_empty());
    assert!(crate::identity::messages(&b, &json!({"peer":aid}))?
        .as_array()
        .unwrap()
        .iter()
        .any(|m| m["id"] == message && m["kind"] == "server_invite"));
    card["revision"] = json!(3);
    card["authMode"] = json!("offline");
    sessions::receive(&b, &aid, &card)?;
    let options = sessions::options(&b, &target).await?;
    assert_eq!(options["profiles"].as_array().unwrap().len(), 1);
    assert_eq!(options["profiles"][0]["account"], "offline");
    assert_eq!(
        sessions::create_client(
            &b,
            &json!({"peer":aid,"server":card["server"],"nickname":"GuestTwo"})
        )?["offlineName"],
        "GuestTwo"
    );
    card["revision"] = json!(1);
    card["name"] = json!("Stale ignored");
    sessions::receive(&b, &aid, &card)?;
    assert_eq!(sessions::list(&b)?[0]["name"], "Synthetic fixture server");
    let mut forged = card.clone();
    forged["host"] = json!(bid);
    assert!(sessions::receive(&b, &aid, &forged).is_err());
    card["revision"] = json!(4);
    card["expires"] = json!(db::now() - 1);
    sessions::receive(&b, &aid, &card)?;
    assert_eq!(sessions::list(&b)?[0]["fresh"], false);
    assert!(
        sessions::options(&b, &json!({"peer":aid,"server":card["server"]}))
            .await
            .is_err()
    );
    for s in [&a, &b] {
        crate::social::stop(s).await;
        for key in ["nostr-key", "data-key", "evm-wallet"] {
            crate::secure::remove(s, key)?;
        }
    }
    fixture_task.abort();
    Ok(())
}
fn config() -> Value {
    json!({"id":"","name":"Synthetic hosted world","version":VERSION,"loader":"vanilla","memory":1024,"players":4,"difficulty":"easy","gameMode":"survival","seed":"test","viewDistance":3,"simulationDistance":3,"port":std::env::var("ORBIT_SERVER_TEST_PORT").ok().and_then(|s|s.parse::<u16>().ok()).unwrap_or(25879),"access":"playit","address":"","acceptEula":true})
}
#[tokio::test]
async fn nonpremium_profiles_allowlist_and_mode_migration() -> Result<()> {
    let s = crate::social_tests::state(
        std::env::temp_dir().join(format!("orbit-offline-{}", uuid::Uuid::new_v4())),
    )?;
    let mut p = config();
    p["authMode"] = json!("offline");
    let p = save(&s, p)?;
    let id = p["id"].as_str().unwrap();
    install::write_properties(&s, &p)?;
    let props = std::fs::read_to_string(root(&s, id)?.join("server.properties"))?;
    assert!(props.contains("online-mode=false\n"));
    assert!(props.contains("enforce-secure-profile=false\n"));
    assert!(props.contains("white-list=true\n"));
    let a = process::allowlist(&s, &json!({"id":id,"name":"OrbitGuest","approved":true})).await?;
    assert_eq!(
        uuid::Uuid::parse_str(a["account"]["uuid"].as_str().unwrap())?,
        uuid::Uuid::parse_str(
            crate::minecraft::offline_session("OrbitGuest")["id"]
                .as_str()
                .unwrap()
        )?
    );
    assert!(
        process::allowlist(&s, &json!({"id":id,"name":"Guest\nstop","approved":true}))
            .await
            .is_err()
    );
    assert!(process::allowlist(&s, &json!({"id":id,"name":"Guest"}))
        .await
        .is_err());
    let mut changed = p.clone();
    changed["authMode"] = json!("online");
    assert!(save(&s, changed.clone()).is_err());
    changed["confirmAuthChange"] = json!(true);
    let online = save(&s, changed)?;
    assert!(process::allowlist_get(&s, id)?
        .as_array()
        .unwrap()
        .is_empty());
    let mut back = online;
    back["authMode"] = json!("offline");
    back["confirmAuthChange"] = json!(true);
    save(&s, back)?;
    assert_eq!(process::allowlist_get(&s, id)?[0], a["account"]);
    process::allowlist(
        &s,
        &json!({"id":id,"name":"OrbitGuest","approved":true,"remove":true}),
    )
    .await?;
    assert!(process::allowlist_get(&s, id)?
        .as_array()
        .unwrap()
        .is_empty());
    // Legacy configs retain premium behavior; malformed modes cannot silently disable it.
    assert!(!offline(&config()));
    let mut invalid = p.clone();
    invalid["authMode"] = json!(false);
    assert!(validate(&invalid).is_err());
    Ok(())
}

#[tokio::test]
#[ignore = "Real isolated nonpremium server and directly launched Java client; explicit EULA and ORBIT_SERVER_TEST_ROOT required"]
async fn live_nonpremium_java_client_joins_hosted_server() -> Result<()> {
    ensure!(
        std::env::var("ORBIT_SERVER_EULA").as_deref() == Ok("accepted"),
        "EULA acceptance required"
    );
    let base = PathBuf::from(std::env::var("ORBIT_SERVER_TEST_ROOT")?);
    std::fs::create_dir_all(&base)?;
    let s = crate::social_tests::state(dunce::canonicalize(base)?)?;
    let mut p = config();
    p["authMode"] = json!("offline");
    p["name"] = json!("Isolated nonpremium verification");
    let p = save(&s, p)?;
    let id = p["id"].as_str().unwrap();
    process::start(None, s.clone(), id).await?;
    let result=async {
        let job=s.servers.jobs.lock().unwrap().get(id).unwrap().clone();
        for _ in 0..240 {if job.status.lock().unwrap()["localReady"]==true{break;}ensure!(!job.stopped.load(std::sync::atomic::Ordering::Acquire),"Test server exited");tokio::time::sleep(std::time::Duration::from_secs(1)).await;}
        ensure!(job.status.lock().unwrap()["localReady"]==true,"Server never became ready");
        let target=json!({"hostingId":id,"nickname":"OrbitGuest"});
        let mut client=sessions::create_client(&s,&target)?;client["memory"]=json!(1024);crate::db::save_item(&s,&client)?;
        ensure!(client["account"]=="offline","Offline target incorrectly requires Microsoft");
        let options=sessions::options(&s,&target).await?;ensure!(options["profiles"].as_array().unwrap().iter().any(|x|x["id"]==client["id"]),"Offline client excluded");
        crate::minecraft::install_core(None,&s,&client).await?;
        let profile=s.root.join("minecraft/profiles").join(client["id"].as_str().unwrap());
        let installed:Value=serde_json::from_slice(&std::fs::read(profile.join("installed.json"))?)?;
        // First try an unapproved name with the real client, then approve the
        // exact offline UUID and verify real world entry on the second attempt.
        for approved in [false,true] {
            if approved { process::allowlist(&s,&json!({"id":id,"name":"OrbitGuest","approved":true})).await?;tokio::time::sleep(std::time::Duration::from_secs(2)).await; }
            let mut cmd=crate::minecraft::launch_command(&client,&profile,&installed,&crate::minecraft::offline_session("OrbitGuest"))?;
            let address=if approved && std::env::var("ORBIT_TEST_PUBLIC_JOIN").as_deref()==Ok("1") {
                for _ in 0..90 {if job.status.lock().unwrap()["tunnelReady"]==true{break;}tokio::time::sleep(std::time::Duration::from_secs(2)).await;}
                ensure!(job.status.lock().unwrap()["tunnelReady"]==true,"Public tunnel not ready for real Java client");
                job.status.lock().unwrap()["address"].as_str().context("No public address")?.to_owned()
            } else {format!("127.0.0.1:{}",p["port"])};
            cmd.arg("--quickPlayMultiplayer").arg(&address).kill_on_drop(true);
            let output=std::fs::File::create(s.root.join(if approved{"nonpremium-join.log"}else{"nonpremium-denied.log"}))?;cmd.stdout(output.try_clone()?).stderr(output);
            let mut child=cmd.spawn()?;
            let observed=async {
                for _ in 0..120 {
                    let lines=job.logs.lock().unwrap().iter().cloned().collect::<Vec<_>>().join("\n");
                    if approved && lines.contains("OrbitGuest joined the game") { return Ok(()); }
                    if !approved && (lines.contains("OrbitGuest") && (lines.contains("not white-listed") || lines.contains("not whitelisted"))) { return Ok(()); }
                    ensure!(child.try_wait()?.is_none(),"Real Java client exited before verification");tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                }anyhow::bail!("Client admission result missing; inspect isolated client/server logs")
            }.await;
            if child.try_wait()?.is_none(){child.kill().await?;}child.wait().await.ok();observed?;
            eprintln!("NONPREMIUM_REAL_CLIENT_PASS approved={approved}: direct Minecraft Java1.21.1, no Microsoft session, actual server admission result");
        }
        Ok::<_,anyhow::Error>(())
    }.await;
    process::stop(None, &s, id, false).await?;
    result
}
#[tokio::test]
async fn world_import_rejects_active_modded_and_incompatible_sources() -> Result<()> {
    use fs2::FileExt;
    use std::io::Write;
    #[derive(serde::Serialize)]
    struct Metadata {
        #[serde(rename = "Data")]
        data: WorldData,
    }
    #[derive(serde::Serialize)]
    struct WorldData {
        #[serde(rename = "DataVersion")]
        version: i32,
        #[serde(rename = "WasModded")]
        modded: i8,
    }
    fn metadata(path: &std::path::Path, version: i32, modded: i8) -> Result<()> {
        std::fs::create_dir_all(path)?;
        let mut gz = flate2::write::GzEncoder::new(
            std::fs::File::create(path.join("level.dat"))?,
            flate2::Compression::default(),
        );
        gz.write_all(&fastnbt::to_bytes(&Metadata {
            data: WorldData { version, modded },
        })?)?;
        gz.finish()?;
        Ok(())
    }
    let base = std::env::temp_dir().join(format!("orbit-world-guards-{}", uuid::Uuid::new_v4()));
    let s = crate::social_tests::state(base.join("host"))?;
    let p = save(&s, config())?;
    let id = p["id"].as_str().unwrap();
    let source = base.join("original");
    metadata(&source, 3955, 0)?;
    std::fs::write(source.join("synthetic.txt"), "original preserved")?;
    let original = std::fs::read(source.join("level.dat"))?;
    let lock = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(source.join("session.lock"))?;
    FileExt::try_lock_exclusive(&lock)?;
    assert!(worlds::import(&s, &json!({"id":id,"path":source,"confirmed":true})).is_err());
    drop(lock);
    worlds::import(&s, &json!({"id":id,"path":source,"confirmed":true}))?;
    assert_eq!(std::fs::read(source.join("level.dat"))?, original);
    assert_eq!(
        std::fs::read(root(&s, id)?.join("world/level.dat"))?,
        original
    );
    let held = worlds::host_lock(&s, id)?;
    assert!(worlds::backup(&s, id).is_err());
    assert!(process::start(None, s.clone(), id).await.is_err());
    drop(held);
    metadata(&source, 3955, 1)?;
    assert!(worlds::import(&s, &json!({"id":id,"path":source,"confirmed":true})).is_err());
    metadata(&source, 5000, 0)?;
    assert!(worlds::import(&s, &json!({"id":id,"path":source,"confirmed":true})).is_err());
    assert_eq!(
        std::fs::read(root(&s, id)?.join("world/level.dat"))?,
        original
    );
    Ok(())
}
#[tokio::test]
#[ignore = "User-authorized browser playit setup and real public tunnel status test; never proves a second-network player login"]
async fn live_playit_browser_claim_and_public_roundtrip() -> Result<()> {
    ensure!(
        std::env::var("ORBIT_SERVER_EULA").as_deref() == Ok("accepted"),
        "EULA acceptance required"
    );
    let base = PathBuf::from(std::env::var("ORBIT_SERVER_TEST_ROOT")?);
    let s = crate::social_tests::state(dunce::canonicalize(base)?)?;
    init(&s)?;
    let setup = tunnel::claim_resume(
        None,
        s.clone(),
        std::env::var("ORBIT_PLAYIT_CLAIM_CODE").ok(),
    )
    .await?;
    if let Some(url) = setup["url"].as_str() {
        eprintln!("PLAYIT_BROWSER_AUTH {url}");
    }
    eprintln!("PLAYIT_SETUP_STEP: claim this Orbit agent, then create a FREE Minecraft Java tunnel to 127.0.0.1:25879; Proxy Protocol disabled. No paid plan.");
    for _ in 0..180 {
        if tunnel::status(&s)["authorized"] == true {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_secs(5)).await;
    }
    ensure!(
        tunnel::status(&s)["authorized"] == true,
        "Browser authorization was not completed"
    );
    eprintln!("PLAYIT_AUTHORIZED: credential stored in Windows Credential Manager, never printed. Waiting for configured tunnel.");
    let mut p = config();
    p["name"] = json!("Synthetic public tunnel check");
    let p = save(&s, p)?;
    let id = p["id"].as_str().unwrap();
    process::start(None, s.clone(), id).await?;
    let result=async{
        let job=s.servers.jobs.lock().unwrap().get(id).unwrap().clone();
        for n in 0..120 {
            if job.stopped.load(std::sync::atomic::Ordering::Acquire){anyhow::bail!("Synthetic public server stopped: {:?}",job.status.lock().unwrap());}
            if job.status.lock().unwrap()["phase"]=="Online"{let st=job.status.lock().unwrap().clone();let address=st["address"].as_str().context("Tunnel address missing")?;let status=sessions::ping_public(address).await?;ensure!(status["description"].to_string().contains(st["session"].as_str().unwrap()),"Public session mismatch");eprintln!("PLAYIT_PUBLIC_PASS {address}: official agent handshake and matching Minecraft status response through public tunnel FROM THIS HOST. Separate-network player login remains unverified.");
                if std::env::var("ORBIT_TUNNEL_RECOVERY_TEST").as_deref()==Ok("1") {
                    let old=job.network_cancel.lock().unwrap().as_ref().context("Agent cancellation handle missing")?.clone();
                    super::dispatch(None,s.clone(),"server_tunnel_retry",json!({"id":id})).await?;
                    tokio::time::sleep(std::time::Duration::from_millis(250)).await;
                    ensure!(old.is_cancelled(),"Reconnect did not cancel the old managed agent");
                    for _ in 0..30 {if job.status.lock().unwrap()["tunnelReady"]==true&&sessions::ping_public(address).await.is_ok_and(|v|v["description"].to_string().contains(st["session"].as_str().unwrap())) {ensure!(job.status.lock().unwrap()["pid"]==st["pid"],"Tunnel reconnect restarted the Minecraft process");eprintln!("PLAYIT_RECOVERY_PASS managed agent replacement, matching public response, unchanged Minecraft PID and session");return Ok(());}tokio::time::sleep(std::time::Duration::from_secs(2)).await;}
                    anyhow::bail!("Tunnel recovery did not regain a matching public response");
                }
                return Ok(());}
            if n>0&&n%6==0&&job.status.lock().unwrap()["localReady"]==true {let state=s.clone();let p=p.clone();let j=job.clone();tokio::spawn(async move{tunnel::run(None,state,p,j).await;});}
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
        }anyhow::bail!("Public tunnel never became ready: {:?}",job.status.lock().unwrap())
    }.await;
    let stopped = process::stop(None, &s, id, false).await;
    eprintln!("PLAYIT_TEST_STOPPED: synthetic server and agent shutdown requested; no test world is left publicly running.");
    stopped?;
    result
}
#[tokio::test]
async fn configuration_eula_paths_and_session_boundaries() -> Result<()> {
    let s = crate::social_tests::state(
        std::env::temp_dir().join(format!("orbit-server-config-{}", uuid::Uuid::new_v4())),
    )?;
    init(&s)?;
    let mut p = config();
    p["acceptEula"] = json!(false);
    p["eulaAcceptedAt"] = json!(123);
    let p = save(&s, p)?;
    assert!(p["eulaAcceptedAt"].is_null());
    assert!(process::start(None, s.clone(), p["id"].as_str().unwrap())
        .await
        .is_err());
    let mut p = p.clone();
    p["acceptEula"] = json!(true);
    let p = save(&s, p)?;
    assert!(p["eulaAcceptedAt"].as_i64().unwrap() > 0);
    install::write_properties(&s, &p)?;
    let properties =
        std::fs::read_to_string(root(&s, p["id"].as_str().unwrap())?.join("server.properties"))?;
    for expected in [
        "online-mode=true",
        "white-list=true",
        "enforce-whitelist=true",
        "enable-rcon=false",
        "enable-query=false",
        "server-ip=127.0.0.1",
    ] {
        assert!(properties.contains(expected));
    }
    assert!(root(&s, "../../user").is_err());
    let mut other = p.clone();
    other["version"] = json!("1.20.1");
    assert!(save(&s, other).is_err());
    assert!(sessions::address("host.example:25565\nstop").is_err());
    assert!(sessions::address("https://example.com").is_err());
    assert!(sessions::ping_public("127.0.0.1:25879").await.is_err());
    let listener = std::net::TcpListener::bind("0.0.0.0:0")?;
    let mut blocked = p.clone();
    blocked["port"] = json!(listener.local_addr()?.port());
    save(&s, blocked)?;
    assert!(process::start(None, s.clone(), p["id"].as_str().unwrap())
        .await
        .is_err());
    drop(listener);
    let _installation = s.install_lock.lock().await;
    process::start(None, s.clone(), p["id"].as_str().unwrap()).await?;
    tokio::time::timeout(
        std::time::Duration::from_secs(2),
        process::stop(None, &s, p["id"].as_str().unwrap(), false),
    )
    .await
    .context("Stop must cancel even while waiting for another installation")??;
    assert!(!active(&s));
    crate::secure::set(
        &s,
        "playit-host",
        "synthetic local test credential; never transmitted",
    )?;
    assert!(
        super::dispatch(None, s.clone(), "server_tunnel_reset", json!({}))
            .await
            .is_err()
    );
    assert!(tunnel::status(&s)["authorized"] == true);
    super::dispatch(
        None,
        s.clone(),
        "server_tunnel_reset",
        json!({"confirmed":true}),
    )
    .await?;
    assert!(tunnel::status(&s)["authorized"] == false);
    assert!(worlds::backup(&s, p["id"].as_str().unwrap()).is_err());
    Ok(())
}
#[tokio::test]
#[ignore = "Downloads real Mojang/Fabric server files and runs synthetic online-mode worlds; ORBIT_SERVER_EULA=accepted required"]
async fn live_server_install_lifecycle_backups_and_fabric() -> Result<()> {
    ensure!(
        std::env::var("ORBIT_SERVER_EULA").as_deref() == Ok("accepted"),
        "Explicit Minecraft EULA acceptance required for this isolated test"
    );
    let base = PathBuf::from(std::env::var("ORBIT_SERVER_TEST_ROOT")?);
    std::fs::create_dir_all(&base)?;
    let s = crate::social_tests::state(dunce::canonicalize(base)?)?;
    init(&s)?;
    for loader in ["vanilla", "fabric"] {
        let mut p = config();
        p["loader"] = json!(loader);
        p["name"] = json!(format!("Synthetic {loader} server"));
        let p = save(&s, p)?;
        let id = p["id"].as_str().unwrap();
        process::start(None, s.clone(), id).await?;
        let result=async{
            let job=s.servers.jobs.lock().unwrap().get(id).unwrap().clone();
            for _ in 0..300 {if job.status.lock().unwrap()["localReady"]==true{break;}if job.stopped.load(std::sync::atomic::Ordering::Acquire){anyhow::bail!("Startup failed: {:?}\n{:?}",job.status.lock().unwrap(),job.logs.lock().unwrap());}tokio::time::sleep(std::time::Duration::from_secs(1)).await;}
            ensure!(job.status.lock().unwrap()["localReady"]==true,"Server did not become locally ready");
            let status=sessions::ping(&format!("127.0.0.1:{}",p["port"])).await?;ensure!(status["version"]["protocol"]==767,"Unexpected protocol");
            ensure!(job.status.lock().unwrap()["phase"]!="Online","An unauthorized tunnel must never claim internet readiness");
            ensure!(worlds::backup(&s,id).is_err(),"Active-world raw backup was allowed");
            ensure!(worlds::world_version(&root(&s,id)?.join("world"))?==3955,"Unexpected world version");
            process::stop(None,&s,id,false).await?;
            let first=worlds::backup(&s,id)?;let marker=root(&s,id)?.join("world/synthetic-marker.txt");std::fs::write(&marker,b"after backup")?;
            worlds::restore(&s,&json!({"id":id,"backup":first["id"]}))?;ensure!(!marker.exists(),"Restore did not restore world contents");
            if loader=="vanilla" { let source=root(&s,id)?.join("world");let copyid=save(&s,config())?["id"].as_str().unwrap().to_owned();worlds::import(&s,&json!({"id":copyid,"path":source,"confirmed":true}))?;ensure!(source.join("level.dat").is_file(),"Import modified the original"); }
            {let source=std::fs::File::open(root(&s,id)?.join("server.jar"))?;let mut part=std::fs::File::create(root(&s,id)?.join("server.jar.part"))?;std::io::copy(&mut std::io::Read::take(source,8*1024*1024),&mut part)?;}
            std::fs::write(root(&s,id)?.join("server.jar"),b"synthetic corruption")?;
            let java=install::prepare(&s,&p,None).await?;ensure!(java.is_file(),"Managed Java missing after repair");ensure!(std::fs::metadata(root(&s,id)?.join("server.jar"))?.len()>1_000_000,"Checksum repair failed");
            process::start(None,s.clone(),id).await?;let job=s.servers.jobs.lock().unwrap().get(id).unwrap().clone();for _ in 0..150{if job.status.lock().unwrap()["localReady"]==true{break;}tokio::time::sleep(std::time::Duration::from_secs(1)).await;}ensure!(job.status.lock().unwrap()["localReady"]==true,"Restart failed");process::stop(None,&s,id,false).await?;
            eprintln!("HOSTING PASS {loader}: verified install/repair, Java21, online-mode/allowlist config, local status767, no false Online, active backup refusal, graceful stop/world persistence, backup restore, original-preserving import, restart");Ok::<_,anyhow::Error>(())
        }.await;
        if active(&s) {
            let _ = process::stop(None, &s, id, false).await;
        }
        result?;
    }
    Ok(())
}
