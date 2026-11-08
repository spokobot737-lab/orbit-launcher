use crate::{db, secure, AppState};
use anyhow::{ensure, Context, Result};
use nostr_sdk::prelude::*;
use rusqlite::{params, OptionalExtension};
use serde_json::{json, Value};
pub fn init(s: &AppState) -> Result<()> {
    s.db.lock().unwrap().execute_batch("CREATE TABLE IF NOT EXISTS orbit_profile(singleton INTEGER PRIMARY KEY CHECK(singleton=1),data TEXT NOT NULL); CREATE TABLE IF NOT EXISTS social_config(singleton INTEGER PRIMARY KEY CHECK(singleton=1),data TEXT NOT NULL); CREATE TABLE IF NOT EXISTS contacts(pubkey TEXT PRIMARY KEY,name TEXT NOT NULL,state TEXT NOT NULL,data TEXT NOT NULL); CREATE TABLE IF NOT EXISTS messages(id TEXT PRIMARY KEY,peer TEXT NOT NULL,direction TEXT NOT NULL,kind TEXT NOT NULL,body TEXT NOT NULL,state TEXT NOT NULL,created INTEGER NOT NULL,error TEXT); CREATE INDEX IF NOT EXISTS messages_peer_time ON messages(peer,created); CREATE TABLE IF NOT EXISTS outbox(id TEXT PRIMARY KEY,event TEXT NOT NULL,relays TEXT NOT NULL,attempts INTEGER NOT NULL DEFAULT 0,next_try INTEGER NOT NULL DEFAULT 0); CREATE TABLE IF NOT EXISTS social_seen(id TEXT PRIMARY KEY,seen INTEGER NOT NULL); CREATE TABLE IF NOT EXISTS siwe_challenges(nonce TEXT PRIMARY KEY,peer TEXT NOT NULL,message TEXT NOT NULL,expires INTEGER NOT NULL,used INTEGER NOT NULL DEFAULT 0); CREATE TABLE IF NOT EXISTS connections(provider TEXT PRIMARY KEY,data TEXT NOT NULL); CREATE TABLE IF NOT EXISTS media(id TEXT PRIMARY KEY,data TEXT NOT NULL); CREATE TABLE IF NOT EXISTS friend_activity(peer TEXT NOT NULL,kind TEXT NOT NULL,data TEXT NOT NULL,expires INTEGER NOT NULL,PRIMARY KEY(peer,kind));")?;
    s.db.lock().unwrap().execute_batch("CREATE TABLE IF NOT EXISTS storage_health(service TEXT PRIMARY KEY,until INTEGER NOT NULL,status INTEGER NOT NULL,min_size INTEGER NOT NULL,reason TEXT NOT NULL); CREATE TABLE IF NOT EXISTS profile_jobs(singleton INTEGER PRIMARY KEY CHECK(singleton=1),revision INTEGER NOT NULL,status TEXT NOT NULL,error TEXT,attempts INTEGER NOT NULL DEFAULT 0,next_try INTEGER NOT NULL DEFAULT 0,snapshot TEXT); CREATE TABLE IF NOT EXISTS profile_assets(kind TEXT PRIMARY KEY,source_hash TEXT NOT NULL,media_id TEXT NOT NULL); CREATE TABLE IF NOT EXISTS profile_snapshots(peer TEXT PRIMARY KEY,revision INTEGER NOT NULL,event TEXT NOT NULL); CREATE TABLE IF NOT EXISTS profile_downloads(peer TEXT PRIMARY KEY,next_try INTEGER NOT NULL DEFAULT 0,attempts INTEGER NOT NULL DEFAULT 0); CREATE TABLE IF NOT EXISTS profile_deliveries(peer TEXT PRIMARY KEY,revision INTEGER NOT NULL,message_id TEXT NOT NULL);")?;
    Ok(())
}
pub fn profile(s: &AppState) -> Result<Value> {
    let raw: Option<String> =
        s.db.lock()
            .unwrap()
            .query_row(
                "SELECT data FROM orbit_profile WHERE singleton=1",
                [],
                |r| r.get(0),
            )
            .optional()?;
    Ok(raw
        .map(|x| serde_json::from_str(&x))
        .transpose()?
        .unwrap_or(Value::Null))
}
pub fn keys(s: &AppState) -> Result<Keys> {
    Keys::parse(&secure::get(s, "nostr-key")?).map_err(Into::into)
}
pub fn profile_image(v: &Value) -> Result<Value> {
    use base64::Engine;
    let path = std::path::Path::new(v["path"].as_str().context("Choose an image")?);
    ensure!(path.is_absolute() && path.is_file(), "Image is missing");
    ensure!(
        std::fs::metadata(path)?.len() <= 32 * 1024 * 1024,
        "Image input limit is 32 MiB"
    );
    let mut reader = ::image::ImageReader::open(path)?.with_guessed_format()?;
    let mut limits = ::image::Limits::default();
    limits.max_alloc = Some(128 * 1024 * 1024);
    limits.max_image_width = Some(16000);
    limits.max_image_height = Some(16000);
    reader.limits(limits);
    let (width, height) = if v["banner"] == true {
        (1600, 600)
    } else {
        (512, 512)
    };
    let img =
        reader
            .decode()?
            .resize_to_fill(width, height, ::image::imageops::FilterType::Triangle);
    let mut bytes = vec![];
    ::image::codecs::jpeg::JpegEncoder::new_with_quality(&mut bytes, 85).encode_image(&img)?;
    ensure!(
        bytes.len() <= 1_400_000,
        "Image is too large after resizing"
    );
    Ok(json!(format!(
        "data:image/jpeg;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(bytes)
    )))
}
pub fn save(s: &AppState, v: &Value) -> Result<Value> {
    let old = profile(s)?;
    let name = v["name"].as_str().unwrap_or("").trim();
    ensure!(
        !name.is_empty() && name.chars().count() <= 60,
        "Choose a display name of up to 60 characters"
    );
    let bio = v["bio"].as_str().unwrap_or("");
    ensure!(bio.len() <= 2000, "Bio is too long");
    let mut p = json!({"name":name,"bio":bio,"gameVisibility":v["gameVisibility"].as_bool().unwrap_or(false),"musicVisibility":v["musicVisibility"].as_bool().unwrap_or(false)});
    for k in ["avatar", "banner"] {
        let x = v[k].as_str().unwrap_or("");
        ensure!(
            x.is_empty() || (x.starts_with("data:image/") && x.len() <= 2_000_000),
            "Profile images must be locally selected images under 1.5 MB"
        );
        p[k] = json!(x);
    }
    if old.is_null() {
        let keys = Keys::generate();
        secure::set(s, "nostr-key", &keys.secret_key().to_secret_hex())?;
        secure::create_data_key(s)?;
        p["id"] = json!(keys.public_key().to_hex());
        p["npub"] = json!(keys.public_key().to_bech32()?);
        p["created"] = json!(db::now());
    } else {
        for k in ["id", "npub", "created", "revision"] {
            p[k] = old[k].clone();
        }
        ensure!(
            keys(s)?.public_key().to_hex() == p["id"],
            "Communication key does not match this profile"
        );
    }
    s.db.lock().unwrap().execute(
        "INSERT OR REPLACE INTO orbit_profile VALUES(1,?)",
        [p.to_string()],
    )?;
    crate::social_network::initialize(s)?;
    crate::profile_sync::mark_dirty(s)?;
    profile(s)
}
pub fn config(s: &AppState) -> Result<Value> {
    crate::social_network::config(s)
}
pub fn validate_endpoint(raw: &str, relay: bool) -> Result<url::Url> {
    let u = url::Url::parse(raw)?;
    let host = u.host_str().context("Service host missing")?;
    let local = host == "127.0.0.1" || host == "[::1]" || host == "localhost";
    ensure!(
        (u.scheme() == if relay { "wss" } else { "https" })
            || (local && u.scheme() == if relay { "ws" } else { "http" }),
        "Use a secure service URL; unencrypted URLs are allowed only for loopback tests"
    );
    ensure!(
        u.username().is_empty() && u.password().is_none() && u.fragment().is_none(),
        "Service URL must not contain credentials or a fragment"
    );
    ensure!(
        local
            || (!host.parse::<std::net::IpAddr>().is_ok_and(|ip| match ip {
                std::net::IpAddr::V4(a) =>
                    a.is_private() || a.is_link_local() || a.is_unspecified(),
                std::net::IpAddr::V6(a) => a.is_loopback() || a.is_unspecified(),
            }) && !host.ends_with(".local")),
        "Private network endpoints are not supported"
    );
    Ok(u)
}
pub fn save_config(s: &AppState, v: &Value) -> Result<Value> {
    crate::social_network::save(s, v)
}
pub fn contacts(s: &AppState) -> Result<Value> {
    let c = s.db.lock().unwrap();
    let mut q = c.prepare("SELECT data,state FROM contacts ORDER BY name")?;
    let rows = q.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
    let mut out = vec![];
    for r in rows {
        let (raw, state) = r?;
        let mut v: Value = serde_json::from_str(&raw)?;
        v["state"] = json!(state);
        out.push(v);
    }
    Ok(json!(out))
}
pub fn contact(s: &AppState, pubkey: &str) -> Result<Value> {
    let row: (String, String) = s.db.lock().unwrap().query_row(
        "SELECT data,state FROM contacts WHERE pubkey=?",
        [pubkey],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    let mut v: Value = serde_json::from_str(&row.0)?;
    v["state"] = json!(row.1);
    Ok(v)
}
pub fn put_contact(s: &AppState, v: &Value, state: &str) -> Result<()> {
    let pk = v["id"].as_str().context("Contact identity missing")?;
    PublicKey::from_hex(pk)?;
    let mut data = v.clone();
    data.as_object_mut()
        .context("Invalid contact")?
        .remove("state");
    s.db.lock().unwrap().execute("INSERT INTO contacts VALUES(?,?,?,?) ON CONFLICT(pubkey) DO UPDATE SET name=excluded.name,state=excluded.state,data=excluded.data",params![pk,v["name"].as_str().unwrap_or("Orbit user"),state,data.to_string()])?;
    Ok(())
}
pub fn block(s: &AppState, v: &Value) -> Result<Value> {
    let id = v["id"].as_str().context("Contact missing")?;
    let mut c = s.db.lock().unwrap();
    let t = c.transaction()?;
    t.execute(
        "UPDATE contacts SET state=? WHERE pubkey=?",
        params![
            if v["blocked"] == true {
                "blocked"
            } else {
                "removed"
            },
            id
        ],
    )?;
    t.execute(
        "DELETE FROM outbox WHERE substr(id,1,36) IN(SELECT id FROM messages WHERE peer=?)",
        [id],
    )?;
    t.execute("DELETE FROM friend_activity WHERE peer=?", [id])?;
    t.commit()?;
    Ok(json!(true))
}
pub fn messages(s: &AppState, v: &Value) -> Result<Value> {
    let peer = v["peer"].as_str().context("Choose a conversation")?;
    let before = v["before"].as_i64().unwrap_or(i64::MAX);
    let before_id = v["beforeId"].as_str().unwrap_or("");
    let raw = {
        let c = s.db.lock().unwrap();
        let mut q=c.prepare("SELECT id,direction,kind,body,state,created,error FROM messages WHERE peer=? AND kind IN('message','wallet_challenge','wallet_proof') AND (created<? OR (created=? AND id<?)) ORDER BY created DESC,id DESC LIMIT 60")?;
        let rows = q
            .query_map(params![peer, before, before, before_id], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, String>(3)?,
                    r.get::<_, String>(4)?,
                    r.get::<_, i64>(5)?,
                    r.get::<_, Option<String>>(6)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        rows
    };
    let mut out = vec![];
    for (id, direction, kind, body, state, created, error) in raw {
        let mut v: Value = serde_json::from_str(&secure::unseal(s, &body)?)?;
        if let Some(a) = v["attachments"].as_array_mut() {
            for f in a {
                if let Some(o) = f.as_object_mut() {
                    o.remove("key");
                }
            }
        }
        out.push(json!({"id":id,"direction":direction,"kind":kind,"body":v,"state":state,"created":created,"error":error}));
    }
    out.reverse();
    Ok(json!(out))
}
