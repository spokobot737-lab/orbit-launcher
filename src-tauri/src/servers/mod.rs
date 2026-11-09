//! Local dedicated server hosting. Gameplay never passes through Nostr.
mod install;
mod process;
pub(crate) mod sessions;
#[cfg(test)]
mod tests;
mod tunnel;
mod worlds;
use crate::{db, AppState, Shared};
use anyhow::{ensure, Context, Result};
use serde_json::{json, Value};
use std::{
    collections::{HashMap, VecDeque},
    path::PathBuf,
    sync::{Arc, Mutex},
};
use tauri::Emitter;
use tokio::sync::{mpsc, Notify};
use tokio_util::sync::CancellationToken;
pub const VERSION: &str = "1.21.1";
pub const FABRIC: &str = "0.16.14";
#[derive(Default)]
pub struct Runtime {
    pub jobs: Mutex<HashMap<String, Arc<Job>>>,
    pub lifecycle: tokio::sync::Mutex<()>,
    pub claim: Mutex<Value>,
    pub claim_cancel: Mutex<Option<CancellationToken>>,
    pub setup_cancel: Mutex<Option<CancellationToken>>,
    pub authorization_lock: tokio::sync::Mutex<()>,
    pub provision_lock: tokio::sync::Mutex<()>,
    pub access_lock: tokio::sync::Mutex<()>,
}
pub struct Job {
    pub status: Mutex<Value>,
    pub logs: Mutex<VecDeque<String>>,
    pub commands: mpsc::Sender<String>,
    pub force: CancellationToken,
    pub cancel: CancellationToken,
    pub finished: Notify,
    pub network_cancel: Mutex<Option<CancellationToken>>,
    pub stopped: std::sync::atomic::AtomicBool,
}
pub fn init(s: &AppState) -> Result<()> {
    s.db.lock().unwrap().execute_batch("CREATE TABLE IF NOT EXISTS hosted_servers(id TEXT PRIMARY KEY,data TEXT NOT NULL); CREATE TABLE IF NOT EXISTS server_sessions(peer TEXT NOT NULL,server TEXT NOT NULL,data TEXT NOT NULL,expires INTEGER NOT NULL,PRIMARY KEY(peer,server)); CREATE TABLE IF NOT EXISTS server_shares(server TEXT NOT NULL,peer TEXT NOT NULL,PRIMARY KEY(server,peer));")?;
    Ok(())
}
pub fn active(s: &AppState) -> bool {
    s.servers
        .jobs
        .lock()
        .unwrap()
        .values()
        .any(|j| !j.stopped.load(std::sync::atomic::Ordering::Acquire))
}
pub fn client_memory(s: &AppState, client_mib: u64) -> Result<()> {
    if !active(s) {
        return Ok(());
    }
    let memory = process::memory();
    let server_mib: u64 = s
        .servers
        .jobs
        .lock()
        .unwrap()
        .values()
        .filter(|j| !j.stopped.load(std::sync::atomic::Ordering::Acquire))
        .map(|j| j.status.lock().unwrap()["memoryMiB"].as_u64().unwrap_or(0))
        .sum();
    if let Some(total) = memory["totalMiB"].as_u64() {
        ensure!(client_mib+server_mib+2048<=total,"Combined client/server heap limits leave too little RAM for Windows. Reduce client or server memory before launching.");
    }
    if let Some(available) = memory["availableMiB"].as_u64() {
        ensure!(available>client_mib+1024,"Not enough available RAM for the client alongside the server. Close other applications or reduce memory settings.");
    }
    Ok(())
}
pub fn root(s: &AppState, id: &str) -> Result<PathBuf> {
    uuid::Uuid::parse_str(id)?;
    Ok(s.root.join("minecraft/servers").join(id))
}
pub fn load(s: &AppState, id: &str) -> Result<Value> {
    init(s)?;
    uuid::Uuid::parse_str(id)?;
    let raw: String =
        s.db.lock()
            .unwrap()
            .query_row("SELECT data FROM hosted_servers WHERE id=?", [id], |r| {
                r.get(0)
            })
            .context("Server not found")?;
    Ok(serde_json::from_str(&raw)?)
}
pub fn signal(app: Option<&tauri::AppHandle>) {
    if let Some(a) = app {
        let _ = a.emit("servers-changed", json!({}));
    }
}
pub fn state(app: Option<&tauri::AppHandle>, job: &Job, phase: &str, error: Option<String>) {
    let mut v = job.status.lock().unwrap();
    v["phase"] = json!(phase);
    v["error"] = json!(error);
    drop(v);
    signal(app);
}
pub fn log(job: &Job, line: String) {
    let mut lines = job.logs.lock().unwrap();
    if lines.len() >= 400 {
        lines.pop_front();
    }
    lines.push_back(line.chars().take(1500).collect());
}
pub fn validate(p: &Value) -> Result<()> {
    ensure!(
        p["authMode"].is_null() || matches!(p["authMode"].as_str(), Some("online" | "offline")),
        "Choose premium or nonpremium authentication"
    );
    uuid::Uuid::parse_str(p["id"].as_str().context("Missing server ID")?)?;
    ensure!(
        p["name"].as_str().is_some_and(|x| !x.trim().is_empty()
            && x.len() <= 80
            && !x.chars().any(char::is_control)),
        "Enter a server name (up to 80 characters)"
    );
    ensure!(p["version"]==VERSION,"This release supports Minecraft Java 1.21.1 servers only; version changes and downgrades are disabled");
    ensure!(
        matches!(p["loader"].as_str(), Some("vanilla" | "fabric")),
        "Choose Vanilla or bare Fabric"
    );
    for (k, min, max) in [
        ("memory", 1024, 16384),
        ("players", 1, 20),
        ("viewDistance", 2, 16),
        ("simulationDistance", 2, 12),
        ("port", 1024, 65535),
    ] {
        ensure!(
            p[k].as_u64().is_some_and(|n| (min..=max).contains(&n)),
            "Invalid {k}: use {min}–{max}"
        );
    }
    ensure!(
        matches!(
            p["difficulty"].as_str(),
            Some("peaceful" | "easy" | "normal" | "hard")
        ),
        "Invalid difficulty"
    );
    ensure!(
        matches!(
            p["gameMode"].as_str(),
            Some("survival" | "creative" | "adventure" | "spectator")
        ),
        "Invalid game mode"
    );
    ensure!(
        p["seed"].as_str().is_some_and(|x| x.len() <= 64
            && x.bytes()
                .all(|b| b.is_ascii_alphanumeric() || b" -_".contains(&b))),
        "Seed may contain letters, digits, spaces, hyphens and underscores"
    );
    ensure!(
        matches!(p["access"].as_str(), Some("playit" | "manual")),
        "Choose playit or Advanced manual access"
    );
    if p["access"] == "manual" && !p["address"].as_str().unwrap_or("").is_empty() {
        sessions::address(p["address"].as_str().unwrap())?;
    }
    Ok(())
}
/// Missing values belong to pre-0.4.1 authenticated servers and invitations.
pub fn offline(p: &Value) -> bool {
    p["authMode"] == "offline"
}
pub fn auth_mode(p: &Value) -> &'static str {
    if offline(p) {
        "offline"
    } else {
        "online"
    }
}
pub fn save(s: &AppState, mut p: Value) -> Result<Value> {
    init(s)?;
    if p["id"].as_str().unwrap_or("").is_empty() {
        p["id"] = json!(uuid::Uuid::new_v4().to_string());
    }
    validate(&p)?;
    let id = p["id"].as_str().unwrap().to_owned();
    ensure!(
        !s.servers
            .jobs
            .lock()
            .unwrap()
            .get(&id)
            .is_some_and(|j| !j.stopped.load(std::sync::atomic::Ordering::Acquire)),
        "Stop this server before changing settings"
    );
    let _world_lock = worlds::host_lock(s, &id)?;
    if let Ok(old) = load(s, &id) {
        ensure!(old["version"]==p["version"]&&old["loader"]==p["loader"],"Changing version or loader is not supported. Create a separate server; never downgrade a world.");
        p["eulaAcceptedAt"] = old["eulaAcceptedAt"].clone();
        if offline(&old) != offline(&p) {
            ensure!(p["confirmAuthChange"] == true, "Confirm authentication change: player UUIDs, inventories and permissions differ. Mode-specific allowlists are preserved; no player data is converted.");
            let folder = root(s, &id)?;
            let current = folder.join("whitelist.json");
            if current.exists() {
                std::fs::copy(
                    &current,
                    folder.join(format!("whitelist-{}.json", auth_mode(&old))),
                )?;
            }
            let restore = folder.join(format!("whitelist-{}.json", auth_mode(&p)));
            let bytes = if restore.exists() {
                std::fs::read(restore)?
            } else {
                b"[]".to_vec()
            };
            std::fs::write(current, bytes)?;
        }
    } else {
        p["eulaAcceptedAt"] = Value::Null;
    }
    if p["acceptEula"] == true {
        p["eulaAcceptedAt"] = json!(db::now());
    }
    p.as_object_mut().unwrap().remove("acceptEula");
    p.as_object_mut().unwrap().remove("confirmAuthChange");
    p["authMode"] = json!(auth_mode(&p));
    s.db.lock().unwrap().execute(
        "INSERT OR REPLACE INTO hosted_servers VALUES(?,?)",
        rusqlite::params![id, p.to_string()],
    )?;
    Ok(p)
}
pub fn list(s: &AppState) -> Result<Value> {
    init(s)?;
    let raw: Vec<String> = {
        let c = s.db.lock().unwrap();
        let mut q = c.prepare("SELECT data FROM hosted_servers ORDER BY rowid")?;
        let r = q.query_map([], |r| r.get(0))?.collect::<Result<_, _>>()?;
        r
    };
    let jobs = s.servers.jobs.lock().unwrap();
    let mut out = vec![];
    for raw in raw {
        let mut p: Value = serde_json::from_str(&raw)?;
        p["runtime"] = jobs
            .get(p["id"].as_str().unwrap())
            .map(|j| j.status.lock().unwrap().clone())
            .unwrap_or(json!({"phase":"Offline"}));
        out.push(p);
    }
    Ok(json!(out))
}
pub async fn dispatch(
    app: Option<tauri::AppHandle>,
    s: Shared,
    op: &str,
    v: Value,
) -> Result<Value> {
    init(&s)?;
    let id = v["id"].as_str().unwrap_or("");
    match op {
        "server_license" => Ok(json!(include_str!(
            "../../../docs/third-party/PLAYIT-LICENSE.txt"
        ))),
        "server_list" => list(&s),
        "server_save" => {
            let _life = s.servers.lifecycle.lock().await;
            let _access = s.servers.access_lock.lock().await;
            save(&s, v)
        }
        "server_start" => process::start(app, s, id).await,
        "server_stop" => {
            process::stop(app.as_ref(), &s, id, false).await?;
            Ok(json!(true))
        }
        "server_force" => {
            ensure!(
                v["confirmed"] == true,
                "Confirm force termination; unsaved progress may be lost"
            );
            process::stop(app.as_ref(), &s, id, true).await?;
            Ok(json!(true))
        }
        "server_restart" => {
            process::stop(app.as_ref(), &s, id, false).await?;
            process::start(app, s, id).await
        }
        "server_logs" => {
            let j = s
                .servers
                .jobs
                .lock()
                .unwrap()
                .get(id)
                .cloned()
                .context("No logs in this session")?;
            let lines = j.logs.lock().unwrap().iter().cloned().collect::<Vec<_>>();
            Ok(json!(lines))
        }
        "server_backups" => worlds::backups(&s, id),
        "server_backup" => {
            let _guard = s.servers.lifecycle.lock().await;
            let state = s.clone();
            let id = id.to_owned();
            tokio::task::spawn_blocking(move || worlds::backup(&state, &id)).await?
        }
        "server_restore" => {
            let _guard = s.servers.lifecycle.lock().await;
            ensure!(v["confirmed"] == true, "Confirm restoring this backup");
            let state = s.clone();
            tokio::task::spawn_blocking(move || worlds::restore(&state, &v)).await?
        }
        "server_delete_backup" => {
            let _guard = s.servers.lifecycle.lock().await;
            ensure!(v["confirmed"] == true, "Confirm deleting this backup");
            worlds::delete_backup(&s, &v)
        }
        "server_import" => {
            let _guard = s.servers.lifecycle.lock().await;
            let state = s.clone();
            tokio::task::spawn_blocking(move || worlds::import(&state, &v)).await?
        }
        "server_allowlist" => process::allowlist(&s, &v).await,
        "server_allowlist_get" => process::allowlist_get(&s, id),
        "server_share" => sessions::share(app, s, &v).await,
        "server_shared" => sessions::list(&s),
        "server_join_options" => sessions::options(&s, &v).await,
        "server_join" => {
            sessions::join(app.as_ref().context("Native launcher required")?, &s, &v).await
        }
        "server_create_client" => sessions::create_client(&s, &v),
        "server_tunnel_setup" => tunnel::claim(app, s).await,
        "server_tunnel_configure" => tunnel::configure(app, s, id).await,
        "server_tunnel_status" => Ok(tunnel::status(&s)),
        "server_tunnel_reset" => {
            ensure!(
                v["confirmed"] == true,
                "Confirm disconnecting this host authorization"
            );
            ensure!(
                !active(&s),
                "Stop the hosted server before disconnecting playit"
            );
            let _auth = s.servers.authorization_lock.lock().await;
            if let Some(c) = s.servers.claim_cancel.lock().unwrap().take() {
                c.cancel();
            }
            if let Some(c) = s.servers.setup_cancel.lock().unwrap().take() {
                c.cancel();
            }
            crate::secure::remove(&s, "playit-host")?;
            crate::secure::remove(&s, "playit-claim")?;
            *s.servers.claim.lock().unwrap() = json!({"phase":"Disconnected","note":"Local host credential removed. Revoke or remove the agent separately in the playit dashboard if needed."});
            signal(app.as_ref());
            Ok(json!(true))
        }
        "server_tunnel_retry" => {
            let p = load(&s, id)?;
            let job = s
                .servers
                .jobs
                .lock()
                .unwrap()
                .get(id)
                .cloned()
                .context("Start the server first")?;
            ensure!(
                !job.stopped.load(std::sync::atomic::Ordering::Acquire),
                "Start the server first"
            );
            tokio::spawn(tunnel::run(app, s, p, job));
            Ok(json!(true))
        }
        "server_tunnel_dashboard" => {
            open::that_detached("https://playit.gg/account/tunnels")?;
            Ok(json!(true))
        }
        "server_eula" => {
            open::that_detached("https://www.minecraft.net/eula")?;
            Ok(json!(true))
        }
        "server_memory" => Ok(process::memory()),
        "server_minimize" => {
            use tauri::Manager;
            if let Some(a) = app.as_ref() {
                if let Some(w) = a.get_webview_window("main") {
                    w.minimize()?;
                }
            }
            Ok(json!(true))
        }
        "server_exit" => {
            process::stop_all(app.as_ref(), &s).await?;
            app.context("Native app required")?.exit(0);
            Ok(json!(true))
        }
        _ => anyhow::bail!("Unsupported server operation"),
    }
}
