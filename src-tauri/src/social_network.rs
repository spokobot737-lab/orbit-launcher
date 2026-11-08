//! Versioned defaults and conservative migration; never replace explicit user choices.
use crate::{db, identity, AppState};
use anyhow::{ensure, Result};
use rusqlite::OptionalExtension;
use serde_json::{json, Value};
pub const RELAYS: &[&str] = &["wss://auth.nostr1.com", "wss://relay.primal.net"];
pub const STORAGE: &[&str] = &["https://nostr.download", "https://files.sovbit.host"];
pub const TESTED_FILE_BYTES: u64 = 25 * 1024 * 1024;
pub fn defaults() -> Value {
    json!({"version":1,"mode":"automatic","configured":false,"enabled":true,"relays":RELAYS,"mediaServer":STORAGE[0],"mediaConsent":true,"maxFileBytes":TESTED_FILE_BYTES,"pauseWhileGaming":true})
}
pub fn config(s: &AppState) -> Result<Value> {
    let raw: Option<String> =
        s.db.lock()
            .unwrap()
            .query_row(
                "SELECT data FROM social_config WHERE singleton=1",
                [],
                |r| r.get(0),
            )
            .optional()?;
    let Some(raw) = raw else {
        return Ok(defaults());
    };
    let mut value: Value = serde_json::from_str(&raw)?;
    if value["version"].is_null() {
        // Old persisted rows were only written by explicit Save or backup restore.
        // Absence of a row is the only reliable untouched state in 0.2.
        value["version"] = json!(1);
        value["mode"] = json!("custom");
        value["configured"] = json!(true);
        persist(s, &value)?;
    }
    if value["mode"] == "automatic" {
        let enabled = value["enabled"].clone();
        let configured = value["configured"].clone();
        value = defaults();
        value["enabled"] = enabled;
        value["configured"] = configured;
    }
    Ok(value)
}
pub fn initialize(s: &AppState) -> Result<()> {
    let config = config(s)?;
    persist(s, &config)
}
fn persist(s: &AppState, value: &Value) -> Result<()> {
    s.db.lock().unwrap().execute(
        "INSERT OR REPLACE INTO social_config VALUES(1,?)",
        [value.to_string()],
    )?;
    Ok(())
}
pub fn save(s: &AppState, v: &Value) -> Result<Value> {
    let mut value = if v["restoreDefaults"] == true || v["mode"] == "automatic" {
        defaults()
    } else {
        let relays = v["relays"]
            .as_array()
            .ok_or_else(|| anyhow::anyhow!("Enter your custom inbox services"))?;
        ensure!(relays.len() <= 3, "Use at most three inbox services");
        for r in relays {
            identity::validate_endpoint(r.as_str().unwrap_or(""), true)?;
        }
        let server = v["mediaServer"].as_str().unwrap_or("");
        if !server.is_empty() {
            identity::validate_endpoint(server, false)?;
        }
        let max = v["maxFileBytes"].as_u64().unwrap_or(TESTED_FILE_BYTES);
        ensure!(
            (1..=2 * 1024 * 1024 * 1024).contains(&max),
            "The local file limit must be at most 2 GiB; provider limits still apply"
        );
        json!({"version":1,"mode":"custom","relays":relays,"mediaServer":server.trim_end_matches('/'),"mediaConsent":v["mediaConsent"]==true,"maxFileBytes":max,"pauseWhileGaming":true})
    };
    value["configured"] = json!(true);
    value["enabled"] = json!(v["enabled"] == true);
    ensure!(
        value["enabled"] != true || value["relays"].as_array().is_some_and(|a| !a.is_empty()),
        "Custom online mode needs an inbox service. Use Automatic to restore working defaults."
    );
    persist(s, &value)?;
    Ok(value)
}
pub fn providers(config: &Value) -> Vec<String> {
    if config["mode"] == "automatic" {
        STORAGE.iter().map(|s| s.to_string()).collect()
    } else {
        config["mediaServer"]
            .as_str()
            .filter(|s| !s.is_empty())
            .map(|s| vec![s.to_string()])
            .unwrap_or_default()
    }
}
pub fn retrieval_allowed(provider: &str, retrieval: &str) -> Result<()> {
    let origin = identity::validate_endpoint(provider, false)?;
    let url = identity::validate_endpoint(retrieval, false)?;
    ensure!(
        origin.origin() == url.origin(),
        "File service returned an unapproved download host. No authorization was forwarded."
    );
    Ok(())
}
pub fn operators() -> Value {
    json!({"relays":RELAYS,"storage":STORAGE,"testedPlaintextBytes":TESTED_FILE_BYTES,"terms":["https://auth.nostr1.com","https://primal.net/terms","https://nostr.download","https://sovbit.host"],"notice":"Messages use independent external inbox operators; encrypted files use the listed storage operators. Limits and retention may change; no permanent or unlimited free storage is promised."})
}
pub fn clock_error(headers: &reqwest::header::HeaderMap) -> Option<String> {
    let date = headers.get(reqwest::header::DATE)?.to_str().ok()?;
    let server = httpdate::parse_http_date(date)
        .ok()?
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_secs() as i64;
    let age = headers
        .get(reqwest::header::AGE)
        .and_then(|x| x.to_str().ok())
        .and_then(|s| s.parse::<i64>().ok())
        .unwrap_or(0)
        .clamp(0, 86400);
    let drift = db::now() - server - age;
    (drift.abs()>60).then(||format!("Your computer's clock differs from this service by about {} seconds. In Windows Date & time, choose Sync now, then retry. Orbit does not alter your clock or backdate signatures.",drift.abs()))
}
