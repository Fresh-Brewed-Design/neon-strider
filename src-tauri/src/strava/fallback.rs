//! "Save for Strava", the no-API fallback (pure parts in `strava_save.rs`).
//! It never calls the Strava API, so it works with no account, no credentials
//! and no connection: the walk is written as a TCX file for a manual upload.

use super::live;
use super::settings;
use super::upload;
use crate::persistence;
use crate::strava_plan::{self as plan, Naming, Payload};
use crate::strava_save::{self as save, SavedFile};
use std::path::PathBuf;
use tauri::{AppHandle, Manager};
use tauri_plugin_opener::OpenerExt;

/// `Downloads\Neon Strider`.
fn save_folder(app: &AppHandle) -> Result<PathBuf, String> {
    let downloads = app
        .path()
        .download_dir()
        .map_err(|e| format!("Could not find the Downloads folder: {}", e))?;
    Ok(downloads.join(save::FOLDER_NAME))
}

/// The walk to save when none was chosen: the latest one not on Strava.
async fn latest_unsent(app: &AppHandle) -> Result<Vec<i64>, String> {
    let app_c = app.clone();
    let sessions = super::blocking(move || persistence::db_all_sessions(&app_c)).await?;
    let merge = settings::merge_minutes(app);
    save::latest_unsent(&sessions, upload::active_session_id().await, merge).ok_or_else(|| {
        "No finished walk to save. Every walk is already on Strava, or one is still running."
            .to_string()
    })
}

/// Local `YYYY-MM-DD` of an epoch-ms instant, for the file name.
fn local_date(ms: i64) -> String {
    use chrono::TimeZone;
    chrono::Local
        .timestamp_millis_opt(ms)
        .earliest()
        .map(|d| d.format("%Y-%m-%d").to_string())
        .unwrap_or_default()
}

/// Write the TCX for these walks (or the latest one not on Strava, when
/// `session_ids` is None) to `Downloads\Neon Strider`, open Strava's upload
/// page and show the file.
pub(super) async fn save_for_strava(
    app: AppHandle,
    session_ids: Option<Vec<i64>>,
) -> Result<SavedFile, String> {
    let ids = match session_ids {
        Some(ids) if !ids.is_empty() => ids,
        Some(_) => return Err("No sessions to save".to_string()),
        None => latest_unsent(&app).await?,
    };
    if let Some(active) = upload::active_session_id().await {
        if ids.contains(&active) {
            return Err("The session is still running".to_string());
        }
    }

    let data = upload::load_data(&app, ids.clone()).await?;
    let activity = plan::plan_activity(&data, Naming::Walk, &settings::load_templates(&app));
    let Payload::Tcx(tcx) = activity.payload else {
        return Err("The walk has too few telemetry samples to make a file.".to_string());
    };
    let name = save::file_name(&local_date(activity.start_ms), &activity.name);
    let dir = save_folder(&app)?;
    let path = super::blocking(move || save::write_tcx(&dir, &name, &tcx)).await?;
    log::info!("saved a walk for a manual Strava upload: {:?}", path);

    // The file is written; a browser or Explorer that will not open is not a failed save.
    if let Err(e) = app.opener().open_url(save::UPLOAD_URL, None::<&str>) {
        log::warn!("could not open the Strava upload page: {}", e);
    }
    if let Err(e) = app.opener().reveal_item_in_dir(&path) {
        log::warn!("could not reveal {:?}: {}", path, e);
    }

    let mut saved = settings::read_saved_ids(&app);
    for id in &ids {
        if !saved.contains(id) {
            saved.push(*id);
        }
    }
    settings::write_saved_ids(&app, &saved)?;
    live::reload(&app);

    Ok(SavedFile {
        file_name: path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default(),
        folder: path
            .parent()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_default(),
    })
}

/// The user uploaded the saved file by hand: mark the walks as on Strava.
pub fn mark_on_strava(app: &AppHandle, session_ids: Vec<i64>) -> Result<(), String> {
    if session_ids.is_empty() {
        return Err("No sessions selected".to_string());
    }
    for id in &session_ids {
        persistence::db_mark_uploaded(app, *id, "manual")?;
    }
    let mut saved = settings::read_saved_ids(app);
    saved.retain(|id| !session_ids.contains(id));
    settings::write_saved_ids(app, &saved)?;
    live::clear_upload_errors(&session_ids);
    live::reload(app);
    live::record(true, "Marked as on Strava");
    Ok(())
}

/// Show `Downloads\Neon Strider` in Explorer.
pub fn open_save_folder(app: &AppHandle) -> Result<(), String> {
    let dir = save_folder(app)?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    app.opener()
        .open_path(dir.to_string_lossy(), None::<&str>)
        .map_err(|e| format!("Could not open the folder: {}", e))
}
