use crate::{db, AppState};
use anyhow::{bail, ensure, Result};
use futures_util::StreamExt;
use serde_json::{json, Value};
use sha1::{Digest, Sha1};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};
use tokio::{io::AsyncWriteExt, sync::Semaphore};
pub struct Network {
    pub client: reqwest::Client,
    slots: Arc<Semaphore>,
}
fn allowed(url: &url::Url) -> bool {
    let h = url.host_str().unwrap_or("");
    url.scheme() == "https"
        && [
            "roblox.com",
            "rbxcdn.com",
            "mojang.com",
            "minecraft.net",
            "minecraftservices.com",
            "microsoftonline.com",
            "xboxlive.com",
            "fabricmc.net",
            "modrinth.com",
            "scdn.co",
            "spotifycdn.com",
            "bsky.app",
            "cdn.discordapp.com",
        ]
        .iter()
        .any(|d| h == *d || h.ends_with(&format!(".{d}")))
}
impl Network {
    pub fn new() -> Result<Self> {
        Ok(Self {
            client: reqwest::Client::builder()
                .user_agent("OrbitLauncher/0.2 (+local desktop client)")
                .connect_timeout(Duration::from_secs(20))
                .timeout(Duration::from_secs(180))
                .redirect(reqwest::redirect::Policy::custom(|a| {
                    if a.previous().len() > 5 || !allowed(a.url()) {
                        a.stop()
                    } else {
                        a.follow()
                    }
                }))
                .build()?,
            slots: Arc::new(Semaphore::new(4)),
        })
    }
    pub async fn request(
        &self,
        method: reqwest::Method,
        url: &str,
        body: Option<&Value>,
    ) -> Result<reqwest::Response> {
        let u = url::Url::parse(url)?;
        ensure!(allowed(&u), "Untrusted service URL");
        for attempt in 0..4 {
            let mut q = self.client.request(method.clone(), u.clone());
            if let Some(b) = body {
                q = q.json(b);
            }
            let r = q.send().await;
            match r {
                Ok(r) => {
                    let status = r.status();
                    if (status == 429 || status.is_server_error()) && attempt < 3 {
                        let wait = r
                            .headers()
                            .get("retry-after")
                            .and_then(|h| h.to_str().ok())
                            .and_then(|x| x.parse::<u64>().ok())
                            .unwrap_or(1 << attempt)
                            .clamp(1, 20);
                        tokio::time::sleep(Duration::from_secs(wait)).await;
                        continue;
                    }
                    ensure!(
                        status.is_success(),
                        "{} returned HTTP {}{}",
                        u.host_str().unwrap_or("Service"),
                        status.as_u16(),
                        if status == 403 {
                            ". Access may require application approval or account permission."
                        } else {
                            ""
                        }
                    );
                    return Ok(r);
                }
                Err(_) if attempt < 3 => {
                    tokio::time::sleep(Duration::from_secs(1 << attempt)).await
                }
                Err(_) => bail!(
                    "Unable to connect to {}. Check your connection and retry.",
                    u.host_str().unwrap_or("service")
                ),
            }
        }
        bail!("Retry limit reached")
    }
    pub async fn json(&self, url: &str) -> Result<Value> {
        let _slot = self.slots.acquire().await?;
        Ok(self
            .request(reqwest::Method::GET, url, None)
            .await?
            .json()
            .await?)
    }
    pub async fn post(&self, url: &str, v: &Value) -> Result<Value> {
        let _slot = self.slots.acquire().await?;
        Ok(self
            .request(reqwest::Method::POST, url, Some(v))
            .await?
            .json()
            .await?)
    }
    pub async fn download(&self, url: &str, path: &Path, sha: &str) -> Result<()> {
        ensure!(
            sha.len() == 40 && sha.bytes().all(|b| b.is_ascii_hexdigit()),
            "Download has no trusted SHA-1 digest"
        );
        let _slot = self.slots.acquire().await?;
        if verify(path, sha).await? {
            return Ok(());
        }
        if let Some(p) = path.parent() {
            tokio::fs::create_dir_all(p).await?;
        }
        let part = path.with_extension(format!(
            "{}.part",
            path.extension().unwrap_or_default().to_string_lossy()
        ));
        for _attempt in 0..3 {
            let offset = tokio::fs::metadata(&part)
                .await
                .map(|m| m.len())
                .unwrap_or(0);
            let u = url::Url::parse(url)?;
            ensure!(allowed(&u), "Untrusted download host");
            let response = self
                .client
                .get(u)
                .header(reqwest::header::RANGE, format!("bytes={offset}-"))
                .send()
                .await?;
            if response.status() == 416 {
                let _ = tokio::fs::remove_file(&part).await;
                continue;
            }
            let append = response.status() == 206 && offset > 0;
            let response = response.error_for_status().map_err(|_| {
                anyhow::anyhow!("Download server refused a file; retry installation.")
            })?;
            if append {
                let expected = format!("bytes {offset}-");
                ensure!(
                    response
                        .headers()
                        .get("content-range")
                        .and_then(|v| v.to_str().ok())
                        .unwrap_or("")
                        .starts_with(&expected),
                    "Invalid range response"
                );
            }
            let mut f = tokio::fs::OpenOptions::new()
                .create(true)
                .write(true)
                .append(append)
                .truncate(!append)
                .open(&part)
                .await?;
            let mut stream = response.bytes_stream();
            while let Some(chunk) = stream.next().await {
                f.write_all(&chunk?).await?;
            }
            f.flush().await?;
            drop(f);
            if verify(&part, sha).await? {
                if path.exists() {
                    tokio::fs::remove_file(path).await?;
                }
                tokio::fs::rename(&part, path).await?;
                return Ok(());
            }
            let _ = tokio::fs::remove_file(&part).await;
        }
        bail!("Integrity verification failed. Retry to recover the download.")
    }
}
pub async fn verify(path: &Path, sha: &str) -> Result<bool> {
    use tokio::io::AsyncReadExt;
    let Ok(mut f) = tokio::fs::File::open(path).await else {
        return Ok(false);
    };
    let mut hash = Sha1::new();
    let mut buf = vec![0; 128 * 1024];
    loop {
        let n = f.read(&mut buf).await?;
        if n == 0 {
            break;
        }
        hash.update(&buf[..n]);
    }
    Ok(format!("{:x}", hash.finalize()).eq_ignore_ascii_case(sha))
}
pub fn safe_join(root: &Path, rel: &str) -> Result<PathBuf> {
    let p = Path::new(rel);
    ensure!(
        !p.is_absolute()
            && !rel.contains(':')
            && !rel.contains('\\')
            && p.components()
                .all(|c| matches!(c, std::path::Component::Normal(_))),
        "Unsafe relative path"
    );
    Ok(root.join(p))
}
pub async fn cached(s: &AppState, key: &str, url: &str, ttl: i64) -> Result<Value> {
    if let Some(v) = db::cache(s, key, false)? {
        return Ok(v);
    }
    let v = s.net.json(url).await?;
    db::put_cache(s, key, &v, ttl)?;
    Ok(v)
}
pub async fn image(s: &AppState, v: &Value) -> Result<Value> {
    let raw = v["url"].as_str().unwrap_or("");
    let u = url::Url::parse(raw)?;
    let h = u.host_str().unwrap_or("");
    ensure!(
        u.scheme() == "https"
            && [
                "rbxcdn.com",
                "scdn.co",
                "spotifycdn.com",
                "cdn.bsky.app",
                "cdn.discordapp.com"
            ]
            .iter()
            .any(|d| h == *d || h.ends_with(&format!(".{d}"))),
        "Only supported provider CDN images are allowed"
    );
    let id = format!("{:x}", Sha1::digest(raw.as_bytes()));
    let path = s.root.join("cache/images").join(format!("{id}.png"));
    let fresh = std::fs::metadata(&path)
        .ok()
        .and_then(|m| m.modified().ok())
        .and_then(|t| t.elapsed().ok())
        .is_some_and(|d| d.as_secs() < 7 * 86400);
    if !fresh {
        let _slot = s.net.slots.acquire().await?;
        let r = s.net.request(reqwest::Method::GET, raw, None).await?;
        ensure!(
            r.content_length().unwrap_or(0) <= 5_000_000,
            "Image too large"
        );
        let mut bytes = Vec::new();
        let mut stream = r.bytes_stream();
        while let Some(b) = stream.next().await {
            bytes.extend_from_slice(&b?);
            ensure!(bytes.len() <= 5_000_000, "Image too large");
        }
        tokio::fs::create_dir_all(path.parent().unwrap()).await?;
        tokio::fs::write(&path, bytes).await?;
    }
    Ok(json!(path))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn paths_and_hosts() {
        let r = Path::new("C:/test");
        for bad in ["../escape", "/absolute", "C:/evil", "a\\..\\x"] {
            assert!(safe_join(r, bad).is_err())
        }
        assert!(safe_join(r, "a/b.jar").is_ok());
        assert!(!allowed(
            &url::Url::parse("https://roblox.com.evil.test/a").unwrap()
        ));
        assert!(allowed(
            &url::Url::parse("https://piston-data.mojang.com/a").unwrap()
        ));
    }
}
