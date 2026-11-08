#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod auth;
mod db;
#[cfg(test)]
mod live_tests;
mod minecraft;
mod mods;
mod net;
mod programs;
mod roblox;
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
            let root = app.path().app_local_data_dir()?;
            std::fs::create_dir_all(&root)?;
            let conn = db::init(&root)?;
            app.manage(Arc::new(AppState {
                root,
                db: Mutex::new(conn),
                net: net::Network::new()?,
                install_lock: tokio::sync::Mutex::new(()),
                auth_lock: tokio::sync::Mutex::new(()),
            }));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![api])
        .run(tauri::generate_context!())
        .expect("Unable to start Orbit");
}
