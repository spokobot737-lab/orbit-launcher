//! All long-lived secrets are scoped to one installation and kept in Credential Manager.
use crate::AppState;
use age::secrecy::ExposeSecret;
use anyhow::{Context, Result};
use sha2::{Digest, Sha256};
static VAULT_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
pub fn entry(s: &AppState, name: &str) -> Result<keyring::Entry> {
    let scope = format!("{:x}", Sha256::digest(s.root.to_string_lossy().as_bytes()));
    Ok(keyring::Entry::new(
        "app.orbit.social",
        &format!("{}:{name}", &scope[..24]),
    )?)
}
pub fn get(s: &AppState, name: &str) -> Result<String> {
    let _lock = VAULT_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let raw = entry(s, name)?.get_password().context(
        "Protected credential unavailable. Restore an Orbit backup on this Windows account.",
    )?;
    anyhow::ensure!(
        raw != "orbit-revoked",
        "Protected credential was disconnected"
    );
    if let Some(index) = raw.strip_prefix("orbit-chunks:") {
        use base64::Engine;
        let (generation, count) = index
            .split_once(':')
            .context("Invalid protected credential index")?;
        let count: usize = count.parse()?;
        anyhow::ensure!(count <= 128, "Protected credential is too large");
        let mut bytes = vec![];
        for n in 0..count {
            let chunk = entry(s, &format!("{name}:{generation}:{n}"))?.get_password()?;
            bytes.extend(base64::engine::general_purpose::STANDARD.decode(chunk)?);
        }
        return Ok(String::from_utf8(bytes)?);
    }
    Ok(raw)
}
pub fn set(s: &AppState, name: &str, value: &str) -> Result<()> {
    let _lock = VAULT_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    use base64::Engine;
    anyhow::ensure!(
        value.len() <= 89_600,
        "Protected credential exceeds local storage limit"
    );
    let previous = entry(s, name)?.get_password().ok();
    if value.len() > 900 {
        let generation = uuid::Uuid::new_v4().simple().to_string();
        let chunks = value.as_bytes().chunks(700).collect::<Vec<_>>();
        for (n, chunk) in chunks.iter().enumerate() {
            entry(s, &format!("{name}:{generation}:{n}"))?
                .set_password(&base64::engine::general_purpose::STANDARD.encode(chunk))?;
        }
        entry(s, name)?.set_password(&format!("orbit-chunks:{generation}:{}", chunks.len()))?;
    } else {
        entry(s, name)?.set_password(value)?;
    }
    if let Some(old) = previous {
        remove_chunks(s, name, &old)?;
    }
    Ok(())
}
fn remove_chunks(s: &AppState, name: &str, index: &str) -> Result<()> {
    if let Some(index) = index.strip_prefix("orbit-chunks:") {
        if let Some((generation, count)) = index.split_once(':') {
            for n in 0..count.parse::<usize>().unwrap_or(0).min(128) {
                let _ = entry(s, &format!("{name}:{generation}:{n}"))?.delete_credential();
            }
        }
    }
    Ok(())
}
pub fn remove(s: &AppState, name: &str) -> Result<()> {
    let _lock = VAULT_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    if let Ok(index) = entry(s, name)?.get_password() {
        entry(s, name)?.set_password("orbit-revoked")?;
        remove_chunks(s, name, &index)?;
    }
    match entry(s, name)?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(e.into()),
    }
}
pub fn data_identity(s: &AppState) -> Result<age::x25519::Identity> {
    get(s, "data-key")?
        .parse()
        .map_err(|_| anyhow::anyhow!("Invalid protected data key"))
}
pub fn create_data_key(s: &AppState) -> Result<()> {
    if get(s, "data-key").is_err() {
        set(
            s,
            "data-key",
            age::x25519::Identity::generate()
                .to_string()
                .expose_secret(),
        )?;
    }
    Ok(())
}
pub fn seal(s: &AppState, plain: &str) -> Result<String> {
    use base64::Engine;
    use std::io::Write;
    let identity = data_identity(s)?;
    let recipient = identity.to_public();
    let mut bytes = vec![];
    let mut stream =
        age::Encryptor::with_recipients(std::iter::once(&recipient as &dyn age::Recipient))?
            .wrap_output(&mut bytes)?;
    stream.write_all(plain.as_bytes())?;
    stream.finish()?;
    Ok(base64::engine::general_purpose::STANDARD.encode(bytes))
}
pub fn unseal(s: &AppState, encrypted: &str) -> Result<String> {
    use base64::Engine;
    use std::io::Read;
    let bytes = base64::engine::general_purpose::STANDARD.decode(encrypted)?;
    let identity = data_identity(s)?;
    let decryptor = age::Decryptor::new(&bytes[..])?;
    let mut reader = decryptor.decrypt(std::iter::once(&identity as &dyn age::Identity))?;
    let mut plain = String::new();
    reader.read_to_string(&mut plain)?;
    Ok(plain)
}
