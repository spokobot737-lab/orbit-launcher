use crate::{db, identity, secure, AppState};
use anyhow::{ensure, Context, Result};
use k256::{ecdsa::SigningKey, elliptic_curve::rand_core::OsRng};
use rusqlite::{params, OptionalExtension};
use serde_json::{json, Value};
use sha3::{Digest, Keccak256};
use time::{format_description::well_known::Rfc3339, OffsetDateTime};
fn address(key: &SigningKey) -> String {
    let point = key.verifying_key().to_encoded_point(false);
    let hash = Keccak256::digest(&point.as_bytes()[1..]);
    checksum(&hex::encode(&hash[12..]))
}
fn checksum(lower: &str) -> String {
    let lower = lower.to_ascii_lowercase();
    let hash = hex::encode(Keccak256::digest(lower.as_bytes()));
    let mut address = String::from("0x");
    for (c, n) in lower.chars().zip(hash.chars()) {
        address.push(
            if c.is_ascii_alphabetic() && n.to_digit(16).unwrap_or(0) >= 8 {
                c.to_ascii_uppercase()
            } else {
                c
            },
        );
    }
    address
}
pub fn info(s: &AppState) -> Result<Value> {
    let raw: Option<String> =
        s.db.lock()
            .unwrap()
            .query_row(
                "SELECT data FROM connections WHERE provider='wallet'",
                [],
                |r| r.get(0),
            )
            .optional()?;
    Ok(raw
        .map(|x| serde_json::from_str(&x))
        .transpose()?
        .unwrap_or(Value::Null))
}
pub fn create(s: &AppState) -> Result<Value> {
    ensure!(
        !identity::profile(s)?.is_null(),
        "Create your Orbit profile first"
    );
    ensure!(
        info(s)?.is_null(),
        "Disconnect the existing wallet first; keep its backup safe"
    );
    let key = SigningKey::random(&mut OsRng);
    secure::set(s, "evm-wallet", &hex::encode(key.to_bytes()))?;
    let v = json!({"address":address(&key),"mode":"local","verified":"this-device","chainId":1,"balance":"not queried","created":db::now()});
    s.db.lock().unwrap().execute(
        "INSERT OR REPLACE INTO connections VALUES('wallet',?)",
        [v.to_string()],
    )?;
    Ok(v)
}
pub fn disconnect(s: &AppState) -> Result<Value> {
    secure::remove(s, "evm-wallet")?;
    s.db.lock()
        .unwrap()
        .execute("DELETE FROM connections WHERE provider='wallet'", [])?;
    Ok(json!(true))
}
pub fn issue(s: &AppState, peer: &str, address: &str, domain: &str, uri: &str) -> Result<String> {
    let raw_address = address
        .strip_prefix("0x")
        .context("Use an EVM address beginning with 0x")?;
    ensure!(
        raw_address.len() == 40 && raw_address.bytes().all(|b| b.is_ascii_hexdigit()),
        "Invalid EVM address"
    );
    let address = checksum(raw_address);
    let nonce = uuid::Uuid::new_v4().simple().to_string();
    let now = OffsetDateTime::now_utc();
    let expires = now + time::Duration::minutes(3);
    let me = identity::keys(s)?.public_key().to_hex();
    let raw=format!("{domain} wants you to sign in with your Ethereum account:\n{address}\n\nVerify Orbit communication identity {peer}. No transaction or spending permission.\n\nURI: {uri}\nVersion: 1\nChain ID: 1\nNonce: {nonce}\nIssued At: {}\nExpiration Time: {}\nResources:\n- urn:orbit:identity:{peer}\n- urn:orbit:verifier:{me}",now.format(&Rfc3339)?,expires.format(&Rfc3339)?);
    let _: siwe::Message = raw
        .parse()
        .context("Invalid wallet address or SIWE request")?;
    s.db.lock().unwrap().execute(
        "INSERT INTO siwe_challenges VALUES(?,?,?,?,0)",
        params![nonce, peer, raw, expires.unix_timestamp()],
    )?;
    Ok(raw)
}
pub async fn verify(
    s: &AppState,
    peer: &str,
    raw: &str,
    signature: &str,
    domain: &str,
    uri: &str,
) -> Result<String> {
    let msg: siwe::Message = raw.parse()?;
    ensure!(
        msg.uri.to_string() == uri
            && msg.domain.to_string() == domain
            && msg.chain_id == 1
            && msg.expiration_time.is_some(),
        "SIWE audience, chain, or expiration mismatch"
    );
    let challenge: Option<(String, String, i64, i64)> =
        s.db.lock()
            .unwrap()
            .query_row(
                "SELECT peer,message,expires,used FROM siwe_challenges WHERE nonce=?",
                [&msg.nonce],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )
            .optional()?;
    let (p, expected, expires, used) = challenge.context("Unknown SIWE challenge")?;
    ensure!(
        p == peer && expected == raw && expires > db::now() && used == 0,
        "SIWE challenge expired, consumed, or belongs to another identity"
    );
    let bytes = hex::decode(signature.strip_prefix("0x").unwrap_or(signature))?;
    msg.verify(
        &bytes,
        &siwe::VerificationOpts {
            domain: Some(domain.parse()?),
            nonce: Some(msg.nonce.clone()),
            ..Default::default()
        },
    )
    .await
    .context("Wallet signature is invalid")?;
    let changed = s.db.lock().unwrap().execute(
        "UPDATE siwe_challenges SET used=1 WHERE nonce=? AND used=0 AND expires>?",
        params![msg.nonce, db::now()],
    )?;
    ensure!(changed == 1, "SIWE challenge was already consumed");
    Ok(checksum(&hex::encode(msg.address)))
}
pub fn sign_local(s: &AppState, raw: &str) -> Result<String> {
    let wallet = info(s)?;
    ensure!(
        wallet["mode"] == "local",
        "This wallet signs through its browser extension"
    );
    let key = SigningKey::from_slice(&hex::decode(secure::get(s, "evm-wallet")?)?)?;
    let msg: siwe::Message = raw.parse()?;
    ensure!(
        format!("0x{}", hex::encode(msg.address)).eq_ignore_ascii_case(&address(&key)),
        "Challenge requests another wallet"
    );
    let mut digest = Keccak256::new();
    digest.update(msg.eip191_bytes()?);
    let (sig, recid) = key.sign_digest_recoverable(digest)?;
    let mut bytes = sig.to_bytes().to_vec();
    bytes.push(recid.to_byte() + 27);
    Ok(format!("0x{}", hex::encode(bytes)))
}
pub async fn browser_connect(s: crate::Shared) -> Result<Value> {
    use axum::{
        extract::State,
        response::Html,
        routing::{get, post},
        Json, Router,
    };
    use std::sync::Arc;
    use tokio::sync::Mutex;
    ensure!(info(&s)?.is_null(), "Disconnect your current wallet first");
    let me = identity::keys(&s)?.public_key().to_hex();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let domain = format!("127.0.0.1:{}", listener.local_addr()?.port());
    let uri = format!("http://{domain}/wallet");
    let csrf = uuid::Uuid::new_v4().simple().to_string();
    let (tx, rx) = tokio::sync::oneshot::channel();
    let result = Arc::new(Mutex::new(Some(tx)));
    #[derive(Clone)]
    struct Web {
        app: crate::Shared,
        peer: String,
        domain: String,
        uri: String,
        csrf: String,
        result: Arc<Mutex<Option<tokio::sync::oneshot::Sender<Value>>>>,
    }
    async fn challenge(
        State(w): State<Web>,
        Json(v): Json<Value>,
    ) -> Result<Json<Value>, (axum::http::StatusCode, String)> {
        if v["csrf"] != w.csrf {
            return Err((axum::http::StatusCode::FORBIDDEN, "Invalid request".into()));
        }
        let raw = issue(
            &w.app,
            &w.peer,
            v["address"].as_str().unwrap_or(""),
            &w.domain,
            &w.uri,
        )
        .map_err(|e| (axum::http::StatusCode::BAD_REQUEST, e.to_string()))?;
        Ok(Json(json!({"message":raw})))
    }
    async fn finish(
        State(w): State<Web>,
        Json(v): Json<Value>,
    ) -> Result<Json<Value>, (axum::http::StatusCode, String)> {
        if v["csrf"] != w.csrf {
            return Err((axum::http::StatusCode::FORBIDDEN, "Invalid request".into()));
        }
        let address = verify(
            &w.app,
            &w.peer,
            v["message"].as_str().unwrap_or(""),
            v["signature"].as_str().unwrap_or(""),
            &w.domain,
            &w.uri,
        )
        .await
        .map_err(|e| (axum::http::StatusCode::BAD_REQUEST, e.to_string()))?;
        let data =
            json!({"address":address,"mode":"external","verified":"this-device","chainId":1});
        if let Some(tx) = w.result.lock().await.take() {
            let _ = tx.send(data);
        }
        Ok(Json(json!({"ok":true})))
    }
    let page = include_str!("wallet-connect.html").replace("__CSRF__", &csrf);
    let web = Web {
        app: s.clone(),
        peer: me,
        domain,
        uri: uri.clone(),
        csrf: csrf.clone(),
        result,
    };
    let route = format!("/wallet/{csrf}");
    let server = Router::new()
        .route(&route, get(move || async move { Html(page) }))
        .route("/challenge", post(challenge))
        .route("/finish", post(finish))
        .with_state(web);
    let stop = tokio_util::sync::CancellationToken::new();
    let stopped = stop.clone();
    let task = tokio::spawn(async move {
        let _ = axum::serve(listener, server)
            .with_graceful_shutdown(stopped.cancelled_owned())
            .await;
    });
    open::that_detached(format!("{uri}/{csrf}"))?;
    let value = tokio::time::timeout(std::time::Duration::from_secs(180), rx).await;
    stop.cancel();
    let _ = task.await;
    let value = value.context("Wallet connection timed out")??;
    s.db.lock().unwrap().execute(
        "INSERT OR REPLACE INTO connections VALUES('wallet',?)",
        [value.to_string()],
    )?;
    Ok(value)
}
