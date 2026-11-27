//! Uploading sessions to Strava: one activity per call, with the duplicate
//! check, upload polling, marking the sessions, and the shoe/mute update.
//! Also the shoe list, which is fetched the same way.

use super::api::{self, NOT_CONNECTED};
use super::auth;
use super::live::{self, live, GearLoading, Pending, UploadActive};
use super::settings::{self, Prefs};
use crate::bluetooth::ORCHESTRATOR;
use crate::persistence::{self, Sample, Session};
use crate::strava_plan::{self as plan, ActivityPlan, Naming, Payload, UploadStatus};
use crate::strava_status::StravaStep;
use std::time::Duration;
use tauri::AppHandle;

/// Strava processes a TCX in a few seconds; 30 polls 2 s apart wait a minute,
/// longer than its processing normally takes, and well under its rate limits.
const POLL_EVERY: Duration = Duration::from_secs(2);
const POLL_TRIES: u32 = 30;

/// Every upload, manual or background, runs under this lock: one at a time,
/// and each re-reads the sessions after taking it so none can be sent twice.
pub(super) static UPLOAD_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// `"s"` for any count but one.
pub(super) fn plural(n: usize) -> &'static str {
    if n == 1 {
        ""
    } else {
        "s"
    }
}

/// How an upload ended well.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct Done {
    pub activity_id: String,
    pub distance_m: f64,
    /// The walk was already on Strava from another app, so nothing was sent.
    pub already_there: bool,
    /// The upload worked but the shoe/mute update did not.
    pub update_error: Option<String>,
}

impl Done {
    pub fn message(&self) -> String {
        if self.already_there {
            return "Already on Strava (from another app)".to_string();
        }
        let base = format!(
            "Uploaded {:.2} km to Strava (activity {})",
            self.distance_m / 1000.0,
            self.activity_id
        );
        match &self.update_error {
            Some(e) => format!("{}. Shoes and mute not applied: {}", base, e),
            None => base,
        }
    }
}

pub(super) type SessionData = Vec<(Session, Vec<Sample>)>;

fn load_session_data(app: &AppHandle, ids: &[i64]) -> Result<SessionData, String> {
    let sessions = persistence::db_sessions_by_ids(app, ids)?;
    if sessions.len() != ids.len() {
        return Err("Some of the selected sessions no longer exist".to_string());
    }
    sessions
        .into_iter()
        .map(|s| {
            let samples = persistence::db_get_samples(app, s.id.unwrap_or_default())?;
            Ok((s, samples))
        })
        .collect()
}

/// These sessions with their samples, read off the async runtime.
pub(super) async fn load_data(app: &AppHandle, ids: Vec<i64>) -> Result<SessionData, String> {
    let app = app.clone();
    super::blocking(move || load_session_data(&app, &ids)).await
}

pub(super) async fn active_session_id() -> Option<i64> {
    ORCHESTRATOR.lock().await.session.active_session_id
}

/// Upload exactly these sessions as one activity. Returns once queued; the
/// outcome arrives in `snapshot().last_result`.
pub fn upload_manual(app: &AppHandle, session_ids: Vec<i64>) -> Result<(), String> {
    if session_ids.is_empty() {
        return Err("No sessions to upload".to_string());
    }
    if !live().connected {
        return Err(NOT_CONNECTED.to_string());
    }
    // A retry starts clean: the old reason goes away at once.
    live::clear_upload_errors(&session_ids);
    let pending = Pending::new();
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let _pending = pending;
        let _lock = UPLOAD_LOCK.lock().await;
        match upload_sessions(&app, &session_ids, |_| Naming::Walk, true).await {
            Ok(done) => {
                live::clear_upload_errors(&session_ids);
                live::record(true, done.message());
            }
            Err(e) => {
                log::warn!("upload failed: {}", e);
                let issue = live::record_failure("Upload failed.", &e);
                live::set_upload_errors(&session_ids, &issue.message);
            }
        }
        let app_c = app.clone();
        let _ = super::blocking(move || {
            super::today::refresh(&app_c);
            Ok(())
        })
        .await;
    });
    Ok(())
}

