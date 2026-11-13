use crate::{db, identity, secure, AppState};
use age::secrecy::ExposeSecret;
use anyhow::{bail, ensure, Context, Result};
use base64::Engine;
use futures_util::StreamExt;
use nostr_sdk::prelude::*;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};
use tokio_util::sync::CancellationToken;
pub struct Transfers {
    pub tokens: Mutex<HashMap<String, CancellationToken>>,
    pub jobs: Mutex<HashMap<String, Value>>,
    pub slots: tokio::sync::Semaphore,
}
impl Transfers {
    pub fn new() -> Self {
        Self {
            tokens: Mutex::new(HashMap::new()),
            jobs: Mutex::new(HashMap::new()),
            slots: tokio::sync::Semaphore::new(2),
        }
    }
}
// Always release cancellation handles, including early HTTP/validation failures.
struct TransferGuard {
    state: crate::Shared,
    app: Option<tauri::AppHandle>,
    id: String,
    token: CancellationToken,
}
impl Drop for TransferGuard {
    fn drop(&mut self) {
        self.state.transfers.tokens.lock().unwrap().remove(&self.id);
        let unfinished = self
            .state
            .transfers
            .jobs
            .lock()
            .unwrap()
            .get(&self.id)
            .is_none_or(|v| {
                !matches!(
                    v["state"].as_str(),
                    Some("staged" | "uploaded" | "ready" | "failed" | "canceled")
                )
            });
        if unfinished {
            report(
                &self.state,
                self.app.as_ref(),
                &self.id,
                if self.token.is_cancelled() {
                    "canceled"
                } else {
                    "failed"
                },
                0,
                0,
                Some(
                    "Transfer stopped. Retry explicitly; downloads retain resumable partial data."
                        .into(),
                ),
            );
        }
    }
}
fn report(
    s: &AppState,
    app: Option<&tauri::AppHandle>,
    id: &str,
    state: &str,
    done: u64,
    total: u64,
    error: Option<String>,
) {
    let v = json!({"id":id,"state":state,"done":done,"total":total,"error":error});
    s.transfers
        .jobs
        .lock()
        .unwrap()
        .insert(id.into(), v.clone());
    if let Some(app) = app {
        use tauri::Emitter;
        let _ = app.emit("media-progress", v);
    }
}
pub fn jobs(s: &AppState) -> Value {
    json!(s
        .transfers
        .jobs
        .lock()
        .unwrap()
        .values()
        .cloned()
        .collect::<Vec<_>>())
}
pub fn clear_cache(s: &AppState) -> Result<Value> {
    let tokens = s.transfers.tokens.lock().unwrap();
    ensure!(
        tokens.is_empty(),
        "Wait for or cancel active transfers before clearing media"
    );
    let cache = s.root.join("cache/media");
    if cache.exists() {
        std::fs::remove_dir_all(cache)?;
    }
    Ok(json!({"cleared":true}))
}
fn begin(s: &AppState, id: &str, token: &CancellationToken) -> Result<()> {
    let mut tokens = s.transfers.tokens.lock().unwrap();
    ensure!(
        !tokens.contains_key(id),
        "This attachment already has an active transfer"
    );
    tokens.insert(id.to_owned(), token.clone());
    Ok(())
}
pub fn cancel(s: &AppState, v: &Value) -> Result<Value> {
    let id = v["id"].as_str().context("Transfer missing")?;
    if let Some(t) = s.transfers.tokens.lock().unwrap().get(id) {
        t.cancel();
    }
    Ok(json!(true))
}
pub fn cancel_all(s: &AppState) {
    for token in s.transfers.tokens.lock().unwrap().values() {
        token.cancel();
    }
}
pub fn store(s: &AppState, v: &Value) -> Result<()> {
    let raw = secure::seal(s, &v.to_string())?;
    s.db.lock().unwrap().execute(
        "INSERT OR REPLACE INTO media VALUES(?,?)",
        rusqlite::params![v["id"].as_str().context("Media ID missing")?, raw],
    )?;
    Ok(())
}
pub fn load(s: &AppState, id: &str) -> Result<Value> {
    let raw: String =
        s.db.lock()
            .unwrap()
            .query_row("SELECT data FROM media WHERE id=?", [id], |r| r.get(0))?;
    Ok(serde_json::from_str(&secure::unseal(s, &raw)?)?)
}
pub fn public(v: &Value) -> Value {
    let mut v = v.clone();
    if let Some(o) = v.as_object_mut() {
        for k in ["key", "source", "encryptedPath"] {
            o.remove(k);
        }
    }
    v
}
fn content_type(path: &Path) -> String {
    let guessed = mime_guess::from_path(path)
        .first_or_octet_stream()
        .to_string();
    if guessed == "image/svg+xml" {
        "application/octet-stream".into()
    } else {
        guessed
    }
}
pub async fn prepare(app: Option<tauri::AppHandle>, s: crate::Shared, v: &Value) -> Result<Value> {
    let path = PathBuf::from(v["path"].as_str().context("Choose a file")?);
    ensure!(
        path.is_absolute() && path.is_file(),
        "File is missing or moved"
    );
    let cfg = identity::config(&s)?;
    let size = tokio::fs::metadata(&path).await?.len();
    ensure!(
        size > 0 && size <= cfg["maxFileBytes"].as_u64().unwrap_or(20 * 1024 * 1024),
        "File exceeds your configured local limit. Provider limits can be lower."
    );
    let id = uuid::Uuid::new_v4().to_string();
    let token = CancellationToken::new();
    begin(&s, &id, &token)?;
    let _guard = TransferGuard {
        state: s.clone(),
        app: app.clone(),
        id: id.clone(),
        token: token.clone(),
    };
    report(&s, app.as_ref(), &id, "waiting", 0, 0, None);
    let _slot = s.transfers.slots.acquire().await?;
    let state = s.clone();
    let key = age::x25519::Identity::generate();
    let secret = key.to_string().expose_secret().to_owned();
    let recipient = key.to_public();
    let dest = s
        .root
        .join("cache/media/encrypted")
        .join(format!("{id}.age"));
    std::fs::create_dir_all(dest.parent().unwrap())?;
    let name = path
        .file_name()
        .context("Filename missing")?
        .to_string_lossy()
        .into_owned();
    let voice = v["voice"] == true && path.starts_with(s.root.join("voice-tmp"));
    let mime = if voice {
        match path.extension().and_then(|ext| ext.to_str()) {
            Some("webm") => "audio/webm".to_owned(),
            Some("ogg") => "audio/ogg".to_owned(),
            Some("m4a") => "audio/mp4".to_owned(),
            _ => anyhow::bail!("Unsupported voice format"),
        }
    } else {
        content_type(&path)
    };
    let compressed = v["compress"] == true && mime.starts_with("image/") && mime != "image/gif";
    let result = tokio::task::spawn_blocking(move || -> Result<Value> {
        // Remove derived plaintext even when encoding/encryption is canceled or fails.
        struct Temporary(Option<PathBuf>);
        impl Drop for Temporary {
            fn drop(&mut self) {
                if let Some(p) = &self.0 {
                    let _ = std::fs::remove_file(p);
                }
            }
        }
        let mut temporary = Temporary(None);
        let mut original = path.clone();
        let mut actual_name = name;
        let mut actual_type = mime;
        if compressed {
            ensure!(
                size <= 32 * 1024 * 1024,
                "Compression is limited to 32 MiB input; send larger photos at original quality"
            );
            let mut decoder = ::image::ImageReader::open(&path)?.with_guessed_format()?;
            let mut limits = ::image::Limits::default();
            limits.max_alloc = Some(128 * 1024 * 1024);
            limits.max_image_width = Some(16000);
            limits.max_image_height = Some(16000);
            decoder.limits(limits);
            let img = decoder
                .decode()?
                .resize(2560, 2560, ::image::imageops::FilterType::Triangle);
            let tmp = dest.with_extension("compressed.jpg");
            temporary.0 = Some(tmp.clone());
            let mut output = std::io::BufWriter::new(std::fs::File::create(&tmp)?);
            ::image::codecs::jpeg::JpegEncoder::new_with_quality(&mut output, 88)
                .encode_image(&img)?;
            output.flush()?;
            original = tmp;
            actual_name = format!(
                "{}.jpg",
                path.file_stem().unwrap_or_default().to_string_lossy()
            );
            actual_type = "image/jpeg".into();
        }
        let total = std::fs::metadata(&original)?.len();
        let mut input = std::io::BufReader::new(std::fs::File::open(&original)?);
        let file = std::io::BufWriter::new(std::fs::File::create(&dest)?);
        let mut output =
            age::Encryptor::with_recipients(std::iter::once(&recipient as &dyn age::Recipient))?
                .wrap_output(file)?;
        let mut hash = Sha256::new();
        let mut buffer = vec![0; 256 * 1024];
        let mut done = 0;
        loop {
            ensure!(!token.is_cancelled(), "Transfer canceled");
            let n = input.read(&mut buffer)?;
            if n == 0 {
                break;
            }
            hash.update(&buffer[..n]);
            output.write_all(&buffer[..n])?;
            done += n as u64;
            if done % (4 * 1024 * 1024) < n as u64 || done == total {
                report(&state, app.as_ref(), &id, "encrypting", done, total, None);
            }
        }
        output.finish()?.flush()?;
        drop(input);
        drop(temporary);
        let encrypted_hash = file_hash(&dest)?;
        let encrypted_size = std::fs::metadata(&dest)?.len();
        let value = json!({"id":id,"name":actual_name,"type":actual_type,"size":total,
            "sha256":format!("{:x}",hash.finalize()),"encryptedSha256":encrypted_hash,
            "encryptedSize":encrypted_size,"key":secret,"source":path,"encryptedPath":dest,
            "state":"staged","compressed":compressed,"voice":voice});
        store(&state, &value)?;
        report(&state, app.as_ref(), &id, "staged", total, total, None);
        Ok(value)
    })
    .await??;
    Ok(public(&result))
}

