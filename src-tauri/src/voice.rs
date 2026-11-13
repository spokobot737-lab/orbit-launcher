use crate::{db, identity, media, AppState, Shared};
use anyhow::{ensure, Context, Result};
use base64::{engine::general_purpose::STANDARD, Engine};
use nostr_sdk::{nostr::nips::nip44, prelude::*};
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex,
    },
    time::Duration,
};
use tauri::Emitter;

pub const SIGNAL: Kind = Kind::Custom(21080);
const LIFETIME: i64 = 35;
const MAX_CLIP: usize = 4 * 1024 * 1024;

#[derive(Default)]
pub struct Runtime {
    active: AtomicBool,
    seen: Mutex<HashMap<(PublicKey, String), i64>>,
}
pub fn active(s: &AppState) -> bool {
    s.voice.active.load(Ordering::Acquire)
}
fn validate(value: &Value) -> Result<()> {
    let id = value["call"].as_str().context("Missing call ID")?;
    ensure!(
        uuid::Uuid::parse_str(id)?.to_string() == id,
        "Invalid call ID"
    );
    match value["type"].as_str() {
        Some("invite" | "accept" | "decline" | "end") => {
            ensure!(value["data"].is_null(), "Unexpected call payload");
        }
        Some("offer" | "answer") => {
            let sdp = value["data"].as_str().context("Missing description")?;
            ensure!(
                sdp.len() <= 8000 && sdp.starts_with("v=0"),
                "Invalid description"
            );
        }
        Some("ice") => {
            let candidates = value["data"].as_array().context("Missing ICE candidates")?;
            ensure!(
                candidates.len() <= 12
                    && candidates.iter().all(|x| x["candidate"]
                        .as_str()
                        .is_some_and(|c| c.len() <= 512 && c.starts_with("candidate:"))
                        && x.to_string().len() <= 800),
                "Invalid ICE batch"
            );
        }
        _ => anyhow::bail!("Unsupported call signal"),
    }
    Ok(())
}

pub async fn send(s: &AppState, args: &Value) -> Result<Value> {
    let peer = args["peer"].as_str().context("Choose a friend")?;
    let contact = identity::contact(s, peer)?;
    ensure!(
        contact["state"] == "accepted",
        "Calls require an accepted friendship"
    );
    ensure!(identity::config(s)?["enabled"] == true, "Orbit is offline");
    let keys = identity::keys(s)?;
    let target = PublicKey::from_hex(peer)?;
    let mut value = json!({"call":args["call"],"type":args["type"],"data":args["data"]});
    validate(&value)?;
    value["nonce"] = json!(uuid::Uuid::new_v4());
    let payload = nip44::encrypt(
        keys.secret_key(),
        &target,
        value.to_string(),
        nip44::Version::V2,
    )?;
    let expires = Timestamp::from((db::now() + LIFETIME) as u64);
    let event = EventBuilder::new(SIGNAL, payload)
        .tags([Tag::public_key(target), Tag::expiration(expires)])
        .sign_with_keys(&keys)?;
    let urls = crate::social::relays(&contact["relays"])?;
    let client = s
        .social
        .client
        .lock()
        .await
        .clone()
        .context("Orbit is offline")?;
    let sent = tokio::time::timeout(Duration::from_secs(8), async {
        for url in &urls {
            client.add_relay(url).await?;
            let _ = client.connect_relay(url).await;
        }
        client.send_event_to(&urls, &event).await
    })
    .await
    .ok()
    .and_then(Result::ok);
    ensure!(
        sent.is_some_and(|r| !r.success.is_empty()),
        "Call signal could not reach a relay. Try again."
    );
    Ok(json!(true))
}

pub fn receive(
    s: &AppState,
    app: Option<&tauri::AppHandle>,
    event: &Event,
    since: i64,
) -> Result<()> {
    let now = db::now();
    let created = event.created_at.as_secs() as i64;
    ensure!(
        event.kind == SIGNAL
            && event.content.len() <= 18_000
            && created >= since
            && created <= now + 5
            && now - created < LIFETIME,
        "Stale call signal"
    );
    ensure!(
        event.tags.expiration().is_some_and(|t| {
            let expiry = t.as_secs() as i64;
            expiry > now && expiry <= created + LIFETIME + 1
        }),
        "Expired call signal"
    );
    event.verify()?;
    let peer = event.pubkey.to_hex();
    ensure!(
        identity::contact(s, &peer)?["state"] == "accepted",
        "Unknown caller"
    );
    let keys = identity::keys(s)?;
    ensure!(
        event.tags.public_keys().any(|k| *k == keys.public_key()),
        "Wrong call recipient"
    );
    let value: Value = serde_json::from_str(&nip44::decrypt(
        keys.secret_key(),
        &event.pubkey,
        &event.content,
    )?)?;
    validate(&value)?;
    let nonce = value["nonce"].as_str().context("Missing signal ID")?;
    uuid::Uuid::parse_str(nonce)?;
    let mut seen = s.voice.seen.lock().unwrap();
    seen.retain(|_, expires| *expires > now);
    let key = (event.pubkey, nonce.to_owned());
    if seen.contains_key(&key) {
        return Ok(());
    }
    ensure!(seen.len() < 2048, "Too many pending call signals");
    seen.insert(key, now + LIFETIME * 2);
    drop(seen);
    if let Some(app) = app {
        app.emit(
            "voice-signal",
            json!({"peer":peer,"call":value["call"],"type":value["type"],
            "data":value["data"],"expires":created + LIFETIME}),
        )?;
    }
    Ok(())
}