/// Upload these sessions as one activity and mark each with its Strava id.
/// The caller holds `UPLOAD_LOCK`. `name` picks the activity name from the
/// number of merged walks; `explicit` is a manual request (so a session already
/// on Strava or still running is an error rather than a silent skip).
pub(super) async fn upload_sessions(
    app: &AppHandle,
    ids: &[i64],
    name: impl Fn(usize) -> Naming,
    explicit: bool,
) -> Result<Done, String> {
    let _active = UploadActive::new(ids, format!("{} session{}", ids.len(), plural(ids.len())));
    let data = load_data(app, ids.to_vec()).await?;
    let active = active_session_id().await;
    if data.iter().any(|(s, _)| active.is_some() && s.id == active) {
        return Err("The session is still running".to_string());
    }
    if data.iter().any(|(s, _)| plan::is_uploaded(&s.strava_id)) {
        return Err(if explicit {
            "Already on Strava"
        } else {
            "Session already uploaded"
        }
        .to_string());
    }

    let sessions: Vec<Session> = data.iter().map(|(s, _)| s.clone()).collect();
    let walks = plan::merge_sessions(&sessions, settings::merge_minutes(app)).len();
    let prefs = settings::load_prefs(app);
    let pc_walked = super::pc_walked_pct(app, &data).await;
    let activity = plan::plan_activity(&data, name(walks), &prefs.templates, pc_walked);
    live::set_label(format!("{:.2} km", activity.distance_m / 1000.0));

    if let Some(existing) = find_existing(app, &activity).await? {
        // Another app already put this walk on Strava: link to it, send nothing.
        mark_uploaded(app, ids, &existing.id).await?;
        return Ok(Done {
            activity_id: existing.id,
            distance_m: activity.distance_m,
            already_there: true,
            update_error: None,
        });
    }

    let activity_id = push(app, &activity, &external_id(ids)).await?;
    mark_uploaded(app, ids, &activity_id).await?;
    let update_error = apply_update(app, &activity_id, &prefs, activity.elapsed_s).await;

    Ok(Done {
        activity_id,
        distance_m: activity.distance_m,
        already_there: false,
        update_error,
    })
}

/// Strava's `external_id` for an upload: stable for the same sessions, so a
/// repeated upload is recognised as a duplicate.
fn external_id(ids: &[i64]) -> String {
    let joined: Vec<String> = ids.iter().map(i64::to_string).collect();
    format!("neon-strider-{}", joined.join("-"))
}

/// Look for the same walk already on Strava (another app got there first):
/// one list call around our start and end. Skipped, with no call, when the
/// login lacks `activity:read`. A failed call fails the upload (it is retried
/// later) rather than risking a duplicate.
async fn find_existing(
    app: &AppHandle,
    activity: &ActivityPlan,
) -> Result<Option<plan::Existing>, String> {
    if !plan::scopes(auth::stored_scope(app).as_deref()).list_activities {
        log::info!("duplicate check skipped: the Strava login cannot list activities");
        return Ok(None);
    }
    live().checking_duplicates = true;
    let (after, before) = plan::duplicate_window(activity.start_ms, activity.end_ms);
    let rows = api::with_token(app, |t| api::list_activities(t, after, before)).await?;
    Ok(plan::find_duplicate(&plan::parse_activities(&rows), activity.start_ms).cloned())
}

/// Send the plan to Strava and return the activity id. File uploads are
/// processed asynchronously on Strava's side, so poll until it has an activity
/// or an error.
async fn push(
    app: &AppHandle,
    activity: &ActivityPlan,
    external_id: &str,
) -> Result<String, String> {
    live::set_step(StravaStep::Sending);
    let tcx = match &activity.payload {
        Payload::Manual => {
            let v = api::with_token(app, |t| api::post_manual(t, activity)).await?;
            return plan::json_id(&v["id"])
                .ok_or_else(|| "Strava did not return an activity id".to_string());
        }
        Payload::Tcx(tcx) => tcx,
    };
    let mut reply = api::with_token(app, |t| {
        api::post_tcx(t, activity, external_id, tcx.clone())
    })
    .await?;
    let upload_id = plan::json_id(&reply["id"]);
    for attempt in 0..=POLL_TRIES {
        match plan::parse_upload_status(&reply) {
            UploadStatus::Done(id) => return Ok(id),
            UploadStatus::Failed(e) => return Err(format!("Strava rejected the file: {}", e)),
            UploadStatus::Pending => {}
        }
        let Some(upload_id) = upload_id.clone() else {
            return Err("Strava did not return an upload id".to_string());
        };
        if attempt == POLL_TRIES {
            break;
        }
        live::set_step(StravaStep::Processing);
        tokio::time::sleep(POLL_EVERY).await;
        reply = api::with_token(app, |t| api::get_upload(t, upload_id.clone())).await?;
    }
    Err("Strava is still processing the upload; it will be retried".to_string())
}

