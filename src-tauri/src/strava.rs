//! Strava, owned by Rust: credentials, OAuth, token refresh, uploads and the
//! background auto-sync worker. The windows only dispatch `Strava*` commands
//! and read `AppSnapshot.strava`; tokens and the client secret never leave
//! this module.
//!
//! The decisions (what to upload, how to build it, how to read Strava's
//! replies) are pure and live in `strava_plan.rs`, `strava_status.rs` and
//! `strava_template.rs`. This module is the I/O, split into:
//! - `settings`: the settings keys and the commands that change them
//! - `live`: the in-memory state behind `snapshot()`
//! - `auth`: the OAuth login, token refresh and disconnect
//! - `api`: the REST calls and running them with a valid token
//! - `upload`: uploading sessions as one activity, and the shoe list
//! - `worker`: the background auto-sync pass
//! - `fallback`: "Save for Strava", the TCX file for a manual upload
//!
//! The client secret and the token set live in Windows Credential Manager
//! (`secrets.rs`); the settings table only holds them as a fallback when the
//! credential store is unavailable.

mod api;
mod auth;
mod fallback;
mod live;
mod settings;
mod upload;
mod worker;

pub use auth::{cancel_connect, connect, disconnect};
pub use fallback::{mark_on_strava, open_save_folder};
pub use live::snapshot;
pub use settings::{
    init, open_api_settings, set_auto_sync, set_credentials, set_daily_rollup, set_gear, set_mute,
    set_templates,
};
pub use upload::{refresh_gear_command, upload_manual};
pub use worker::{notify_session_ended, retry_now, spawn_worker, sync_now};

use crate::strava_plan::{self as plan, Naming, Payload};
use crate::strava_save::SavedFile;
use crate::strava_template::{self as template, Templates};
use crate::util::blocking;
use serde::Serialize;
use tauri::AppHandle;

/// Write the TCX for these walks (or the latest one not on Strava, when
/// `session_ids` is None) to `Downloads\Neon Strider`, open Strava's upload
/// page and show the file.
#[tauri::command]
pub async fn strava_save_for_strava(
    app: AppHandle,
    session_ids: Option<Vec<i64>>,
) -> Result<SavedFile, String> {
    fallback::save_for_strava(app, session_ids).await
}

/// The TCX Strava would receive for these sessions, for the History preview.
#[tauri::command]
pub async fn strava_tcx_preview(app: AppHandle, session_ids: Vec<i64>) -> Result<String, String> {
    if session_ids.is_empty() {
        return Err("No sessions selected".to_string());
    }
    let data = upload::load_data(&app, session_ids).await?;
    let templates = settings::load_templates(&app);
    match plan::plan_activity(&data, Naming::Walk, &templates).payload {
        Payload::Tcx(tcx) => Ok(tcx),
        Payload::Manual => Err(
            "Fewer than 2 telemetry samples, so Strava would get a manual activity with no file."
                .to_string(),
        ),
    }
}

/// The templates rendered for a sample walk, for the settings preview.
#[derive(Serialize)]
pub struct TemplatePreview {
    title: String,
    rollup_title: String,
    description: String,
}

/// Render the (unsaved) templates against a sample walk. A blank one previews
/// its default.
#[tauri::command]
pub fn strava_template_preview(
    title: String,
    rollup_title: String,
    description: String,
) -> TemplatePreview {
    let t = Templates::from_stored(Some(&title), Some(&rollup_title), Some(&description));
    let stats = template::sample_stats();
    TemplatePreview {
        title: template::render_title(&t.title, &stats),
        rollup_title: template::render_title(&t.rollup_title, &stats),
        description: template::render_description(&t.description, &stats),
    }
}
