use crate::{db, publisher, secure, AppState, Shared};
use anyhow::{bail, ensure, Context, Result};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use rusqlite::params;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
pub fn list(s: &AppState) -> Result<Value> {
    let c = s.db.lock().unwrap();
    let mut q = c.prepare("SELECT provider,data FROM connections WHERE provider!='wallet'")?;
    let rows = q.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
    let mut out = json!({});
    for row in rows {
        let (k, v) = row?;
        out[k] = serde_json::from_str(&v)?;
    }
    Ok(out)
}
pub fn shared(s: &AppState) -> Result<Value> {
    let all = list(s)?;
    let mut out = json!({});
    for (provider, c) in all.as_object().context("Invalid connections")? {
        if c["visible"] == true {
            out[provider] = json!({"name":c["name"],"url":c["url"],"claim":"shared-by-contact; external ownership not independently verified by recipient"});
        }
    }
    Ok(out)
}
pub fn save(s: &AppState, provider: &str, value: &Value) -> Result<Value> {
    s.db.lock().unwrap().execute(
        "INSERT OR REPLACE INTO connections VALUES(?,?)",
        params![provider, value.to_string()],
    )?;
    crate::profile_sync::mark_dirty(s)?;
    Ok(value.clone())
}
pub fn open_profile(s: &AppState, v: &Value) -> Result<Value> {
    let provider = v["provider"].as_str().context("Choose a provider")?;
    let data = list(s)?;
    let raw = data[provider]["url"]
        .as_str()
        .context("No profile URL is available")?;
    let url = url::Url::parse(raw)?;
    ensure!(
        url.scheme() == "https" && url.username().is_empty() && url.password().is_none(),
        "Invalid profile URL"
    );
    open::that_detached(url.as_str())?;
    Ok(json!(true))
}
pub fn open_contact(s: &AppState, v: &Value) -> Result<Value> {
    let contact = crate::identity::contact(s, v["peer"].as_str().context("Choose a friend")?)?;
    ensure!(
        contact["state"] == "accepted",
        "Only accepted friends have shared profile links"
    );
    let provider = v["provider"].as_str().context("Choose a provider")?;
    let raw = contact["connections"][provider]["url"]
        .as_str()
        .context("No shared profile link")?;
    let url = url::Url::parse(raw)?;
    let host = url.host_str().unwrap_or("");
    let allowed = match provider {
        "spotify" => host == "open.spotify.com",
        "discord" => host == "discord.com",
        "roblox" => host == "www.roblox.com" || host == "roblox.com",
        "bluesky" => host == "bsky.app",
        "x" => host == "x.com" || host == "www.x.com",
        "instagram" => host == "www.instagram.com" || host == "instagram.com",
        _ => false,
    };
    ensure!(
        allowed && url.scheme() == "https" && url.username().is_empty() && url.password().is_none(),
        "Shared URL does not belong to this provider"
    );
    open::that_detached(url.as_str())?;
    Ok(json!(true))
}
pub fn manual(s: &AppState, v: &Value) -> Result<Value> {
    let provider = v["provider"].as_str().context("Choose a provider")?;
    let url = v["url"].as_str().context("Enter a profile link")?;
    let parsed = url::Url::parse(url)?;
    ensure!(
        parsed.scheme() == "https"
            && parsed.username().is_empty()
            && parsed.password().is_none()
            && parsed.query().is_none()
            && parsed.fragment().is_none(),
        "Use a plain HTTPS profile URL without credentials or tracking parameters"
    );
    let host = parsed.host_str().unwrap_or("");
    let valid = match provider {
        "instagram" => host == "www.instagram.com" || host == "instagram.com",
        "x" => host == "x.com" || host == "www.x.com",
        "discord" => host == "discord.com",
        "spotify" => host == "open.spotify.com",
        "roblox" => host == "www.roblox.com" || host == "roblox.com",
        "bluesky" => host == "bsky.app",
        _ => false,
    };
    ensure!(valid, "This URL does not belong to the selected provider");
    let old = list(s)?[provider].clone();
    ensure!(
        old.is_null() || old["authenticated"] != true,
        "Disconnect the authenticated account before replacing it with a manual link"
    );
    save(
        s,
        provider,
        &json!({"provider":provider,"url":url,"name":v["name"].as_str().unwrap_or(url),"authenticated":false,"verification":"manual-link","visible":v["visible"]==true,"connected":db::now()}),
    )
}
pub fn visibility(s: &AppState, v: &Value) -> Result<Value> {
    let provider = v["provider"].as_str().context("Choose a provider")?;
    let mut value = list(s)?[provider].clone();
    ensure!(!value.is_null(), "Account is not connected");
    value["visible"] = json!(v["visible"] == true);
    save(s, provider, &value)
}
pub async fn disconnect(s: &AppState, v: &Value) -> Result<Value> {
    let p = v["provider"].as_str().context("Choose a provider")?;
    ensure!(
        ["spotify", "discord", "x", "instagram", "roblox", "bluesky"].contains(&p),
        "Unsupported provider"
    );
    secure::remove(s, &format!("oauth:{p}"))?;
    if p == "bluesky" {
        let data = list(s)?;
        if let Some(id) = data[p]["id"].as_str() {
            secure::remove(s, &format!("bsky-session:{id}"))?;
        }
    }
    s.db.lock()
        .unwrap()
        .execute("DELETE FROM connections WHERE provider=?", [p])?;
    crate::profile_sync::mark_dirty(s)?;
    Ok(
        json!({"disconnected":true,"note":"Saved credentials removed from this device. Use the provider's authorized-app settings to revoke the external grant as well."}),
    )
}
struct Provider {
    auth: &'static str,
    token: &'static str,
    profile: &'static str,
    scope: &'static str,
    port: u16,
}
fn provider(p: &str) -> Result<Provider> {
    Ok(match p {
        "spotify" => Provider {
            auth: "https://accounts.spotify.com/authorize",
            token: "https://accounts.spotify.com/api/token",
            profile: "https://api.spotify.com/v1/me",
            scope: "user-read-private user-read-currently-playing",
            port: 43821,
        },
        "discord" => Provider {
            auth: "https://discord.com/oauth2/authorize",
            token: "https://discord.com/api/oauth2/token",
            profile: "https://discord.com/api/v10/users/@me",
            scope: "identify",
            port: 43822,
        },
        "roblox" => Provider {
            auth: "https://apis.roblox.com/oauth/v1/authorize",
            token: "https://apis.roblox.com/oauth/v1/token",
            profile: "https://apis.roblox.com/oauth/v1/userinfo",
            scope: "openid profile",
            port: 43823,
        },
        "x" => {
            ensure!(option_env!("ORBIT_X_ENABLE_BILLABLE_API")==Some("true"),"X profile API is billable and has not been enabled by Orbit's publisher. Add a manual link instead.");
            Provider {
                auth: "https://x.com/i/oauth2/authorize",
                token: "https://api.x.com/2/oauth2/token",
                profile: "https://api.x.com/2/users/me?user.fields=profile_image_url",
                scope: "users.read tweet.read offline.access",
                port: 43824,
            }
        }
        _ => bail!("Official desktop OAuth is not available for this provider in this build"),
    })
}
fn http() -> Result<reqwest::Client> {
    Ok(reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .redirect(reqwest::redirect::Policy::none())
        .build()?)
}
pub async fn oauth(s: Shared, p: &str) -> Result<Value> {
    let cfg = provider(p)?;
    let client_id=publisher::id(p).context("Account connection is not enabled in this Orbit build. The publisher must configure its registration; users do not need API keys.")?;
    let _lock = s
        .social
        .wallet_lock
        .try_lock()
        .map_err(|_| anyhow::anyhow!("Another browser connection is in progress"))?;
    let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, cfg.port))
        .await
        .context(
            "The account callback port is in use. Close the other sign-in attempt and retry.",
        )?;
    let redirect = format!("http://127.0.0.1:{}/callback", cfg.port);
    let state = uuid::Uuid::new_v4().simple().to_string();
    let verifier = format!(
        "{}{}",
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    );
    let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
    let mut url = url::Url::parse(cfg.auth)?;
    url.query_pairs_mut().extend_pairs([
        ("client_id", client_id),
        ("response_type", "code"),
        ("redirect_uri", &redirect),
        ("scope", cfg.scope),
        ("state", &state),
        ("code_challenge", &challenge),
        ("code_challenge_method", "S256"),
    ]);
    open::that_detached(url.as_str())?;
    let query = callback(listener, &state).await?;
    if p == "discord" {
        ensure!(
            query.get("iss").is_some_and(|v| v == "https://discord.com"),
            "Discord callback issuer mismatch"
        );
    }
    let code = query
        .get("code")
        .context("Provider returned no authorization code")?;
    let response = http()?
        .post(cfg.token)
        .form(&[
            ("client_id", client_id),
            ("grant_type", "authorization_code"),
            ("code", code.as_str()),
            ("redirect_uri", &redirect),
            ("code_verifier", &verifier),
        ])
        .send()
        .await?;
    ensure!(response.status().is_success(),"Account token exchange returned HTTP {}. Publisher registration, public-client support, approval or access may be missing.",response.status().as_u16());
    let mut token: Value = response.json().await?;
    let access = token["access_token"]
        .as_str()
        .context("Provider returned no access token")?;
    let response = http()?.get(cfg.profile).bearer_auth(access).send().await?;
    ensure!(response.status().is_success(),"Profile API returned HTTP {}. Check provider quota, account eligibility and publisher approval.",response.status().as_u16());
    let raw: Value = response.json().await?;
    let value = normalize(p, &raw)?;
    token["expiresAt"] = json!(db::now() + token["expires_in"].as_i64().unwrap_or(3600));
    secure::set(&s, &format!("oauth:{p}"), &token.to_string())?;
    save(&s, p, &value)
}
pub async fn callback(
    listener: tokio::net::TcpListener,
    state: &str,
) -> Result<std::collections::HashMap<String, String>> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    tokio::time::timeout(std::time::Duration::from_secs(180),async{loop{let(mut stream,_)=listener.accept().await?;let mut buffer=vec![0;32768];let n=tokio::time::timeout(std::time::Duration::from_secs(5),stream.read(&mut buffer)).await??;let request=String::from_utf8_lossy(&buffer[..n]);let path=request.lines().next().and_then(|s|s.split_whitespace().nth(1)).unwrap_or("/");let u=url::Url::parse(&format!("http://127.0.0.1{path}"))?;let query:std::collections::HashMap<_,_>=u.query_pairs().into_owned().collect();let valid=u.path()=="/callback"&&query.get("state").is_some_and(|s|state.is_empty()||s==state);let reply=if valid{"HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nConnection: close\r\n\r\nReturn to Orbit. You may close this browser tab."}else{"HTTP/1.1 400 Bad Request\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"};let _=stream.write_all(reply.as_bytes()).await;if !valid{continue}ensure!(!query.contains_key("error"),"Account authorization was declined");return Ok(query)}}).await.context("Account connection timed out after three minutes")?
}
fn normalize(p: &str, v: &Value) -> Result<Value> {
    let data = if p == "x" { &v["data"] } else { v };
    let id = if p == "roblox" {
        data["sub"].as_str()
    } else {
        data["id"].as_str()
    }
    .context("Provider profile has no identity")?;
    let name = match p {
        "spotify" => data["display_name"].as_str(),
        "discord" => data["global_name"].as_str().or(data["username"].as_str()),
        "roblox" => data["name"].as_str(),
        _ => data["name"].as_str(),
    }
    .unwrap_or(id);
    let url = match p {
        "spotify" => format!("https://open.spotify.com/user/{id}"),
        "discord" => format!("https://discord.com/users/{id}"),
        "roblox" => format!("https://www.roblox.com/users/{id}/profile"),
        "x" => format!("https://x.com/{}", data["username"].as_str().unwrap_or(id)),
        _ => String::new(),
    };
    let avatar = match p {
        "spotify" => data["images"][0]["url"].as_str(),
        "roblox" => data["picture"].as_str(),
        "x" => data["profile_image_url"].as_str(),
        _ => None,
    };
    let discord_avatar = if p == "discord" {
        data["avatar"]
            .as_str()
            .map(|hash| format!("https://cdn.discordapp.com/avatars/{id}/{hash}.png?size=128"))
    } else {
        None
    };
    Ok(
        json!({"provider":p,"id":id,"name":name,"url":url,"avatarUrl":discord_avatar.as_deref().or(avatar),"authenticated":true,"verification":"oauth","visible":false,"connected":db::now()}),
    )
}
pub async fn spotify_activity(s: &AppState) -> Result<Value> {
    let raw = secure::get(s, "oauth:spotify")
        .context("Connect Spotify to read playback on your account")?;
    let mut token: Value = serde_json::from_str(&raw)?;
    if token["expiresAt"].as_i64().unwrap_or(0) < db::now() + 60 {
        let refresh = token["refresh_token"]
            .as_str()
            .context("Spotify session expired; reconnect")?;
        let response = http()?
            .post("https://accounts.spotify.com/api/token")
            .form(&[
                ("grant_type", "refresh_token"),
                ("refresh_token", refresh),
                (
                    "client_id",
                    publisher::id("spotify").context("Spotify is unavailable in this build")?,
                ),
            ])
            .send()
            .await?;
        ensure!(
            response.status().is_success(),
            "Spotify refresh failed; reconnect your account"
        );
        let mut next: Value = response.json().await?;
        if next["refresh_token"].is_null() {
            next["refresh_token"] = token["refresh_token"].clone();
        }
        next["expiresAt"] = json!(db::now() + next["expires_in"].as_i64().unwrap_or(3600));
        secure::set(s, "oauth:spotify", &next.to_string())?;
        token = next;
    }
    let response = http()?
        .get("https://api.spotify.com/v1/me/player/currently-playing")
        .bearer_auth(
            token["access_token"]
                .as_str()
                .context("Spotify access token missing")?,
        )
        .send()
        .await?;
    if response.status().as_u16() == 204 {
        return Ok(Value::Null);
    }
    ensure!(response.status().is_success(),"Spotify playback returned HTTP {}. Account allowlist, Premium app-owner status or quota may restrict access.",response.status().as_u16());
    let raw: Value = response.json().await?;
    if raw["item"].is_null() {
        return Ok(Value::Null);
    }
    Ok(
        json!({"source":"spotify-account","title":raw["item"]["name"],"artist":raw["item"]["artists"].as_array().map(|a|a.iter().filter_map(|x|x["name"].as_str()).collect::<Vec<_>>().join(", ")),"artworkUrl":raw["item"]["album"]["images"][0]["url"],"url":raw["item"]["external_urls"]["spotify"],"playing":raw["is_playing"],"expires":db::now()+90}),
    )
}
