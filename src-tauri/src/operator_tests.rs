//! Opt-in real operator probes. Only fresh synthetic keys and generated files are used.
use anyhow::{ensure, Context, Result};
use nostr_sdk::prelude::*;
use serde_json::json;
use std::{io::Write, time::Duration};

#[tokio::test]
#[ignore = "Publishes synthetic encrypted NIP-17 events to candidate public relays"]
async fn live_default_relay_candidates() -> Result<()> {
    let urls = std::env::var("ORBIT_RELAY_PROBE")
        .unwrap_or_else(|_| "wss://nos.lol,wss://auth.nostr1.com".into());
    for url in urls.split(',') {
        let result = tokio::time::timeout(Duration::from_secs(45), async {
            let alice = Keys::generate();
            let bob = Keys::generate();
            let sender = Client::new(alice.clone());
            sender.automatic_authentication(true);
            let receiver = Client::new(bob.clone());
            receiver.automatic_authentication(true);
            for c in [&sender, &receiver] {
                c.add_relay(url).await?;
                c.connect().await;
                c.wait_for_connection(Duration::from_secs(8)).await;
            }
            let mut notifications = receiver.notifications();
            receiver
                .subscribe(
                    Filter::new()
                        .kind(Kind::GiftWrap)
                        .pubkey(bob.public_key())
                        .limit(10),
                    None,
                )
                .await?;
            let text = format!("Orbit synthetic interoperability {}", uuid::Uuid::new_v4());
            let rumor = EventBuilder::private_msg_rumor(bob.public_key(), text.clone())
                .build(alice.public_key());
            let gift = EventBuilder::gift_wrap(
                &alice,
                &bob.public_key(),
                rumor,
                [Tag::expiration(Timestamp::from(
                    crate::db::now() as u64 + 3600,
                ))],
            )
            .await?;
            let output = sender.send_event_to([url], &gift).await?;
            ensure!(
                !output.success.is_empty(),
                "No signed-event acknowledgment: {:?}",
                output.failed
            );
            tokio::time::timeout(Duration::from_secs(12), async {
                loop {
                    if let RelayPoolNotification::Event { event, .. } = notifications.recv().await?
                    {
                        if event.id == gift.id {
                            let unwrapped = receiver.unwrap_gift_wrap(&event).await?;
                            ensure!(
                                unwrapped.sender == alice.public_key()
                                    && unwrapped.rumor.content == text,
                                "Recipient decryption mismatch"
                            );
                            break;
                        }
                    }
                }
                Ok::<_, anyhow::Error>(())
            })
            .await??;
            receiver.shutdown().await;
            let offline = Client::new(bob);
            offline.automatic_authentication(true);
            offline.add_relay(url).await?;
            offline.connect().await;
            offline.wait_for_connection(Duration::from_secs(8)).await;
            let events = offline
                .fetch_events_from(
                    [url],
                    Filter::new().id(gift.id).kind(Kind::GiftWrap),
                    Duration::from_secs(12),
                )
                .await?;
            ensure!(
                events.iter().any(|e| e.id == gift.id),
                "Retained event was not retrievable after reconnect"
            );
            offline.shutdown().await;
            sender.shutdown().await;
            Ok::<_, anyhow::Error>(())
        })
        .await;
        eprintln!("PUBLIC_RELAY {} {}",url,match result {Ok(Ok(()))=>"PASS signed publication, recipient subscription/decryption, fresh-client offline retrieval; NIP-42 enabled".into(),Ok(Err(e))=>format!("FAIL {e:#}"),Err(_)=>"FAIL 45-second timeout".into()});
    }
    Ok(())
}

