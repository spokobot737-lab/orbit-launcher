use crate::{db, identity, secure, social, AppState, Shared};
use anyhow::{Context, Result};
use serde_json::{json, Value};
use std::sync::atomic::Ordering;
pub fn game_started(s: &AppState, item: &Value, pid: Option<u32>) {
    s.social.gaming.store(true, Ordering::Relaxed);
    *s.social.game.lock().unwrap() = json!({"name":item["name"],"kind":item["kind"],"id":item["id"],"universeId":item["universeId"],"version":item["version"],"loader":item["loader"],"pid":pid,"source":"orbit-process","expires":db::now()+90});
}
pub fn game_stopped(s: &AppState, pid: Option<u32>) {
    let mut game = s.social.game.lock().unwrap();
    if game["pid"].as_u64() == pid.map(|x| x as u64) {
        *game = Value::Null;
        s.social.gaming.store(false, Ordering::Relaxed);
    }
}
fn active_game(s: &AppState) -> Value {
    let mut game = s.social.game.lock().unwrap();
    if let Some(pid) = game["pid"].as_u64() {
        #[cfg(windows)]
        unsafe {
            use windows_sys::Win32::{
                Foundation::CloseHandle,
                System::Threading::{
                    GetExitCodeProcess, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
                },
            };
            let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid as u32);
            let mut code = 0;
            let alive =
                !process.is_null() && GetExitCodeProcess(process, &mut code) != 0 && code == 259;
            if !process.is_null() {
                CloseHandle(process);
            }
            if alive {
                game["expires"] = json!(db::now() + 90);
            } else {
                *game = Value::Null;
            }
        }
    } else if game["expires"].as_i64().unwrap_or(0) <= db::now() {
        *game = Value::Null;
    }
    s.social.gaming.store(!game.is_null(), Ordering::Relaxed);
    game.clone()
}
pub fn detected_games(s: &AppState) -> Result<Value> {
    let library = crate::db::library(s)?;
    let mut out = vec![];
    #[cfg(windows)]
    unsafe {
        use windows_sys::Win32::{
            Foundation::{CloseHandle, INVALID_HANDLE_VALUE},
            System::{
                Diagnostics::ToolHelp::{
                    CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
                    TH32CS_SNAPPROCESS,
                },
                Threading::{
                    OpenProcess, QueryFullProcessImageNameW, PROCESS_QUERY_LIMITED_INFORMATION,
                },
            },
        };
        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snapshot != INVALID_HANDLE_VALUE {
            let mut entry: PROCESSENTRY32W = std::mem::zeroed();
            entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
            let mut more = Process32FirstW(snapshot, &mut entry);
            while more != 0 {
                let process =
                    OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, entry.th32ProcessID);
                if !process.is_null() {
                    let mut path = vec![0u16; 32768];
                    let mut size = path.len() as u32;
                    if QueryFullProcessImageNameW(process, 0, path.as_mut_ptr(), &mut size) != 0 {
                        let path = String::from_utf16_lossy(&path[..size as usize]);
                        for item in library.as_array().unwrap_or(&vec![]) {
                            if item["kind"] == "program"
                                && item["gameActivity"] == true
                                && item["path"]
                                    .as_str()
                                    .is_some_and(|p| p.eq_ignore_ascii_case(&path))
                            {
                                out.push(json!({"id":item["id"],"name":item["name"],"kind":"program","pid":entry.th32ProcessID,"source":"exact-executable-path","expires":db::now()+90}));
                            }
                        }
                    }
                    CloseHandle(process);
                }
                more = Process32NextW(snapshot, &mut entry);
            }
            CloseHandle(snapshot);
        }
    }
    let own = active_game(s);
    if !own.is_null() {
        out.push(own);
    }
    Ok(json!(out))
}
pub fn select_game(s: &AppState, v: &Value) -> Result<Value> {
    let games = detected_games(s)?;
    let game = games
        .as_array()
        .unwrap()
        .iter()
        .find(|g| g["id"] == v["id"])
        .context("Game is no longer running")?;
    *s.social.game.lock().unwrap() = game.clone();
    Ok(game.clone())
}
pub async fn launch(app: &tauri::AppHandle, s: &AppState, v: &Value) -> Result<Value> {
    let activity = current(s)?;
    let entry = activity["friends"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["peer"] == v["peer"] && a["activity"]["kind"] == "game_activity")
        .context("Activity expired")?;
    let body = &entry["activity"]["body"];
    let library = db::library(s)?;
    let candidates = library
        .as_array()
        .unwrap()
        .iter()
        .filter(|p| {
            p["kind"] == body["kind"]
                && match p["kind"].as_str() {
                    Some("minecraft") => {
                        p["version"] == body["version"] && p["loader"] == body["loader"]
                    }
                    Some("program") => p["gameActivity"] == true && p["name"] == body["name"],
                    _ => false,
                }
        })
        .collect::<Vec<_>>();
    anyhow::ensure!(candidates.len()==1,"Choose a matching local game in your library; no unique installed profile matches this card.");
    let args = json!({"id":candidates[0]["id"]});
    if body["kind"] == "minecraft" {
        let _lock = s
            .install_lock
            .try_lock()
            .context("An installation is running")?;
        crate::minecraft::launch(app, s, &args).await
    } else {
        crate::programs::launch(Some(app), s, &args).await
    }
}
pub async fn local_music(s: Shared) -> Result<Value> {
    let value = tokio::task::spawn_blocking(move || read_media_session()).await??;
    *s.social.music.lock().unwrap() = value.clone();
    Ok(value)
}
fn read_media_session() -> Result<Value> {
    #[cfg(windows)]
    {
        use base64::Engine;
        use windows::Media::Control::{
            GlobalSystemMediaTransportControlsSessionManager,
            GlobalSystemMediaTransportControlsSessionPlaybackStatus,
        };
        use windows::Storage::Streams::DataReader;
        let manager = GlobalSystemMediaTransportControlsSessionManager::RequestAsync()?.join()?;
        let session = match manager.GetCurrentSession() {
            Ok(s) => s,
            Err(_) => return Ok(Value::Null),
        };
        let properties = session.TryGetMediaPropertiesAsync()?.join()?;
        let status = session.GetPlaybackInfo()?.PlaybackStatus()?;
        let title = properties.Title()?.to_string();
        if title.is_empty() {
            return Ok(Value::Null);
        }
        let artwork = (|| -> windows::core::Result<Option<String>> {
            let stream = properties.Thumbnail()?.OpenReadAsync()?.join()?;
            let size = stream.Size()?;
            if size == 0 || size > 2 * 1024 * 1024 {
                return Ok(None);
            }
            let reader = DataReader::CreateDataReader(&stream.GetInputStreamAt(0)?)?;
            reader.LoadAsync(size as u32)?.join()?;
            let mut bytes = vec![0; size as usize];
            reader.ReadBytes(&mut bytes)?;
            let mime = if bytes.starts_with(b"\x89PNG") {
                "image/png"
            } else if bytes.starts_with(b"\xff\xd8") {
                "image/jpeg"
            } else {
                return Ok(None);
            };
            Ok(Some(format!(
                "data:{mime};base64,{}",
                base64::engine::general_purpose::STANDARD.encode(bytes)
            )))
        })()
        .ok()
        .flatten();
        Ok(
            json!({"source":"windows-this-device","title":title,"artist":properties.Artist()?.to_string(),"album":properties.AlbumTitle()?.to_string(),"playing":status==GlobalSystemMediaTransportControlsSessionPlaybackStatus::Playing,"artwork":artwork,"sourceApp":session.SourceAppUserModelId()?.to_string(),"url":Value::Null,"expires":db::now()+90}),
        )
    }
    #[cfg(not(windows))]
    {
        anyhow::bail!("Windows media-session activity is available on Windows only")
    }
}
pub struct Watcher {
    #[cfg(windows)]
    manager: windows::Media::Control::GlobalSystemMediaTransportControlsSessionManager,
    #[cfg(windows)]
    token: i64,
}
impl Drop for Watcher {
    fn drop(&mut self) {
        #[cfg(windows)]
        {
            let _ = self.manager.RemoveCurrentSessionChanged(self.token);
        }
    }
}
pub async fn watch(s: Shared) -> Result<Value> {
    #[cfg(windows)]
    {
        if s.social.watcher.lock().unwrap().is_some() {
            return Ok(json!(true));
        }
        let manager=windows::Media::Control::GlobalSystemMediaTransportControlsSessionManager::RequestAsync()?.join()?;
        let state = s.clone();
        let token = manager.CurrentSessionChanged(&windows::Foundation::TypedEventHandler::new(
            move |_, _| {
                let state = state.clone();
                tauri::async_runtime::spawn(async move {
                    let _ = local_music(state.clone()).await;
                    let _ = publish(&state).await;
                });
                Ok(())
            },
        ))?;
        *s.social.watcher.lock().unwrap() = Some(Watcher { manager, token });
        let _ = local_music(s).await?;
        Ok(json!(true))
    }
    #[cfg(not(windows))]
    {
        anyhow::bail!("Windows media sessions are unavailable")
    }
}
pub async fn publish(s: &AppState) -> Result<Value> {
    let profile = identity::profile(s)?;
    if profile.is_null() {
        return Ok(json!({"shared":0}));
    }
    let contacts = identity::contacts(s)?;
    let game = active_game(s);
    let music = s.social.music.lock().unwrap().clone();
    let mut count = 0;
    for contact in contacts.as_array().unwrap_or(&vec![]) {
        if contact["state"] != "accepted" {
            continue;
        }
        let peer = contact["id"].as_str().context("Invalid contact")?;
        if profile["gameVisibility"] == true && !game.is_null() {
            let mut game = game.clone();
            game["expires"] = json!(db::now() + 90);
            social::queue(s, peer, "game_activity", game, true).await?;
            count += 1;
        }
        if profile["musicVisibility"] == true
            && !music.is_null()
            && music["expires"].as_i64().unwrap_or(0) > db::now()
        {
            let mut music = music.clone();
            if let Some(obj) = music.as_object_mut() {
                obj.remove("artwork");
            }
            social::queue(s, peer, "music_activity", music, true).await?;
            count += 1;
        }
    }
    Ok(json!({"shared":count}))
}
pub fn current(s: &AppState) -> Result<Value> {
    let mut friends = vec![];
    let rows = {
        let c = s.db.lock().unwrap();
        let mut q = c.prepare("SELECT peer,data,expires FROM friend_activity WHERE expires>?")?;
        let rows = q
            .query_map([db::now()], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, i64>(2)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        rows
    };
    for (peer, data, expires) in rows {
        friends.push(json!({"peer":peer,"activity":serde_json::from_str::<Value>(&secure::unseal(s,&data)?)?,"expires":expires}));
    }
    Ok(
        json!({"game":active_game(s),"music":s.social.music.lock().unwrap().clone(),"friends":friends}),
    )
}
pub async fn spotify(s: Shared) -> Result<Value> {
    let value = crate::connections::spotify_activity(&s).await?;
    *s.social.music.lock().unwrap() = value.clone();
    Ok(value)
}
