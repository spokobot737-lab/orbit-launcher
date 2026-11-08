//! Friends-only signed snapshots. Image keys remain in protected local records and encrypted DMs.
use crate::{db, identity, media, secure, social, AppState, Shared};
use anyhow::{ensure, Context, Result};
use base64::Engine;
use nostr_sdk::prelude::*;
use rusqlite::{params, OptionalExtension};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

pub fn mark_dirty(s: &AppState) -> Result<()> {
    let mut c = s.db.lock().unwrap();
    let tx = c.transaction()?;
    let raw: Option<String> = tx
        .query_row(
            "SELECT data FROM orbit_profile WHERE singleton=1",
            [],
            |r| r.get(0),
        )
        .optional()?;
    let Some(raw) = raw else { return Ok(()) };
    let mut p: Value = serde_json::from_str(&raw)?;
    let previous: i64 = tx
        .query_row(
            "SELECT revision FROM profile_jobs WHERE singleton=1",
            [],
            |r| r.get(0),
        )
        .optional()?
        .unwrap_or(0);
    let revision = (previous.max(p["revision"].as_i64().unwrap_or(0)) + 1).max(db::now() * 1000);
    p["revision"] = json!(revision);
    tx.execute(
        "UPDATE orbit_profile SET data=? WHERE singleton=1",
        [p.to_string()],
    )?;
    tx.execute("INSERT INTO profile_jobs(singleton,revision,status) VALUES(1,?,'pending') ON CONFLICT(singleton) DO UPDATE SET revision=excluded.revision,status='pending',error=NULL,attempts=0,next_try=0",[revision])?;
    tx.commit()?;
    Ok(())
}
pub fn retry(s: &AppState) -> Result<Value> {
    s.db.lock().unwrap().execute("UPDATE profile_jobs SET status='pending',attempts=0,next_try=0,error=NULL WHERE singleton=1",[])?;
    Ok(json!(true))
}
pub fn retry_contact(s: &AppState, v: &Value) -> Result<Value> {
    let mut contact = identity::contact(s, v["peer"].as_str().context("Choose a friend")?)?;
    ensure!(
        contact["state"] == "accepted",
        "Accept this friendship first"
    );
    contact["imageAttempts"] = json!(0);
    contact["imageRetryAt"] = json!(0);
    identity::put_contact(s, &contact, "accepted")?;
    s.db.lock().unwrap().execute(
        "INSERT OR REPLACE INTO profile_downloads VALUES(?,0,0)",
        [v["peer"].as_str().unwrap()],
    )?;
    Ok(json!(true))
}
pub fn status(s: &AppState) -> Result<Value> {
    let row: Option<(i64, String, Option<String>)> =
        s.db.lock()
            .unwrap()
            .query_row(
                "SELECT revision,status,error FROM profile_jobs WHERE singleton=1",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()?;
    let Some((revision, state, error)) = row else {
        return Ok(json!({"state":"saved","label":"Saved locally"}));
    };
    let (pending,failed):(i64,i64)=s.db.lock().unwrap().query_row("SELECT COALESCE(SUM(m.state IN('queued')),0),COALESCE(SUM(m.state='failed'),0) FROM profile_deliveries p JOIN messages m ON p.message_id=m.id JOIN contacts c ON c.pubkey=p.peer WHERE p.revision=? AND c.state='accepted'",[revision],|r|Ok((r.get(0)?,r.get(1)?)))?;
    let state = if state == "failed" || failed > 0 {
        "failed"
    } else if state != "ready" || pending > 0 {
        "pending"
    } else {
        "synced"
    };
    Ok(
        json!({"revision":revision,"state":state,"error":error,"label":match state{"synced"=>"Saved and synced","failed"=>"Could not sync — Retry",_=>"Saved locally — waiting to sync"}}),
    )
}
pub fn shared(s: &AppState) -> Result<Option<Value>> {
    let raw: Option<String> =
        s.db.lock()
            .unwrap()
            .query_row(
                "SELECT snapshot FROM profile_jobs WHERE singleton=1 AND snapshot IS NOT NULL",
                [],
                |r| r.get(0),
            )
            .optional()?;
    raw.map(|raw| Ok(serde_json::from_str(&secure::unseal(s, &raw)?)?))
        .transpose()
}
pub fn validate(p: &Value, author: PublicKey) -> Result<()> {
    let revision = p["revision"].as_i64().unwrap_or(0);
    if revision > 0 {
        let mut unsigned = p.clone();
        unsigned
            .as_object_mut()
            .context("Invalid profile")?
            .remove("proof");
        let mut raw = p["proof"].clone();
        if raw["content"].is_null() {
            raw["content"] =
                json!(json!({"schema":"orbit-profile","version":1,"profile":unsigned}).to_string());
        }
        let proof: Event = serde_json::from_value(raw).context("Signed profile proof missing")?;
        proof.verify()?;
        ensure!(
            proof.pubkey == author && proof.kind == Kind::Custom(30078),
            "Profile author mismatch"
        );
        let payload: Value = serde_json::from_str(&proof.content)?;
        ensure!(
            payload["schema"] == "orbit-profile"
                && payload["version"] == 1
                && payload["profile"] == unsigned,
            "Signed profile content or schema mismatch"
        );
    }
    for kind in ["avatar", "banner"] {
        let field = format!("{kind}Ref");
        let image = &p[&field];
        if !image.is_null() {
            ensure!(
                image["type"] == "image/jpeg"
                    && image["size"].as_u64().is_some_and(|x| x <= 2_000_000),
                "Invalid profile image metadata"
            );
            uuid::Uuid::parse_str(image["id"].as_str().context("Image ID missing")?)?;
            crate::social_network::retrieval_allowed(
                image["provider"]
                    .as_str()
                    .context("Image provider missing")?,
                image["url"].as_str().context("Image URL missing")?,
            )?;
        }
    }
    Ok(())
}
pub fn cache(s: &AppState, p: &Value, state: &str) -> Result<bool> {
    let peer = p["id"].as_str().context("Profile identity missing")?;
    let old = identity::contact(s, peer).unwrap_or(Value::Null);
    let revision = p["revision"].as_i64().unwrap_or(0);
    if !old.is_null() && revision > 0 && revision <= old["revision"].as_i64().unwrap_or(0) {
        if old["state"] != state {
            identity::put_contact(s, &old, state)?;
        }
        return Ok(false);
    }
    if old["revision"].as_i64().unwrap_or(0) > 0 && revision == 0 {
        return Ok(false);
    }
    let protected = secure::seal(s, &p.to_string())?;
    let mut public = p.clone();
    let obj = public.as_object_mut().context("Invalid profile")?;
    obj.remove("proof");
    let mut pending = false;
    for kind in ["avatar", "banner"] {
        let field = format!("{kind}Ref");
        if let Some(image) = obj.get_mut(&field).and_then(Value::as_object_mut) {
            image.remove("key");
            pending = true;
            if let Some(previous) = old[kind].as_str() {
                obj.insert(kind.into(), json!(previous));
            }
        }
    }
    if !old["wallet"].is_null() {
        public["wallet"] = old["wallet"].clone();
    }
    public["imagesPending"] = json!(pending);
    identity::put_contact(s, &public, state)?;
    s.db.lock().unwrap().execute(
        "INSERT OR REPLACE INTO profile_snapshots VALUES(?,?,?)",
        params![peer, revision, protected],
    )?;
    if pending {
        s.db.lock().unwrap().execute(
            "INSERT OR REPLACE INTO profile_downloads VALUES(?,0,0)",
            [peer],
        )?;
    } else {
        s.db.lock()
            .unwrap()
            .execute("DELETE FROM profile_downloads WHERE peer=?", [peer])?;
    }
    Ok(true)
}
async fn image_ref(s: Shared, kind: &str, source: &str) -> Result<Value> {
    if source.is_empty() {
        return Ok(Value::Null);
    }
    let source_hash = format!("{:x}", Sha256::digest(source.as_bytes()));
    let old: Option<(String, String)> =
        s.db.lock()
            .unwrap()
            .query_row(
                "SELECT source_hash,media_id FROM profile_assets WHERE kind=?",
                [kind],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
    if let Some((old_hash, id)) = old {
        if old_hash == source_hash {
            if let Ok(value) = media::load(&s, &id) {
                if value["state"] == "uploaded" {
                    return Ok(portable(value));
                }
            }
        }
    }
    let (_, encoded) = source
        .split_once(";base64,")
        .context("Unsupported profile image. Choose a JPEG, PNG or WebP image again.")?;
    let bytes = base64::engine::general_purpose::STANDARD.decode(encoded)?;
    let path = s
        .root
        .join("cache/profile-inputs")
        .join(format!("{source_hash}.jpg"));
    tokio::fs::create_dir_all(path.parent().unwrap()).await?;
    tokio::fs::write(&path, bytes).await?;
    let prepared = media::prepare(None, s.clone(), &json!({"path":path})).await?;
    let _ = tokio::fs::remove_file(&path).await;
    media::upload(None, s.clone(), &json!({"id":prepared["id"]})).await?;
    let id = prepared["id"].as_str().context("Prepared image missing")?;
    let value = media::load(&s, id)?;
    s.db.lock().unwrap().execute(
        "INSERT OR REPLACE INTO profile_assets VALUES(?,?,?)",
        params![kind, source_hash, id],
    )?;
    Ok(portable(value))
}
fn portable(mut value: Value) -> Value {
    if let Some(o) = value.as_object_mut() {
        for key in ["source", "encryptedPath"] {
            o.remove(key);
        }
    }
    value
}
async fn build(s: Shared, own: &Value) -> Result<Value> {
    let mut p = json!({"id":own["id"],"name":own["name"],"bio":own["bio"],"revision":own["revision"],"relays":identity::config(&s)?["relays"],"connections":crate::connections::shared(&s)?});
    for kind in ["avatar", "banner"] {
        p[format!("{kind}Ref")] =
            image_ref(s.clone(), kind, own[kind].as_str().unwrap_or("")).await?;
    }
    let event = EventBuilder::new(
        Kind::Custom(30078),
        json!({"schema":"orbit-profile","version":1,"profile":p}).to_string(),
    )
    .sign_with_keys(&identity::keys(&s)?)?;
    let mut proof = serde_json::to_value(event)?;
    proof.as_object_mut().unwrap().remove("content");
    p["proof"] = proof;
    ensure!(p.to_string().len()+600<=16_000,"Shared profile links are too long to sync. Shorten profile links or your bio and save again.");
    Ok(p)
}
pub async fn work(app: Option<tauri::AppHandle>, s: Shared) -> Result<()> {
    let _lock = s.social.profile_lock.lock().await;
    if identity::config(&s)?["enabled"] != true {
        return Ok(());
    }
    let job:Option<(i64,i64)>=s.db.lock().unwrap().query_row("SELECT revision,attempts FROM profile_jobs WHERE singleton=1 AND status IN('pending','syncing') AND attempts<5 AND next_try<=?",[db::now()],|r|Ok((r.get(0)?,r.get(1)?))).optional()?;
    if let Some((revision, attempts)) = job {
        s.db.lock().unwrap().execute(
            "UPDATE profile_jobs SET status='syncing' WHERE revision=?",
            [revision],
        )?;
        let own = identity::profile(&s)?;
        let result = build(s.clone(), &own).await;
        match result {
            Ok(profile) => {
                if identity::profile(&s)?["revision"] == revision {
                    for contact in identity::contacts(&s)?.as_array().unwrap() {
                        if contact["state"] == "accepted" {
                            let peer = contact["id"].as_str().context("Contact ID missing")?;
                            let message = social::queue(
                                &s,
                                peer,
                                "profile_update",
                                json!({"profile":profile}),
                                true,
                            )
                            .await?;
                            s.db.lock().unwrap().execute(
                                "INSERT OR REPLACE INTO profile_deliveries VALUES(?,?,?)",
                                params![peer, revision, message],
                            )?;
                        }
                    }
                    s.db.lock().unwrap().execute("UPDATE profile_jobs SET status='ready',error=NULL,snapshot=? WHERE revision=?",params![secure::seal(&s,&profile.to_string())?,revision])?;
                }
            }
            Err(e) => {
                s.db.lock().unwrap().execute("UPDATE profile_jobs SET status=?,error=?,attempts=attempts+1,next_try=? WHERE revision=?",params![if attempts>=4{"failed"}else{"pending"},e.to_string(),db::now()+(15_i64<<attempts).min(600),revision])?;
            }
        }
        social::notify(app.as_ref());
    }
    hydrate(app, s.clone()).await?;
    Ok(())
}
async fn hydrate(app: Option<tauri::AppHandle>, s: Shared) -> Result<()> {
    let peers: Vec<String> = {
        let c = s.db.lock().unwrap();
        let mut q=c.prepare("SELECT p.peer FROM profile_downloads p JOIN contacts c ON c.pubkey=p.peer WHERE c.state='accepted' AND p.attempts<5 AND p.next_try<=? LIMIT 8")?;
        let rows = q
            .query_map([db::now()], |r| r.get(0))?
            .collect::<Result<Vec<_>, _>>()?;
        rows
    };
    for peer in peers {
        let contact = identity::contact(&s, &peer)?;
        let raw: String = s.db.lock().unwrap().query_row(
            "SELECT event FROM profile_snapshots WHERE peer=?",
            [&peer],
            |r| r.get(0),
        )?;
        let profile: Value = serde_json::from_str(&secure::unseal(&s, &raw)?)?;
        let mut cached = contact.clone();
        let mut complete = true;
        for kind in ["avatar", "banner"] {
            let image = &profile[format!("{kind}Ref")];
            if image.is_null() {
                cached[kind] = json!("");
                continue;
            }
            match media::retrieve_reference(app.clone(), s.clone(), image.clone()).await {
                Ok(v) => {
                    let bytes =
                        tokio::fs::read(v["path"].as_str().context("Verified image missing")?)
                            .await?;
                    cached[kind] = json!(format!(
                        "data:image/jpeg;base64,{}",
                        base64::engine::general_purpose::STANDARD.encode(bytes)
                    ));
                }
                Err(_error) => {
                    #[cfg(test)]
                    eprintln!("Synthetic profile image failed: {_error:#}");
                    complete = false;
                }
            }
        }
        if identity::contact(&s, &peer)?["revision"] == contact["revision"] {
            cached["imagesPending"] = json!(!complete);
            cached["imageAttempts"] = json!(if complete {
                0
            } else {
                contact["imageAttempts"].as_i64().unwrap_or(0) + 1
            });
            cached["imageRetryAt"] = json!(
                db::now() + (15_i64 << contact["imageAttempts"].as_i64().unwrap_or(0).min(5))
            );
            identity::put_contact(&s, &cached, "accepted")?;
            if complete {
                s.db.lock()
                    .unwrap()
                    .execute("DELETE FROM profile_downloads WHERE peer=?", [&peer])?;
            } else {
                s.db.lock().unwrap().execute(
                    "UPDATE profile_downloads SET attempts=attempts+1,next_try=? WHERE peer=?",
                    params![
                        cached["imageRetryAt"].as_i64().unwrap_or(db::now() + 60),
                        peer
                    ],
                )?;
            }
            social::notify(app.as_ref());
        }
    }
    Ok(())
}
pub fn schedule(app: Option<tauri::AppHandle>, s: Shared) {
    tokio::spawn(async move {
        let _result = work(app, s).await;
        #[cfg(test)]
        if let Err(error) = _result {
            eprintln!("Synthetic profile worker failed: {error:#}");
        }
    });
}