#[tokio::test]
#[ignore = "Uploads, verifies and deletes a synthetic encrypted 25 MiB file at candidate public providers"]
async fn live_default_storage_candidates() -> Result<()> {
    use futures_util::StreamExt;
    let dir = std::env::temp_dir().join(format!("orbit-operators-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir)?;
    let path = dir.join("synthetic.age");
    let recipient = age::x25519::Identity::generate().to_public();
    let mut stream =
        age::Encryptor::with_recipients(std::iter::once(&recipient as &dyn age::Recipient))?
            .wrap_output(std::fs::File::create(&path)?)?;
    let buffer = vec![0u8; 256 * 1024];
    for _ in 0..100 {
        stream.write_all(&buffer)?;
    }
    stream.finish()?;
    let hash = crate::media::file_hash(&path)?;
    let size = std::fs::metadata(&path)?.len();
    let keys = Keys::generate();
    let http = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(90))
        .build()?;
    for server in ["https://nostr.download", "https://files.sovbit.host"] {
        let result = async {
            let auth = authorization(&keys, "upload", &hash)?;
            let head = http
                .head(format!("{server}/upload"))
                .header("Authorization", &auth)
                .header("X-SHA-256", &hash)
                .header("X-Content-Type", "application/octet-stream")
                .header("X-Content-Length", size)
                .send()
                .await?;
            eprintln!(
                "PUBLIC_STORAGE_PREFLIGHT {} HTTP {} Date {:?} local {}",
                server,
                head.status(),
                head.headers().get("date"),
                crate::db::now()
            );
            ensure!(
                head.status().is_success() || matches!(head.status().as_u16(), 404 | 405),
                "Preflight rejected HTTP {} {:?}",
                head.status(),
                head.headers().get("x-reason")
            );
            let response = http
                .put(format!("{server}/upload"))
                .header("Authorization", auth)
                .header("Content-Type", "application/octet-stream")
                .header("Content-Length", size)
                .header("X-SHA-256", &hash)
                .body(reqwest::Body::wrap_stream(
                    tokio_util::io::ReaderStream::new(tokio::fs::File::open(&path).await?),
                ))
                .send()
                .await?;
            let status = response.status();
            let descriptor: serde_json::Value = response.json().await?;
            eprintln!(
                "PUBLIC_STORAGE_DESCRIPTOR {} HTTP {} {}",
                server, status, descriptor
            );
            ensure!(status.is_success(), "Upload rejected HTTP {status}");
            // Delete even if descriptor or retrieval validation fails.
            let check = async {
                ensure!(
                    descriptor["sha256"] == hash && descriptor["size"] == size,
                    "Descriptor mismatch"
                );
                let raw = descriptor["url"].as_str().context("No retrieval URL")?;
                let url = url::Url::parse(raw)?;
                let origin = url::Url::parse(server)?;
                ensure!(
                    url.origin() == origin.origin(),
                    "Cross-host retrieval requires explicit operator verification: {raw}"
                );
                let response = http.get(url).send().await?;
                ensure!(response.status().is_success(), "Download failed");
                let downloaded = dir.join("download.age");
                let mut file = tokio::fs::File::create(&downloaded).await?;
                let mut bytes = response.bytes_stream();
                let mut count = 0;
                while let Some(chunk) = bytes.next().await {
                    let chunk = chunk?;
                    count += chunk.len() as u64;
                    ensure!(count <= size, "Oversized response");
                    tokio::io::AsyncWriteExt::write_all(&mut file, &chunk).await?;
                }
                drop(file);
                ensure!(
                    count == size && crate::media::file_hash(&downloaded)? == hash,
                    "Downloaded hash/size mismatch"
                );
                Ok::<_, anyhow::Error>(())
            }
            .await;
            let deleted = http
                .delete(format!("{server}/{hash}"))
                .header("Authorization", authorization(&keys, "delete", &hash)?)
                .send()
                .await?;
            eprintln!("PUBLIC_STORAGE_DELETE {} HTTP {}", server, deleted.status());
            ensure!(
                deleted.status().is_success(),
                "Synthetic upload cleanup failed"
            );
            check?;
            Ok::<_, anyhow::Error>(json!({"encryptedBytes":size,"sha256":hash}))
        }
        .await;
        eprintln!(
            "PUBLIC_STORAGE {} {}",
            server,
            match result {
                Ok(v) => format!("PASS {v}"),
                Err(e) => format!("FAIL {e:#}"),
            }
        );
    }
    std::fs::remove_dir_all(dir)?;
    Ok(())
}
pub(crate) fn authorization(keys: &Keys, action: &str, hash: &str) -> Result<String> {
    use base64::Engine;
    let event = EventBuilder::new(Kind::Custom(24242), "Orbit synthetic compatibility probe")
        .tags([
            Tag::parse(["t", action])?,
            Tag::parse(["x", hash])?,
            Tag::expiration(Timestamp::from(crate::db::now() as u64 + 300)),
        ])
        .sign_with_keys(keys)?;
    Ok(format!(
        "Nostr {}",
        base64::engine::general_purpose::STANDARD.encode(event.as_json())
    ))
}
