use crate::*;
use anyhow::Result;
use serde_json::json;
#[tokio::test]
async fn peer_wallet_proof_and_independent_expiring_activity() -> Result<()> {
    use crate::social_tests::{eventually, fixture, state};
    let root = std::env::temp_dir().join(format!("orbit-peer-{}", uuid::Uuid::new_v4()));
    let (relay, _, server, _) = fixture(&root).await?;
    let a = state(root.join("a"))?;
    let b = state(root.join("b"))?;
    for (s, name) in [(&a, "Synthetic Alice"), (&b, "Synthetic Bob")] {
        identity::save(
            s,
            &json!({"name":name,"gameVisibility":true,"musicVisibility":true}),
        )?;
        identity::save_config(s, &json!({"enabled":true,"relays":[relay]}))?;
        social::start(None, s.clone()).await?;
    }
    let aid = identity::keys(&a)?.public_key().to_hex();
    let bid = identity::keys(&b)?.public_key().to_hex();
    let invitation = social::invitation(&a).await?;
    social::import_invitation(&b, &json!({"code":invitation["code"]})).await?;
    social::flush(&b).await?;
    eventually(|| identity::contact(&a, &bid).is_ok()).await?;
    social::accept(&a, &json!({"id":bid})).await?;
    social::flush(&a).await?;
    eventually(|| identity::contact(&b, &aid).is_ok_and(|c| c["state"] == "accepted")).await?;
    let wallet = wallet::create(&a)?;
    let mut updated = identity::profile(&a)?;
    updated["bio"] = json!("Synthetic updated bio");
    identity::save(&a, &updated)?;
    social::share_profile(&a).await?;
    social::flush(&a).await?;
    eventually(|| identity::contact(&b, &aid).is_ok_and(|c| c["bio"] == "Synthetic updated bio"))
        .await?;
    let request =
        social::wallet_request(&b, &json!({"peer":aid,"address":wallet["address"]})).await?;
    social::flush(&b).await?;
    eventually(|| {
        identity::messages(&a, &json!({"peer":bid})).is_ok_and(|v| {
            v.as_array()
                .unwrap()
                .iter()
                .any(|m| m["id"] == request["id"])
        })
    })
    .await?;
    social::wallet_approve(&a, &json!({"id":request["id"]})).await?;
    social::flush(&a).await?;
    eventually(|| {
        identity::contact(&b, &aid).is_ok_and(|c| c["wallet"]["address"] == wallet["address"])
    })
    .await?;
    assert_eq!(identity::contact(&b, &aid)?["wallet"]["verifiedBy"], bid);
    activity::game_started(
        &a,
        &json!({"name":"Synthetic test process","kind":"program","id":"test"}),
        Some(std::process::id()),
    );
    *a.social.music.lock().unwrap() = json!({"title":"Synthetic track","artist":"Synthetic artist","playing":true,"source":"test-fixture","expires":db::now()+90});
    activity::publish(&a).await?;
    social::flush(&a).await?;
    eventually(|| activity::current(&b).is_ok_and(|v| v["friends"].as_array().unwrap().len() == 2))
        .await?;
    b.db.lock()
        .unwrap()
        .execute("UPDATE friend_activity SET expires=0", [])?;
    assert!(activity::current(&b)?["friends"]
        .as_array()
        .unwrap()
        .is_empty());
    let reopened = state(b.root.clone())?;
    assert_eq!(identity::contact(&reopened, &aid)?["state"], "accepted");
    for s in [&a, &b] {
        social::stop(s).await;
        for key in ["nostr-key", "data-key", "evm-wallet"] {
            secure::remove(s, key)?;
        }
    }
    server.abort();
    Ok(())
}
