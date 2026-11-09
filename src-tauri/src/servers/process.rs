use super::*;
use std::{process::Stdio, sync::atomic::Ordering, time::Duration};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
pub fn memory() -> Value {
    #[cfg(windows)]
    unsafe {
        use windows_sys::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
        let mut m: MEMORYSTATUSEX = std::mem::zeroed();
        m.dwLength = std::mem::size_of::<MEMORYSTATUSEX>() as u32;
        if GlobalMemoryStatusEx(&mut m) != 0 {
            let available = m.ullAvailPhys / 1024 / 1024;
            return json!({"totalMiB":m.ullTotalPhys/1024/1024,"availableMiB":available,"recommendedServerMiB":(available/4).clamp(1024,4096),"note":"Reserve memory for Windows and a separate Minecraft client. Capacity depends on CPU, upload bandwidth, world and mods."});
        }
    }
    json!({"recommendedServerMiB":2048,"note":"Keep at least 4 GiB available for Windows and the client; actual requirements vary."})
}
pub fn rss(pid: u32) -> Option<u64> {
    #[cfg(windows)]
    unsafe {
        use windows_sys::Win32::System::{
            ProcessStatus::{K32GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS},
            Threading::{OpenProcess, PROCESS_QUERY_INFORMATION, PROCESS_VM_READ},
        };
        let h = OpenProcess(PROCESS_QUERY_INFORMATION | PROCESS_VM_READ, 0, pid);
        if h.is_null() {
            return None;
        }
        let mut m: PROCESS_MEMORY_COUNTERS = std::mem::zeroed();
        m.cb = std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32;
        let ok = K32GetProcessMemoryInfo(h, &mut m, m.cb);
        windows_sys::Win32::Foundation::CloseHandle(h);
        if ok != 0 {
            return Some(m.WorkingSetSize as u64 / 1024 / 1024);
        }
    }
    None
}
pub async fn start(app: Option<tauri::AppHandle>, s: Shared, id: &str) -> Result<Value> {
    let _guard = s.servers.lifecycle.lock().await;
    let p = load(&s, id)?;
    validate(&p)?;
    ensure!(
        !active(&s),
        "This release hosts one managed server at a time. Stop the active server first."
    );
    ensure!(
        p["eulaAcceptedAt"].as_i64().is_some_and(|n| n > 0),
        "Review and accept the Minecraft EULA before hosting"
    );
    let available = memory()["availableMiB"].as_u64().unwrap_or(u64::MAX);
    ensure!(p["memory"].as_u64().unwrap()+1536<available,"Not enough available RAM. Close other applications or reduce server memory; reserve memory for the client too.");
    let port = p["port"].as_u64().unwrap() as u16;
    let check = std::net::TcpListener::bind(("0.0.0.0", port)).context(
        "Port is already in use or blocked. Stop the other server or change the local port.",
    )?;
    drop(check);
    let world_lock = worlds::host_lock(&s, id)?;
    let (tx, rx) = mpsc::channel(8);
    let session = uuid::Uuid::new_v4().to_string();
    let job = Arc::new(Job {
        status: Mutex::new(
            json!({"phase":"Starting","session":session,"started":db::now(),"memoryMiB":p["memory"],"localReady":false,"tunnelReady":false,"externalVerified":false,"players":null}),
        ),
        logs: Mutex::new(VecDeque::new()),
        commands: tx,
        force: CancellationToken::new(),
        cancel: CancellationToken::new(),
        finished: Notify::new(),
        network_cancel: Mutex::new(None),
        stopped: std::sync::atomic::AtomicBool::new(false),
    });
    s.servers
        .jobs
        .lock()
        .unwrap()
        .insert(id.into(), job.clone());
    signal(app.as_ref());
    let state = s.clone();
    let j = job.clone();
    tokio::spawn(async move {
        let world_lock = world_lock;
        let result = run(app.clone(), state.clone(), p, j.clone(), rx).await;
        j.cancel.cancel();
        {
            let mut st = j.status.lock().unwrap();
            st["localReady"] = json!(false);
            st["tunnelReady"] = json!(false);
            st["players"] = Value::Null;
            st["pid"] = Value::Null;
            st["residentMiB"] = Value::Null;
        }
        drop(world_lock);
        j.stopped.store(true, Ordering::Release);
        if let Err(e) = result {
            log(&j, format!("ERROR: {e:#}"));
            super::state(app.as_ref(), &j, "Error", Some(format!("{e:#}")));
        } else {
            super::state(app.as_ref(), &j, "Offline", None);
        }
        sessions::announce(state.clone(), &j, false).await.ok();
        j.finished.notify_waiters();
        signal(app.as_ref());
    });
    let snapshot = job.status.lock().unwrap().clone();
    Ok(snapshot)
}
async fn run(
    app: Option<tauri::AppHandle>,
    s: Shared,
    p: Value,
    job: Arc<Job>,
    mut commands: mpsc::Receiver<String>,
) -> Result<()> {
    let java = {
        let _lock = tokio::select! {_=job.cancel.cancelled()=>return Ok(()),guard=s.install_lock.lock()=>guard};
        tokio::select! {_ = job.cancel.cancelled()=>return Ok(()),result=install::prepare(&s,&p,app.as_ref())=>result?}
    };
    if job.cancel.is_cancelled() {
        return Ok(());
    }
    let id = p["id"].as_str().unwrap();
    let folder = root(&s, id)?;
    let marker = job.status.lock().unwrap()["session"]
        .as_str()
        .unwrap()
        .to_owned();
    let mut properties = std::fs::read_to_string(folder.join("server.properties"))?;
    properties.push_str(&format!("motd=Orbit {marker}\n"));
    std::fs::write(folder.join("server.properties"), properties)?;
    let mut cmd = tokio::process::Command::new(java);
    cmd.args([
        format!("-Xmx{}M", p["memory"]),
        "-Xms512M".into(),
        "-Djava.awt.headless=true".into(),
    ]);
    if p["loader"] == "fabric" {
        let plan: Value = serde_json::from_slice(&std::fs::read(folder.join("fabric-plan.json"))?)?;
        let cp = plan["classpath"]
            .as_array()
            .context("Fabric classpath missing")?
            .iter()
            .map(|p| p.as_str().unwrap_or(""))
            .collect::<Vec<_>>()
            .join(";");
        cmd.arg("-cp").arg(cp).arg(
            plan["mainClass"]
                .as_str()
                .context("Fabric entry point missing")?,
        );
    } else {
        cmd.arg("-jar").arg("server.jar");
    }
    cmd.arg("nogui")
        .current_dir(&folder)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    #[cfg(windows)]
    cmd.creation_flags(0x08000000);
    let mut child = cmd
        .spawn()
        .context("Managed Java could not start. Repair the runtime and retry.")?;
    let pid = child.id().context("Server process ID missing")?;
    {
        let mut st = job.status.lock().unwrap();
        st["pid"] = json!(pid);
        st["serverId"] = p["id"].clone();
    }
    let mut input = child.stdin.take().unwrap();
    let streams: [Box<dyn tokio::io::AsyncRead + Send + Unpin>; 2] = [
        Box::new(child.stdout.take().unwrap()),
        Box::new(child.stderr.take().unwrap()),
    ];
    let ready = Arc::new(std::sync::atomic::AtomicBool::new(false));
    for stream in streams {
        let j = job.clone();
        let ready = ready.clone();
        tokio::spawn(async move {
            let mut lines = BufReader::new(stream).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                if line.contains("Done (") && line.contains("For help") {
                    ready.store(true, Ordering::Release);
                }
                log(&j, line);
            }
        });
    }
    let port = p["port"].as_u64().unwrap() as u16;
    let mut ticks = tokio::time::interval(Duration::from_secs(5));
    let mut stopping = false;
    let mut tunnel_started = false;
    let mut count = 0;
    loop {
        tokio::select! {
            exit=child.wait()=>{let status=exit?;ensure!(status.success()||stopping,"Minecraft server exited ({status}). Check logs; common causes are authentication service failure, insufficient memory, port conflicts or incompatible files.");break;},
            _=job.force.cancelled()=>{child.kill().await?;child.wait().await.ok();anyhow::bail!("Server force-terminated. Check the latest backup before continuing.");},
            command=commands.recv()=>{if let Some(command)=command{if command=="stop"{stopping=true;}input.write_all(format!("{command}\n").as_bytes()).await?;input.flush().await?;}},
            _=ticks.tick()=>{
                count+=1;{let mut st=job.status.lock().unwrap();st["residentMiB"]=json!(rss(pid));st["uptimeSeconds"]=json!(db::now()-st["started"].as_i64().unwrap_or(db::now()));}
                if ready.load(Ordering::Acquire)&&!stopping {
                    if let Ok(status)=sessions::ping(&format!("127.0.0.1:{port}")).await {
                        let mut st=job.status.lock().unwrap();st["localReady"]=json!(true);st["players"]=status["players"].clone();
                    }
                    if !tunnel_started {tunnel_started=true;let state=s.clone();let j=job.clone();let app=app.clone();let p=p.clone();tokio::spawn(async move{tunnel::run(app,state,p,j).await;});}
                    let address=job.status.lock().unwrap()["address"].as_str().unwrap_or("").to_owned();
                    if count%3==0&&!address.is_empty(){let reached=sessions::ping_public(&address).await.is_ok_and(|v|v["description"].to_string().contains(&marker));let mut st=job.status.lock().unwrap();st["addressCheckedFromHost"]=json!(reached);if p["access"]=="manual" {st["tunnelReady"]=json!(reached);}}
                    let online={let st=job.status.lock().unwrap();st["localReady"]==true&&st["tunnelReady"]==true};super::state(app.as_ref(),&job,if online{"Online"}else{"Preparing online access"},None);
                    if count%12==0 {sessions::announce(s.clone(),&job,online).await.ok();}
                }else if !ready.load(Ordering::Acquire)&&db::now()-job.status.lock().unwrap()["started"].as_i64().unwrap_or(db::now())>300 {log(&job,"Startup is taking unusually long. Inspect logs or Stop; generation speed depends on the host.".into());}
                signal(app.as_ref());
            }
        }
    }
    Ok(())
}
pub async fn stop(
    app: Option<&tauri::AppHandle>,
    s: &AppState,
    id: &str,
    force: bool,
) -> Result<()> {
    let job = s
        .servers
        .jobs
        .lock()
        .unwrap()
        .get(id)
        .cloned()
        .context("No managed server is active")?;
    if job.stopped.load(Ordering::Acquire) {
        return Ok(());
    }
    let done = job.finished.notified();
    state(app, &job, "Stopping", None);
    job.cancel.cancel();
    if force {
        job.force.cancel();
    } else {
        let _ = job.commands.send("stop".into()).await;
    }
    if !job.stopped.load(Ordering::Acquire) {
        tokio::time::timeout(Duration::from_secs(90),done).await.context("Server has not stopped yet. Inspect logs; Force stop is available as explicit recovery.")?;
    }
    Ok(())
}
pub async fn stop_all(app: Option<&tauri::AppHandle>, s: &AppState) -> Result<()> {
    let ids = s
        .servers
        .jobs
        .lock()
        .unwrap()
        .keys()
        .cloned()
        .collect::<Vec<_>>();
    for id in ids {
        stop(app, s, &id, false).await?;
    }
    if let Some(c) = s.servers.setup_cancel.lock().unwrap().take() {
        c.cancel();
    }
    if let Some(c) = s.servers.claim_cancel.lock().unwrap().take() {
        c.cancel();
    }
    Ok(())
}
pub fn allowlist_get(s: &AppState, id: &str) -> Result<Value> {
    let file = root(s, id)?.join("whitelist.json");
    if !file.exists() {
        return Ok(json!([]));
    }
    Ok(serde_json::from_slice(&std::fs::read(file)?)?)
}
pub async fn allowlist(s: &AppState, v: &Value) -> Result<Value> {
    let _life = s.servers.lifecycle.lock().await;
    let _access = s.servers.access_lock.lock().await;
    let id = v["id"].as_str().context("Choose a server")?;
    let p = load(s, id)?;
    let name = v["name"]
        .as_str()
        .context("Enter a Minecraft account name")?;
    ensure!(
        (3..=16).contains(&name.len())
            && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_'),
        "Use the Minecraft account's 3–16 character name"
    );
    ensure!(v["approved"]==true,"Host approval is required. An Orbit friendship or claimed username is not proof of account ownership.");
    let account = if offline(&p) {
        crate::minecraft::offline_session(name)
    } else {
        s.net
            .json(&format!(
                "https://api.mojang.com/users/profiles/minecraft/{name}"
            ))
            .await
            .context("Minecraft account could not be resolved. Check the exact name and retry.")?
    };
    let uuid = uuid::Uuid::parse_str(account["id"].as_str().context("Minecraft UUID missing")?)?;
    let canonical = account["name"].as_str().context("Minecraft name missing")?;
    ensure!(
        canonical.eq_ignore_ascii_case(name)
            && canonical
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_'),
        "Minecraft returned an unexpected account name"
    );
    let item = json!({"uuid":uuid.to_string(),"name":canonical});
    let job = s
        .servers
        .jobs
        .lock()
        .unwrap()
        .get(id)
        .cloned()
        .filter(|j| !j.stopped.load(Ordering::Acquire));
    if offline(&p) {
        // Resolve locally, never via Mojang's name cache: even a premium-looking
        // name has the deterministic OfflinePlayer UUID on an offline server.
        let _world_lock = if job.is_none() {
            Some(worlds::host_lock(s, id)?)
        } else {
            None
        };
        let mut entries = allowlist_get(s, id)?
            .as_array()
            .context("Invalid allowlist")?
            .clone();
        entries.retain(|x| x["uuid"] != item["uuid"]);
        if v["remove"] != true {
            entries.push(item.clone());
        }
        std::fs::create_dir_all(root(s, id)?)?;
        std::fs::write(
            root(s, id)?.join("whitelist.json"),
            serde_json::to_vec_pretty(&entries)?,
        )?;
        if let Some(job) = job {
            job.commands.send("whitelist reload".into()).await?;
            if v["remove"] == true {
                job.commands
                    .send(format!("kick {canonical} Access removed by host"))
                    .await?;
            }
        }
    } else if let Some(job) = job {
        job.commands
            .send(format!(
                "whitelist {} {canonical}",
                if v["remove"] == true { "remove" } else { "add" }
            ))
            .await?;
        if v["remove"] == true {
            job.commands
                .send(format!("kick {canonical} Access removed by host"))
                .await?;
        }
    } else {
        let _world_lock = worlds::host_lock(s, id)?;
        let mut list = allowlist_get(s, id)?
            .as_array()
            .context("Invalid server allowlist")?
            .clone();
        list.retain(|x| x["uuid"] != item["uuid"]);
        if v["remove"] != true {
            list.push(item.clone());
        }
        std::fs::create_dir_all(root(s, id)?)?;
        std::fs::write(
            root(s, id)?.join("whitelist.json"),
            serde_json::to_vec_pretty(&list)?,
        )?;
    }
    Ok(
        json!({"account":item,"authMode":auth_mode(&p),"note":if offline(&p){"Exact-case offline nickname approved. This is not identity verification: another client can claim the same name. No operator permission is granted."}else{"Minecraft online-mode verifies the connecting account. Removal kicks that player; an Orbit friendship alone does not grant access."}}),
    )
}
