use crate::{connections, publisher, secure, AppState, Shared};
use anyhow::{ensure, Context, Result};
use atrium_api::{
    agent::{Agent, SessionManager},
    types::string::Did,
};
use atrium_common::store::Store;
use atrium_identity::{
    did::{CommonDidResolver, CommonDidResolverConfig, DEFAULT_PLC_DIRECTORY_URL},
    handle::{
        AtprotoHandleResolver, AtprotoHandleResolverConfig, DohDnsTxtResolver,
        DohDnsTxtResolverConfig,
    },
};
use atrium_oauth::{
    store::{
        session::{Session, SessionStore},
        state::MemoryStateStore,
    },
    AtprotoClientMetadata, AtprotoLocalhostClientMetadata, AuthMethod, AuthorizeOptions,
    DefaultHttpClient, GrantType, KnownScope, OAuthClient, OAuthClientConfig, OAuthResolverConfig,
    Scope,
};
use serde_json::{json, Value};
use std::sync::Arc;
#[derive(Clone)]
struct ProtectedSessions(Shared);
impl Store<Did, Session> for ProtectedSessions {
    type Error = std::io::Error;
    async fn get(&self, key: &Did) -> Result<Option<Session>, Self::Error> {
        Ok(
            secure::get(&self.0, &format!("bsky-session:{}", key.as_str()))
                .ok()
                .and_then(|s| serde_json::from_str(&s).ok()),
        )
    }
    async fn set(&self, key: Did, value: Session) -> Result<(), Self::Error> {
        let raw = serde_json::to_string(&value).map_err(std::io::Error::other)?;
        secure::set(&self.0, &format!("bsky-session:{}", key.as_str()), &raw)
            .map_err(|_| std::io::Error::other("Could not protect Bluesky session"))
    }
    async fn del(&self, key: &Did) -> Result<(), Self::Error> {
        secure::remove(&self.0, &format!("bsky-session:{}", key.as_str()))
            .map_err(|_| std::io::Error::other("Could not remove Bluesky session"))
    }
    async fn clear(&self) -> Result<(), Self::Error> {
        Ok(())
    }
}
impl SessionStore for ProtectedSessions {}
pub async fn connect(s: Shared, v: &Value) -> Result<Value> {
    let handle = v["handle"]
        .as_str()
        .context("Enter your Bluesky handle")?
        .trim()
        .trim_start_matches('@');
    ensure!(
        handle.len() <= 253 && !handle.contains('/') && handle.contains('.'),
        "Enter a Bluesky handle such as name.bsky.social"
    );
    let _lock = s
        .social
        .wallet_lock
        .try_lock()
        .map_err(|_| anyhow::anyhow!("Another browser connection is active"))?;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:43825")
        .await
        .context("Bluesky callback port 43825 is in use")?;
    let redirect = "http://127.0.0.1:43825/callback".to_owned();
    let http = Arc::new(DefaultHttpClient::default());
    let resolver = OAuthResolverConfig {
        did_resolver: CommonDidResolver::new(CommonDidResolverConfig {
            plc_directory_url: DEFAULT_PLC_DIRECTORY_URL.into(),
            http_client: http.clone(),
        }),
        handle_resolver: AtprotoHandleResolver::new(AtprotoHandleResolverConfig {
            dns_txt_resolver: DohDnsTxtResolver::new(DohDnsTxtResolverConfig {
                service_url: "https://cloudflare-dns.com/dns-query".into(),
                http_client: http.clone(),
            }),
            http_client: http.clone(),
        }),
        authorization_server_metadata: Default::default(),
        protected_resource_metadata: Default::default(),
    };
    let state_store = MemoryStateStore::default();
    let session_store = ProtectedSessions(s.clone());
    let scopes = vec![Scope::Known(KnownScope::Atproto)];
    let publisher_id = publisher::id("bluesky");
    let client = if let Some(id) = publisher_id {
        OAuthClient::new(OAuthClientConfig {
            client_metadata: AtprotoClientMetadata {
                client_id: id.into(),
                client_uri: Some("https://github.com/spokobot737-lab/orbit-launcher".into()),
                redirect_uris: vec![redirect],
                token_endpoint_auth_method: AuthMethod::None,
                grant_types: vec![GrantType::AuthorizationCode, GrantType::RefreshToken],
                scopes: scopes.clone(),
                jwks_uri: None,
                token_endpoint_auth_signing_alg: None,
            },
            keys: None,
            resolver,
            state_store,
            session_store,
        })?
    } else {
        OAuthClient::new(OAuthClientConfig {
            client_metadata: AtprotoLocalhostClientMetadata {
                redirect_uris: Some(vec![redirect]),
                scopes: Some(scopes.clone()),
            },
            keys: None,
            resolver,
            state_store,
            session_store,
        })?
    };
    let url = client
        .authorize(
            handle,
            AuthorizeOptions {
                scopes,
                ..Default::default()
            },
        )
        .await
        .context("Bluesky account discovery or authorization request failed")?;
    open::that_detached(url)?;
    let query = connections::callback(listener, "").await?;
    let params = serde_json::from_value(
        json!({"code":query.get("code"),"state":query.get("state"),"iss":query.get("iss")}),
    )?;
    let (session, _) = client
        .callback(params)
        .await
        .context("Bluesky callback or token verification failed")?;
    let did = session
        .did()
        .await
        .context("Bluesky session did not identify an account")?;
    let agent = Agent::new(session);
    let profile = agent
        .api
        .app
        .bsky
        .actor
        .get_profile(
            atrium_api::app::bsky::actor::get_profile::ParametersData {
                actor: did.as_str().parse().map_err(anyhow::Error::msg)?,
            }
            .into(),
        )
        .await
        .context("Could not read the authenticated Bluesky profile")?;
    let raw = serde_json::to_value(profile)?;
    let name = raw["displayName"]
        .as_str()
        .or(raw["handle"].as_str())
        .unwrap_or(did.as_str());
    let data = json!({"provider":"bluesky","id":did.as_str(),"name":name,"handle":raw["handle"],"avatarUrl":raw["avatar"],"url":format!("https://bsky.app/profile/{}",did.as_str()),"authenticated":true,"verification":"oauth","visible":false,"clientMode":if publisher_id.is_some(){"publisher"}else{"local-development"}});
    connections::save(&s, "bluesky", &data)
}
pub async fn posts(s: &AppState, v: &Value) -> Result<Value> {
    let all = connections::list(s)?;
    let connection = &all["bluesky"];
    let actor = connection["id"]
        .as_str()
        .or(connection["url"]
            .as_str()
            .and_then(|s| s.strip_prefix("https://bsky.app/profile/")))
        .context("Connect Bluesky or add its manual profile link first")?;
    let mut url = url::Url::parse("https://public.api.bsky.app/xrpc/app.bsky.feed.getAuthorFeed")?;
    url.query_pairs_mut()
        .append_pair("actor", actor)
        .append_pair("limit", "20")
        .append_pair("filter", "posts_no_replies");
    if let Some(cursor) = v["cursor"].as_str() {
        ensure!(cursor.len() < 2000, "Invalid page cursor");
        url.query_pairs_mut().append_pair("cursor", cursor);
    }
    let response = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()?
        .get(url)
        .send()
        .await?;
    ensure!(
        response.status().is_success(),
        "Bluesky public posts returned HTTP {}",
        response.status().as_u16()
    );
    Ok(response.json().await?)
}
