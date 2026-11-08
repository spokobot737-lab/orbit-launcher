use super::*;
use crate::{identity, minecraft, secure, social};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
pub fn address(input: &str) -> Result<(String, u16)> {
    ensure!(
        !input.is_empty() && input.len() <= 253 && !input.chars().any(char::is_whitespace),
        "Invalid Minecraft address"
    );
    let (host, port) = if input.starts_with('[') {
        let end = input.find(']').context("Invalid IPv6 address")?;
        let host = &input[1..end];
        let port = if input.len() > end + 1 {
            input[end + 1..]
                .strip_prefix(':')
                .context("Invalid port")?
                .parse::<u16>()?
        } else {
            25565
        };
        (host.to_owned(), port)
    } else if let Some((h, p)) = input.rsplit_once(':') {
        (h.to_owned(), p.parse::<u16>()?)
    } else {
        (input.to_owned(), 25565)
    };
    ensure!(
        !host.is_empty()
            && port > 0
            && host
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b".-_:".contains(&b)),
        "Invalid Minecraft host or port"
    );
    Ok((host, port))
}
fn varint(mut n: i32) -> Vec<u8> {
    let mut bytes = vec![];
    loop {
        let mut b = (n & 127) as u8;
        n = ((n as u32) >> 7) as i32;
        if n != 0 {
            b |= 128;
        }
        bytes.push(b);
        if n == 0 {
            break;
        }
    }
    bytes
}
async fn read_varint(stream: &mut tokio::net::TcpStream) -> Result<usize> {
    let mut value = 0;
    for shift in (0..35).step_by(7) {
        let b = stream.read_u8().await?;
        value |= ((b & 127) as usize) << shift;
        if b & 128 == 0 {
            return Ok(value);
        }
    }
    anyhow::bail!("Invalid Minecraft status frame")
}
fn public_ip(ip: std::net::IpAddr) -> bool {
    match ip {
        std::net::IpAddr::V4(x) => {
            !x.is_private()
                && !x.is_loopback()
                && !x.is_link_local()
                && !x.is_unspecified()
                && !x.is_multicast()
                && x.octets()[0] != 0
                && x.octets()[0] < 224
        }
        std::net::IpAddr::V6(x) => {
            !x.is_loopback()
                && !x.is_unspecified()
                && !x.is_multicast()
                && (x.segments()[0] & 0xfe00) != 0xfc00
                && (x.segments()[0] & 0xffc0) != 0xfe80
                && x.to_ipv4_mapped().is_none_or(|v| public_ip(v.into()))
        }
    }
}
pub async fn ping(input: &str) -> Result<Value> {
    ping_inner(input, false).await
}
pub async fn ping_public(input: &str) -> Result<Value> {
    ping_inner(input, true).await
}
async fn ping_inner(input: &str, public: bool) -> Result<Value> {
    tokio::time::timeout(Duration::from_secs(5), async {
        let (host, port) = address(input)?;
        let (mut target, mut target_port) = (host.clone(), port);
        if port == 25565 && host.parse::<std::net::IpAddr>().is_err() {
            if let Ok(resolver) = hickory_resolver::TokioAsyncResolver::tokio_from_system_conf() {
                if let Ok(records) = resolver.srv_lookup(format!("_minecraft._tcp.{host}")).await {
                    if let Some(record) = records.iter().min_by_key(|r| r.priority()) {
                        target = record.target().to_utf8().trim_end_matches('.').into();
                        target_port = record.port();
                    }
                }
            }
        }
        let addresses = tokio::net::lookup_host((target.as_str(), target_port))
            .await?
            .collect::<Vec<_>>();
        ensure!(!addresses.is_empty(), "Server address has no DNS records");
        if public {
            ensure!(
                addresses.iter().all(|a| public_ip(a.ip())),
                "Friend server addresses must resolve to public internet addresses"
            );
        }
        let mut stream = tokio::net::TcpStream::connect(addresses.as_slice()).await?;
        let mut packet = vec![0];
        packet.extend(varint(767));
        packet.extend(varint(host.len() as i32));
        packet.extend(host.as_bytes());
        packet.extend(port.to_be_bytes());
        packet.push(1);
        let mut framed = varint(packet.len() as i32);
        framed.extend(packet);
        framed.extend([1, 0]);
        stream.write_all(&framed).await?;
        let length = read_varint(&mut stream).await?;
        ensure!(length <= 128 * 1024, "Server status is too large");
        ensure!(
            read_varint(&mut stream).await? == 0,
            "Invalid status packet"
        );
        let length = read_varint(&mut stream).await?;
        ensure!(length <= 128 * 1024, "Status JSON too large");
        let mut bytes = vec![0; length];
        stream.read_exact(&mut bytes).await?;
        Ok::<_, anyhow::Error>(serde_json::from_slice(&bytes)?)
    })
    .await
    .context("Minecraft status check timed out")?
}
fn revision(s: &AppState, id: &str) -> Result<i64> {
    let key = format!("serverRevision:{id}");
    let mut c = s.db.lock().unwrap();
    let t = c.transaction()?;
    let raw = t
        .query_row("SELECT value FROM settings WHERE key=?", [&key], |r| {
            r.get::<_, String>(0)
        })
        .unwrap_or_default();
    let n = (raw.parse::<i64>().unwrap_or(0) + 1).max(db::now() * 1000);
    t.execute(
        "INSERT OR REPLACE INTO settings VALUES(?,?)",
        rusqlite::params![key, n.to_string()],
    )?;
    t.commit()?;
    Ok(n)
}
fn card(s: &AppState, p: &Value, job: &Job, online: bool) -> Result<Value> {
    let st = job.status.lock().unwrap().clone();
    let host = identity::keys(s)?.public_key().to_hex();
    Ok(
        json!({"schema":1,"server":p["id"],"session":st["session"],"host":host,"name":p["name"],"version":p["version"],"loader":p["loader"],"fabricLoader":if p["loader"]=="fabric"{Some(FABRIC)}else{None},"mods":{"scope":"bare","required":[],"serverOnly":[],"optional":[]},"address":st["address"],"online":online,"expires":db::now()+150,"revision":revision(s,p["id"].as_str().unwrap())?,"note":"Minecraft account login and host-approved allowlist entry are required. A shared address is not access permission."}),
    )
}
pub fn receive(s: &AppState, peer: &str, v: &Value) -> Result<()> {
    init(s)?;
    ensure!(
        v.to_string().len() < 6000 && v["schema"] == 1 && v["host"] == peer,
        "Invalid server announcement author or schema"
    );
    uuid::Uuid::parse_str(v["server"].as_str().context("Server ID missing")?)?;
    uuid::Uuid::parse_str(v["session"].as_str().context("Session ID missing")?)?;
    ensure!(
        v["version"] == VERSION
            && matches!(v["loader"].as_str(), Some("vanilla" | "fabric"))
            && v["mods"]["scope"] == "bare"
            && ["required", "serverOnly", "optional"]
                .iter()
                .all(|k| v["mods"][k].as_array().is_some_and(|a| a.is_empty())),
        "Unsupported server/client mod compatibility scope"
    );
    address(v["address"].as_str().context("Server address missing")?)?;
    ensure!(
        v["name"]
            .as_str()
            .is_some_and(|x| !x.is_empty() && x.len() <= 80)
            && v["expires"].as_i64().is_some_and(|x| x <= db::now() + 180)
            && v["revision"].as_i64().is_some_and(|n| n > 0),
        "Invalid server session fields"
    );
    let id = v["server"].as_str().unwrap();
    let old: Option<String> = {
        use rusqlite::OptionalExtension;
        s.db.lock()
            .unwrap()
            .query_row(
                "SELECT data FROM server_sessions WHERE peer=? AND server=?",
                rusqlite::params![peer, id],
                |r| r.get(0),
            )
            .optional()?
    };
    if let Some(raw) = old {
        let old: Value = serde_json::from_str(&secure::unseal(s, &raw)?)?;
        if old["revision"].as_i64().unwrap_or(0) >= v["revision"].as_i64().unwrap() {
            return Ok(());
        }
    }
    s.db.lock().unwrap().execute(
        "INSERT OR REPLACE INTO server_sessions VALUES(?,?,?,?)",
        rusqlite::params![
            peer,
            id,
            secure::seal(s, &v.to_string())?,
            v["expires"].as_i64()
        ],
    )?;
    Ok(())
}
pub fn list(s: &AppState) -> Result<Value> {
    init(s)?;
    let rows: Vec<(String, String)> = {
        let c = s.db.lock().unwrap();
        let mut q=c.prepare("SELECT s.peer,s.data FROM server_sessions s JOIN contacts c ON c.pubkey=s.peer WHERE c.state='accepted' ORDER BY s.expires DESC LIMIT 100")?;
        let r = q
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<Result<_, _>>()?;
        r
    };
    let mut out = vec![];
    for (peer, raw) in rows {
        let mut v: Value = serde_json::from_str(&secure::unseal(s, &raw)?)?;
        v["peer"] = json!(peer);
        v["fresh"] = json!(v["online"] == true && v["expires"].as_i64().unwrap_or(0) > db::now());
        out.push(v);
    }
    Ok(json!(out))
}
pub async fn share(app: Option<tauri::AppHandle>, s: Shared, v: &Value) -> Result<Value> {
    let id = v["id"].as_str().context("Choose a server")?;
    let peer = v["peer"].as_str().context("Choose an accepted friend")?;
    ensure!(
        identity::contact(&s, peer)?["state"] == "accepted",
        "Accept this friendship first"
    );
    let p = load(&s, id)?;
    let job = s
        .servers
        .jobs
        .lock()
        .unwrap()
        .get(id)
        .cloned()
        .context("Start this server first")?;
    ensure!(
        job.status.lock().unwrap()["phase"] == "Online",
        "Wait for server and online access readiness before inviting friends"
    );
    let card = card(&s, &p, &job, true)?;
    let message = social::queue(&s, peer, "server_invite", card, true).await?;
    s.db.lock().unwrap().execute(
        "INSERT OR REPLACE INTO server_shares VALUES(?,?)",
        rusqlite::params![id, peer],
    )?;
    social::wake(app, s.clone());
    tokio::spawn(async move {
        let _ = social::flush(&s).await;
    });
    Ok(
        json!({"queued":true,"message":message,"note":"Ask the intended Minecraft account name and approve it in Allowlist. Orbit identity is not proof of that account."}),
    )
}
pub async fn announce(s: Shared, job: &Job, online: bool) -> Result<()> {
    if identity::config(&s)?["enabled"] != true {
        return Ok(());
    }
    let id = job.status.lock().unwrap()["serverId"]
        .as_str()
        .unwrap_or("")
        .to_owned();
    if id.is_empty() {
        return Ok(());
    }
    let p = load(&s, &id)?;
    let peers: Vec<String> = {
        let c = s.db.lock().unwrap();
        let mut q=c.prepare("SELECT peer FROM server_shares JOIN contacts ON pubkey=peer WHERE server=? AND state='accepted'")?;
        let r = q
            .query_map([&id], |r| r.get(0))?
            .collect::<Result<_, _>>()?;
        r
    };
    if peers.is_empty() {
        return Ok(());
    }
    let body = card(&s, &p, job, online)?;
    for peer in peers {
        social::queue(&s, &peer, "server_session", body.clone(), true).await?;
    }
    tokio::spawn(async move {
        let _ = social::flush(&s).await;
    });
    Ok(())
}
fn target(s: &AppState, v: &Value) -> Result<Value> {
    if let Some(id) = v["hostingId"].as_str() {
        let p = load(s, id)?;
        let job = s
            .servers
            .jobs
            .lock()
            .unwrap()
            .get(id)
            .cloned()
            .context("Start the server first")?;
        ensure!(
            job.status.lock().unwrap()["localReady"] == true
                && !job.stopped.load(std::sync::atomic::Ordering::Acquire),
            "Wait for the local server to be ready"
        );
        return Ok(
            json!({"version":p["version"],"loader":p["loader"],"address":format!("127.0.0.1:{}",p["port"]),"local":true}),
        );
    }
    let peer = v["peer"].as_str().context("Choose a friend server")?;
    ensure!(
        identity::contact(s, peer)?["state"] == "accepted",
        "This host is no longer an accepted friend"
    );
    list(s)?
        .as_array()
        .unwrap()
        .iter()
        .find(|x| x["peer"] == peer && x["server"] == v["server"] && x["fresh"] == true)
        .cloned()
        .context("This server invitation expired or the host stopped. Ask for a fresh invitation.")
}
fn compatible(s: &AppState, p: &Value) -> bool {
    if p["kind"] != "minecraft"
        || p["version"] != VERSION
        || p["account"] != "microsoft"
        || p["performance"] == true
    {
        return false;
    }
    if !matches!(p["loader"].as_str(), Some("vanilla" | "fabric")) {
        return false;
    }
    let Ok(id) = p["id"].as_str().context("ID") else {
        return false;
    };
    let mods = s.root.join("minecraft/profiles").join(id).join("game/mods");
    !mods.exists() || std::fs::read_dir(mods).is_ok_and(|mut entries| entries.next().is_none())
}
pub async fn options(s: &AppState, v: &Value) -> Result<Value> {
    let t = target(s, v)?;
    let profiles = db::library(s)?
        .as_array()
        .unwrap()
        .iter()
        .filter(|p| compatible(s, p))
        .cloned()
        .collect::<Vec<_>>();
    Ok(
        json!({"target":t,"profiles":profiles,"note":"Only authenticated 1.21.1 Vanilla or bare Fabric profiles without extra mods are offered. Offline profiles cannot join online-mode servers."}),
    )
}
pub fn create_client(s: &AppState, v: &Value) -> Result<Value> {
    let _ = target(s, v)?;
    let p = json!({"id":uuid::Uuid::new_v4().to_string(),"kind":"minecraft","name":"Server play · 1.21.1","version":VERSION,"loader":"vanilla","memory":2048,"width":1280,"height":720,"javaArgs":[],"account":"microsoft","offlineName":"Player","performance":false,"fullscreen":false});
    db::save_item(s, &p)
}
pub async fn join(app: &tauri::AppHandle, s: &AppState, v: &Value) -> Result<Value> {
    let t = target(s, v)?;
    let p = db::item(
        s,
        v["profile"]
            .as_str()
            .context("Choose a compatible Minecraft profile")?,
    )?;
    ensure!(
        compatible(s, &p),
        "This client is not in the supported compatibility scope. Create a clean 1.21.1 profile."
    );
    let address = t["address"].as_str().context("Server address missing")?;
    if t["local"] != true {
        let status=ping_public(address).await.context("Server is not reachable now. Ask the host to check online access; availability is not guaranteed by an invitation.")?;
        ensure!(
            status["version"]["protocol"] == 767,
            "Server protocol changed; do not launch an incompatible client"
        );
        ensure!(
            status["description"]
                .to_string()
                .contains(t["session"].as_str().context("Session marker missing")?),
            "This address no longer identifies the shared session. Ask the host for a fresh card."
        );
    }
    let mem = process::memory();
    let client_memory = p["memory"].as_u64().unwrap_or(2048);
    if let Some(available) = mem["availableMiB"].as_u64() {
        ensure!(available>client_memory+1024,"Not enough available RAM for the client alongside the hosted server. Reduce client/server memory or close other apps.");
    }
    if t["local"] == true {
        let owned = crate::auth::session(s).await?;
        let approved = process::allowlist(
            s,
            &json!({"id":v["hostingId"],"name":owned["name"],"approved":true}),
        )
        .await?;
        ensure!(
            uuid::Uuid::parse_str(approved["account"]["uuid"].as_str().unwrap())?
                == uuid::Uuid::parse_str(
                    owned["id"]
                        .as_str()
                        .context("Authenticated Minecraft UUID missing")?
                )?,
            "Resolved account differs from authenticated host"
        );
    }
    minecraft::launch(app, s, &json!({"id":p["id"],"join":address})).await
}
