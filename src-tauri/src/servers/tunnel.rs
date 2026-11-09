//! Replaceable provider boundary, using the unmodified official playit core v1.2.0.
//! Credentials stay in Windows Credential Manager and Rust memory, never argv/files.
use super::*;
use crate::secure;
use playit_agent_core::{
    network::{
        origin_lookup::{OriginIp, OriginLookup, OriginResource, OriginTarget},
        tcp::tcp_settings::TcpSettings,
        udp::udp_settings::UdpSettings,
    },
    playit_agent::{PlayitAgent, PlayitAgentSettings},
};
use playit_api_client::{api::*, PlayitApi};
use std::time::Duration;
const API: &str = "https://api.playit.gg";
pub fn status(s: &AppState) -> Value {
    let mut v = s.servers.claim.lock().unwrap().clone();
    if !v.is_object() {
        v = json!({});
    }
    v["authorized"] = json!(secure::get(s, "playit-host").is_ok());
    v
}
pub async fn claim(app: Option<tauri::AppHandle>, s: Shared) -> Result<Value> {
    claim_resume(app, s, None).await
}
pub(super) async fn claim_resume(
    app: Option<tauri::AppHandle>,
    s: Shared,
    resume: Option<String>,
) -> Result<Value> {
    if secure::get(&s, "playit-host").is_ok() {
        start_setup(app.clone(), s.clone());
        return Ok(
            json!({"authorized":true,"note":"Host authorized. Create a Minecraft Java tunnel for this Orbit agent in the playit dashboard, then Reconnect online access."}),
        );
    }
    if let Some(old) = s.servers.claim_cancel.lock().unwrap().take() {
        old.cancel();
    }
    let cancel = CancellationToken::new();
    *s.servers.claim_cancel.lock().unwrap() = Some(cancel.clone());
    let code = resume
        .or_else(|| secure::get(&s, "playit-claim").ok())
        .unwrap_or_else(|| uuid::Uuid::new_v4().simple().to_string()[..10].to_owned());
    ensure!(
        code.len() == 10 && code.bytes().all(|b| b.is_ascii_hexdigit()),
        "Invalid pending playit authorization"
    );
    let url = format!("https://playit.gg/claim/{code}");
    let api = PlayitApi::create(API.into(), None);
    let first = tokio::time::timeout(
        Duration::from_secs(15),
        api.claim_setup(ReqClaimSetup {
            code: code.clone(),
            agent_type: ClaimAgentType::SelfManaged,
            version: "Orbit / playit core 1.2.0".into(),
        }),
    )
    .await
    .context("Playit setup timed out; retry")?;
    if matches!(
        first,
        Err(ApiError::Fail(
            ClaimSetupError::CodeExpired | ClaimSetupError::InvalidCode
        ))
    ) {
        secure::remove(&s, "playit-claim")?;
        anyhow::bail!("This playit authorization link expired. Start Set up online access again to get a new link.");
    }
    ensure!(
        first.is_ok(),
        "Playit could not prepare host authorization; retry later"
    );
    secure::set(&s, "playit-claim", &code)?;
    *s.servers.claim.lock().unwrap() =
        json!({"phase":"Waiting for browser authorization","url":url});
    open::that_detached(&url)?;
    let output = status(&s);
    tokio::spawn(async move {
        let result=async{
            for _ in 0..300 {
                tokio::select!{_=cancel.cancelled()=>anyhow::bail!("Authorization cancelled"),_=tokio::time::sleep(Duration::from_secs(2))=>{}}
                let result=tokio::time::timeout(Duration::from_secs(12),api.claim_setup(ReqClaimSetup{code:code.clone(),agent_type:ClaimAgentType::SelfManaged,version:"Orbit / playit core 1.2.0".into()})).await;
                match result {
                    Ok(Ok(ClaimSetupResponse::UserAccepted))=>{let key=tokio::time::timeout(Duration::from_secs(15),api.claim_exchange(ReqClaimExchange{code:code.clone()})).await.context("Playit authorization exchange timed out")?.map_err(|_|anyhow::anyhow!("Playit authorization exchange failed; retry setup"))?;let _auth=s.servers.authorization_lock.lock().await;ensure!(!cancel.is_cancelled(),"Host authorization was cancelled");secure::set(&s,"playit-host",&key.secret_key)?;secure::remove(&s,"playit-claim")?;return Ok(());},
                    Ok(Ok(ClaimSetupResponse::UserRejected))=>anyhow::bail!("Host authorization was rejected"),
                    _=>{}
                }
            }anyhow::bail!("Browser authorization expired. Start setup again.")
        }.await;
        *s.servers.claim.lock().unwrap() = match result {
            Ok(()) => {
                json!({"phase":"Authorized","note":"In playit, create one Minecraft Java tunnel for this Orbit agent to 127.0.0.1 and the server port. Return to Orbit and reconnect online access."})
            }
            Err(e) => json!({"phase":"Error","error":e.to_string()}),
        };
        if secure::get(&s, "playit-host").is_ok() {
            start_setup(app.clone(), s.clone());
        }
        signal(app.as_ref());
    });
    Ok(output)
}
fn start_setup(app: Option<tauri::AppHandle>, s: Shared) {
    if active(&s) {
        // Authorization can finish after Java is already ready. Resume that
        // installation's real job, not an isolated setup-only agent.
        let jobs = s
            .servers
            .jobs
            .lock()
            .unwrap()
            .iter()
            .filter(|(_, j)| {
                j.status.lock().unwrap()["localReady"] == true
                    && !j.stopped.load(std::sync::atomic::Ordering::Acquire)
            })
            .map(|(id, j)| (id.clone(), j.clone()))
            .collect::<Vec<_>>();
        for (id, job) in jobs {
            if let Ok(p) = load(&s, &id) {
                tokio::spawn(run(app.clone(), s.clone(), p, job));
            }
        }
        return;
    }
    let cancel = CancellationToken::new();
    if let Some(old) = s
        .servers
        .setup_cancel
        .lock()
        .unwrap()
        .replace(cancel.clone())
    {
        old.cancel();
    }
    tokio::spawn(async move {
        let result=async{
            let secret=secure::get(&s,"playit-host")?;
            let settings=PlayitAgentSettings{api_url:API.into(),secret_key:secret,tcp_settings:TcpSettings::default(),udp_settings:UdpSettings::default()};
            let agent=tokio::select!{_=cancel.cancelled()=>return Ok(()),v=tokio::time::timeout(Duration::from_secs(30),PlayitAgent::new(settings,Arc::new(OriginLookup::default())))=>v.context("Setup agent handshake timed out")?.map_err(|_|anyhow::anyhow!("Setup agent could not connect; check the network and try again"))?};
            let agent_cancel=agent.cancellation_token();let _guard=CancelOnDrop(agent_cancel.clone());let mut handle=tokio::spawn(agent.run());
            *s.servers.claim.lock().unwrap()=json!({"phase":"Agent connected for setup","note":"Create a free Minecraft Java tunnel in the dashboard. No ports are forwarded until you host a server."});signal(app.as_ref());
            tokio::select!{_=cancel.cancelled()=>{},_=tokio::time::sleep(Duration::from_secs(600))=>{},_=&mut handle=>{anyhow::bail!("Setup agent disconnected; retry Set up online access");}}
            agent_cancel.cancel();if !handle.is_finished()&&tokio::time::timeout(Duration::from_secs(5),&mut handle).await.is_err(){handle.abort();}Ok::<_,anyhow::Error>(())
        }.await;
        if let Err(e) = result {
            *s.servers.claim.lock().unwrap() =
                json!({"phase":"Setup agent error","error":e.to_string()});
            signal(app.as_ref());
        }
    });
}
/// The self-managed agent may create a free tunnel only for itself. No account
/// API key, web session, region upgrade or paid fallback is requested.
pub async fn configure(app: Option<tauri::AppHandle>, s: Shared, id: &str) -> Result<Value> {
    let p = load(&s, id)?;
    ensure!(
        p["access"] == "playit",
        "Choose playit access for this server first"
    );
    let secret = secure::get(&s, "playit-host")
        .context("Authorize this installation with Set up online access first")?;
    let api = PlayitApi::create(API.into(), Some(secret));
    provision(&s, &api, p["port"].as_u64().unwrap() as u16).await?;
    start_setup(app, s);
    Ok(
        json!({"note":"Free Minecraft Java tunnel requested or reused. Allocation may take a moment; the address appears after the provider allocates it."}),
    )
}
fn free_request(agent: uuid::Uuid, port: u16) -> ReqTunnelsCreate {
    ReqTunnelsCreate {
        name: Some(format!("Orbit Minecraft {port}")),
        tunnel_type: Some(TunnelType::MinecraftJava),
        port_type: PortType::Tcp,
        port_count: 1,
        origin: TunnelOriginCreate::Agent(AssignedAgentCreate {
            agent_id: agent,
            local_ip: std::net::Ipv4Addr::LOCALHOST.into(),
            local_port: Some(port),
        }),
        enabled: true,
        alloc: Some(TunnelCreateUseAllocation::Region(UseRegion {
            region: PlayitNetwork::Global,
        })),
        firewall_id: None,
        proxy_protocol: None,
    }
}
async fn provision(s: &AppState, api: &PlayitApi, port: u16) -> Result<()> {
    let _guard = s.servers.provision_lock.lock().await;
    let data = tokio::time::timeout(Duration::from_secs(15), api.v1_agents_rundata())
        .await
        .context("Playit configuration timed out")?
        .map_err(|_| {
            anyhow::anyhow!(
                "Playit could not read this host. Check authorization and account verification."
            )
        })?;
    if data.tunnels.iter().any(|t| selected(t, port).is_some())
        || data
            .pending
            .iter()
            .any(|t| t.name == format!("Orbit Minecraft {port}"))
    {
        return Ok(());
    }
    ensure!(data.permissions.account_status == AccountStatus::Verified,"Playit requires a verified account. Complete email verification in its dashboard, then retry. No paid plan is needed for the requested Minecraft Java tunnel.");
    let key = format!("playitProvision:{}:{port}", data.agent_id);
    let previous = db::setting(s, &key)
        .ok()
        .and_then(|v| v.as_i64())
        .unwrap_or(0);
    ensure!(db::now()-previous >= 300,"A tunnel allocation was recently requested. Wait up to five minutes and check the playit dashboard before retrying; this avoids duplicate allocations after an uncertain response.");
    s.db.lock().unwrap().execute(
        "INSERT OR REPLACE INTO settings VALUES(?,?)",
        rusqlite::params![key, db::now().to_string()],
    )?;
    match tokio::time::timeout(Duration::from_secs(20),api.tunnels_create(free_request(data.agent_id,port))).await {
        Ok(Ok(_)) => Ok(()),
        Ok(Err(ApiError::Fail(reason))) => { s.db.lock().unwrap().execute("DELETE FROM settings WHERE key=?",[key])?; anyhow::bail!("Playit refused the free Minecraft Java tunnel: {reason}. Check account limits in the dashboard; Orbit will not purchase or upgrade anything."); },
        Ok(Err(ApiError::ApiError(ApiResponseError::Auth(reason)))) => anyhow::bail!("Playit tunnel creation permission: {reason:?}. Create a free Minecraft Java tunnel manually for this agent to 127.0.0.1:{port}, Proxy Protocol off, then reconnect."),
        _ => anyhow::bail!("Playit tunnel creation response was not confirmed. Check the dashboard before retrying in five minutes; no duplicate request was sent."),
    }
}
#[cfg(test)]
mod verification {
    use super::*;
    #[tokio::test]
    #[ignore = "Creates and deletes only one isolated free playit test tunnel using the explicitly authorized test host"]
    async fn live_free_self_managed_tunnel_creation() -> Result<()> {
        let root = PathBuf::from(std::env::var("ORBIT_SERVER_TEST_ROOT")?);
        let s = crate::social_tests::state(dunce::canonicalize(root)?)?;
        let secret = secure::get(&s, "playit-host")?;
        let api = PlayitApi::create(API.into(), Some(secret));
        let data = api
            .v1_agents_rundata()
            .await
            .map_err(|_| anyhow::anyhow!("Read authorized test agent failed"))?;
        let created = api
            .tunnels_create(free_request(data.agent_id, 25881))
            .await
            .map_err(|error| match error {
                ApiError::Fail(v) => anyhow::anyhow!("Free tunnel creation refused: {v}"),
                ApiError::ApiError(ApiResponseError::Auth(v)) => {
                    anyhow::anyhow!("Free tunnel creation authorization: {v:?}")
                }
                ApiError::ApiError(v) => anyhow::anyhow!("Free tunnel provider validation: {v:?}"),
                ApiError::ClientError(
                    playit_api_client::http_client::HttpClientError::ParseError(_, status, body),
                ) => {
                    let v: Value = serde_json::from_str(&body).unwrap_or(Value::Null);
                    let code = v["data"]
                        .as_str()
                        .filter(|x| {
                            x.len() < 100
                                && x.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
                        })
                        .unwrap_or("unrecognized-schema");
                    anyhow::anyhow!(
                        "Free tunnel creation HTTP {status}; provider code {code}; data keys {:?}",
                        v["data"].as_object().map(|o| o.keys().collect::<Vec<_>>())
                    )
                }
                _ => anyhow::anyhow!("Free tunnel creation failed; response unconfirmed"),
            })?;
        let checked = async {
            for _ in 0..30 {
                let data = api
                    .v1_agents_rundata()
                    .await
                    .map_err(|_| anyhow::anyhow!("Agent config failed"))?;
                if let Some(t) = data.tunnels.iter().find(|t| t.id == created.id) {
                    ensure!(
                        selected(t, 25881).is_some(),
                        "Provider tunnel does not match exact restricted origin"
                    );
                    return Ok(());
                }
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
            anyhow::bail!("Free tunnel allocation did not finish")
        }
        .await;
        api.tunnels_delete(ReqTunnelsDelete {
            tunnel_id: created.id,
        })
        .await
        .map_err(|_| anyhow::anyhow!("Isolated test tunnel cleanup failed"))?;
        checked?;
        eprintln!("PLAYIT_FREE_PROVISION_PASS: self-managed Agent-Key created a free global MinecraftJava tunnel, exact origin25881 verified, only that test tunnel deleted");
        Ok(())
    }
}
struct CancelOnDrop(CancellationToken);
impl Drop for CancelOnDrop {
    fn drop(&mut self) {
        self.0.cancel();
    }
}
pub fn selected(tunnel: &AgentTunnelV1, port: u16) -> Option<OriginResource> {
    if tunnel.tunnel_type.as_deref() != Some("minecraft-java")
        || tunnel.disabled_reason.is_some()
        || tunnel.port_type != PortType::Tcp
        || tunnel.port_count != 1
    {
        return None;
    }
    let res = OriginResource::from_agent_tunnel(tunnel)?;
    if res.proxy_protocol.is_some() {
        return None;
    }
    match &res.target {
        OriginTarget::Port {
            ip: OriginIp::IpAddress(ip),
            port: p,
        } if *ip == std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST) && *p == port => Some(res),
        _ => None,
    }
}
fn update(app: Option<&tauri::AppHandle>, job: &Job, ready: bool, address: &str, note: &str) {
    let mut st = job.status.lock().unwrap();
    st["tunnelReady"] = json!(ready);
    st["address"] = json!(address);
    st["accessNote"] = json!(note);
    drop(st);
    signal(app);
}
pub async fn run(app: Option<tauri::AppHandle>, s: Shared, p: Value, job: Arc<Job>) {
    if let Some(setup) = s.servers.setup_cancel.lock().unwrap().take() {
        setup.cancel();
    }
    let cancel = job.cancel.child_token();
    if let Some(old) = job.network_cancel.lock().unwrap().replace(cancel.clone()) {
        old.cancel();
    }
    if p["access"] == "manual" {
        update(app.as_ref(),&job,false,p["address"].as_str().unwrap_or(""),"Advanced manual access: requires a reachable public address, matching port-forward/firewall rules and may fail behind CGNAT. Online requires a successful status check from this host; that is not an independent remote-player test.");
        return;
    }
    let result = tokio::select! {_=cancel.cancelled()=>return,result=playit(app.as_ref(),&s,&p,&job,&cancel)=>result};
    if let Err(error) = result {
        update(app.as_ref(), &job, false, "", &error.to_string());
        log(&job, format!("Online access: {error}"));
    }
}
async fn playit(
    app: Option<&tauri::AppHandle>,
    s: &AppState,
    p: &Value,
    job: &Job,
    cancel: &CancellationToken,
) -> Result<()> {
    let secret=secure::get(s,"playit-host").context("Authorize this host in your browser using Set up online access. No developer API key is needed.")?;
    let api = PlayitApi::create(API.into(), Some(secret.clone()));
    let port = p["port"].as_u64().unwrap() as u16;
    job.status.lock().unwrap()["setupError"] = match provision(s, &api, port).await {
        Ok(()) => Value::Null,
        Err(e) => {
            log(job, format!("Tunnel setup: {e}"));
            json!(e.to_string())
        }
    };
    for attempt in 0..5 {
        update(
            app,
            job,
            false,
            "",
            "Connecting the authorized playit agent…",
        );
        let data=tokio::time::timeout(Duration::from_secs(15),api.v1_agents_rundata()).await.context("Playit configuration request timed out")?.map_err(|_|anyhow::anyhow!("Playit rejected the agent or is unavailable. Check authorization, account limits and network access."))?;
        let lookup = Arc::new(OriginLookup::default());
        let mut selected_id = None;
        let mut address = String::new();
        if let Some(t) = data.tunnels.iter().find(|t| selected(t, port).is_some()) {
            sessions::address(&t.display_address)?;
            selected_id = Some(t.id);
            address = t.display_address.clone();
            lookup
                .update(std::iter::once(selected(t, port).unwrap()))
                .await;
        }
        // Connect even before a tunnel exists, so the browser setup never waits on
        // an agent that is itself waiting for setup. An empty map exposes no ports.
        let settings = PlayitAgentSettings {
            api_url: API.into(),
            secret_key: secret.clone(),
            tcp_settings: TcpSettings::default(),
            udp_settings: UdpSettings::default(),
        };
        let agent = match tokio::time::timeout(
            Duration::from_secs(30),
            PlayitAgent::new(settings, lookup.clone()),
        )
        .await
        {
            Ok(Ok(agent)) => agent,
            _ => {
                update(
                    app,
                    job,
                    false,
                    &address,
                    "Playit handshake failed; retrying with bounded backoff.",
                );
                tokio::time::sleep(Duration::from_secs(2u64 << attempt)).await;
                continue;
            }
        };
        let agent_cancel = agent.cancellation_token();
        let _guard = CancelOnDrop(agent_cancel.clone());
        let stats = agent.stats();
        let mut handle = tokio::spawn(agent.run());
        let mut misses = 0;
        let configured_since = std::time::Instant::now();
        loop {
            let marker = job.status.lock().unwrap()["session"]
                .as_str()
                .unwrap_or("")
                .to_owned();
            let reached = !address.is_empty()
                && sessions::ping_public(&address)
                    .await
                    .is_ok_and(|v| v["description"].to_string().contains(&marker));
            let note = if address.is_empty() {
                "Agent connected. In playit, create a free Minecraft Java tunnel for this Orbit agent to 127.0.0.1 and this server's port; disable Proxy Protocol. No ports are forwarded until that exact origin matches."
            } else if reached {
                "Playit agent authenticated; matching public tunnel status round-trip passed from this host. An independent remote player has not been verified."
            } else {
                "Agent connected; waiting for a matching Minecraft response through the public address. Check the tunnel origin and provider/network availability."
            };
            update(app, job, reached, &address, note);
            let snapshot = stats.snapshot();
            {
                let mut st = job.status.lock().unwrap();
                st["tunnelBytesIn"] = json!(snapshot.bytes_in);
                st["tunnelBytesOut"] = json!(snapshot.bytes_out);
                st["addressCheckedFromHost"] = json!(reached);
            }
            if address.is_empty() {
                misses = 0;
                ensure!(configured_since.elapsed()<Duration::from_secs(600),"Tunnel setup is incomplete. Configure the Minecraft Java tunnel, then Reconnect online access.");
            } else if reached {
                misses = 0;
            } else {
                misses += 1;
            }
            if misses >= 4 {
                agent_cancel.cancel();
                if tokio::time::timeout(Duration::from_secs(5), &mut handle)
                    .await
                    .is_err()
                {
                    handle.abort();
                }
                break;
            }
            tokio::select! {
             _=cancel.cancelled()=>{agent_cancel.cancel();if tokio::time::timeout(Duration::from_secs(5),&mut handle).await.is_err(){handle.abort();}return Ok(());},
             _=&mut handle=>break,
             _=tokio::time::sleep(Duration::from_secs(15))=>{
              let data=match tokio::time::timeout(Duration::from_secs(12),api.v1_agents_rundata()).await {Ok(Ok(data))=>data,_=>{lookup.update(std::iter::empty()).await;agent_cancel.cancel();if tokio::time::timeout(Duration::from_secs(5),&mut handle).await.is_err(){handle.abort();}break;}};
              let chosen=data.tunnels.iter().find(|t|selected(t,port).is_some()&&selected_id.is_none_or(|id|t.id==id));
              if let Some(t)=chosen {sessions::address(&t.display_address)?;lookup.update(std::iter::once(selected(t,port).unwrap())).await;selected_id=Some(t.id);address=t.display_address.clone();}else{lookup.update(std::iter::empty()).await;selected_id=None;address.clear();}
             }
            }
        }
        tokio::time::sleep(Duration::from_secs(2u64 << attempt)).await;
    }
    anyhow::bail!("Playit retry limit reached. The world remains on this PC. Check the dashboard/network and use Reconnect online access.")
}
