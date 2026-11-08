use crate::{db, identity, secure, wallet, AppState, Shared};
use anyhow::{bail, ensure, Context, Result};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use nostr_sdk::prelude::*;
use rusqlite::params;
use serde_json::{json, Value};
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::Emitter;
#[derive(Default)]
pub struct Runtime {
    pub client: tokio::sync::Mutex<Option<Client>>,
    pub generation: std::sync::atomic::AtomicU64,
    pub send_lock: tokio::sync::Mutex<()>,
    pub start_lock: tokio::sync::Mutex<()>,
    pub recovery_lock: tokio::sync::Mutex<()>,
    pub profile_lock: tokio::sync::Mutex<()>,
    pub ever_connected: AtomicBool,
    pub last_relay_ack: std::sync::atomic::AtomicI64,
    pub connect_started: std::sync::atomic::AtomicI64,
    pub connection_phase: std::sync::Mutex<String>,
    pub wallet_lock: tokio::sync::Mutex<()>,
    pub gaming: AtomicBool,
    pub game: std::sync::Mutex<Value>,
    pub music: std::sync::Mutex<Value>,
    pub watcher: std::sync::Mutex<Option<crate::activity::Watcher>>,
}
pub(crate) fn notify(app: Option<&tauri::AppHandle>) {
    if let Some(a) = app {
        let _ = a.emit("social-changed", json!({}));
    }
}
fn public_profile(s: &AppState) -> Result<Value> {
    let p = identity::profile(s)?;
    Ok(
        json!({"id":p["id"],"name":p["name"],"bio":p["bio"],"connections":crate::connections::shared(s)?,"relays":identity::config(s)?["relays"]}),
    )
}
fn friend_profile(s: &AppState) -> Result<Value> {
    if let Some(profile) = crate::profile_sync::shared(s)? {
        return Ok(profile);
    }
    let mut p = public_profile(s)?;
    let own = identity::profile(s)?;
    if let Some(encoded) = own["avatar"]
        .as_str()
        .and_then(|s| s.strip_prefix("data:image/jpeg;base64,"))
    {
        let bytes = base64::engine::general_purpose::STANDARD.decode(encoded)?;
        let mut decoder =
            ::image::ImageReader::new(std::io::Cursor::new(bytes)).with_guessed_format()?;
        let mut limits = ::image::Limits::default();
        limits.max_alloc = Some(128 * 1024 * 1024);
        decoder.limits(limits);
        let image = decoder
            .decode()?
            .resize(64, 64, ::image::imageops::FilterType::Triangle);
        let mut bytes = vec![];
        ::image::codecs::jpeg::JpegEncoder::new_with_quality(&mut bytes, 60)
            .encode_image(&image)?;
        let avatar = format!(
            "data:image/jpeg;base64,{}",
            base64::engine::general_purpose::STANDARD.encode(bytes)
        );
        if avatar.len() <= 4096 {
            p["avatar"] = json!(avatar);
        }
    }
    Ok(p)
}
fn relays(v: &Value) -> Result<Vec<String>> {
    let a = v.as_array().context("Inbox relays missing")?;
    ensure!(
        !a.is_empty() && a.len() <= 3,
        "Invitation needs one to three inbox relays"
    );
    a.iter()
        .map(|v| {
            let raw = v.as_str().context("Invalid relay")?;
            identity::validate_endpoint(raw, true)?;
            Ok(raw.to_owned())
        })
        .collect()
}
pub async fn invitation(s: &AppState) -> Result<Value> {
    let keys = identity::keys(s)?;
    let config = identity::config(s)?;
    let urls = relays(&config["relays"])?;
    let token = uuid::Uuid::new_v4().simple().to_string();
    let expires = db::now() + 7 * 86400;
    s.db.lock().unwrap().execute_batch("CREATE TABLE IF NOT EXISTS invitations(token TEXT PRIMARY KEY,expires INTEGER NOT NULL,used_by TEXT)")?;
    s.db.lock().unwrap().execute(
        "INSERT INTO invitations VALUES(?,?,NULL)",
        params![token, expires],
    )?;
    let profile = public_profile(s)?;
    let event = EventBuilder::new(
        Kind::Custom(30078),
        json!({"orbit":1,"token":token,"expires":expires,"profile":profile}).to_string(),
    )
    .sign_with_keys(&keys)?;
    let tags = urls
        .iter()
        .map(|r| Tag::parse(["relay", r.as_str()]))
        .collect::<Result<Vec<_>, _>>()?;
    let inbox = EventBuilder::new(Kind::Custom(10050), "")
        .tags(tags)
        .sign_with_keys(&keys)?;
    let code = format!(
        "orbit:invite:{}",
        URL_SAFE_NO_PAD.encode(json!({"event":event,"inbox":inbox}).to_string())
    );
    Ok(json!({"code":code,"expires":expires,"identity":keys.public_key().to_hex()}))
}
pub async fn import_invitation(s: &AppState, v: &Value) -> Result<Value> {
    let code = v["code"]
        .as_str()
        .context("Paste an Orbit invitation")?
        .trim();
    ensure!(code.len() < 24_000, "Invitation is too long");
    let raw = URL_SAFE_NO_PAD.decode(
        code.strip_prefix("orbit:invite:")
            .context("Not an Orbit invitation")?,
    )?;
    let decoded: Value = serde_json::from_slice(&raw)?;
    let event: Event = serde_json::from_value(decoded["event"].clone())?;
    let inbox: Event = serde_json::from_value(decoded["inbox"].clone())?;
    event.verify()?;
    inbox.verify()?;
    ensure!(
        event.kind == Kind::Custom(30078)
            && inbox.kind == Kind::Custom(10050)
            && event.pubkey == inbox.pubkey,
        "Invalid signed invitation"
    );
    let data: Value = serde_json::from_str(&event.content)?;
    ensure!(
        data["orbit"] == 1
            && data["expires"]
                .as_i64()
                .is_some_and(|e| e > db::now() && e <= db::now() + 8 * 86400),
        "Invitation expired or unsupported"
    );
    let me = identity::keys(s)?.public_key();
    ensure!(event.pubkey != me, "This invitation belongs to you");
    let urls = inbox
        .tags
        .iter()
        .filter_map(|t| {
            let a = t.as_slice();
            if a.first().map(String::as_str) == Some("relay") {
                a.get(1).cloned()
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    relays(&json!(urls))?;
    let mut p = data["profile"].clone();
    validate_public_profile(&p, event.pubkey)?;
    p["relays"] = json!(urls);
    if let Ok(old) = identity::contact(s, &event.pubkey.to_hex()) {
        ensure!(old["state"] != "blocked", "This person is blocked");
        ensure!(old["state"] != "accepted", "You are already friends");
    }
    crate::profile_sync::cache(s, &p, "outgoing")?;
    queue(
        s,
        &event.pubkey.to_hex(),
        "friend_request",
        json!({"token":data["token"],"profile":friend_profile(s)?}),
        false,
    )
    .await?;
    Ok(p)
}
fn validate_public_profile(p: &Value, key: PublicKey) -> Result<()> {
    crate::profile_sync::validate(p, key)?;
    ensure!(
        p["wallet"].is_null(),
        "A shared profile cannot assert a verified wallet; a peer-issued proof is required"
    );
    if let Some(avatar) = p["avatar"].as_str() {
        ensure!(
            avatar.starts_with("data:image/jpeg;base64,") && avatar.len() <= 4096,
            "Invalid shared avatar"
        );
    }
    ensure!(
        p["id"] == key.to_hex(),
        "Profile does not match its authenticated sender"
    );
    ensure!(
        p["name"]
            .as_str()
            .is_some_and(|s| !s.trim().is_empty() && s.chars().count() <= 60)
            && p["bio"].as_str().unwrap_or("").len() <= 2000,
        "Invalid profile"
    );
    relays(&p["relays"])?;
    Ok(())
}
pub async fn accept(s: &AppState, v: &Value) -> Result<Value> {
    let id = v["id"].as_str().context("Choose an invitation")?;
    let contact = identity::contact(s, id)?;
    ensure!(contact["state"] == "incoming", "No pending invitation");
    identity::put_contact(s, &contact, "accepted")?;
    crate::profile_sync::mark_dirty(s)?;
    queue(
        s,
        id,
        "friend_accept",
        json!({"profile":friend_profile(s)?}),
        false,
    )
    .await?;
    Ok(json!(true))
}
pub async fn queue(
    s: &AppState,
    peer: &str,
    kind: &str,
    body: Value,
    requires_friend: bool,
) -> Result<String> {
    let contact = identity::contact(s, peer)?;
    ensure!(
        contact["state"] != "blocked" && contact["state"] != "removed",
        "Contact is blocked or removed"
    );
    if requires_friend {
        ensure!(
            contact["state"] == "accepted",
            "Accept the friendship before messaging"
        );
    }
    let keys = identity::keys(s)?;
    let target = PublicKey::from_hex(peer)?;
    let id = uuid::Uuid::new_v4().to_string();
    let created = db::now();
    let envelope = json!({"orbit":1,"id":id,"from":keys.public_key().to_hex(),"to":peer,"kind":kind,"created":created,"body":body});
    let payload = envelope.to_string();
    ensure!(
        payload.len() <= 16_000,
        "Message metadata exceeds 16 KB; attach files separately"
    );
    let rumor = EventBuilder::private_msg_rumor(target, payload).build(keys.public_key());
    let event = EventBuilder::gift_wrap(&keys, &target, rumor.clone(), []).await?;
    let self_event = EventBuilder::gift_wrap(&keys, &keys.public_key(), rumor, []).await?;
    let encrypted = secure::seal(s, &body.to_string())?;
    let receiver_relays = relays(&contact["relays"])?;
    let self_relays = relays(&identity::config(s)?["relays"])?;
    let mut c = s.db.lock().unwrap();
    let tx = c.transaction()?;
    tx.execute("INSERT INTO messages(id,peer,direction,kind,body,state,created) VALUES(?,?,'out',?,?,'queued',?)",params![id,peer,kind,encrypted,created])?;
    for (suffix, e, rs) in [
        ("peer", event, receiver_relays),
        ("self", self_event, self_relays),
    ] {
        tx.execute(
            "INSERT INTO outbox(id,event,relays) VALUES(?,?,?)",
            params![format!("{id}:{suffix}"), e.as_json(), json!(rs).to_string()],
        )?;
    }
    tx.commit()?;
    Ok(id)
}
pub async fn send(s: &AppState, v: &Value) -> Result<Value> {
    let peer = v["peer"].as_str().context("Choose a friend")?;
    let text = v["text"].as_str().unwrap_or("").trim();
    ensure!(text.len() <= 8000, "Text is limited to 8 KB");
    let ids = v["attachments"].as_array().cloned().unwrap_or_default();
    ensure!(ids.len() <= 10, "Attach at most ten files per message");
    let mut files = vec![];
    for id in ids {
        let raw: String = s.db.lock().unwrap().query_row(
            "SELECT data FROM media WHERE id=?",
            [id.as_str().context("Invalid attachment")?],
            |r| r.get(0),
        )?;
        let mut f: Value = serde_json::from_str(&secure::unseal(s, &raw)?)?;
        ensure!(
            f["state"] == "uploaded",
            "Finish uploading attachments before sending"
        );
        if let Some(o) = f.as_object_mut() {
            for key in ["source", "encryptedPath"] {
                o.remove(key);
            }
        }
        files.push(f);
    }
    ensure!(
        !text.is_empty() || !files.is_empty(),
        "Write a message or attach a file"
    );
    let id = queue(
        s,
        peer,
        "message",
        json!({"text":text,"attachments":files}),
        true,
    )
    .await?;
    Ok(json!({"id":id,"state":"queued"}))
}
pub async fn start(app: Option<tauri::AppHandle>, s: Shared) -> Result<Value> {
    stop(&s).await;
    s.social.last_relay_ack.store(0, Ordering::Relaxed);
    s.social.connect_started.store(db::now(), Ordering::Relaxed);
    let cfg = identity::config(&s)?;
    ensure!(
        cfg["enabled"] == true,
        "Enable relay connections in Profile → Network"
    );
    let urls = relays(&cfg["relays"])?;
    let keys = identity::keys(&s)?;
    let client = Client::new(keys.clone());
    client.automatic_authentication(true);
    for r in &urls {
        client.add_relay(r).await?;
    }
    *s.social.client.lock().await = Some(client.clone());
    let mut notifications = client.notifications();
    client.connect().await;
    client
        .subscribe_to(
            &urls,
            Filter::new()
                .kind(Kind::GiftWrap)
                .pubkey(keys.public_key())
                .limit(500),
            None,
        )
        .await?;
    let inbox = EventBuilder::new(Kind::Custom(10050), "")
        .tags(
            urls.iter()
                .map(|r| Tag::parse(["relay", r.as_str()]))
                .collect::<Result<Vec<_>, _>>()?,
        )
        .sign_with_keys(&keys)?;
    if let Ok(Ok(output)) = tokio::time::timeout(
        std::time::Duration::from_secs(12),
        client.send_event_to(&urls, &inbox),
    )
    .await
    {
        if !output.success.is_empty() {
            s.social.last_relay_ack.store(db::now(), Ordering::Relaxed);
        }
    }
    *s.social.client.lock().await = Some(client.clone());
    let generation = s.social.generation.load(Ordering::SeqCst);
    let jobs_exist: bool =
        s.db.lock()
            .unwrap()
            .query_row("SELECT EXISTS(SELECT 1 FROM profile_jobs)", [], |r| {
                r.get(0)
            })?;
    if !jobs_exist {
        crate::profile_sync::mark_dirty(&s)?;
    }
    let recovery = s.clone();
    let recover_client = client.clone();
    tokio::spawn(async move {
        let _ = recover(recovery, recover_client).await;
    });
    let state = s.clone();
    let handle = app.clone();
    let incoming = client.clone();
    tokio::spawn(async move {
        while let Ok(n) = notifications.recv().await {
            if state.social.generation.load(Ordering::SeqCst) != generation {
                break;
            }
            if let RelayPoolNotification::Event { event, .. } = n {
                if event.kind == Kind::GiftWrap {
                    let _result = receive(&state, &incoming, &event).await;
                    #[cfg(test)]
                    if let Err(e) = _result {
                        eprintln!("Synthetic envelope rejected: {e:#}");
                    }
                    notify(handle.as_ref());
                    crate::profile_sync::schedule(handle.clone(), state.clone());
                }
            }
        }
    });
    let state = s.clone();
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(std::time::Duration::from_secs(15));
        loop {
            tick.tick().await;
            if state.social.generation.load(Ordering::SeqCst) != generation {
                break;
            }
            crate::profile_sync::schedule(app.clone(), state.clone());
            if let Ok(snapshot) = status(&state).await {
                let phase = snapshot["phase"].as_str().unwrap_or("Offline").to_owned();
                let changed = {
                    let mut previous = state.social.connection_phase.lock().unwrap();
                    let changed = *previous != phase;
                    *previous = phase.clone();
                    changed
                };
                if changed {
                    notify(app.as_ref());
                    if phase == "Connected" {
                        if let Some(client) = state.social.client.lock().await.clone() {
                            let recovery = state.clone();
                            tokio::spawn(async move {
                                let _ = recover(recovery, client).await;
                            });
                        }
                    }
                }
            }
            let pending: bool = state
                .db
                .lock()
                .unwrap()
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM outbox WHERE next_try<=? AND attempts<5)",
                    [db::now()],
                    |r| r.get(0),
                )
                .unwrap_or(false);
            if pending {
                let _ = flush(&state).await;
                notify(app.as_ref());
            }
        }
    });
    Ok(
        json!({"connected":true,"note":"Connections started; sent state requires relay acknowledgment. Retention is provider-controlled."}),
    )
}
pub async fn stop(s: &AppState) {
    s.social.generation.fetch_add(1, Ordering::SeqCst);
    if let Some(client) = s.social.client.lock().await.take() {
        client.shutdown().await;
    }
}
pub async fn flush(s: &AppState) -> Result<()> {
    let _lock = s.social.send_lock.lock().await;
    let Some(client) = s.social.client.lock().await.clone() else {
        return Ok(());
    };
    let jobs = {
        let c = s.db.lock().unwrap();
        let mut q=c.prepare("SELECT id,event,relays,attempts FROM outbox WHERE attempts<5 AND next_try<=? ORDER BY next_try LIMIT 12")?;
        let rows = q
            .query_map([db::now()], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, i64>(3)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        rows
    };
    for (id, raw, urls, attempts) in jobs {
        let event = Event::from_json(raw)?;
        let urls: Vec<String> = serde_json::from_str(&urls)?;
        for url in &urls {
            identity::validate_endpoint(url, true)?;
            client.add_relay(url).await?;
            let _ = client.connect_relay(url).await;
        }
        let result = tokio::time::timeout(
            std::time::Duration::from_secs(12),
            client.send_event_to(&urls, &event),
        )
        .await;
        let ok = matches!(result,Ok(Ok(ref output))if !output.success.is_empty());
        #[cfg(test)]
        if !ok {
            eprintln!("Synthetic relay send failed: {result:?}");
        }
        let message_id = id.rsplit_once(':').context("Invalid outbox ID")?.0;
        let c = s.db.lock().unwrap();
        if ok {
            s.social.last_relay_ack.store(db::now(), Ordering::Relaxed);
            c.execute("DELETE FROM outbox WHERE id=?", [&id])?;
            if id.ends_with(":peer") {
                c.execute("UPDATE messages SET state='sent',error=NULL WHERE id=? AND state IN('queued','failed')",[message_id])?;
            }
        } else {
            c.execute(
                "UPDATE outbox SET attempts=attempts+1,next_try=? WHERE id=?",
                params![db::now() + (5_i64 << attempts).min(300), id],
            )?;
            if id.ends_with(":peer") {
                c.execute(
                    "UPDATE messages SET state=?,error=? WHERE id=? AND state!='delivered'",
                    params![
                        if attempts >= 4 { "failed" } else { "queued" },
                        "No recipient relay acknowledged this event. Check relay access and retry.",
                        message_id
                    ],
                )?;
            }
        }
    }
    Ok(())
}
pub fn retry(s: &AppState, v: &Value) -> Result<Value> {
    let id = v["id"].as_str().context("Choose a failed message")?;
    let c = s.db.lock().unwrap();
    c.execute(
        "UPDATE outbox SET attempts=0,next_try=0 WHERE id LIKE ?",
        [format!("{id}:%")],
    )?;
    c.execute(
        "UPDATE messages SET state='queued',error=NULL WHERE id=? AND state='failed'",
        [id],
    )?;
    Ok(json!(true))
}
pub async fn receive(s: &AppState, client: &Client, event: &Event) -> Result<()> {
    event.verify()?;
    ensure!(
        event.content.len() <= 100_000,
        "Encrypted event is too large"
    );
    let gift = client.unwrap_gift_wrap(event).await?;
    ensure!(
        gift.sender == gift.rumor.pubkey && gift.rumor.kind == Kind::PrivateDirectMessage,
        "NIP-17 sender mismatch"
    );
    let e: Value = serde_json::from_str(&gift.rumor.content)?;
    let me = identity::keys(s)?.public_key().to_hex();
    let sender = gift.sender.to_hex();
    ensure!(
        e["orbit"] == 1 && e["from"] == sender && e["to"] == me,
        "Message is not addressed to this Orbit identity"
    );
    let id = e["id"].as_str().context("Message ID missing")?;
    uuid::Uuid::parse_str(id)?;
    let kind = e["kind"].as_str().context("Message kind missing")?;
    let created = e["created"].as_i64().context("Message time missing")?;
    ensure!(
        created <= db::now() + 300
            && (kind == "profile_update" || created > db::now() - 90 * 86400),
        "Message time is outside the supported window"
    );
    let seen: bool = s.db.lock().unwrap().query_row(
        "SELECT EXISTS(SELECT 1 FROM social_seen WHERE id=?)",
        [id],
        |r| r.get(0),
    )?;
    if seen {
        return Ok(());
    }
    let contact = identity::contact(s, &sender).unwrap_or(Value::Null);
    ensure!(
        contact["state"] != "blocked" && contact["state"] != "removed",
        "Sender is blocked"
    );
    let body = &e["body"];
    match kind {
        "friend_request" => {
            s.db.lock().unwrap().execute_batch("CREATE TABLE IF NOT EXISTS invitations(token TEXT PRIMARY KEY,expires INTEGER NOT NULL,used_by TEXT)")?;
            let token = body["token"].as_str().context("Invitation token missing")?;
            let changed=s.db.lock().unwrap().execute("UPDATE invitations SET used_by=? WHERE token=? AND expires>? AND (used_by IS NULL OR used_by=?)",params![sender,token,db::now(),sender])?;
            ensure!(changed == 1, "Invitation is expired or already used");
            validate_public_profile(&body["profile"], gift.sender)?;
            let count: i64 = s.db.lock().unwrap().query_row(
                "SELECT COUNT(*) FROM contacts WHERE state='incoming'",
                [],
                |r| r.get(0),
            )?;
            ensure!(count < 30, "Too many pending invitations");
            if contact["state"] != "accepted" {
                crate::profile_sync::cache(s, &body["profile"], "incoming")?;
            }
        }
        "friend_accept" => {
            ensure!(
                contact["state"] == "outgoing",
                "No outgoing friendship request"
            );
            validate_public_profile(&body["profile"], gift.sender)?;
            crate::profile_sync::cache(s, &body["profile"], "accepted")?;
        }
        "profile_update" => {
            ensure!(
                contact["state"] == "accepted",
                "Only accepted friends can update their profile"
            );
            validate_public_profile(&body["profile"], gift.sender)?;
            crate::profile_sync::cache(s, &body["profile"], "accepted")?;
        }
        "ack" => {
            ensure!(
                contact["state"] == "accepted",
                "Unknown acknowledgment sender"
            );
            if let Some(ack) = body["id"].as_str() {
                s.db.lock().unwrap().execute("UPDATE messages SET state='delivered',error=NULL WHERE id=? AND peer=? AND direction='out'",params![ack,sender])?;
            }
        }
        "message" | "game_activity" | "music_activity" | "wallet_challenge" | "wallet_proof" => {
            ensure!(
                contact["state"] == "accepted",
                "Only accepted friends can send messages"
            );
            ensure!(body.to_string().len() <= 16_000, "Message too large");
            if kind == "wallet_proof" {
                let raw = body["message"].as_str().context("SIWE message missing")?;
                let uri = format!("urn:orbit:peer:{me}");
                let address = wallet::verify(
                    s,
                    &sender,
                    raw,
                    body["signature"].as_str().unwrap_or(""),
                    "orbit.local",
                    &uri,
                )
                .await?;
                let mut contact = contact.clone();
                contact["wallet"] =
                    json!({"address":address,"verifiedBy":me,"expires":db::now()+86400});
                identity::put_contact(s, &contact, "accepted")?;
            }
            if kind.ends_with("_activity") {
                let expires = body["expires"].as_i64().unwrap_or(0);
                ensure!(
                    expires <= db::now() + 180 && expires > db::now(),
                    "Activity is stale"
                );
                let raw = secure::seal(s, &json!({"kind":kind,"body":body}).to_string())?;
                s.db.lock().unwrap().execute(
                    "INSERT OR REPLACE INTO friend_activity VALUES(?,?,?,?)",
                    params![sender, kind, raw, expires],
                )?;
            } else {
                let encrypted = secure::seal(s, &body.to_string())?;
                s.db.lock().unwrap().execute("INSERT OR IGNORE INTO messages(id,peer,direction,kind,body,state,created) VALUES(?,?,'in',?,?,'delivered',?)",params![id,sender,kind,encrypted,created])?;
                if kind == "message" {
                    queue(s, &sender, "ack", json!({"id":id}), true).await?;
                }
            }
        }
        _ => bail!("Unsupported Orbit message type"),
    }
    s.db.lock().unwrap().execute(
        "INSERT OR IGNORE INTO social_seen VALUES(?,?)",
        params![id, db::now()],
    )?;
    Ok(())
}
pub async fn wallet_request(s: &AppState, v: &Value) -> Result<Value> {
    let peer = v["peer"].as_str().context("Choose a friend")?;
    let address = v["address"]
        .as_str()
        .context("Enter the wallet address they claim")?;
    let me = identity::keys(s)?.public_key().to_hex();
    let raw = wallet::issue(
        s,
        peer,
        address,
        "orbit.local",
        &format!("urn:orbit:peer:{me}"),
    )?;
    let id = queue(s, peer, "wallet_challenge", json!({"message":raw}), true).await?;
    Ok(json!({"id":id}))
}
pub async fn wallet_approve(s: &AppState, v: &Value) -> Result<Value> {
    let id = v["id"].as_str().context("Choose a challenge")?;
    let (peer, raw): (String, String) = s.db.lock().unwrap().query_row(
        "SELECT peer,body FROM messages WHERE id=? AND direction='in' AND kind='wallet_challenge'",
        [id],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    let body: Value = serde_json::from_str(&secure::unseal(s, &raw)?)?;
    let message = body["message"]
        .as_str()
        .context("Challenge message missing")?;
    let parsed: siwe::Message = message.parse()?;
    let me = identity::keys(s)?.public_key().to_hex();
    ensure!(
        parsed.domain.to_string() == "orbit.local"
            && parsed.uri.to_string() == format!("urn:orbit:peer:{peer}")
            && parsed
                .resources
                .iter()
                .any(|x| x.to_string() == format!("urn:orbit:identity:{me}"))
            && parsed
                .resources
                .iter()
                .any(|x| x.to_string() == format!("urn:orbit:verifier:{peer}"))
            && parsed.valid_now(),
        "Challenge audience, identity or time mismatch"
    );
    let signature = wallet::sign_local(s, message)?;
    queue(
        s,
        &peer,
        "wallet_proof",
        json!({"message":message,"signature":signature}),
        true,
    )
    .await?;
    Ok(json!(true))
}
pub fn wake(app: Option<tauri::AppHandle>, s: Shared) {
    tokio::spawn(async move {
        let _lock = s.social.start_lock.lock().await;
        if identity::config(&s).is_ok_and(|c| c["enabled"] == true)
            && identity::profile(&s).is_ok_and(|p| !p.is_null())
            && s.social.client.lock().await.is_none()
        {
            let _ = start(app.clone(), s.clone()).await;
            notify(app.as_ref());
        }
        crate::profile_sync::schedule(app, s.clone());
    });
}
async fn recover(s: Shared, client: Client) -> Result<()> {
    let Ok(_guard) = s.social.recovery_lock.try_lock() else {
        return Ok(());
    };
    let key = identity::keys(&s)?.public_key();
    let cfg = identity::config(&s)?;
    for url in relays(&cfg["relays"])? {
        let mut until = None;
        let mut previous = None;
        let mut limit = 256;
        for _ in 0..32 {
            let mut filter = Filter::new().kind(Kind::GiftWrap).pubkey(key).limit(limit);
            if let Some(time) = until {
                filter = filter.until(Timestamp::from(time));
            }
            let events = match client
                .fetch_events_from([url.as_str()], filter, std::time::Duration::from_secs(10))
                .await
            {
                Ok(events) => events,
                Err(_) => break,
            };
            let count = events.len();
            let mut oldest = u64::MAX;
            for event in events.iter() {
                oldest = oldest.min(event.created_at.as_secs());
                let _ = receive(&s, &client, event).await;
            }
            if count < limit || count == 0 {
                break;
            }
            if previous == Some(oldest) {
                if limit >= 2048 {
                    break;
                }
                limit *= 2;
            } else {
                previous = Some(oldest);
                until = Some(oldest);
            }
        }
    }
    Ok(())
}
pub async fn status(s: &AppState) -> Result<Value> {
    let enabled = identity::config(s)?["enabled"] == true;
    let client = s.social.client.lock().await.clone();
    let mut diagnostics = vec![];
    let mut connected = 0;
    if let Some(ref client) = client {
        for (url, relay) in client.relays().await {
            let state = relay.status();
            if state == RelayStatus::Connected {
                connected += 1;
            }
            diagnostics.push(json!({"url":url.to_string(),"state":state.to_string()}));
        }
    }
    let acknowledged = s.social.last_relay_ack.load(Ordering::Relaxed) > 0;
    if connected > 0 && acknowledged {
        s.social.ever_connected.store(true, Ordering::Relaxed);
    }
    let phase = if !enabled {
        "Offline"
    } else if connected > 0 && acknowledged {
        "Connected"
    } else if s.social.ever_connected.load(Ordering::Relaxed) {
        "Reconnecting"
    } else if client.is_none() || db::now() - s.social.connect_started.load(Ordering::Relaxed) > 30
    {
        "Offline"
    } else {
        "Connecting"
    };
    Ok(
        json!({"enabled":enabled,"running":client.is_some(),"phase":phase,"lastAcknowledgment":s.social.last_relay_ack.load(Ordering::Relaxed),"gaming":s.social.gaming.load(Ordering::Relaxed),"diagnostics":diagnostics,"operators":crate::social_network::operators()}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn profile_cannot_inject_wallet_verification() {
        let key = Keys::generate().public_key();
        let mut p = json!({"id":key.to_hex(),"name":"Synthetic friend","bio":"","relays":["wss://relay.example"]});
        assert!(validate_public_profile(&p, key).is_ok());
        p["wallet"] = json!({"address":"0x0000000000000000000000000000000000000000","verifiedBy":"forged","expires":i64::MAX});
        assert!(validate_public_profile(&p, key).is_err());
    }
}
