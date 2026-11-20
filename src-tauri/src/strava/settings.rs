//! Strava settings: the keys in the `settings` table, how they are read, and
//! the commands that change them.
//!
//! Some keys were first written by the dashboard as JSON (`"18:00"`, `"true"`),
//! so string values are read with any surrounding quotes removed.

use super::auth;
use super::live::{self, live};
use crate::persistence;
use crate::secrets::{self, Secret};
use crate::snapshot::{MuteMode, StravaGear};
use crate::strava_plan as plan;
use crate::strava_status as status;
use crate::strava_template::{self as template, Templates};
use chrono::NaiveTime;
use tauri::AppHandle;

pub(super) const KEY_CLIENT_ID: &str = "stravaClientId";
pub(super) const KEY_AUTO_SYNC: &str = "stravaAutoSync";
/// Epoch ms when auto-sync was last turned on; older walks are never auto-uploaded.
pub(super) const KEY_SINCE: &str = "strava_auto_sync_since";
pub(super) const KEY_ROLLUP: &str = "strava_daily_rollup_enabled";
/// Local "HH:MM", stored JSON-quoted.
pub(super) const KEY_SHIFT_END: &str = "shift_end_time";
/// Shared with the dashboard's session grouping.
pub(super) const KEY_MERGE_MINUTES: &str = "session_merge_threshold_minutes";
/// Shoe for every upload; absent means none.
pub(super) const KEY_GEAR_ID: &str = "strava_gear_id";
/// JSON list of the athlete's shoes.
pub(super) const KEY_GEAR_CACHE: &str = "strava_gear_cache";
/// JSON array of session ids saved as a TCX file for a manual upload.
pub(super) const KEY_SAVED_IDS: &str = "strava_saved_ids";
pub(super) const KEY_MUTE_MODE: &str = "strava_mute_mode";
pub(super) const KEY_MUTE_MINUTES: &str = "strava_mute_under_minutes";
/// Raw template text; absent or blank means the default.
pub(super) const KEY_TITLE_TEMPLATE: &str = "strava_title_template";
pub(super) const KEY_ROLLUP_TITLE_TEMPLATE: &str = "strava_rollup_title_template";
pub(super) const KEY_DESCRIPTION_TEMPLATE: &str = "strava_description_template";

/// A stored string with whitespace and JSON quotes removed.
pub(super) fn read_str(app: &AppHandle, key: &str) -> Option<String> {
    persistence::read_setting(app, key).map(|s| s.trim().trim_matches('"').to_string())
}

pub(super) fn read_flag(app: &AppHandle, key: &str) -> bool {
    read_str(app, key).as_deref() == Some("true")
}

fn write(app: &AppHandle, key: &str, value: &str) -> Result<(), String> {
    persistence::write_setting(app, key, value)
}

/// The client ID and secret, when both are saved and non-blank.
pub(super) fn load_credentials(app: &AppHandle) -> Option<(String, String)> {
    let id = read_str(app, KEY_CLIENT_ID).filter(|s| !s.is_empty())?;
    let secret = secrets::get(app, Secret::ClientSecret)
        .map(|s| s.trim().trim_matches('"').to_string())
        .filter(|s| !s.is_empty())?;
    Some((id, secret))
}

/// The shift end as stored, or the default when it is missing or malformed.
pub(super) fn shift_end_text(app: &AppHandle) -> String {
    read_str(app, KEY_SHIFT_END)
        .filter(|s| plan::parse_shift_end(s).is_some())
        .unwrap_or_else(|| plan::DEFAULT_SHIFT_END.to_string())
}

pub(super) fn shift_end(app: &AppHandle) -> Option<NaiveTime> {
    plan::parse_shift_end(&shift_end_text(app))
}

pub(super) fn merge_minutes(app: &AppHandle) -> u32 {
    plan::parse_merge_minutes(persistence::read_setting(app, KEY_MERGE_MINUTES).as_deref())
}

pub(super) fn cached_gear(app: &AppHandle) -> Vec<StravaGear> {
    persistence::read_setting(app, KEY_GEAR_CACHE)
        .and_then(|raw| serde_json::from_str::<Vec<StravaGear>>(&raw).ok())
        .unwrap_or_default()
}

pub(super) fn write_gear_cache(app: &AppHandle, shoes: &[StravaGear]) {
    match serde_json::to_string(shoes) {
        Ok(json) => {
            if let Err(e) = write(app, KEY_GEAR_CACHE, &json) {
                log::warn!("could not save the shoe list: {}", e);
            }
        }
        Err(e) => log::warn!("could not encode the shoe list: {}", e),
    }
}

pub(super) fn read_saved_ids(app: &AppHandle) -> Vec<i64> {
    persistence::read_setting(app, KEY_SAVED_IDS)
        .and_then(|raw| serde_json::from_str::<Vec<i64>>(&raw).ok())
        .unwrap_or_default()
}

pub(super) fn write_saved_ids(app: &AppHandle, ids: &[i64]) -> Result<(), String> {
    let json = serde_json::to_string(ids).map_err(|e| e.to_string())?;
    write(app, KEY_SAVED_IDS, &json)
}

/// Settings that shape an upload.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct Prefs {
    pub gear_id: Option<String>,
    pub mute: MuteMode,
    pub mute_minutes: u32,
    pub templates: Templates,
}

impl Prefs {
    /// Before anything is loaded. Blank templates read as their defaults.
    pub const DEFAULT: Prefs = Prefs {
        gear_id: None,
        mute: MuteMode::All,
        mute_minutes: plan::DEFAULT_MUTE_MINUTES,
        templates: Templates {
            title: String::new(),
            rollup_title: String::new(),
            description: String::new(),
        },
    };
}