/// Record the Strava activity id on each session. A session that cannot be
/// marked is logged: the walk is on Strava either way.
async fn mark_uploaded(app: &AppHandle, ids: &[i64], activity_id: &str) -> Result<(), String> {
    let app = app.clone();
    let ids = ids.to_vec();
    let activity_id = activity_id.to_string();
    super::blocking(move || {
        for id in ids {
            if let Err(e) = persistence::db_mark_uploaded(&app, id, &activity_id) {
                log::warn!("uploaded but could not mark session {}: {}", id, e);
            }
        }
        Ok(())
    })
    .await
}

/// The single `PUT /activities/{id}` for the shoe and the mute. Best effort:
/// the walk is already uploaded and marked, so a failure is reported, not retried.
async fn apply_update(
    app: &AppHandle,
    activity_id: &str,
    prefs: &Prefs,
    duration_s: i64,
) -> Option<String> {
    let hide = plan::should_hide(prefs.mute, prefs.mute_minutes, duration_s);
    let body = plan::activity_update(prefs.gear_id.as_deref(), hide)?;
    live::set_step(StravaStep::Finishing);
    match api::with_token(app, |t| {
        api::put_activity(t, activity_id.to_string(), body.clone())
    })
    .await
    {
        Ok(_) => None,
        Err(e) => {
            log::warn!(
                "activity {} uploaded but the update failed: {}",
                activity_id,
                e
            );
            Some(e)
        }
    }
}

/// Re-read the shoe list. Returns once queued; the list lands in the snapshot.
pub fn refresh_gear_command(app: &AppHandle) -> Result<(), String> {
    if !live().connected {
        return Err(NOT_CONNECTED.to_string());
    }
    let app = app.clone();
    tauri::async_runtime::spawn(async move { refresh_gear(&app, true).await });
    Ok(())
}

/// Fetch the athlete's shoes and cache them. `report` shows the outcome in the
/// dashboard; the call after connecting only logs, since the "Connected" result
/// is already there.
pub(super) async fn refresh_gear(app: &AppHandle, report: bool) {
    let _loading = GearLoading::new();
    match fetch_shoes(app).await {
        Ok(shoes) => {
            settings::write_gear_cache(app, &shoes);
            live::reload(app);
            if report {
                let n = shoes.iter().filter(|g| !g.retired).count();
                live::record(true, format!("Loaded {} shoe{} from Strava", n, plural(n)));
            }
        }
        Err(e) => {
            log::warn!("could not load shoes: {}", e);
            if report {
                live::record_failure("Could not load shoes.", &e);
            }
        }
    }
}

async fn fetch_shoes(app: &AppHandle) -> Result<Vec<crate::snapshot::StravaGear>, String> {
    if !plan::scopes(auth::stored_scope(app).as_deref()).gear {
        return Err("Reconnect Strava and allow profile access to see your shoes.".to_string());
    }
    let athlete = api::with_token(app, api::get_athlete).await?;
    plan::parse_shoes(&athlete)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn done(already_there: bool, update_error: Option<&str>) -> Done {
        Done {
            activity_id: "42".to_string(),
            distance_m: 2345.0,
            already_there,
            update_error: update_error.map(str::to_string),
        }
    }

    #[test]
    fn done_messages() {
        assert_eq!(
            done(false, None).message(),
            "Uploaded 2.35 km to Strava (activity 42)"
        );
        assert_eq!(
            done(false, Some("boom")).message(),
            "Uploaded 2.35 km to Strava (activity 42). Shoes and mute not applied: boom"
        );
        assert_eq!(
            done(true, Some("ignored")).message(),
            "Already on Strava (from another app)"
        );
    }

    #[test]
    fn external_id_joins_session_ids() {
        assert_eq!(external_id(&[7]), "neon-strider-7");
        assert_eq!(external_id(&[3, 4, 9]), "neon-strider-3-4-9");
    }

    #[test]
    fn plural_suffix() {
        assert_eq!(plural(0), "s");
        assert_eq!(plural(1), "");
        assert_eq!(plural(2), "s");
    }
}
