use crate::{db, identity, secure, AppState};
use age::secrecy::{ExposeSecret, SecretString};
use anyhow::{ensure, Context, Result};
use rusqlite::types::Value as SqlValue;
use serde_json::{json, Value};
use std::{
    io::{Read, Write},
    path::Path,
};
const TABLES: &[(&str, &str)] = &[
    ("contacts", "pubkey,name,state,data"),
    (
        "messages",
        "id,peer,direction,kind,body,state,created,error",
    ),
    ("media", "id,data"),
    ("outbox", "id,event,relays,attempts,next_try"),
    ("connections", "provider,data"),
];
pub fn snapshot(s: &AppState) -> Result<Value> {
    let profile = identity::profile(s)?;
    ensure!(!profile.is_null(), "Create an Orbit profile first");
    let mut out = json!({"format":"orbit-backup","version":1,"created":db::now(),"profile":profile,"config":identity::config(s)?,"keys":{"nostr":secure::get(s,"nostr-key")?,"data":secure::get(s,"data-key")?,"wallet":secure::get(s,"evm-wallet").ok()}});
    let c = s.db.lock().unwrap();
    for (table, cols) in TABLES {
        let mut q = c.prepare(&format!("SELECT {cols} FROM {table}"))?;
        let n = q.column_count();
        let rows = q
            .query_map([], |r| {
                let mut row = vec![];
                for i in 0..n {
                    let x = r.get_ref(i)?;
                    row.push(match x {
                        rusqlite::types::ValueRef::Null => Value::Null,
                        rusqlite::types::ValueRef::Integer(x) => json!(x),
                        rusqlite::types::ValueRef::Text(x) => json!(String::from_utf8_lossy(x)),
                        _ => Value::Null,
                    });
                }
                Ok(json!(row))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        out[*table] = json!(rows);
    }
    Ok(out)
}
pub fn export(s: &AppState, v: &Value) -> Result<Value> {
    let path = Path::new(v["path"].as_str().context("Choose a backup destination")?);
    ensure!(
        path.is_absolute() && !path.exists(),
        "Choose a new absolute backup filename"
    );
    let password = v["password"]
        .as_str()
        .context("Choose a backup passphrase")?;
    ensure!(
        password.chars().count() >= 16,
        "Use a backup passphrase of at least 16 characters"
    );
    let plain = serde_json::to_vec(&snapshot(s)?)?;
    ensure!(
        plain.len() <= 64 * 1024 * 1024,
        "Backup exceeds the 64 MiB metadata limit; media bytes are excluded"
    );
    let output = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?;
    let mut stream = age::Encryptor::with_user_passphrase(SecretString::from(password.to_owned()))
        .wrap_output(output)?;
    stream.write_all(&plain)?;
    stream.finish()?;
    Ok(json!({"path":path,"includesMediaFiles":false}))
}
pub fn restore(s: &AppState, v: &Value) -> Result<Value> {
    let path = Path::new(v["path"].as_str().context("Choose an Orbit backup")?);
    ensure!(
        std::fs::metadata(path)?.len() <= 70 * 1024 * 1024,
        "Backup file is too large"
    );
    let password = v["password"]
        .as_str()
        .context("Enter the backup passphrase")?;
    let decryptor = age::Decryptor::new(std::io::BufReader::new(std::fs::File::open(path)?))?;
    ensure!(
        decryptor.is_scrypt(),
        "Expected a passphrase-protected Orbit backup"
    );
    let pass = age::scrypt::Identity::new(SecretString::from(password.to_owned()));
    let mut reader = decryptor.decrypt(std::iter::once(&pass as &dyn age::Identity))?;
    let mut bytes = vec![];
    reader
        .by_ref()
        .take(64 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() <= 64 * 1024 * 1024,
        "Decrypted backup exceeds the metadata limit"
    );
    let backup: Value = serde_json::from_slice(&bytes)?;
    restore_snapshot(s, &backup)
}
pub fn restore_snapshot(s: &AppState, b: &Value) -> Result<Value> {
    ensure!(
        b["format"] == "orbit-backup" && b["version"] == 1,
        "Unsupported Orbit backup"
    );
    let p = &b["profile"];
    let keys = nostr_sdk::Keys::parse(
        b["keys"]["nostr"]
            .as_str()
            .context("Communication key missing")?,
    )?;
    ensure!(
        p["id"] == keys.public_key().to_hex(),
        "Backup identity/key mismatch"
    );
    let data: age::x25519::Identity = b["keys"]["data"]
        .as_str()
        .context("Local data key missing")?
        .parse()
        .map_err(|_| anyhow::anyhow!("Invalid backup data key"))?;
    ensure!(identity::profile(s)?.is_null(),"Restore into a fresh Orbit installation to preserve the current profile. Export this device first.");
    if let Some(w) = b["keys"]["wallet"].as_str() {
        k256::ecdsa::SigningKey::from_slice(&hex::decode(w)?)?;
    }
    let mut c = s.db.lock().unwrap();
    let tx = c.transaction()?;
    tx.execute(
        "INSERT OR REPLACE INTO orbit_profile VALUES(1,?)",
        [p.to_string()],
    )?;
    let mut config = b["config"].clone();
    config["enabled"] = json!(false);
    config["mediaConsent"] = json!(false);
    tx.execute(
        "INSERT OR REPLACE INTO social_config VALUES(1,?)",
        [config.to_string()],
    )?;
    for (table, cols) in TABLES {
        let rows = b[*table].as_array().context("Backup table missing")?;
        ensure!(rows.len() <= 100_000, "Backup table is too large");
        let width = cols.split(',').count();
        let placeholders = vec!["?"; width].join(",");
        for row in rows {
            let a = row.as_array().context("Invalid backup row")?;
            ensure!(a.len() == width, "Backup row has an invalid shape");
            let mut values = a
                .iter()
                .map(|v| match v {
                    Value::Null => Ok(SqlValue::Null),
                    Value::String(s) => Ok(SqlValue::Text(s.clone())),
                    Value::Number(n) => {
                        Ok(SqlValue::Integer(n.as_i64().context("Invalid integer")?))
                    }
                    _ => Err(anyhow::anyhow!("Invalid backup value")),
                })
                .collect::<Result<Vec<_>>>()?;
            if *table == "connections" && a[0] != "wallet" {
                let mut v: Value =
                    serde_json::from_str(a[1].as_str().context("Invalid connection")?)?;
                v["authenticated"] = json!(false);
                v["verification"] = json!("restored-link");
                values[1] = SqlValue::Text(v.to_string());
            }
            tx.execute(
                &format!("INSERT OR REPLACE INTO {table}({cols}) VALUES({placeholders})"),
                rusqlite::params_from_iter(values),
            )?;
        }
    }
    secure::set(s, "nostr-key", &keys.secret_key().to_secret_hex())?;
    secure::set(s, "data-key", data.to_string().expose_secret())?;
    if let Some(w) = b["keys"]["wallet"].as_str() {
        secure::set(s, "evm-wallet", w)?;
    }
    tx.commit()?;
    Ok(json!({"restored":true,"identity":p["id"],"networkEnabled":false}))
}