pub async fn prepare_clip(app: Option<tauri::AppHandle>, s: Shared, v: &Value) -> Result<Value> {
    let encoded = v["audio"].as_str().context("Missing recording")?;
    ensure!(
        encoded.len() <= MAX_CLIP.div_ceil(3) * 4,
        "Recording exceeds 4 MiB"
    );
    let bytes = STANDARD.decode(encoded)?;
    ensure!(
        !bytes.is_empty() && bytes.len() <= MAX_CLIP,
        "Invalid recording size"
    );
    let extension = match v["format"].as_str() {
        Some("webm") if bytes.starts_with(&[0x1a, 0x45, 0xdf, 0xa3]) => "webm",
        Some("ogg") if bytes.starts_with(b"OggS") => "ogg",
        Some("m4a") if bytes.get(4..8) == Some(b"ftyp") => "m4a",
        _ => anyhow::bail!("Unsupported recording format"),
    };
    let folder = s
        .root
        .join("voice-tmp")
        .join(uuid::Uuid::new_v4().to_string());
    tokio::fs::create_dir_all(&folder).await?;
    let path = folder.join(format!("Voice message.{extension}"));
    let result = async {
        tokio::fs::write(&path, bytes).await?;
        media::prepare(app, s, &json!({"path":path, "voice":true})).await
    }
    .await;
    let _ = tokio::fs::remove_file(path).await;
    let _ = tokio::fs::remove_dir(folder).await;
    result
}

pub async fn dispatch(app: &tauri::AppHandle, s: Shared, op: &str, v: &Value) -> Result<Value> {
    match op {
        "orbit_voice_send" => send(&s, v).await,
        "orbit_voice_active" => {
            s.voice.active.store(v["active"] == true, Ordering::Release);
            Ok(json!(true))
        }
        "orbit_voice_exit" => {
            s.voice.active.store(false, Ordering::Release);
            app.exit(0);
            Ok(json!(true))
        }
        "orbit_voice_health" => {
            let peer = v["peer"].as_str().context("Choose a friend")?;
            let status = crate::social::status(&s).await?;
            Ok(json!(
                identity::contact(&s, peer)?["state"] == "accepted"
                    && status["enabled"] == true
                    && status["phase"] != "Offline"
            ))
        }
        "orbit_voice_prepare" => prepare_clip(Some(app.clone()), s, v).await,
        "orbit_voice_discard" => media::discard_voice(&s, v),
        _ => anyhow::bail!("Unknown voice operation"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn old_or_replayed_calls_are_rejected_without_chat_rows() -> Result<()> {
        let root = std::env::temp_dir().join(format!("orbit-voice-{}", uuid::Uuid::new_v4()));
        let s = crate::social_tests::state(root)?;
        identity::save(&s, &json!({"name":"Test receiver"}))?;
        let receiver = identity::keys(&s)?;
        let caller = Keys::generate();
        identity::put_contact(
            &s,
            &json!({"id":caller.public_key().to_hex(),"name":"Test caller","relays":["wss://relay.example"]}),
            "accepted",
        )?;
        let now = db::now();
        let call =
            json!({"type":"invite","call":uuid::Uuid::new_v4(),"nonce":uuid::Uuid::new_v4()});
        let event = |created: i64| -> Result<Event> {
            let encrypted = nip44::encrypt(
                caller.secret_key(),
                &receiver.public_key(),
                call.to_string(),
                nip44::Version::V2,
            )?;
            Ok(EventBuilder::new(SIGNAL, encrypted)
                .custom_created_at(Timestamp::from(created as u64))
                .tags([
                    Tag::public_key(receiver.public_key()),
                    Tag::expiration(Timestamp::from((created + LIFETIME) as u64)),
                ])
                .sign_with_keys(&caller)?)
        };
        assert!(receive(&s, None, &event(now - 40)?, now - 60).is_err());
        assert!(receive(&s, None, &event(now - 2)?, now - 1).is_err());
        receive(&s, None, &event(now)?, now)?;
        receive(&s, None, &event(now)?, now)?;
        assert_eq!(s.voice.seen.lock().unwrap().len(), 1);
        let db = s.db.lock().unwrap();
        for table in ["messages", "outbox"] {
            let rows: i64 = db.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                row.get(0)
            })?;
            assert_eq!(rows, 0);
        }
        drop(db);
        crate::secure::remove(&s, "nostr-key")?;
        crate::secure::remove(&s, "data-key")?;
        Ok(())
    }
}
