//! The in-memory Strava state the snapshot shows, and the guards that keep it
//! honest while uploads, logins and shoe refreshes run.

use super::auth;
use super::settings::{self, Prefs};
use crate::snapshot::{StravaGear, StravaResult, StravaSnapshot, StravaToday};
use crate::strava_plan as plan;
use crate::strava_status::{self as status, ActivityInputs, StravaHint, StravaIssue, StravaStep};
use crate::strava_template::Templates;
use std::collections::BTreeMap;
use std::sync::{Mutex, MutexGuard};
use std::time::Instant;
use tauri::AppHandle;

pub(super) struct Live {
    pub has_credentials: bool,
    pub connected: bool,
    pub athlete_name: Option<String>,
    pub auto_sync: bool,
    pub daily_rollup: bool,
    /// Already validated; empty until the first reload.
    pub shift_end: String,
    /// Uploads queued or running; `syncing` is `pending > 0`.
    pub pending: u32,
    pub last_result: Option<StravaResult>,
    pub backoff_until: Option<Instant>,
    /// Why the back-off started, for the countdown line.
    pub backoff_reason: String,
    /// Auto-sync is stopped until the user acts (e.g. the API app is inactive).
    pub paused: Option<String>,
    /// The browser login is open until this moment.
    pub authorizing_until: Option<Instant>,
    /// The upload that is running: its sessions, label and step.
    pub uploading_ids: Vec<i64>,
    pub upload_label: String,
    pub upload_step: Option<StravaStep>,
    pub checking_duplicates: bool,
    /// Why each session's last upload failed.
    pub upload_errors: BTreeMap<i64, String>,
    /// Walks saved as a file for a manual upload.
    pub saved_ids: Vec<i64>,
    pub needs_reconnect: bool,
    /// Why the login is not in Credential Manager, while a plaintext copy is in use.
    pub login_storage: Option<&'static str>,
    pub gear: Vec<StravaGear>,
    pub gear_loading: bool,
    /// Shoe, mute and templates, as the next upload would use them.
    pub prefs: Prefs,
    /// What DAILY_UPLINK would send, from the last read of today's walks;
    /// None until the first read.
    pub today: Option<StravaToday>,
}

impl Live {
    /// Forget any back-off or pause so the next pass runs.
    pub fn clear_backoff(&mut self) {
        self.backoff_until = None;
        self.paused = None;
    }

    fn backing_off(&self) -> bool {
        self.backoff_until.is_some_and(|t| Instant::now() < t)
    }

    /// A timed pass should wait: auto-sync is paused or backing off.
    pub fn holding(&self) -> bool {
        self.paused.is_some() || self.backing_off()
    }
}

static LIVE: Mutex<Live> = Mutex::new(Live {
    has_credentials: false,
    connected: false,
    athlete_name: None,
    auto_sync: false,
    daily_rollup: false,
    shift_end: String::new(),
    pending: 0,
    last_result: None,
    backoff_until: None,
    backoff_reason: String::new(),
    paused: None,
    authorizing_until: None,
    uploading_ids: Vec::new(),
    upload_label: String::new(),
    upload_step: None,
    checking_duplicates: false,
    upload_errors: BTreeMap::new(),
    saved_ids: Vec::new(),
    needs_reconnect: false,
    login_storage: None,
    gear: Vec::new(),
    gear_loading: false,
    prefs: Prefs::DEFAULT,
    today: None,
});

pub(super) fn live() -> MutexGuard<'static, Live> {
    LIVE.lock().unwrap_or_else(|e| e.into_inner())
}

/// Whole seconds until `t`, rounded up so the countdown never shows 0 early.
fn secs_left(t: Instant) -> u32 {
    t.saturating_duration_since(Instant::now())
        .as_secs_f32()
        .ceil() as u32
}

/// Strava as the windows see it. Built from memory only; no I/O.
pub fn snapshot() -> StravaSnapshot {
    let l = live();
    // Before the first reload the stored templates are blank: show the defaults.
    let stored = &l.prefs.templates;
    let templates = Templates::from_stored(
        Some(&stored.title),
        Some(&stored.rollup_title),
        Some(&stored.description),
    );
    StravaSnapshot {
        has_credentials: l.has_credentials,
        connected: l.connected,
        athlete_name: l.athlete_name.clone(),
        auto_sync: l.auto_sync,
        daily_rollup: l.daily_rollup,
        shift_end: if l.shift_end.is_empty() {
            plan::DEFAULT_SHIFT_END.to_string()
        } else {
            l.shift_end.clone()
        },
        syncing: l.pending > 0,
        activity: status::activity(&ActivityInputs {
            authorizing_left_s: l.authorizing_until.map(secs_left),
            pending: l.pending,
            checking_duplicates: l.checking_duplicates,
            uploading: l.upload_step.map(|step| (l.upload_label.clone(), step)),
            backoff: l
                .backoff_until
                .filter(|t| Instant::now() < *t)
                .map(|t| (secs_left(t), l.backoff_reason.clone())),
            paused: l.paused.clone(),
        }),
        uploading_session_ids: l.uploading_ids.clone(),
        upload_errors: l.upload_errors.clone(),
        saved_session_ids: l.saved_ids.clone(),
        last_result: l.last_result.clone(),
        needs_reconnect: l.needs_reconnect,
        login_storage: l.login_storage.map(str::to_string),
        gear: l.gear.clone(),
        gear_id: l.prefs.gear_id.clone(),
        gear_loading: l.gear_loading,
        mute_mode: l.prefs.mute,
        mute_under_minutes: l.prefs.mute_minutes,
        title_template: templates.title,
        rollup_title_template: templates.rollup_title,
        description_template: templates.description,
        today: super::today::shown(l.connected, l.today.as_ref()),
    }
}

