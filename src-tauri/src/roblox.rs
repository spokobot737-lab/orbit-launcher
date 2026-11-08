use crate::{db, net, post_launch, AppState};
use anyhow::{bail, ensure, Result};
use serde_json::{json, Value};
pub async fn home(s: &AppState) -> Result<Value> {
    let body = json!({"pageType":"Home","sessionId":uuid::Uuid::new_v4().to_string()});
    match s.net.post("https://apis.roblox.com/discovery-api/omni-recommendation",&body).await {
        Ok(v) => Ok(json!({"source":"Roblox Home","data":v})),
        Err(e) if e.to_string().contains("HTTP 401") => bail!("Roblox Home requires an authenticated Roblox session (HTTP 401). Roblox does not document an Open Cloud OAuth scope for this feed. Public Explore is available separately; it is not your personalized Home feed."),
        Err(e) => Err(e),
    }
}
pub async fn explore(s: &AppState) -> Result<Value> {
    let key = "roblox:explore";
    if let Some(v) = db::cache(s, key, false)? {
        return Ok(json!({"source":"Public Roblox Explore","stale":false,"data":v}));
    }
    match s
        .net
        .json(&format!(
            "https://apis.roblox.com/explore-api/v1/get-sorts?sessionId={}",
            uuid::Uuid::new_v4()
        ))
        .await
    {
        Ok(v) => {
            db::put_cache(s, key, &v, 600)?;
            Ok(json!({"source":"Public Roblox Explore","stale":false,"data":v}))
        }
        Err(e) => {
            if let Some(v) = db::cache(s, key, true)? {
                Ok(json!({"source":"Public Roblox Explore — cached","stale":true,"data":v}))
            } else {
                Err(e)
            }
        }
    }
}
pub async fn search(s: &AppState, v: &Value) -> Result<Value> {
    let q = v["query"].as_str().unwrap_or("").trim();
    ensure!(
        (2..=100).contains(&q.len()),
        "Search needs 2–100 characters"
    );
    let mut u = url::Url::parse("https://apis.roblox.com/search-api/omni-search")?;
    u.query_pairs_mut()
        .append_pair("searchQuery", q)
        .append_pair("sessionId", &uuid::Uuid::new_v4().to_string())
        .append_pair("pageType", "games");
    if let Some(t) = v["pageToken"].as_str() {
        ensure!(t.len() < 4096, "Invalid page token");
        u.query_pairs_mut().append_pair("pageToken", t);
    }
    net::cached(
        s,
        &format!("search:{}:{}", q, v["pageToken"]),
        u.as_str(),
        300,
    )
    .await
}
pub async fn details(s: &AppState, v: &Value) -> Result<Value> {
    let ids = v["ids"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("Universe IDs required"))?;
    ensure!(
        !ids.is_empty() && ids.len() <= 50,
        "Request 1–50 experiences"
    );
    let ids = ids
        .iter()
        .map(|v| {
            v.as_u64()
                .filter(|n| *n > 0)
                .map(|n| n.to_string())
                .ok_or_else(|| anyhow::anyhow!("Invalid universe ID"))
        })
        .collect::<Result<Vec<_>>>()?
        .join(",");
    let data = net::cached(
        s,
        &format!("games:{ids}"),
        &format!("https://games.roblox.com/v1/games?universeIds={ids}"),
        600,
    )
    .await?;
    let thumbs=net::cached(s,&format!("thumbs:{ids}"),&format!("https://thumbnails.roblox.com/v1/games/icons?universeIds={ids}&returnPolicy=PlaceHolder&size=256x256&format=Png&isCircular=false"),86400).await?;
    Ok(json!({"games":data["data"],"thumbnails":thumbs["data"]}))
}
pub async fn launch(app: Option<&tauri::AppHandle>, s: &AppState, v: &Value) -> Result<Value> {
    let universe = v["universeId"]
        .as_u64()
        .filter(|x| *x > 0)
        .ok_or_else(|| anyhow::anyhow!("Invalid universe ID"))?;
    let d = details(s, &json!({"ids":[universe]})).await?;
    let game = d["games"]
        .as_array()
        .and_then(|a| a.first())
        .ok_or_else(|| anyhow::anyhow!("Experience no longer exists"))?;
    let place = game["rootPlaceId"]
        .as_u64()
        .filter(|x| *x > 0)
        .ok_or_else(|| anyhow::anyhow!("Experience has no playable place"))?;
    #[cfg(windows)]
    {
        let mut c = tokio::process::Command::new("reg.exe");
        c.args(["query", "HKEY_CLASSES_ROOT\\roblox", "/ve"])
            .creation_flags(0x08000000);
        ensure!(c.output().await?.status.success(),"Roblox runtime is not registered. Install Roblox from roblox.com, sign in there, then retry.");
    }
    open::that_detached(format!("roblox://placeId={place}"))?;
    let id = format!("roblox-{universe}");
    let old = db::item(s, &id).unwrap_or(json!({}));
    db::save_item(
        s,
        &json!({"id":id,"kind":"roblox","name":game["name"],"universeId":universe,"rootPlaceId":place,"image":d["thumbnails"][0]["imageUrl"],"favorite":old["favorite"].as_bool().unwrap_or(false)}),
    )?;
    db::record(s, &id)?;
    if let Some(app) = app {
        post_launch(app, s);
    }
    Ok(
        json!({"dispatched":true,"message":"Experience launch requested through the installed Roblox runtime. Roblox controls sign-in and join permission."}),
    )
}
