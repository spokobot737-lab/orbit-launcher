use crate::AppState;
use anyhow::{ensure, Result};
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::{json, Value};
use std::path::Path;
pub fn init(root: &Path) -> Result<Connection> {
    let c = Connection::open(root.join("orbit.sqlite"))?;
    c.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON; CREATE TABLE IF NOT EXISTS items(id TEXT PRIMARY KEY,kind TEXT NOT NULL,name TEXT NOT NULL,data TEXT NOT NULL,favorite INTEGER NOT NULL DEFAULT 0,last_launch INTEGER); CREATE TABLE IF NOT EXISTS history(id INTEGER PRIMARY KEY,item_id TEXT NOT NULL,launched_at INTEGER NOT NULL); CREATE TABLE IF NOT EXISTS collections(id TEXT PRIMARY KEY,name TEXT NOT NULL); CREATE TABLE IF NOT EXISTS members(collection_id TEXT REFERENCES collections(id) ON DELETE CASCADE,item_id TEXT REFERENCES items(id) ON DELETE CASCADE,PRIMARY KEY(collection_id,item_id)); CREATE TABLE IF NOT EXISTS settings(key TEXT PRIMARY KEY,value TEXT NOT NULL); CREATE TABLE IF NOT EXISTS cache(key TEXT PRIMARY KEY,value TEXT NOT NULL,expires INTEGER NOT NULL);")?;
    Ok(c)
}
pub fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}
pub fn library(s: &AppState) -> Result<Value> {
    let c = s.db.lock().unwrap();
    let mut q = c.prepare(
        "SELECT data,favorite,last_launch FROM items ORDER BY COALESCE(last_launch,0) DESC,name",
    )?;
    let rows = q.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, bool>(1)?,
            r.get::<_, Option<i64>>(2)?,
        ))
    })?;
    let mut out = vec![];
    for r in rows {
        let (raw, fav, last) = r?;
        let mut v: Value = serde_json::from_str(&raw)?;
        v["favorite"] = json!(fav);
        v["lastLaunch"] = json!(last);
        out.push(v);
    }
    Ok(json!(out))
}
pub fn item(s: &AppState, id: &str) -> Result<Value> {
    let raw: String =
        s.db.lock()
            .unwrap()
            .query_row("SELECT data FROM items WHERE id=?", [id], |r| r.get(0))?;
    Ok(serde_json::from_str(&raw)?)
}
pub fn save_item(s: &AppState, v: &Value) -> Result<Value> {
    let id = v["id"].as_str().unwrap_or("");
    let kind = v["kind"].as_str().unwrap_or("");
    let name = v["name"].as_str().unwrap_or("");
    ensure!(
        !id.is_empty() && id.len() < 150 && !name.trim().is_empty() && name.len() < 300,
        "A valid item name and ID are required"
    );
    ensure!(
        ["program", "minecraft", "roblox"].contains(&kind),
        "Invalid item type"
    );
    if kind == "minecraft" {
        crate::minecraft::validate_profile(v)?;
        if let Ok(old) = item(s, id) {
            if old["performance"] == true {
                ensure!(v["performance"]==true&&v["version"]==old["version"]&&v["loader"]==old["loader"],"Performance presets retain their version and loader. Create another profile to change them.");
            }
        }
    }
    if kind == "program" {
        crate::programs::validate(v)?;
    }
    s.db.lock().unwrap().execute("INSERT INTO items(id,kind,name,data,favorite) VALUES(?,?,?,?,?) ON CONFLICT(id) DO UPDATE SET name=excluded.name,data=excluded.data,favorite=excluded.favorite",params![id,kind,name,v.to_string(),v["favorite"].as_bool().unwrap_or(false)])?;
    Ok(v.clone())
}
pub fn remove_item(s: &AppState, v: &Value) -> Result<Value> {
    s.db.lock().unwrap().execute(
        "DELETE FROM items WHERE id=?",
        [v["id"].as_str().unwrap_or("")],
    )?;
    Ok(json!(true))
}
pub fn record(s: &AppState, id: &str) -> Result<()> {
    let mut c = s.db.lock().unwrap();
    let t = c.transaction()?;
    t.execute(
        "UPDATE items SET last_launch=? WHERE id=?",
        params![now(), id],
    )?;
    t.execute(
        "INSERT INTO history(item_id,launched_at) VALUES(?,?)",
        params![id, now()],
    )?;
    t.commit()?;
    Ok(())
}
pub fn setting(s: &AppState, key: &str) -> Result<Value> {
    let raw: Option<String> =
        s.db.lock()
            .unwrap()
            .query_row("SELECT value FROM settings WHERE key=?", [key], |r| {
                r.get(0)
            })
            .optional()?;
    Ok(raw
        .map(|r| serde_json::from_str(&r))
        .transpose()?
        .unwrap_or(Value::Null))
}
pub fn settings(s: &AppState) -> Result<Value> {
    let c = s.db.lock().unwrap();
    let mut q = c.prepare("SELECT key,value FROM settings")?;
    let mut o = json!({});
    for r in q.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))? {
        let (k, v) = r?;
        o[k] = serde_json::from_str(&v)?;
    }
    Ok(o)
}
pub fn set_setting(s: &AppState, v: &Value) -> Result<Value> {
    let k = v["key"].as_str().unwrap_or("");
    ensure!(
        ["clientId", "afterLaunch", "robloxPublic"].contains(&k),
        "Unknown setting"
    );
    if k == "clientId" {
        let id = v["value"].as_str().unwrap_or("");
        ensure!(
            id.is_empty() || uuid::Uuid::parse_str(id).is_ok(),
            "Client ID must be a UUID"
        );
    }
    s.db.lock().unwrap().execute(
        "INSERT OR REPLACE INTO settings VALUES(?,?)",
        params![k, v["value"].to_string()],
    )?;
    Ok(json!(true))
}
pub fn collections(s: &AppState) -> Result<Value> {
    let c = s.db.lock().unwrap();
    let mut q=c.prepare("SELECT c.id,c.name,COALESCE(json_group_array(m.item_id) FILTER(WHERE m.item_id IS NOT NULL),'[]') FROM collections c LEFT JOIN members m ON c.id=m.collection_id GROUP BY c.id")?;
    let rows=q.query_map([],|r|Ok(json!({"id":r.get::<_,String>(0)?,"name":r.get::<_,String>(1)?,"items":serde_json::from_str::<Value>(&r.get::<_,String>(2)?).unwrap_or(json!([]))})))?;
    Ok(json!(rows.collect::<Result<Vec<_>, _>>()?))
}
pub fn save_collection(s: &AppState, v: &Value) -> Result<Value> {
    let name = v["name"].as_str().unwrap_or("").trim();
    ensure!(
        !name.is_empty() && name.len() < 100,
        "Collection name is required"
    );
    let id = uuid::Uuid::new_v4().to_string();
    s.db.lock()
        .unwrap()
        .execute("INSERT INTO collections VALUES(?,?)", params![id, name])?;
    Ok(json!(id))
}
pub fn collect(s: &AppState, v: &Value) -> Result<Value> {
    s.db.lock().unwrap().execute(
        if v["remove"] == true {
            "DELETE FROM members WHERE collection_id=? AND item_id=?"
        } else {
            "INSERT OR IGNORE INTO members VALUES(?,?)"
        },
        params![
            v["collection"].as_str().unwrap_or(""),
            v["id"].as_str().unwrap_or("")
        ],
    )?;
    Ok(json!(true))
}
pub fn cache(s: &AppState, key: &str, stale: bool) -> Result<Option<Value>> {
    let raw: Option<String> =
        s.db.lock()
            .unwrap()
            .query_row(
                "SELECT value FROM cache WHERE key=? AND (expires>? OR ?)",
                params![key, now(), stale],
                |r| r.get(0),
            )
            .optional()?;
    Ok(raw.map(|r| serde_json::from_str(&r)).transpose()?)
}
pub fn put_cache(s: &AppState, key: &str, v: &Value, ttl: i64) -> Result<()> {
    s.db.lock().unwrap().execute(
        "INSERT OR REPLACE INTO cache VALUES(?,?,?)",
        params![key, v.to_string(), now() + ttl],
    )?;
    Ok(())
}