/// Re-read everything the snapshot shows from the settings table and the
/// credential store.
pub(super) fn reload(app: &AppHandle) {
    let auth = auth::load(app);
    let has_credentials = settings::load_credentials(app).is_some();
    let shift_end = settings::shift_end_text(app);
    let auto_sync = settings::read_flag(app, settings::KEY_AUTO_SYNC);
    let daily_rollup = settings::read_flag(app, settings::KEY_ROLLUP);
    let gear = settings::cached_gear(app);
    let prefs = settings::load_prefs(app);
    let saved_ids = settings::read_saved_ids(app);
    let login_storage =
        crate::secrets::held_in_database(app).then_some(crate::secrets::NOT_SAVED_SECURELY);

    let mut l = live();
    l.saved_ids = saved_ids;
    l.has_credentials = has_credentials;
    l.connected = auth.is_some();
    l.needs_reconnect = auth
        .as_ref()
        .is_some_and(|a| plan::needs_reconnect(true, a.scope.as_deref()));
    l.athlete_name = auth.and_then(|a| a.athlete_name);
    l.login_storage = login_storage;
    l.auto_sync = auto_sync;
    l.daily_rollup = daily_rollup;
    l.shift_end = shift_end;
    l.gear = gear;
    l.prefs = prefs;
}

/// Record the outcome shown in the dashboard. `at_ms` always moves forward so a
/// window can use it to detect "something new happened".
pub(super) fn record(ok: bool, message: impl Into<String>) {
    record_hint(ok, message, StravaHint::None);
}

pub(super) fn record_hint(ok: bool, message: impl Into<String>, hint: StravaHint) {
    let mut l = live();
    let prev = l.last_result.as_ref().map_or(0, |r| r.at_ms);
    l.last_result = Some(StravaResult {
        ok,
        message: message.into(),
        at_ms: crate::util::now_ms().max(prev + 1),
        hint,
    });
}

/// Record a failure from raw error text: one plain sentence (never Strava's
/// JSON), with what the person can do about it. Returns the explained issue.
pub(super) fn record_failure(prefix: &str, raw: &str) -> StravaIssue {
    let issue = status::describe(raw);
    let message = if prefix.is_empty() {
        issue.message.clone()
    } else {
        format!("{} {}", prefix, issue.message)
    };
    record_hint(false, message, issue.hint);
    issue
}

pub(super) fn set_upload_errors(ids: &[i64], message: &str) {
    let mut l = live();
    for id in ids {
        l.upload_errors.insert(*id, message.to_string());
    }
}

pub(super) fn clear_upload_errors(ids: &[i64]) {
    let mut l = live();
    for id in ids {
        l.upload_errors.remove(id);
    }
}

/// The current upload moved on to `step`.
pub(super) fn set_step(step: StravaStep) {
    let mut l = live();
    l.upload_step = Some(step);
    l.checking_duplicates = false;
}

pub(super) fn set_label(label: String) {
    live().upload_label = label;
}

/// Counts one queued or running upload until dropped.
pub(super) struct Pending;

impl Pending {
    pub fn new() -> Self {
        live().pending += 1;
        Pending
    }
}

impl Drop for Pending {
    fn drop(&mut self) {
        let mut l = live();
        l.pending = l.pending.saturating_sub(1);
    }
}

/// Marks which upload is running (and its step) until dropped.
pub(super) struct UploadActive;

impl UploadActive {
    pub fn new(ids: &[i64], label: String) -> Self {
        let mut l = live();
        l.uploading_ids = ids.to_vec();
        l.upload_label = label;
        l.upload_step = Some(StravaStep::Preparing);
        l.checking_duplicates = false;
        UploadActive
    }
}

impl Drop for UploadActive {
    fn drop(&mut self) {
        let mut l = live();
        l.uploading_ids.clear();
        l.upload_label.clear();
        l.upload_step = None;
        l.checking_duplicates = false;
    }
}

/// Sets `gear_loading` until dropped, however a shoe refresh ends.
pub(super) struct GearLoading;

impl GearLoading {
    pub fn new() -> Self {
        live().gear_loading = true;
        GearLoading
    }
}

impl Drop for GearLoading {
    fn drop(&mut self) {
        live().gear_loading = false;
    }
}
