use crate::social_tests::{eventually, fixture, state};
use crate::*;
use anyhow::{Context, Result};
use serde_json::json;
fn root(label: &str) -> PathBuf {
    std::env::temp_dir().join(format!("orbit-upgrade-{label}-{}", uuid::Uuid::new_v4()))
}
fn cleanup(s: &AppState) -> Result<()> {
    for k in ["nostr-key", "data-key", "evm-wallet"] {
        secure::remove(s, k)?;
    }
    Ok(())
}

#[tokio::test]
async fn defaults_migrate_without_changing_identity_or_explicit_offline() -> Result<()> {
    let s = state(root("migration"))?;
    identity::save(
        &s,
        &json!({"name":"Synthetic existing profile","bio":"Preserve me"}),
    )?;
    let key = secure::get(&s, "nostr-key")?;
    let data = secure::get(&s, "data-key")?;
    let wallet = wallet::create(&s)?;
    s.db.lock()
        .unwrap()
        .execute("DELETE FROM social_config", [])?;
    let cfg = identity::config(&s)?;
    assert_eq!(cfg["mode"], "automatic");
    assert_eq!(cfg["enabled"], true);
    let invite = social::invitation(&s).await?;
    assert!(invite["code"]
        .as_str()
        .unwrap()
        .starts_with("orbit:invite:"));
    let old = json!({"enabled":false,"relays":["wss://custom.example"],"mediaServer":"https://custom.example","mediaConsent":false,"maxFileBytes":41943040});
    s.db.lock().unwrap().execute(
        "INSERT OR REPLACE INTO social_config VALUES(1,?)",
        [old.to_string()],
    )?;
    let migrated = identity::config(&s)?;
    assert_eq!(migrated["mode"], "custom");
    for k in [
        "enabled",
        "relays",
        "mediaServer",
        "mediaConsent",
        "maxFileBytes",
    ] {
        assert_eq!(migrated[k], old[k]);
    }
    s.db.lock().unwrap().execute(
        "UPDATE social_config SET data=?",
        [json!({"enabled":false,"relays":[],"mediaServer":""}).to_string()],
    )?;
    assert_eq!(identity::config(&s)?["enabled"], false);
    let auto = social_network::save(&s, &json!({"mode":"automatic","enabled":false}))?;
    assert_eq!(auto["enabled"], false);
    assert_eq!(identity::config(&s)?["enabled"], false);
    assert_eq!(secure::get(&s, "nostr-key")?, key);
    assert_eq!(secure::get(&s, "data-key")?, data);
    assert_eq!(wallet::info(&s)?, wallet);
    assert_eq!(identity::profile(&s)?["bio"], "Preserve me");
    cleanup(&s)?;
    Ok(())
}

#[tokio::test]
async fn complete_signed_profiles_images_offline_and_ordering() -> Result<()> {
    let root = root("profiles");
    let (relay, storage, server, _) = fixture(&root).await?;
    let a = state(root.join("a"))?;
    let b = state(root.join("b"))?;
    let image = root.join("synthetic.png");
    ::image::RgbImage::from_fn(800, 600, |x, y| {
        ::image::Rgb([(x % 255) as u8, (y % 255) as u8, 100])
    })
    .save(&image)?;
    let avatar = identity::profile_image(&json!({"path":image,"banner":false}))?;
    let banner = identity::profile_image(&json!({"path":image,"banner":true}))?;
    for (s, name) in [(&a, "Synthetic Alice"), (&b, "Synthetic Bob")] {
        identity::save(s, &json!({"name":name,"bio":"Synthetic profile"}))?;
        identity::save_config(
            s,
            &json!({"enabled":true,"relays":[relay],"mediaServer":storage,"mediaConsent":true,"maxFileBytes":32*1024*1024}),
        )?;
    }
    let mut own = identity::profile(&a)?;
    own["avatar"] = avatar.clone();
    own["banner"] = banner.clone();
    identity::save(&a, &own)?;
    connections::manual(
        &a,
        &json!({"provider":"spotify","url":"https://open.spotify.com/user/synthetic-orbit","name":"Synthetic music","visible":true}),
    )?;
    profile_sync::work(None, a.clone()).await?;
    for s in [&a, &b] {
        social::start(None, s.clone()).await?;
    }
    let aid = identity::keys(&a)?.public_key().to_hex();
    let bid = identity::keys(&b)?.public_key().to_hex();
    let invitation = social::invitation(&a).await?;
    social::import_invitation(&b, &json!({"code":invitation["code"]})).await?;
    social::flush(&b).await?;
    eventually(|| identity::contact(&a, &bid).is_ok())
        .await
        .context("incoming request")?;
    social::accept(&a, &json!({"id":bid})).await?;
    social::flush(&a).await?;
    eventually(|| identity::contact(&b, &aid).is_ok_and(|c| c["state"] == "accepted")).await?;
    assert!(identity::messages(&b, &json!({"peer":aid}))?
        .as_array()
        .unwrap()
        .is_empty());
    profile_sync::work(None, a.clone()).await?;
    social::flush(&a).await?;
    profile_sync::work(None, b.clone()).await?;
    eventually(|| {
        identity::contact(&b, &aid).is_ok_and(|c| c["avatar"] == avatar && c["banner"] == banner)
    })
    .await
    .context("avatar and banner hydration")?;
    let first = profile_sync::shared(&a)?.unwrap();
    profile_sync::validate(&first, identity::keys(&a)?.public_key())?;
    let visible = identity::contact(&b, &aid)?;
    assert!(visible["avatarRef"]["key"].is_null());
    assert!(visible["proof"].is_null());
    assert_eq!(visible["connections"]["spotify"]["name"], "Synthetic music");
    social::stop(&b).await;
    let mut edited = identity::profile(&a)?;
    edited["name"] = json!("Synthetic renamed Alice");
    edited["bio"] = json!("Newer revision while friend is away");
    ::image::RgbImage::from_fn(800, 600, |x, y| {
        ::image::Rgb([190, (x % 255) as u8, (y % 255) as u8])
    })
    .save(&image)?;
    let avatar2 = identity::profile_image(&json!({"path":image,"banner":false}))?;
    let banner2 = identity::profile_image(&json!({"path":image,"banner":true}))?;
    edited["avatar"] = avatar2.clone();
    edited["banner"] = banner2.clone();
    identity::save(&a, &edited)?;
    profile_sync::work(None, a.clone()).await?;
    social::flush(&a).await?;
    let second = profile_sync::shared(&a)?.unwrap();
    assert!(second["revision"].as_i64() > first["revision"].as_i64());
    social::stop(&a).await;
    social::start(None, b.clone()).await?;
    eventually(|| {
        identity::contact(&b, &aid).is_ok_and(|c| c["name"] == "Synthetic renamed Alice")
    })
    .await
    .context("offline newer profile retrieval")?;
    profile_sync::work(None, b.clone()).await?;
    assert!(!profile_sync::cache(&b, &first, "accepted")?);
    assert!(!profile_sync::cache(&b, &second, "accepted")?);
    let mut forged = second.clone();
    forged["bio"] = json!("Forged content");
    assert!(profile_sync::validate(&forged, identity::keys(&a)?.public_key()).is_err());
    assert_eq!(identity::contact(&b, &aid)?["banner"], banner2);
    let reopened = state(b.root.clone())?;
    assert_eq!(
        identity::contact(&reopened, &aid)?["name"],
        "Synthetic renamed Alice"
    );
    assert_eq!(identity::contact(&reopened, &aid)?["avatar"], avatar2);
    for s in [&a, &b] {
        social::stop(s).await;
        cleanup(s)?;
    }
    server.abort();
    Ok(())
}

