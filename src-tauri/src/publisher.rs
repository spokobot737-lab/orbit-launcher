use serde_json::{json, Value};
pub fn id(provider: &str) -> Option<&'static str> {
    let id = match provider {
        "microsoft" => option_env!("ORBIT_MICROSOFT_CLIENT_ID"),
        "spotify" => option_env!("ORBIT_SPOTIFY_CLIENT_ID"),
        "discord" => option_env!("ORBIT_DISCORD_CLIENT_ID"),
        "x" => option_env!("ORBIT_X_CLIENT_ID"),
        "roblox" => option_env!("ORBIT_ROBLOX_CLIENT_ID"),
        "bluesky" => option_env!("ORBIT_BLUESKY_CLIENT_METADATA"),
        _ => None,
    };
    id.filter(|s| !s.trim().is_empty())
}
pub fn status() -> Value {
    json!({"xBillable":option_env!("ORBIT_X_ENABLE_BILLABLE_API")==Some("true"),"microsoft":id("microsoft").is_some(),"spotify":id("spotify").is_some(),"discord":id("discord").is_some(),"x":id("x").is_some(),"roblox":id("roblox").is_some(),"bluesky":id("bluesky").is_some()})
}
