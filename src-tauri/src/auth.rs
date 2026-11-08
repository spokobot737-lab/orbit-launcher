use crate::{db, AppState};
use anyhow::{bail, ensure, Context, Result};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
};
fn vault() -> Result<keyring::Entry> {
    Ok(keyring::Entry::new(
        "app.orbit.launcher",
        "minecraft-account",
    )?)
}
fn store(value: &Value) -> Result<()> {
    let mut safe = value.clone();
    safe.as_object_mut()
        .context("Invalid account")?
        .remove("accessToken");
    safe.as_object_mut().unwrap().remove("expires");
    vault()?
        .set_password(&safe.to_string())
        .context("Windows Credential Manager could not save this account")?;
    Ok(())
}
fn load() -> Result<Value> {
    let raw = vault()?
        .get_password()
        .context("No saved Microsoft account. Sign in first.")?;
    Ok(serde_json::from_str(&raw)?)
}
pub fn account(_s: &AppState) -> Result<Value> {
    match load() {
        Ok(v) => Ok(json!({"name":v["name"],"id":v["id"],"authenticated":true})),
        Err(_) => Ok(Value::Null),
    }
}
pub fn signout(_s: &AppState) -> Result<Value> {
    match vault()?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(json!(true)),
        Err(e) => Err(e.into()),
    }
}
fn client_id(s: &AppState) -> Result<String> {
    let v = db::setting(s, "clientId")?;
    let id = v.as_str().unwrap_or("");
    ensure!(uuid::Uuid::parse_str(id).is_ok(),"Microsoft sign-in needs your own application registration. Add its public client ID in Settings. Minecraft API access may require Microsoft approval.");
    Ok(id.to_owned())
}
async fn token(s: &AppState, form: &[(&str, &str)]) -> Result<Value> {
    let r = s
        .net
        .client
        .post("https://login.microsoftonline.com/consumers/oauth2/v2.0/token")
        .form(form)
        .send()
        .await?;
    ensure!(r.status().is_success(),"Microsoft token exchange failed (HTTP {}). Check registration, redirect URI, consent, or sign in again.",r.status().as_u16());
    Ok(r.json().await?)
}
pub async fn signin(s: &AppState) -> Result<Value> {
    let client = client_id(s)?;
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let redirect = format!("http://localhost:{}/", listener.local_addr()?.port());
    let state = uuid::Uuid::new_v4().to_string();
    let verifier = format!(
        "{}{}",
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    );
    let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
    let mut u =
        url::Url::parse("https://login.microsoftonline.com/consumers/oauth2/v2.0/authorize")?;
    u.query_pairs_mut().extend_pairs([
        ("client_id", client.as_str()),
        ("response_type", "code"),
        ("redirect_uri", &redirect),
        ("response_mode", "query"),
        ("scope", "XboxLive.signin offline_access"),
        ("state", &state),
        ("code_challenge", &challenge),
        ("code_challenge_method", "S256"),
        ("prompt", "select_account"),
    ]);
    open::that_detached(u.as_str())?;
    let code=tokio::time::timeout(std::time::Duration::from_secs(180),async {loop{let(mut stream,_)=listener.accept().await?;let mut buf=vec![0;16384];let n=tokio::time::timeout(std::time::Duration::from_secs(5),stream.read(&mut buf)).await??;let request=String::from_utf8_lossy(&buf[..n]);let path=request.lines().next().unwrap_or("").split_whitespace().nth(1).unwrap_or("/");let parsed=url::Url::parse(&format!("http://localhost{path}"))?;let query:std::collections::HashMap<_,_>=parsed.query_pairs().into_owned().collect();if query.get("state")!=Some(&state){let _=stream.write_all(b"HTTP/1.1 400 Bad Request\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").await;continue;}let _=stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nConnection: close\r\n\r\nReturn to Orbit to finish signing in. You may close this tab.").await;if query.contains_key("error"){bail!("Microsoft sign-in was declined or registration is not configured.");}return query.get("code").cloned().ok_or_else(||anyhow::anyhow!("Missing authorization code"));}}).await.context("Sign-in timed out after 3 minutes. Retry when ready.")??;
    let oauth = token(
        s,
        &[
            ("client_id", &client),
            ("grant_type", "authorization_code"),
            ("code", &code),
            ("redirect_uri", &redirect),
            ("code_verifier", &verifier),
        ],
    )
    .await?;
    let stored = exchange(s, &oauth, &client).await?;
    store(&stored)?;
    account(s)
}
async fn exchange(s: &AppState, oauth: &Value, client: &str) -> Result<Value> {
    let msa = oauth["access_token"]
        .as_str()
        .context("Microsoft did not return an access token")?;
    let xbox=s.net.post("https://user.auth.xboxlive.com/user/authenticate",&json!({"Properties":{"AuthMethod":"RPS","SiteName":"user.auth.xboxlive.com","RpsTicket":format!("d={msa}")},"RelyingParty":"http://auth.xboxlive.com","TokenType":"JWT"})).await.context("Xbox account sign-in failed. Verify the account has an Xbox profile and appropriate family permissions.")?;
    let xt = xbox["Token"].as_str().context("Xbox token missing")?;
    let xsts=s.net.post("https://xsts.auth.xboxlive.com/xsts/authorize",&json!({"Properties":{"SandboxId":"RETAIL","UserTokens":[xt]},"RelyingParty":"rp://api.minecraftservices.com/","TokenType":"JWT"})).await.context("Xbox authorization failed. Check account age, family settings, region, and Xbox profile.")?;
    let uhs = xsts["DisplayClaims"]["xui"][0]["uhs"]
        .as_str()
        .context("Xbox identity missing")?;
    let xst = xsts["Token"].as_str().context("XSTS token missing")?;
    let mc=s.net.post("https://api.minecraftservices.com/authentication/login_with_xbox",&json!({"identityToken":format!("XBL3.0 x={uhs};{xst}")})).await.context("Minecraft authentication failed. An HTTP 403 can mean this application's Minecraft API access has not been approved.")?;
    let access = mc["access_token"]
        .as_str()
        .context("Minecraft token missing")?;
    let ent = s
        .net
        .client
        .get("https://api.minecraftservices.com/entitlements/mcstore")
        .bearer_auth(access)
        .send()
        .await?;
    ensure!(
        ent.status().is_success(),
        "Minecraft entitlement check failed (HTTP {})",
        ent.status().as_u16()
    );
    let e: Value = ent.json().await?;
    ensure!(
        e["items"]
            .as_array()
            .is_some_and(|a| a.iter().any(|i| matches!(
                i["name"].as_str(),
                Some("game_minecraft" | "product_minecraft")
            ))),
        "This Microsoft account has no Minecraft Java entitlement."
    );
    let p = s
        .net
        .client
        .get("https://api.minecraftservices.com/minecraft/profile")
        .bearer_auth(access)
        .send()
        .await?;
    ensure!(p.status().is_success(),"Minecraft Java profile is unavailable (HTTP {}). Complete your Java profile at minecraft.net.",p.status().as_u16());
    let p: Value = p.json().await?;
    ensure!(
        p["id"].is_string() && p["name"].is_string(),
        "Invalid Minecraft profile response"
    );
    Ok(
        json!({"id":p["id"],"name":p["name"],"accessToken":access,"refreshToken":oauth["refresh_token"],"clientId":client,"xuid":xsts["DisplayClaims"]["xui"][0]["xid"],"expires":db::now()+mc["expires_in"].as_i64().unwrap_or(3600)}),
    )
}
pub async fn session(s: &AppState) -> Result<Value> {
    let old = load()?;
    let client = client_id(s)?;
    ensure!(
        old["clientId"] == client,
        "Application registration changed. Sign in again."
    );
    let refresh = old["refreshToken"]
        .as_str()
        .context("Session cannot refresh. Sign in again.")?;
    let oauth = token(
        s,
        &[
            ("client_id", &client),
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh),
            ("scope", "XboxLive.signin offline_access"),
        ],
    )
    .await?;
    let mut new = exchange(s, &oauth, &client).await?;
    if new["refreshToken"].is_null() {
        new["refreshToken"] = old["refreshToken"].clone();
    }
    store(&new)?;
    Ok(new)
}