#[tokio::test]
async fn storage_fallback_does_not_repeat_permanent_rejection() -> Result<()> {
    use axum::{http::StatusCode, routing::put, Router};
    use std::sync::atomic::{AtomicUsize, Ordering};
    let root = root("fallback");
    let (_, storage, server, _) = fixture(&root).await?;
    let count = Arc::new(AtomicUsize::new(0));
    let hits = count.clone();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let reject = format!("http://{}", listener.local_addr()?);
    let app = Router::new().route(
        "/upload",
        put(|| async { StatusCode::INTERNAL_SERVER_ERROR }).head(move || {
            let hits = hits.clone();
            async move {
                hits.fetch_add(1, Ordering::Relaxed);
                StatusCode::PAYLOAD_TOO_LARGE
            }
        }),
    );
    let rejecting = tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    let s = state(root.join("a"))?;
    identity::save(&s, &json!({"name":"Synthetic fallback"}))?;
    identity::save_config(
        &s,
        &json!({"enabled":true,"relays":["ws://127.0.0.1:1"],"mediaServer":storage,"mediaConsent":true,"maxFileBytes":32*1024*1024}),
    )?;
    let source = root.join("synthetic.bin");
    std::fs::write(&source, vec![42u8; 256 * 1024])?;
    for _ in 0..2 {
        let staged = media::prepare(None, s.clone(), &json!({"path":source})).await?;
        let uploaded = media::upload_providers(
            None,
            s.clone(),
            &json!({"id":staged["id"]}),
            vec![reject.clone(), storage.clone()],
        )
        .await?;
        assert_eq!(uploaded["provider"], storage);
        let reference = media::load(&s, staged["id"].as_str().unwrap())?;
        let plain = media::retrieve_reference(None, s.clone(), reference).await?;
        assert_eq!(
            media::file_hash(std::path::Path::new(plain["path"].as_str().unwrap()))?,
            media::file_hash(&source)?
        );
    }
    assert_eq!(count.load(Ordering::Relaxed), 1);
    assert!(s.transfers.tokens.lock().unwrap().is_empty());
    cleanup(&s)?;
    server.abort();
    rejecting.abort();
    Ok(())
}

#[test]
fn clock_diagnostics_do_not_backdate_signatures() {
    let mut headers = reqwest::header::HeaderMap::new();
    let date =
        httpdate::fmt_http_date(std::time::SystemTime::now() - std::time::Duration::from_secs(95));
    headers.insert(reqwest::header::DATE, date.parse().unwrap());
    assert!(social_network::clock_error(&headers)
        .unwrap()
        .contains("Sync now"));
    assert!(social_network::retrieval_allowed(
        "https://nostr.download",
        "https://unrelated.example/file"
    )
    .is_err());
}
