#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod activity;
mod auth;
#[cfg(test)]
mod automatic_live_tests;
mod backup;
mod bluesky;
mod connections;
mod db;
mod identity;
#[cfg(test)]
mod live_tests;
mod media;
mod minecraft;
mod mods;
mod net;
#[cfg(test)]
mod operator_tests;
mod profile_sync;
mod programs;
mod publisher;
mod roblox;
mod secure;
mod social;
mod social_commands;
mod social_network;
#[cfg(test)]
mod social_peer_tests;
#[cfg(test)]
mod social_tests;
#[cfg(test)]
mod social_upgrade_tests;
mod wallet;
use serde_json::{json, Value};
use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
};
use tauri::{Emitter, Manager};

pub struct AppState {
    pub root: PathBuf,
    pub db: Mutex<rusqlite::Connection>,
    pub net: net::Network,
    pub install_lock: tokio::sync::Mutex<()>,
    pub auth_lock: tokio::sync::Mutex<()>,
    pub social: social::Runtime,
    pub transfers: media::Transfers,
}
pub type Shared = Arc<AppState>;
pub fn progress(app: &tauri::AppHandle, stage: &str, done: usize, total: usize) {
    let _ = app.emit("progress", json!({"stage":stage,"done":done,"total":total}));
}
pub fn post_launch(app: &tauri::AppHandle, s: &AppState) {
    let setting = db::setting(s, "afterLaunch").unwrap_or_else(|_| json!("stay"));
    if setting == "minimize" {
        if let Some(w) = app.get_webview_window("main") {
            let _ = w.minimize();
        }
    } else if setting == "exit" {
        app.exit(0);
    }
}

#[tauri::command]
async fn api(
    app: tauri::AppHandle,
    state: tauri::State<'_, Shared>,
    op: String,
    args: Value,
) -> Result<Value, String> {
    let s = state.inner().clone();
    let result: anyhow::Result<Value> = async {
        match op.as_str() {
            "library" => db::library(&s),
            "save_item" => db::save_item(&s, &args),
            "remove_item" => db::remove_item(&s, &args),
            "collections" => db::collections(&s),
            "save_collection" => db::save_collection(&s, &args),
            "collect" => db::collect(&s, &args),
            "settings" => db::settings(&s),
            "setting" => db::set_setting(&s, &args),
            "program_import" => programs::import(&s, &args).await,
            "program_image" => programs::image(&args).await,
            "program_launch" => programs::launch(Some(&app), &s, &args).await,
            "roblox_home" => roblox::home(&s).await,
            "roblox_official_home" => {
                open::that_detached("https://www.roblox.com/home")?;
                Ok(json!({"opened":true,"note":"Official Roblox website in your system browser. Orbit does not read this browser session."}))
            }
            "roblox_explore" => roblox::explore(&s).await,
            "roblox_search" => roblox::search(&s, &args).await,
            "roblox_details" => roblox::details(&s, &args).await,
            "roblox_launch" => roblox::launch(Some(&app), &s, &args).await,
            "image" => net::image(&s, &args).await,
            "mc_versions" => minecraft::versions(&s).await,
            "mc_install" => {
                let _lock = s
                    .install_lock
                    .try_lock()
                    .map_err(|_| anyhow::anyhow!("An installation is already running."))?;
                minecraft::install(&app, &s, &args).await
            }
            "mc_launch" => {
                let _lock = s.install_lock.try_lock().map_err(|_| {
                    anyhow::anyhow!("Wait for the installation to finish before launching.")
                })?;
                minecraft::launch(&app, &s, &args).await
            }
            "mc_signin" => {
                let _lock = s
                    .auth_lock
                    .try_lock()
                    .map_err(|_| anyhow::anyhow!("Sign-in is already in progress."))?;
                auth::signin(&s).await
            }
            "mc_account" => auth::account(&s),
            "mc_signout" => auth::signout(&s),
            "logs" => minecraft::logs(&s, &args),
            _ if op.starts_with("orbit_") => {
                social_commands::dispatch(&app, s.clone(), &op, args.clone()).await
            }
            _ => anyhow::bail!("Unknown operation"),
        }
    }
    .await;
    result.map_err(|e| format!("{e:#}"))
}
fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let args: Vec<String> = std::env::args().collect();
            let root = if let Some(index) = args.iter().position(|a| a == "--data-dir") {
                let path = PathBuf::from(
                    args.get(index + 1)
                        .ok_or("--data-dir needs an absolute directory")?,
                );
                if !path.is_absolute() {
                    return Err("--data-dir needs an absolute directory".into());
                }
                path
            } else {
                app.path().app_local_data_dir()?
            };
            std::fs::create_dir_all(&root)?;
            let root = dunce::canonicalize(root)?;
            app.asset_protocol_scope()
                .allow_directory(root.join("cache"), true)?;
            let conn = db::init(&root)?;
            let state = Arc::new(AppState {
                root,
                db: Mutex::new(conn),
                net: net::Network::new()?,
                install_lock: tokio::sync::Mutex::new(()),
                auth_lock: tokio::sync::Mutex::new(()),
                social: social::Runtime::default(),
                transfers: media::Transfers::new(),
            });
            identity::init(&state)?;
            app.manage(state.clone());
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                if identity::config(&state).is_ok_and(|v| v["enabled"] == true)
                    && identity::profile(&state).is_ok_and(|v| !v.is_null())
                {
                    let _ = social::start(Some(handle), state).await;
                }
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![api])
        .run(tauri::generate_context!())
        .expect("Unable to start Orbit");
}
