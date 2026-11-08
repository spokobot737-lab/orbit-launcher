use crate::{identity, media, publisher, social, wallet, Shared};
use anyhow::Result;
use serde_json::{json, Value};
pub async fn dispatch(app: &tauri::AppHandle, s: Shared, op: &str, v: Value) -> Result<Value> {
    let signal = s.clone();
    let result = match op {
        "orbit_profile" => identity::profile(&s),
        "orbit_profile_share" => {
            crate::profile_sync::retry(&s)?;
            Ok(json!(true))
        }
        "orbit_profile_sync_status" => crate::profile_sync::status(&s),
        "orbit_profile_retry" => crate::profile_sync::retry(&s),
        "orbit_contact_profile_retry" => crate::profile_sync::retry_contact(&s, &v),
        "orbit_social_open" => {
            social::wake(Some(app.clone()), s.clone());
            social::status(&s).await
        }
        "orbit_profile_image" => {
            let value = v.clone();
            tokio::task::spawn_blocking(move || identity::profile_image(&value)).await?
        }
        "orbit_connection_open" => crate::connections::open_profile(&s, &v),
        "orbit_contact_link" => crate::connections::open_contact(&s, &v),
        "orbit_game_select" => crate::activity::select_game(&s, &v),
        "orbit_activity_launch" => crate::activity::launch(app, &s, &v).await,
        "orbit_activity" => crate::activity::current(&s),
        "orbit_games_detect" => crate::activity::detected_games(&s),
        "orbit_music_windows" => crate::activity::local_music(s).await,
        "orbit_music_watch" => crate::activity::watch(s).await,
        "orbit_music_spotify" => crate::activity::spotify(s).await,
        "orbit_activity_share" => crate::activity::publish(&s).await,
        "orbit_connections" => crate::connections::list(&s),
        "orbit_connection_manual" => crate::connections::manual(&s, &v),
        "orbit_connection_visibility" => crate::connections::visibility(&s, &v),
        "orbit_connection_disconnect" => crate::connections::disconnect(&s, &v).await,
        "orbit_connection_oauth" => {
            crate::connections::oauth(s, v["provider"].as_str().unwrap_or("")).await
        }
        "orbit_bluesky_connect" => crate::bluesky::connect(s, &v).await,
        "orbit_bluesky_posts" => crate::bluesky::posts(&s, &v).await,
        "orbit_spotify_activity" => crate::connections::spotify_activity(&s).await,
        "orbit_backup_export" => crate::backup::export(&s, &v),
        "orbit_backup_restore" => crate::backup::restore(&s, &v),
        "orbit_profile_save" => identity::save(&s, &v),
        "orbit_config" => identity::config(&s),
        "orbit_config_save" => {
            let result = identity::save_config(&s, &v)?;
            if result["enabled"] != true {
                media::cancel_all(&s);
            }
            social::stop(&s).await;
            if result["enabled"] == true {
                social::wake(Some(app.clone()), s.clone());
            }
            crate::profile_sync::mark_dirty(&s)?;
            Ok(result)
        }
        "orbit_contacts" => identity::contacts(&s),
        "orbit_invitation" => social::invitation(&s).await,
        "orbit_invite_accept" => {
            let result = social::import_invitation(&s, &v).await?;
            let state = s.clone();
            tokio::spawn(async move {
                let _ = social::flush(&state).await;
            });
            Ok(result)
        }
        "orbit_friend_accept" => {
            let result = social::accept(&s, &v).await?;
            let state = s.clone();
            tokio::spawn(async move {
                let _ = social::flush(&state).await;
            });
            Ok(result)
        }
        "orbit_block" => identity::block(&s, &v),
        "orbit_messages" => identity::messages(&s, &v),
        "orbit_send" => {
            let result = social::send(&s, &v).await?;
            let state = s.clone();
            tokio::spawn(async move {
                let _ = social::flush(&state).await;
            });
            Ok(result)
        }
        "orbit_retry" => {
            let result = social::retry(&s, &v)?;
            let state = s.clone();
            tokio::spawn(async move {
                let _ = social::flush(&state).await;
            });
            Ok(result)
        }
        "orbit_network_start" => {
            social::stop(&s).await;
            social::wake(Some(app.clone()), s.clone());
            social::status(&s).await
        }
        "orbit_network_stop" => {
            media::cancel_all(&s);
            let mut cfg = identity::config(&s)?;
            cfg["enabled"] = json!(false);
            identity::save_config(&s, &cfg)?;
            social::stop(&s).await;
            Ok(json!(true))
        }
        "orbit_status" => social::status(&s).await,
        "orbit_wallet" => wallet::info(&s),
        "orbit_wallet_create" => {
            let _lock = s.social.wallet_lock.try_lock()?;
            wallet::create(&s)
        }
        "orbit_wallet_connect" => {
            let _lock = s.social.wallet_lock.try_lock()?;
            wallet::browser_connect(s.clone()).await
        }
        "orbit_wallet_disconnect" => wallet::disconnect(&s),
        "orbit_wallet_request" => social::wallet_request(&s, &v).await,
        "orbit_wallet_approve" => social::wallet_approve(&s, &v).await,
        "orbit_media_prepare" => media::prepare(Some(app.clone()), s, &v).await,
        "orbit_media_upload" => media::upload(Some(app.clone()), s, &v).await,
        "orbit_media_retrieve" => media::retrieve(Some(app.clone()), s, &v).await,
        "orbit_media_cancel" => media::cancel(&s, &v),
        "orbit_media_jobs" => Ok(media::jobs(&s)),
        "orbit_media_clear_cache" => media::clear_cache(&s),
        "orbit_media_save" => media::save_original(&s, &v).await,
        "orbit_publisher" => Ok(publisher::status()),
        _ => anyhow::bail!("Unknown Orbit operation"),
    };
    if result.is_ok()
        && [
            "orbit_profile_save",
            "orbit_profile_share",
            "orbit_profile_retry",
            "orbit_contact_profile_retry",
            "orbit_invitation",
            "orbit_invite_accept",
            "orbit_friend_accept",
            "orbit_send",
            "orbit_connection_visibility",
            "orbit_connection_manual",
            "orbit_connection_disconnect",
            "orbit_connection_oauth",
            "orbit_bluesky_connect",
        ]
        .contains(&op)
    {
        social::wake(Some(app.clone()), signal.clone());
        crate::profile_sync::schedule(Some(app.clone()), signal);
    }
    result
}