pub fn file_hash(path: &Path) -> Result<String> {
    let mut f = std::io::BufReader::new(std::fs::File::open(path)?);
    let mut hash = Sha256::new();
    let mut buffer = vec![0; 256 * 1024];
    loop {
        let n = f.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        hash.update(&buffer[..n]);
    }
    Ok(format!("{:x}", hash.finalize()))
}
fn client(s: &AppState) -> Result<reqwest::Client> {
    Ok(reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(15))
        .timeout(std::time::Duration::from_secs(
            if identity::config(s)?["mode"] == "automatic" {
                120
            } else {
                3600
            },
        ))
        .redirect(reqwest::redirect::Policy::none())
        .build()?)
}
async fn authorization(s: &AppState, hash: &str) -> Result<String> {
    let event = EventBuilder::new(Kind::Custom(24242), "Upload encrypted Orbit attachment")
        .tags([
            Tag::parse(["t", "upload"])?,
            Tag::parse(["x", hash])?,
            Tag::parse(["expiration", &(db::now() + 300).to_string()])?,
        ])
        .sign_with_keys(&identity::keys(s)?)?;
    Ok(format!(
        "Nostr {}",
        base64::engine::general_purpose::STANDARD.encode(event.as_json())
    ))
}
#[derive(Debug)]
struct ProviderFailure {
    status: u16,
    reason: String,
    retry_after: i64,
    clock: bool,
}
impl std::fmt::Display for ProviderFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.reason)
    }
}
impl std::error::Error for ProviderFailure {}
async fn rejected(response: reqwest::Response) -> anyhow::Error {
    let status = response.status().as_u16();
    let clock = crate::social_network::clock_error(response.headers());
    let retry_after = response
        .headers()
        .get("retry-after")
        .and_then(|x| x.to_str().ok())
        .and_then(|s| {
            s.parse::<i64>().ok().or_else(|| {
                httpdate::parse_http_date(s).ok().map(|time| {
                    time.duration_since(std::time::SystemTime::now())
                        .map(|d| d.as_secs().min(3600) as i64)
                        .unwrap_or(5)
                })
            })
        })
        .unwrap_or(60)
        .clamp(5, 3600);
    let mut reason = response
        .headers()
        .get("x-reason")
        .and_then(|x| x.to_str().ok())
        .unwrap_or("")
        .to_owned();
    let mut stream = response.bytes_stream();
    let mut bytes = vec![];
    while let Some(Ok(chunk)) = stream.next().await {
        if bytes.len() + chunk.len() > 2048 {
            break;
        }
        bytes.extend_from_slice(&chunk);
    }
    reason.push_str(&String::from_utf8_lossy(&bytes));
    let timestamp = reason.to_ascii_lowercase();
    let clock = clock.filter(|_| {
        matches!(status, 400 | 401 | 403)
            && (timestamp.contains("time")
                || timestamp.contains("future")
                || timestamp.contains("created"))
    });
    let message = clock.clone().unwrap_or_else(|| {
        format!(
            "File service rejected this upload (HTTP {status}). {}",
            reason.chars().take(400).collect::<String>()
        )
    });
    ProviderFailure {
        status,
        reason: message,
        retry_after,
        clock: clock.is_some(),
    }
    .into()
}
pub async fn upload(app: Option<tauri::AppHandle>, s: crate::Shared, v: &Value) -> Result<Value> {
    let targets = crate::social_network::providers(&identity::config(&s)?);
    upload_providers(app, s, v, targets).await
}
pub(crate) async fn upload_providers(
    app: Option<tauri::AppHandle>,
    s: crate::Shared,
    v: &Value,
    targets: Vec<String>,
) -> Result<Value> {
    let id = v["id"].as_str().context("Attachment missing")?.to_owned();
    let cfg = identity::config(&s)?;
    ensure!(
        cfg["enabled"] == true,
        "You are offline. Your draft and files are still on this computer."
    );
    ensure!(
        cfg["mediaConsent"] == true,
        "Allow encrypted file uploads in Settings → Network, or choose Automatic."
    );
    let token = CancellationToken::new();
    begin(&s, &id, &token)?;
    let _guard = TransferGuard {
        state: s.clone(),
        app: app.clone(),
        id: id.clone(),
        token: token.clone(),
    };
    let _slot = s.transfers.slots.acquire().await?;
    let media = load(&s, &id)?;
    let size = media["encryptedSize"].as_u64().unwrap_or(0);
    let plain_size = media["size"].as_u64().unwrap_or(size);
    let mut errors = vec![];
    for server in targets {
        ensure!(!token.is_cancelled(), "Upload canceled");
        let blocked:bool=s.db.lock().unwrap().query_row("SELECT EXISTS(SELECT 1 FROM storage_health WHERE service=? AND until>? AND (min_size=0 OR min_size<=?))",rusqlite::params![server,db::now(),plain_size as i64],|r|r.get(0))?;
        if blocked {
            errors.push(format!(
                "{server}: temporarily unavailable after a previous rejection"
            ));
            continue;
        }
        report(&s, app.as_ref(), &id, "connecting", 0, size, None);
        let attempt = tokio::select! {_=token.cancelled()=>bail!("Upload canceled"),result=upload_inner(app.clone(), s.clone(), v, &server, &token)=>result};
        match attempt {
            Ok(value) => {
                s.db.lock()
                    .unwrap()
                    .execute("DELETE FROM storage_health WHERE service=?", [server])?;
                return Ok(value);
            }
            Err(e) => {
                if token.is_cancelled() {
                    return Err(e);
                }
                let failure = e.downcast_ref::<ProviderFailure>();
                if failure.is_some_and(|f| f.clock) {
                    return Err(e);
                }
                let status = failure.map(|f| f.status).unwrap_or(0);
                let delay = if matches!(status, 413 | 415) {
                    86400
                } else {
                    failure.map(|f| f.retry_after).unwrap_or(60)
                };
                s.db.lock().unwrap().execute(
                    "INSERT OR REPLACE INTO storage_health VALUES(?,?,?,?,?)",
                    rusqlite::params![
                        server,
                        db::now() + delay,
                        status,
                        if status == 413 { plain_size as i64 } else { 0 },
                        e.to_string()
                    ],
                )?;
                errors.push(format!("{server}: {e}"));
            }
        }
    }
    let error = format!(
        "Could not upload. Your draft is preserved. {}",
        errors.join(" · ")
    );
    report(
        &s,
        app.as_ref(),
        &id,
        "failed",
        0,
        size,
        Some(error.clone()),
    );
    bail!(error)
}
async fn upload_inner(
    app: Option<tauri::AppHandle>,
    s: crate::Shared,
    v: &Value,
    server: &str,
    token: &CancellationToken,
) -> Result<Value> {
    let id = v["id"].as_str().context("Attachment missing")?.to_owned();
    let mut media = load(&s, &id)?;
    identity::validate_endpoint(server, false)?;
    let path = PathBuf::from(
        media["encryptedPath"]
            .as_str()
            .context("Staged file unavailable")?,
    );
    ensure!(
        path.is_file(),
        "Staged file missing. Select the original file again."
    );
    let hash = media["encryptedSha256"]
        .as_str()
        .context("Encrypted hash missing")?;
    let size = media["encryptedSize"]
        .as_u64()
        .context("Encrypted size missing")?;
    let auth = authorization(&s, hash).await?;
    let http = client(&s)?;
    let url = format!("{server}/upload");
    let request = http
        .head(&url)
        .header("Authorization", &auth)
        .header("X-SHA-256", hash)
        .header("X-Content-Type", "application/octet-stream")
        .header("X-Content-Length", size)
        .send();
    let preflight = tokio::select! {_ = token.cancelled()=>bail!("Upload canceled"),r=request=>r?};
    if !(preflight.status().is_success() || matches!(preflight.status().as_u16(), 404 | 405)) {
        return Err(rejected(preflight).await);
    }
    let file = tokio::fs::File::open(&path).await?;
    let total = Arc::new(std::sync::atomic::AtomicU64::new(0));
    let sent = total.clone();
    let state = s.clone();
    let ident = id.clone();
    let handle = app.clone();
    let stream = tokio_util::io::ReaderStream::with_capacity(file, 256 * 1024).map(move |chunk| {
        if let Ok(ref b) = chunk {
            let done = sent.fetch_add(b.len() as u64, std::sync::atomic::Ordering::Relaxed)
                + b.len() as u64;
            if done % (4 * 1024 * 1024) < b.len() as u64 || done == size {
                report(
                    &state,
                    handle.as_ref(),
                    &ident,
                    "uploading",
                    done,
                    size,
                    None,
                )
            }
        }
        chunk
    });
    let response = tokio::select! {_ = token.cancelled()=>bail!("Upload canceled. BUD-02 retries from the beginning."),r=http.put(&url).header("Authorization",auth).header("Content-Type","application/octet-stream").header("Content-Length",size).header("X-SHA-256",hash).body(reqwest::Body::wrap_stream(stream)).send()=>r?};
    if !response.status().is_success() {
        return Err(rejected(response).await);
    }
    let mut chunks = response.bytes_stream();
    let mut bytes = vec![];
    while let Some(chunk) = chunks.next().await {
        bytes.extend_from_slice(&chunk?);
        ensure!(
            bytes.len() <= 16_384,
            "Storage descriptor exceeds metadata limit"
        );
    }
    let descriptor: Value = serde_json::from_slice(&bytes)?;
    ensure!(
        descriptor["sha256"] == hash && descriptor["size"] == size,
        "Storage descriptor hash or size mismatch"
    );
    let retrieval = descriptor["url"]
        .as_str()
        .context("Storage returned no retrieval URL")?;
    crate::social_network::retrieval_allowed(server, retrieval)?;
    media["url"] = json!(retrieval);
    media["provider"] = json!(server);
    media["state"] = json!("uploaded");
    store(&s, &media)?;
    report(&s, app.as_ref(), &id, "uploaded", size, size, None);
    Ok(public(&media))
}
pub fn from_message(s: &AppState, message: &str, id: &str) -> Result<Value> {
    let raw: String =
        s.db.lock()
            .unwrap()
            .query_row("SELECT body FROM messages WHERE id=?", [message], |r| {
                r.get(0)
            })?;
    let body: Value = serde_json::from_str(&secure::unseal(s, &raw)?)?;
    body["attachments"]
        .as_array()
        .and_then(|a| a.iter().find(|x| x["id"] == id))
        .cloned()
        .context("Attachment not found in this message")
}
pub async fn retrieve(app: Option<tauri::AppHandle>, s: crate::Shared, v: &Value) -> Result<Value> {
    let result = retrieve_inner(app.clone(), s.clone(), v).await;
    if let Err(ref e) = result {
        if let Some(id) = v["id"].as_str() {
            report(&s, app.as_ref(), id, "failed", 0, 0, Some(e.to_string()));
        }
    }
    result
}
async fn retrieve_inner(
    app: Option<tauri::AppHandle>,
    s: crate::Shared,
    v: &Value,
) -> Result<Value> {
    let id = v["id"].as_str().context("Attachment missing")?.to_owned();
    uuid::Uuid::parse_str(&id)?;
    let m = from_message(&s, v["message"].as_str().context("Message missing")?, &id)?;
    retrieve_reference(app, s, m).await
}
pub async fn retrieve_reference(
    app: Option<tauri::AppHandle>,
    s: crate::Shared,
    m: Value,
) -> Result<Value> {
    use tokio::io::AsyncWriteExt;
    let id = m["id"].as_str().context("Attachment missing")?.to_owned();
    uuid::Uuid::parse_str(&id)?;
    let url = m["url"].as_str().context("Attachment URL missing")?;
    let provider = m["provider"].as_str().context("Provider missing")?;
    crate::social_network::retrieval_allowed(provider, url)?;
    let size = m["encryptedSize"]
        .as_u64()
        .context("Attachment size missing")?;
    let cfg = identity::config(&s)?;
    ensure!(
        size <= cfg["maxFileBytes"].as_u64().unwrap_or(20 * 1024 * 1024) + 2 * 1024 * 1024,
        "Attachment exceeds your local download bound"
    );
    let hash = m["encryptedSha256"].as_str().context("Hash missing")?;
    ensure!(
        hash.len() == 64 && hash.bytes().all(|b| b.is_ascii_hexdigit()),
        "Invalid attachment hash"
    );
    let dir = s.root.join("cache/media/downloads");
    tokio::fs::create_dir_all(&dir).await?;
    let path = dir.join(format!("{id}.age"));
    let part = dir.join(format!("{id}.part"));
    let token = CancellationToken::new();
    begin(&s, &id, &token)?;
    let _guard = TransferGuard {
        state: s.clone(),
        app: app.clone(),
        id: id.clone(),
        token: token.clone(),
    };
    report(&s, app.as_ref(), &id, "waiting", 0, 0, None);
    let _slot = s.transfers.slots.acquire().await?;
    if path.is_file() {
        let p = path.clone();
        let expected = hash.to_owned();
        if tokio::task::spawn_blocking(move || file_hash(&p).map(|v| v == expected)).await??
            == false
        {
            tokio::fs::remove_file(&path).await?;
        }
    }
    if !path.is_file()
        && tokio::fs::metadata(&part)
            .await
            .is_ok_and(|m| m.len() >= size)
    {
        let p = part.clone();
        let expected = hash.to_owned();
        if tokio::task::spawn_blocking(move || file_hash(&p).map(|v| v == expected)).await?? {
            tokio::fs::rename(&part, &path).await?;
        } else {
            tokio::fs::remove_file(&part).await?;
        }
    }
    if !path.is_file() {
        let offset = tokio::fs::metadata(&part)
            .await
            .map(|m| m.len())
            .unwrap_or(0);
        ensure!(offset <= size, "Partial file exceeds expected size");
        let mut q = client(&s)?.get(url);
        if offset > 0 {
            q = q.header(reqwest::header::RANGE, format!("bytes={offset}-"));
        }
        let response = tokio::select! {_ = token.cancelled()=>bail!("Download canceled; retry to resume"),r=q.send()=>r?};
        let append = response.status() == reqwest::StatusCode::PARTIAL_CONTENT && offset > 0;
        if append {
            ensure!(
                response
                    .headers()
                    .get("content-range")
                    .and_then(|x| x.to_str().ok())
                    .unwrap_or("")
                    .starts_with(&format!("bytes {offset}-")),
                "Invalid resume range"
            );
        }
        ensure!(
            response.status().is_success(),
            "Media server returned HTTP {}",
            response.status().as_u16()
        );
        let mut output = tokio::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .append(append)
            .truncate(!append)
            .open(&part)
            .await?;
        let mut done = if append { offset } else { 0 };
        let mut chunks = response.bytes_stream();
        loop {
            let next = tokio::select! {_=token.cancelled()=>bail!("Download canceled; the partial file is retained for resume."),next=chunks.next()=>next};
            let Some(chunk) = next else { break };
            let bytes = chunk?;
            done += bytes.len() as u64;
            ensure!(done <= size, "Media server exceeded the expected size");
            output.write_all(&bytes).await?;
            if done % (4 * 1024 * 1024) < bytes.len() as u64 || done == size {
                report(&s, app.as_ref(), &id, "downloading", done, size, None)
            }
        }
        output.flush().await?;
        drop(output);
        ensure!(done == size, "Download interrupted; retry to resume");
        let p = part.clone();
        let got = tokio::task::spawn_blocking(move || file_hash(&p)).await??;
        if got != hash {
            tokio::fs::remove_file(&part).await?;
            bail!(
                "Encrypted attachment failed integrity verification. Retry downloads a fresh copy."
            );
        }
        tokio::fs::rename(&part, &path).await?;
    }
    let extension = match m["type"].as_str().unwrap_or("") {
        "image/jpeg" => "jpg",
        "image/png" => "png",
        "image/gif" => "gif",
        "image/webp" => "webp",
        "video/mp4" => "mp4",
        "video/webm" | "audio/webm" => "webm",
        "audio/ogg" => "ogg",
        "audio/mp4" => "m4a",
        _ => "bin",
    };
    let plain = s
        .root
        .join("cache/media/plain")
        .join(format!("{id}.{extension}"));
    let out = plain.clone();
    let data = m.clone();
    let canceled = token.clone();
    let result = tokio::task::spawn_blocking(move || -> Result<()> {
        std::fs::create_dir_all(out.parent().unwrap())?;
        let key: age::x25519::Identity = data["key"]
            .as_str()
            .context("Attachment key missing")?
            .parse()
            .map_err(|_| anyhow::anyhow!("Invalid attachment key"))?;
        let source = std::io::BufReader::new(std::fs::File::open(path)?);
        let decryptor = age::Decryptor::new(source)?;
        let mut reader = decryptor.decrypt(std::iter::once(&key as &dyn age::Identity))?;
        let temp = out.with_extension("verified.part");
        let mut writer = std::io::BufWriter::new(std::fs::File::create(&temp)?);
        let mut hash = Sha256::new();
        let mut buffer = vec![0; 256 * 1024];
        let mut count = 0;
        loop {
            ensure!(!canceled.is_cancelled(), "Decryption canceled");
            let n = reader.read(&mut buffer)?;
            if n == 0 {
                break;
            }
            count += n as u64;
            ensure!(
                count <= data["size"].as_u64().unwrap_or(0),
                "Decrypted attachment exceeds expected size"
            );
            writer.write_all(&buffer[..n])?;
            hash.update(&buffer[..n]);
        }
        writer.flush()?;
        drop(writer);
        ensure!(
            count == data["size"] && format!("{:x}", hash.finalize()) == data["sha256"],
            "Original file failed integrity verification"
        );
        if out.exists() {
            std::fs::remove_file(&out)?;
        }
        std::fs::rename(temp, out)?;
        Ok(())
    })
    .await?;
    s.transfers.tokens.lock().unwrap().remove(&id);
    if let Err(e) = result {
        report(
            &s,
            app.as_ref(),
            &id,
            "failed",
            0,
            size,
            Some(e.to_string()),
        );
        return Err(e);
    }
    report(&s, app.as_ref(), &id, "ready", size, size, None);
    Ok(json!({"path":plain,"name":m["name"],"type":m["type"],"size":m["size"]}))
}
pub async fn save_original(s: &AppState, v: &Value) -> Result<Value> {
    let id = v["id"].as_str().context("Attachment missing")?;
    uuid::Uuid::parse_str(id)?;
    let name = v["path"].as_str().context("Choose a destination")?;
    let destination = Path::new(name);
    ensure!(destination.is_absolute(), "Choose an absolute destination");
    let entries = std::fs::read_dir(s.root.join("cache/media/plain"))?;
    let path = entries
        .filter_map(Result::ok)
        .map(|e| e.path())
        .find(|p| p.file_stem().is_some_and(|x| x == id))
        .context("Download and verify the attachment first")?;
    ensure!(
        !destination.exists(),
        "Choose a new filename; existing files are preserved"
    );
    tokio::fs::copy(path, destination).await?;
    Ok(json!(true))
}

pub fn discard_voice(s: &AppState, v: &Value) -> Result<Value> {
    let id = v["id"].as_str().context("Missing recording")?;
    uuid::Uuid::parse_str(id)?;
    let tokens = s.transfers.tokens.lock().unwrap();
    ensure!(
        !tokens.contains_key(id),
        "Wait for the voice upload to finish"
    );
    let media = load(s, id)?;
    ensure!(media["voice"] == true, "Not a voice draft");
    let file = s
        .root
        .join("cache/media/encrypted")
        .join(format!("{id}.age"));
    match std::fs::remove_file(file) {
        Ok(()) => (),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
        Err(e) => return Err(e.into()),
    }
    s.db.lock()
        .unwrap()
        .execute("DELETE FROM media WHERE id=?", [id])?;
    Ok(json!(true))
}
