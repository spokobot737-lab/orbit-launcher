//! Full opt-in Orbit flow on built-in operators, using synthetic accounts and images only.
use crate::*;
use anyhow::{ensure, Context, Result};
use serde_json::json;
async fn wait(mut check: impl FnMut() -> bool) -> Result<()> {
    for _ in 0..160 {
        if check() {
            return Ok(());
        }
        tokio::time::sleep(std::time::Duration::from_millis(250)).await;
    }
    anyhow::bail!("Public Orbit flow timed out")
}
#[tokio::test]
#[ignore = "Real built-in operators: synthetic invitation, two-way messages, profile images/offline retrieval and attachment cleanup"]
async fn live_automatic_orbit_social() -> Result<()> {
    let root = std::env::temp_dir().join(format!("orbit-automatic-live-{}", uuid::Uuid::new_v4()));
    let a = social_tests::state(root.join("a"))?;
    let b = social_tests::state(root.join("b"))?;
    let result=async {
  for(s,name)in[(&a,"Orbit synthetic operator test A"),(&b,"Orbit synthetic operator test B")]{identity::save(s,&json!({"name":name,"bio":"Synthetic compatibility check; no human account"}))?;ensure!(identity::config(s)?["mode"]=="automatic","Defaults missing");}
  let input=root.join("synthetic.png");::image::RgbImage::from_fn(320,200,|x,y|::image::Rgb([(x%255)as u8,(y%255)as u8,80])).save(&input)?;
  let avatar=identity::profile_image(&json!({"path":input}))?;let banner=identity::profile_image(&json!({"path":input,"banner":true}))?;
  let mut p=identity::profile(&a)?;p["avatar"]=avatar.clone();p["banner"]=banner.clone();identity::save(&a,&p)?;
  profile_sync::work(None,a.clone()).await?;
  social::start(None,a.clone()).await?;social::start(None,b.clone()).await?;
  let aid=identity::keys(&a)?.public_key().to_hex();let bid=identity::keys(&b)?.public_key().to_hex();
  let invitation=social::invitation(&a).await?;social::import_invitation(&b,&json!({"code":invitation["code"]})).await?;social::flush(&b).await?;
  wait(||identity::contact(&a,&bid).is_ok()).await.context("public incoming request")?;
  social::accept(&a,&json!({"id":bid})).await?;social::flush(&a).await?;
  wait(||identity::contact(&b,&aid).is_ok_and(|c|c["state"]=="accepted")).await.context("public mutual acceptance")?;
  ensure!(identity::messages(&b,&json!({"peer":aid}))?.as_array().unwrap().is_empty(),"A conversation was required before listing an accepted friend");
  for(from,to,peer,own,text)in[(&a,&b,&bid,&aid,"Synthetic A to B"),(&b,&a,&aid,&bid,"Synthetic B to A")]{
   let message=social::send(from,&json!({"peer":peer,"text":text})).await?;social::flush(from).await?;
   wait(||identity::messages(to,&json!({"peer":own})).is_ok_and(|m|m.as_array().unwrap().iter().any(|m|m["id"]==message["id"]))).await.context("public two-way message")?;
   social::flush(to).await?;wait(||identity::messages(from,&json!({"peer":peer})).is_ok_and(|m|m.as_array().unwrap().iter().any(|m|m["id"]==message["id"]&&m["state"]=="delivered"))).await.context("public delivery receipt")?;
  }
  social::stop(&b).await;
  let mut p=identity::profile(&a)?;p["name"]=json!("Orbit synthetic updated A");p["bio"]=json!("New signed profile retrieved while owner is closed");identity::save(&a,&p)?;
  profile_sync::work(None,a.clone()).await?;social::flush(&a).await?;
  let file=root.join("synthetic.bin");std::fs::write(&file,vec![17u8;128*1024])?;
  let prepared=media::prepare(None,a.clone(),&json!({"path":file})).await?;media::upload(None,a.clone(),&json!({"id":prepared["id"]})).await?;
  let attachment=social::send(&a,&json!({"peer":bid,"text":"Synthetic encrypted file","attachments":[prepared["id"]]})).await?;social::flush(&a).await?;
  social::stop(&a).await;social::start(None,b.clone()).await?;
  wait(||identity::contact(&b,&aid).is_ok_and(|c|c["name"]=="Orbit synthetic updated A")).await.context("owner-offline signed profile")?;
  profile_sync::work(None,b.clone()).await?;
  wait(||identity::contact(&b,&aid).is_ok_and(|c|c["avatar"]==avatar&&c["banner"]==banner)).await.context("public avatar/banner download")?;
  wait(||identity::messages(&b,&json!({"peer":aid})).is_ok_and(|m|m.as_array().unwrap().iter().any(|m|m["id"]==attachment["id"]))).await.context("offline attachment metadata")?;
  let downloaded=media::retrieve(None,b.clone(),&json!({"id":prepared["id"],"message":attachment["id"]})).await?;
  ensure!(media::file_hash(std::path::Path::new(downloaded["path"].as_str().unwrap()))?==media::file_hash(&file)?,"Public file integrity failed");
  eprintln!("PUBLIC_ORBIT PASS automatic defaults, signed invitation/mutual acceptance, accepted friend before messages, two-way delivery/receipts, owner-offline complete signed profile, retained encrypted attachment and hash");
  Ok::<_,anyhow::Error>(())
 }.await;
    for s in [&a, &b] {
        social::stop(s).await;
    }
    let http = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(std::time::Duration::from_secs(20))
        .build()?;
    let mut cleanup_error = None;
    for s in [&a, &b] {
        let ids: Vec<String> = {
            let c = s.db.lock().unwrap();
            let mut q = c.prepare("SELECT id FROM media")?;
            let rows = q
                .query_map([], |r| r.get(0))?
                .collect::<Result<Vec<_>, _>>()?;
            rows
        };
        for id in ids {
            let m = media::load(s, &id)?;
            if m["state"] == "uploaded" {
                let provider = m["provider"].as_str().unwrap();
                let hash = m["encryptedSha256"].as_str().unwrap();
                let deleted = http
                    .delete(format!("{provider}/{hash}"))
                    .header(
                        "Authorization",
                        operator_tests::authorization(&identity::keys(s)?, "delete", hash)?,
                    )
                    .send()
                    .await;
                match deleted {
                    Ok(r) if r.status().is_success() => {
                        eprintln!("PUBLIC_ORBIT CLEANUP {provider} deleted synthetic blob")
                    }
                    other => {
                        cleanup_error = Some(format!("Synthetic cleanup failed: {other:?}"));
                    }
                }
            }
        }
        for key in ["nostr-key", "data-key", "evm-wallet"] {
            secure::remove(s, key)?;
        }
    }
    result?;
    if let Some(error) = cleanup_error {
        anyhow::bail!(error)
    }
    Ok(())
}