pub(super) fn load_templates(app: &AppHandle) -> Templates {
    let stored = |key: &str| persistence::read_setting(app, key);
    Templates::from_stored(
        stored(KEY_TITLE_TEMPLATE).as_deref(),
        stored(KEY_ROLLUP_TITLE_TEMPLATE).as_deref(),
        stored(KEY_DESCRIPTION_TEMPLATE).as_deref(),
    )
}

pub(super) fn load_prefs(app: &AppHandle) -> Prefs {
    Prefs {
        gear_id: read_str(app, KEY_GEAR_ID).filter(|s| !s.is_empty()),
        mute: plan::parse_mute_mode(persistence::read_setting(app, KEY_MUTE_MODE).as_deref()),
        mute_minutes: plan::parse_mute_minutes(
            persistence::read_setting(app, KEY_MUTE_MINUTES).as_deref(),
        ),
        templates: load_templates(app),
    }
}

/// Load state at startup. Auto-sync already on without a cutoff (a build that
/// predates the cutoff) starts counting from now, so old walks are never
/// auto-uploaded.
pub fn init(app: &AppHandle) {
    // Plaintext secrets from earlier builds move to Credential Manager first.
    secrets::migrate(app);
    if read_flag(app, KEY_AUTO_SYNC) && persistence::read_setting(app, KEY_SINCE).is_none() {
        if let Err(e) = write(app, KEY_SINCE, &crate::util::now_ms().to_string()) {
            log::warn!("could not set the auto-sync cutoff: {}", e);
        }
    }
    live::reload(app);
}

pub fn set_credentials(
    app: &AppHandle,
    client_id: String,
    client_secret: String,
) -> Result<(), String> {
    let id = client_id.trim();
    let secret = client_secret.trim();
    if id.is_empty() || secret.is_empty() {
        return Err("Enter both the client ID and the client secret".to_string());
    }
    // Strava client ids are a few digits and secrets 40 hex characters.
    if id.len() > 100 || secret.len() > 200 {
        return Err("Those credentials look too long".to_string());
    }
    let changed = load_credentials(app).is_none_or(|(i, s)| i != id || s != secret);
    write(app, KEY_CLIENT_ID, id)?;
    secrets::set(app, Secret::ClientSecret, secret)?;
    if changed {
        // New credentials may fix whatever paused auto-sync.
        live().clear_backoff();
    }
    if changed && auth::load(app).is_some() {
        // A token belongs to the app that issued it; it cannot be refreshed with
        // another app's credentials.
        auth::clear(app);
        live::record(true, "Credentials changed. Connect your account again.");
    }
    live::reload(app);
    Ok(())
}

pub fn set_auto_sync(app: &AppHandle, enabled: bool) -> Result<(), String> {
    let was_on = read_flag(app, KEY_AUTO_SYNC);
    write(app, KEY_AUTO_SYNC, &enabled.to_string())?;
    if enabled && !was_on {
        // Only walks that finish from now on are auto-uploaded.
        write(app, KEY_SINCE, &crate::util::now_ms().to_string())?;
    }
    live().clear_backoff();
    live::reload(app);
    Ok(())
}

pub fn set_daily_rollup(app: &AppHandle, enabled: bool, shift_end: &str) -> Result<(), String> {
    if plan::parse_shift_end(shift_end).is_none() {
        return Err("Shift end must be a time like 18:00".to_string());
    }
    write(app, KEY_ROLLUP, &enabled.to_string())?;
    // Quoted like the dashboard's JSON.stringify of a string setting.
    let quoted = serde_json::to_string(shift_end).map_err(|e| e.to_string())?;
    write(app, KEY_SHIFT_END, &quoted)?;
    live::reload(app);
    Ok(())
}

/// Shoe for every upload; `None` (or blank) means no shoe.
pub fn set_gear(app: &AppHandle, gear_id: Option<String>) -> Result<(), String> {
    match plan::clean_gear_id(gear_id.as_deref())? {
        Some(id) => write(app, KEY_GEAR_ID, &id)?,
        None => persistence::delete_setting(app, KEY_GEAR_ID)?,
    }
    live::reload(app);
    Ok(())
}

pub fn set_mute(app: &AppHandle, mode: MuteMode, under_minutes: u32) -> Result<(), String> {
    write(app, KEY_MUTE_MODE, plan::mute_mode_setting(mode))?;
    write(
        app,
        KEY_MUTE_MINUTES,
        &under_minutes.clamp(1, 240).to_string(),
    )?;
    live::reload(app);
    Ok(())
}

/// Save the templates. A blank one is removed, which brings back its default.
pub fn set_templates(
    app: &AppHandle,
    title: String,
    rollup_title: String,
    description: String,
) -> Result<(), String> {
    template::check_length(&title, template::MAX_TITLE_TEMPLATE_CHARS)?;
    template::check_length(&rollup_title, template::MAX_TITLE_TEMPLATE_CHARS)?;
    template::check_length(&description, template::MAX_DESCRIPTION_TEMPLATE_CHARS)?;
    for (key, value) in [
        (KEY_TITLE_TEMPLATE, title),
        (KEY_ROLLUP_TITLE_TEMPLATE, rollup_title),
        (KEY_DESCRIPTION_TEMPLATE, description),
    ] {
        if value.trim().is_empty() {
            persistence::delete_setting(app, key)?;
        } else {
            write(app, key, &value)?;
        }
    }
    live::reload(app);
    Ok(())
}

/// Open the Strava API settings page, where an inactive application is fixed.
pub fn open_api_settings(app: &AppHandle) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;
    app.opener()
        .open_url(status::API_SETTINGS_URL, None::<&str>)
        .map_err(|e| format!("Could not open the browser: {}", e))
}
