//! Real protocol clients against an isolated loopback relay/blob service.
//! Relay events survive client disconnects while the fixture process runs; blobs use disk.
//! No private user files or public relay writes are used by these tests.
use crate::*;
use anyhow::Result;
use axum::{
    body::Body,
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        Path, State,
    },
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, put},
    Router,
};
use futures_util::{SinkExt, StreamExt};
use nostr_sdk::prelude::{Event, JsonUtil};
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Arc, Mutex},
};
use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt};
#[derive(Clone)]
pub(crate) struct Fixture {
    events: Arc<Mutex<Vec<Value>>>,
    broadcast: tokio::sync::broadcast::Sender<Value>,
    files: PathBuf,
    origin: String,
}
async fn relay(State(s): State<Fixture>, ws: WebSocketUpgrade) -> impl IntoResponse {
    ws.on_upgrade(move |socket| session(socket, s))
}
fn matches(event: &Value, filter: &Value) -> bool {
    filter["kinds"]
        .as_array()
        .is_none_or(|a| a.contains(&event["kind"]))
        && filter["#p"].as_array().is_none_or(|a| {
            event["tags"]
                .as_array()
                .is_some_and(|tags| tags.iter().any(|t| t[0] == "p" && a.contains(&t[1])))
        })
}
async fn session(socket: WebSocket, s: Fixture) {
    let (mut tx, mut rx) = socket.split();
    let mut notices = s.broadcast.subscribe();
    let mut subscriptions: HashMap<String, Value> = HashMap::new();
    loop {
        tokio::select! {frame=rx.next()=>{let text=match frame { Some(Ok(Message::Text(text)))=>text,Some(Ok(Message::Ping(data)))=>{let _=tx.send(Message::Pong(data)).await;continue},Some(Ok(Message::Pong(_)))=>continue,_=>break};let Ok(v)=serde_json::from_str::<Value>(&text)else{continue};match v[0].as_str(){Some("EVENT")=>{let Ok(e)=serde_json::from_value::<Event>(v[1].clone())else{continue};let ok=e.verify().is_ok();if ok{s.events.lock().unwrap().push(v[1].clone());let _=s.broadcast.send(v[1].clone());}let _=tx.send(Message::Text(json!(["OK",v[1]["id"],ok,""]).to_string().into())).await;},Some("REQ")=>{let id=v[1].as_str().unwrap_or("").to_owned();let filter=v[2].clone();let events=s.events.lock().unwrap().clone();for e in events{if matches(&e,&filter){let _=tx.send(Message::Text(json!(["EVENT",id,e]).to_string().into())).await;}}let _=tx.send(Message::Text(json!(["EOSE",id]).to_string().into())).await;subscriptions.insert(id,filter);},Some("CLOSE")=>{if let Some(id)=v[1].as_str(){subscriptions.remove(id);}},_=>{}}},event=notices.recv()=>{let Ok(e)=event else{break};for(id,filter)in &subscriptions{if matches(&e,filter){let _=tx.send(Message::Text(json!(["EVENT",id,e]).to_string().into())).await;}}}}
    }
}
async fn preflight() -> StatusCode {
    StatusCode::OK
}
async fn upload(
    State(s): State<Fixture>,
    headers: HeaderMap,
    body: Body,
) -> Result<axum::Json<Value>, StatusCode> {
    use base64::Engine;
    use sha2::{Digest, Sha256};
    let auth = headers
        .get("authorization")
        .and_then(|x| x.to_str().ok())
        .and_then(|s| s.strip_prefix("Nostr "))
        .ok_or(StatusCode::UNAUTHORIZED)?;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(auth)
        .map_err(|_| StatusCode::UNAUTHORIZED)?;
    let event = Event::from_json(bytes).map_err(|_| StatusCode::UNAUTHORIZED)?;
    event.verify().map_err(|_| StatusCode::UNAUTHORIZED)?;
    if event.kind.as_u16() != 24242 {
        return Err(StatusCode::UNAUTHORIZED);
    }
    let hash = headers
        .get("x-sha-256")
        .and_then(|x| x.to_str().ok())
        .ok_or(StatusCode::BAD_REQUEST)?
        .to_owned();
    if hash.len() != 64 || !hash.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(StatusCode::BAD_REQUEST);
    }
    let valid = event
        .tags
        .iter()
        .any(|t| t.as_slice() == ["x".to_string(), hash.clone()]);
    if !valid {
        return Err(StatusCode::UNAUTHORIZED);
    }
    let path = s.files.join(&hash);
    let mut file = tokio::fs::File::create(&path)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let mut chunks = body.into_data_stream();
    let mut digest = Sha256::new();
    let mut size = 0;
    while let Some(chunk) = chunks.next().await {
        let b = chunk.map_err(|_| StatusCode::BAD_REQUEST)?;
        size += b.len() as u64;
        if size > 2 * 1024 * 1024 * 1024 {
            return Err(StatusCode::PAYLOAD_TOO_LARGE);
        }
        digest.update(&b);
        file.write_all(&b)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    }
    file.flush()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if format!("{:x}", digest.finalize()) != hash {
        return Err(StatusCode::CONFLICT);
    }
    Ok(axum::Json(
        json!({"sha256":hash,"size":size,"url":format!("{}/{}",s.origin,hash),"type":"application/octet-stream"}),
    ))
}
async fn download(
    State(s): State<Fixture>,
    Path(hash): Path<String>,
    headers: HeaderMap,
) -> Result<Response, StatusCode> {
    if hash.len() != 64 || !hash.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(StatusCode::BAD_REQUEST);
    }
    let mut file = tokio::fs::File::open(s.files.join(hash))
        .await
        .map_err(|_| StatusCode::NOT_FOUND)?;
    let size = file
        .metadata()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .len();
    let start = headers
        .get("range")
        .and_then(|x| x.to_str().ok())
        .and_then(|s| s.strip_prefix("bytes="))
        .and_then(|x| x.strip_suffix('-'))
        .and_then(|x| x.parse::<u64>().ok())
        .unwrap_or(0);
    if start >= size {
        return Err(StatusCode::RANGE_NOT_SATISFIABLE);
    }
    file.seek(std::io::SeekFrom::Start(start))
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let mut r = Response::builder()
        .status(if start > 0 { 206 } else { 200 })
        .header("Content-Length", size - start)
        .header("Accept-Ranges", "bytes");
    if start > 0 {
        r = r.header(
            "Content-Range",
            format!("bytes {start}-{}/{size}", size - 1),
        );
    }
    r.body(Body::from_stream(tokio_util::io::ReaderStream::new(
        file.take(size - start),
    )))
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}
pub(crate) async fn fixture(
    root: &std::path::Path,
) -> Result<(String, String, tokio::task::JoinHandle<()>, Fixture)> {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let origin = format!("http://{}", listener.local_addr()?);
    let ws = format!("ws://{}/relay", listener.local_addr()?);
    let files = root.join("blobs");
    tokio::fs::create_dir_all(&files).await?;
    let (tx, _) = tokio::sync::broadcast::channel(1024);
    let state = Fixture {
        events: Arc::new(Mutex::new(vec![])),
        broadcast: tx,
        files,
        origin: origin.clone(),
    };
    let app = Router::new()
        .route("/relay", get(relay))
        .route("/upload", put(upload).head(preflight))
        .route("/{hash}", get(download))
        .with_state(state.clone());
    let task = tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    Ok((ws, origin, task, state))
}
pub(crate) fn state(root: PathBuf) -> Result<Shared> {
    std::fs::create_dir_all(&root)?;
    let s = Arc::new(AppState {
        db: Mutex::new(db::init(&root)?),
        root,
        net: net::Network::new()?,
        install_lock: tokio::sync::Mutex::new(()),
        auth_lock: tokio::sync::Mutex::new(()),
        social: social::Runtime::default(),
        transfers: media::Transfers::new(),
    });
    identity::init(&s)?;
    Ok(s)
}
pub(crate) async fn eventually(mut check: impl FnMut() -> bool) -> Result<()> {
    for _ in 0..100 {
        if check() {
            return Ok(());
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    anyhow::bail!("Timed out waiting for real two-client delivery")
}
#[tokio::test]
async fn siwe_replay_audience_signature_and_credentials() -> Result<()> {
    let root = std::env::temp_dir().join(format!("orbit-siwe-{}", uuid::Uuid::new_v4()));
    let a = state(root.join("a"))?;
    let b = state(root.join("b"))?;
    identity::save(&a, &json!({"name":"Synthetic Alice"}))?;
    identity::save(&b, &json!({"name":"Synthetic Bob"}))?;
    let w = wallet::create(&a)?;
    let peer = identity::keys(&a)?.public_key().to_hex();
    let uri = format!(
        "urn:orbit:peer:{}",
        identity::keys(&b)?.public_key().to_hex()
    );
    let raw = wallet::issue(
        &b,
        &peer,
        w["address"].as_str().unwrap(),
        "orbit.local",
        &uri,
    )?;
    let signature = wallet::sign_local(&a, &raw)?;
    assert!(
        wallet::verify(&b, &peer, &raw, &signature, "evil.local", &uri)
            .await
            .is_err()
    );
    assert!(
        wallet::verify(&b, &peer, &raw, &signature, "orbit.local", "urn:wrong")
            .await
            .is_err()
    );
    let mut corrupt = signature.clone();
    corrupt.replace_range(5..6, if &signature[5..6] == "0" { "1" } else { "0" });
    assert!(
        wallet::verify(&b, &peer, &raw, &corrupt, "orbit.local", &uri)
            .await
            .is_err()
    );
    assert_eq!(
        wallet::verify(&b, &peer, &raw, &signature, "orbit.local", &uri).await?,
        w["address"]
    );
    assert!(
        wallet::verify(&b, &peer, &raw, &signature, "orbit.local", &uri)
            .await
            .is_err()
    );
    let expired = wallet::issue(
        &b,
        &peer,
        w["address"].as_str().unwrap(),
        "orbit.local",
        &uri,
    )?;
    b.db.lock()
        .unwrap()
        .execute("UPDATE siwe_challenges SET expires=0", [])?;
    assert!(wallet::verify(
        &b,
        &peer,
        &expired,
        &wallet::sign_local(&a, &expired)?,
        "orbit.local",
        &uri
    )
    .await
    .is_err());
    let old_key = identity::keys(&a)?.public_key();
    let long = "synthetic protected credential ".repeat(100);
    secure::set(&a, "test-chunks", &long)?;
    assert_eq!(secure::get(&a, "test-chunks")?, long);
    secure::remove(&a, "test-chunks")?;
    assert!(secure::get(&a, "test-chunks").is_err());
    let reloaded = state(a.root.clone())?;
    assert_eq!(old_key, identity::keys(&reloaded)?.public_key());
    wallet::disconnect(&a)?;
    assert!(secure::get(&a, "evm-wallet").is_err());
    assert!(wallet::info(&a)?.is_null());
    for s in [&a, &b] {
        for key in ["nostr-key", "data-key", "evm-wallet"] {
            secure::remove(s, key)?;
        }
    }
    Ok(())
}
#[tokio::test]
async fn two_clients_friendship_offline_delivery_media_and_recovery() -> Result<()> {
    let root = std::env::var("ORBIT_SOCIAL_TEST_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            std::env::temp_dir().join(format!("orbit-social-{}", uuid::Uuid::new_v4()))
        });
    let root = root.join(uuid::Uuid::new_v4().to_string());
    let (relay, storage, server, fixture) = fixture(&root).await?;
    let a = state(root.join("a"))?;
    let b = state(root.join("b"))?;
    for (s, name) in [(&a, "Synthetic Alice"), (&b, "Synthetic Bob")] {
        identity::save(s, &json!({"name":name,"bio":"Synthetic integration test"}))?;
        identity::save_config(
            s,
            &json!({"enabled":true,"relays":[relay],"mediaServer":storage,"mediaConsent":true,"maxFileBytes":2_u64*1024*1024*1024}),
        )?;
        social::start(None, s.clone()).await?;
    }
    let aid = identity::keys(&a)?.public_key().to_hex();
    let bid = identity::keys(&b)?.public_key().to_hex();
    let invitation = social::invitation(&a).await?;
    social::import_invitation(&b, &json!({"code":invitation["code"]})).await?;
    social::flush(&b).await?;
    eventually(|| identity::contact(&a, &bid).is_ok_and(|c| c["state"] == "incoming")).await?;
    social::accept(&a, &json!({"id":bid})).await?;
    social::flush(&a).await?;
    eventually(|| identity::contact(&b, &aid).is_ok_and(|c| c["state"] == "accepted")).await?;
    let message = social::send(&a, &json!({"peer":bid,"text":"Synthetic live message"})).await?;
    social::flush(&a).await?;
    eventually(|| {
        identity::messages(&b, &json!({"peer":aid})).is_ok_and(|v| {
            v.as_array()
                .unwrap()
                .iter()
                .any(|m| m["id"] == message["id"])
        })
    })
    .await?;
    social::flush(&b).await?;
    eventually(|| {
        identity::messages(&a, &json!({"peer":bid})).is_ok_and(|v| {
            v.as_array()
                .unwrap()
                .iter()
                .any(|m| m["id"] == message["id"] && m["state"] == "delivered")
        })
    })
    .await?;
    social::stop(&b).await;
    let offline = social::send(&a, &json!({"peer":bid,"text":"Synthetic offline message"})).await?;
    social::flush(&a).await?;
    let before = identity::messages(&a, &json!({"peer":bid}))?;
    assert_eq!(
        before
            .as_array()
            .unwrap()
            .iter()
            .find(|m| m["id"] == offline["id"])
            .unwrap()["state"],
        "sent"
    );
    social::start(None, b.clone()).await?;
    eventually(|| {
        identity::messages(&b, &json!({"peer":aid})).is_ok_and(|v| {
            v.as_array()
                .unwrap()
                .iter()
                .any(|m| m["id"] == offline["id"])
        })
    })
    .await?;
    let file = root.join("synthetic-large.bin");
    let large = std::env::var("ORBIT_TEST_GIB").as_deref() == Ok("1");
    let size = if large {
        1024_u64 * 1024 * 1024
    } else {
        32_u64 * 1024 * 1024
    };
    let mut out = std::fs::File::create(&file)?;
    out.set_len(size)?;
    use std::io::{Seek, Write};
    out.seek(std::io::SeekFrom::End(-16))?;
    out.write_all(b"ORBIT-SYNTHETIC!")?;
    let prepared = media::prepare(None, a.clone(), &json!({"path":file})).await?;
    let uploaded = media::upload(None, a.clone(), &json!({"id":prepared["id"]})).await?;
    assert_eq!(uploaded["state"], "uploaded");
    let sent = social::send(
        &a,
        &json!({"peer":bid,"text":"Synthetic attachment","attachments":[prepared["id"]]}),
    )
    .await?;
    social::flush(&a).await?;
    eventually(|| {
        identity::messages(&b, &json!({"peer":aid}))
            .is_ok_and(|v| v.as_array().unwrap().iter().any(|m| m["id"] == sent["id"]))
    })
    .await?;
    let media_id = prepared["id"].as_str().unwrap();
    let original = media::load(&a, media_id)?;
    let source = PathBuf::from(original["encryptedPath"].as_str().unwrap());
    let part = b
        .root
        .join("cache/media/downloads")
        .join(format!("{media_id}.part"));
    std::fs::create_dir_all(part.parent().unwrap())?;
    let source_file = std::fs::File::open(&source)?;
    let length = source_file.metadata()?.len() / 3;
    let mut partial = std::fs::File::create(&part)?;
    std::io::copy(&mut std::io::Read::take(source_file, length), &mut partial)?;
    drop(partial);
    let result = media::retrieve(
        None,
        b.clone(),
        &json!({"id":media_id,"message":sent["id"]}),
    )
    .await?;
    assert_eq!(
        media::file_hash(std::path::Path::new(result["path"].as_str().unwrap()))?,
        media::file_hash(&file)?
    );
    assert!(!part.exists());
    // A 6 MP photo, animated GIF and H.264 MP4 follow the same encrypted path.
    let photo = root.join("synthetic-photo.png");
    let mut seed = 12345u32;
    let image = ::image::RgbImage::from_fn(3000, 2000, |_, _| {
        seed ^= seed << 13;
        seed ^= seed >> 17;
        seed ^= seed << 5;
        ::image::Rgb([seed as u8, (seed >> 8) as u8, (seed >> 16) as u8])
    });
    image.save(&photo)?;
    drop(image);
    let fixtures = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("test-data");
    for (file, mime) in [
        (&photo, "image/png"),
        (&fixtures.join("synthetic.gif"), "image/gif"),
        (&fixtures.join("synthetic.mp4"), "video/mp4"),
    ] {
        let staged = media::prepare(None, a.clone(), &json!({"path":file})).await?;
        assert_eq!(staged["type"], mime);
        media::upload(None, a.clone(), &json!({"id":staged["id"]})).await?;
        let message = social::send(&a, &json!({"peer":bid,"attachments":[staged["id"]]})).await?;
        social::flush(&a).await?;
        eventually(|| {
            identity::messages(&b, &json!({"peer":aid})).is_ok_and(|v| {
                v.as_array()
                    .unwrap()
                    .iter()
                    .any(|m| m["id"] == message["id"])
            })
        })
        .await?;
        let received = media::retrieve(
            None,
            b.clone(),
            &json!({"id":staged["id"],"message":message["id"]}),
        )
        .await?;
        let path = std::path::Path::new(received["path"].as_str().unwrap());
        assert_eq!(media::file_hash(path)?, media::file_hash(file)?);
        let visible = identity::messages(&b, &json!({"peer":aid}))?;
        let m = visible
            .as_array()
            .unwrap()
            .iter()
            .find(|m| m["id"] == message["id"])
            .unwrap();
        for key in ["key", "source", "encryptedPath"] {
            assert!(m["body"]["attachments"][0][key].is_null());
        }
        if mime == "image/png" {
            assert_eq!(::image::image_dimensions(path)?, (3000, 2000));
        }
        if mime == "image/gif" {
            use ::image::AnimationDecoder;
            let decoder = ::image::codecs::gif::GifDecoder::new(std::io::BufReader::new(
                std::fs::File::open(path)?,
            ))?;
            assert!(decoder.into_frames().collect_frames()?.len() > 1);
        }
        if mime == "video/mp4" {
            if let Ok(ffmpeg) = std::env::var("ORBIT_TEST_FFMPEG") {
                assert!(std::process::Command::new(ffmpeg)
                    .args(["-hide_banner", "-loglevel", "error", "-i"])
                    .arg(path)
                    .args(["-f", "null", "-"])
                    .status()?
                    .success());
            }
        }
    }
    let compressed =
        media::prepare(None, a.clone(), &json!({"path":photo,"compress":true})).await?;
    assert_eq!(compressed["type"], "image/jpeg");
    assert_eq!(compressed["compressed"], true);
    assert!(compressed["size"].as_u64().unwrap() < std::fs::metadata(&photo)?.len());
    assert!(a.transfers.tokens.lock().unwrap().is_empty());
    assert!(b.transfers.tokens.lock().unwrap().is_empty());
    // Cancellation must release its handle; HTTP failures must leave retry possible.
    let cancel_state = a.clone();
    let selected = photo.clone();
    let task =
        tokio::spawn(
            async move { media::prepare(None, cancel_state, &json!({"path":selected})).await },
        );
    eventually(|| !a.transfers.tokens.lock().unwrap().is_empty()).await?;
    let cancel_id = a
        .transfers
        .tokens
        .lock()
        .unwrap()
        .keys()
        .next()
        .unwrap()
        .clone();
    media::cancel(&a, &json!({"id":cancel_id}))?;
    assert!(task.await?.is_err());
    assert!(a.transfers.tokens.lock().unwrap().is_empty());
    let config = identity::config(&a)?;
    let mut bad = config.clone();
    bad["mediaServer"] = json!(format!("{storage}/missing"));
    identity::save_config(&a, &bad)?;
    assert!(
        media::upload(None, a.clone(), &json!({"id":compressed["id"]}))
            .await
            .is_err()
    );
    assert!(a.transfers.tokens.lock().unwrap().is_empty());
    identity::save_config(&a, &config)?;
    media::upload(None, a.clone(), &json!({"id":compressed["id"]})).await?;
    if !large {
        let cached = b
            .root
            .join("cache/media/downloads")
            .join(format!("{media_id}.age"));
        std::fs::write(&cached, b"corrupt cached ciphertext")?;
        let recovered = media::retrieve(
            None,
            b.clone(),
            &json!({"id":media_id,"message":sent["id"]}),
        )
        .await?;
        assert_eq!(
            media::file_hash(std::path::Path::new(recovered["path"].as_str().unwrap()))?,
            media::file_hash(&file)?
        );
    }
    let exported = root.join("recovery.orbit");
    backup::export(
        &a,
        &json!({"path":exported,"password":"synthetic test passphrase only"}),
    )?;
    let c = state(root.join("restored"))?;
    assert!(backup::restore(&c, &json!({"path":exported,"password":"wrong passphrase"})).is_err());
    assert!(identity::profile(&c)?.is_null());
    backup::restore(
        &c,
        &json!({"path":exported,"password":"synthetic test passphrase only"}),
    )?;
    assert_eq!(identity::profile(&c)?["id"], aid);
    assert_eq!(identity::contact(&c, &bid)?["state"], "accepted");
    assert!(
        identity::messages(&c, &json!({"peer":bid}))?
            .as_array()
            .unwrap()
            .len()
            >= 3
    );
    identity::block(&b, &json!({"id":aid,"blocked":true}))?;
    let blocked = social::send(&a, &json!({"peer":bid,"text":"Must be dropped"})).await?;
    social::flush(&a).await?;
    tokio::time::sleep(std::time::Duration::from_millis(250)).await;
    assert!(!identity::messages(&b, &json!({"peer":aid}))?
        .as_array()
        .unwrap()
        .iter()
        .any(|m| m["id"] == blocked["id"]));
    let events = fixture.events.lock().unwrap();
    assert!(!events
        .iter()
        .any(|e| e.to_string().contains("Synthetic live message")));
    drop(events);
    for s in [&a, &b, &c] {
        social::stop(s).await;
        for key in ["nostr-key", "data-key", "evm-wallet"] {
            secure::remove(s, key)?;
        }
    }
    server.abort();
    eprintln!("Real two-client NIP-17/Blossom check passed with {size} plaintext bytes, range resume, friendship, acknowledgments, blocking and encrypted restore");
    Ok(())
}
