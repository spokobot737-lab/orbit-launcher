//! A small Fabric performance preset, fetched directly from authors' Modrinth files.
use crate::{net, AppState};
use anyhow::{bail, ensure, Context, Result};
use serde_json::{json, Value};
use std::{collections::HashSet, path::Path};
pub async fn install(
    s: &AppState,
    root: &Path,
    version: &str,
    app: Option<&tauri::AppHandle>,
) -> Result<Value> {
    ensure!(
        version == "1.21.1",
        "The tested performance preset currently targets Minecraft 1.21.1 only"
    );
    let preset: Value = serde_json::from_str(include_str!("../../docs/performance-profile.json"))?;
    let mut queue = preset["mods"]
        .as_array()
        .context("Invalid performance preset")?
        .iter()
        .map(|m| {
            Ok((
                m["version"]
                    .as_str()
                    .context("Preset version missing")?
                    .to_owned(),
                true,
            ))
        })
        .collect::<Result<Vec<_>>>()?;
    let mut seen = HashSet::new();
    let mut installed = vec![];
    while let Some((id, exact)) = queue.pop() {
        ensure!(seen.len() < 40, "Too many mod dependencies");
        if !seen.insert(id.clone()) {
            continue;
        }
        let v = if exact {
            net::cached(
                s,
                &format!("mod-version:{id}"),
                &format!("https://api.modrinth.com/v2/version/{id}"),
                86400,
            )
            .await?
        } else {
            let mut u =
                url::Url::parse(&format!("https://api.modrinth.com/v2/project/{id}/version"))?;
            u.query_pairs_mut()
                .append_pair("loaders", "[\"fabric\"]")
                .append_pair("game_versions", &json!([version]).to_string());
            let vs = net::cached(s, &format!("mod:{id}:{version}"), u.as_str(), 3600).await?;
            vs.as_array()
                .and_then(|a| a.iter().find(|x| x["version_type"] == "release"))
                .cloned()
                .with_context(|| format!("No stable Fabric {version} release for {id}"))?
        };
        ensure!(
            v["game_versions"]
                .as_array()
                .is_some_and(|a| a.iter().any(|x| x == version)),
            "Incompatible mod dependency: {id}"
        );
        for d in v["dependencies"].as_array().unwrap_or(&vec![]) {
            if d["dependency_type"] == "required" {
                if let Some(id) = d["version_id"].as_str() {
                    queue.push((id.into(), true));
                } else if let Some(id) = d["project_id"].as_str() {
                    queue.push((id.into(), false));
                } else {
                    bail!("Unresolvable mod dependency");
                }
            }
        }
        let files = v["files"].as_array().context("Mod release has no files")?;
        let f = files
            .iter()
            .find(|f| f["primary"] == true)
            .or_else(|| files.first())
            .context("Mod file missing")?;
        let name = f["filename"].as_str().context("Mod filename missing")?;
        ensure!(
            !name.contains('/') && !name.contains('\\') && name.ends_with(".jar"),
            "Unsafe mod filename"
        );
        let dest = net::safe_join(&root.join("game/mods"), name)?;
        let sha = f["hashes"]["sha1"].as_str().context("Mod has no SHA-1")?;
        let url = f["url"].as_str().context("Mod download URL missing")?;
        if let Some(a) = app {
            crate::progress(
                a,
                &format!("Installing performance mod: {id}"),
                installed.len(),
                installed.len() + queue.len() + 1,
            );
        }
        s.net.download(url, &dest, sha).await?;
        installed.push(json!({"project":v["project_id"],"version":v["id"],"name":v["name"],"filename":name,"sha1":sha}));
    }
    Ok(json!(installed))
}
