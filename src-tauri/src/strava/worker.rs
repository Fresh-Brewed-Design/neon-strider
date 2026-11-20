//! The background auto-sync worker: a pass every minute and one shortly after
//! a session ends. A pass finds the finished walks (or rolled-up days) that are
//! not on Strava yet and uploads them one by one, backing off after a failure.

use super::api::NOT_CONNECTED;
use super::live::{self, live, Pending};
use super::settings;
use super::upload::{self, UPLOAD_LOCK};
use crate::persistence;
use crate::strava_plan::{self as plan, MergedSession, Naming, RollupDay};
use crate::strava_status as status;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};

/// A finished walk is also passed at once (`SESSION_ENDED`), so the minute pass is a backstop.
const PASS_INTERVAL: Duration = Duration::from_secs(60);
/// After the treadmill connect at startup, before the update check at 30 s.
const FIRST_PASS_DELAY: Duration = Duration::from_secs(20);
/// The engine writes a session's final totals off-thread when it ends.
/// A single SQLite write takes milliseconds; 3 s is ample. The dashboard waits 1.5 s
/// for the same reason (`sessionCoordinator.ts`).
const AFTER_SESSION_DELAY: Duration = Duration::from_secs(3);
/// Strava's rate limit is per 15 minutes, so a failed pass retries in the next window.
const FAILURE_BACKOFF: Duration = Duration::from_secs(15 * 60);

static SESSION_ENDED: tokio::sync::Notify = tokio::sync::Notify::const_new();

/// Called by the engine when a session ends (belt stopped).
pub fn notify_session_ended() {
    SESSION_ENDED.notify_one();
}

/// One activity a pass will upload.
enum Work {
    Walk(MergedSession),
    Day(RollupDay),
}

impl Work {
    fn ids(&self) -> &[i64] {
        match self {
            Work::Walk(w) => &w.ids,
            Work::Day(d) => &d.ids,
        }
    }

    fn naming(&self, walks: usize) -> Naming {
        match self {
            Work::Walk(_) => Naming::Walk,
            Work::Day(_) => Naming::Rollup { walks },
        }
    }
}

struct PassConfig {
    since_ms: i64,
    rollup: bool,
    shift_end: chrono::NaiveTime,
    merge_minutes: u32,
}

/// None when auto-sync is off, not connected, or has no usable cutoff.
fn load_pass_config(app: &AppHandle) -> Option<PassConfig> {
    if !(live().connected && settings::read_flag(app, settings::KEY_AUTO_SYNC)) {
        return None;
    }
    let since_ms = settings::read_str(app, settings::KEY_SINCE)?
        .parse::<i64>()
        .ok()?;
    Some(PassConfig {
        since_ms,
        rollup: settings::read_flag(app, settings::KEY_ROLLUP),
        shift_end: settings::shift_end(app)?,
        merge_minutes: settings::merge_minutes(app),
    })
}

fn find_work(app: &AppHandle, cfg: &PassConfig, active: Option<i64>) -> Result<Vec<Work>, String> {
    let sessions = persistence::db_sessions_ended_after(app, cfg.since_ms)?;
    let now = chrono::Local::now();
    Ok(if cfg.rollup {
        plan::select_rollup_days(
            &sessions,
            active,
            cfg.since_ms,
            cfg.shift_end,
            cfg.merge_minutes,
            now,
        )
        .into_iter()
        .map(Work::Day)
        .collect()
    } else {
        plan::select_walks(
            &sessions,
            active,
            cfg.since_ms,
            cfg.merge_minutes,
            now.timestamp_millis(),
        )
        .into_iter()
        .map(Work::Walk)
        .collect()
    })
}

fn toast(app: &AppHandle, message: &str, actionable: bool) {
    let _ = app.emit(
        "hud-update",
        serde_json::json!({ "type": "STRAVA", "message": message, "isActionable": actionable }),
    );
}

/// Run the auto-sync pass now, as "Sync now" in the dashboard.
pub fn sync_now(app: &AppHandle) -> Result<(), String> {
    {
        let l = live();
        if !l.connected {
            return Err(NOT_CONNECTED.to_string());
        }
        if !l.auto_sync {
            return Err("Turn auto-sync on first".to_string());
        }
    }
    let pending = Pending::new();
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        run_pass(&app, Some(pending)).await;
    });
    Ok(())
}

/// Clear any back-off or pause, then run the auto-sync pass.
pub fn retry_now(app: &AppHandle) -> Result<(), String> {
    live().clear_backoff();
    sync_now(app)
}

/// One auto-sync pass. `manual` carries the "Sync now" pending marker (so the
/// dashboard shows it syncing straight away); a timed pass marks itself pending
/// only while it is actually uploading.
async fn run_pass(app: &AppHandle, manual: Option<Pending>) {
    let is_manual = manual.is_some();
    let _lock = UPLOAD_LOCK.lock().await;

    let app_c = app.clone();
    let cfg = super::blocking(move || Ok(load_pass_config(&app_c)))
        .await
        .ok()
        .flatten();
    let Some(cfg) = cfg else { return };

    if is_manual {
        live().clear_backoff();
    } else if live().holding() {
        return;
    }

    let active = upload::active_session_id().await;
    let app_c = app.clone();
    let work = match super::blocking(move || find_work(&app_c, &cfg, active)).await {
        Ok(w) => w,
        Err(e) => {
            log::warn!("could not read sessions: {}", e);
            return;
        }
    };

    if work.is_empty() {
        if is_manual {
            live::record(true, "Nothing to upload yet");
        }
        return;
    }

    for item in work {
        let _pending = Pending::new();
        let result =
            upload::upload_sessions(app, item.ids(), |walks| item.naming(walks), false).await;
        match result {
            Ok(done) => {
                live::clear_upload_errors(item.ids());
                live::record(true, done.message());
                if done.already_there {
                    toast(app, "STRAVA SKIPPED \u{b7} ALREADY LOGGED", false);
                } else {
                    toast(
                        app,
                        &format!("STRAVA UPLOADED \u{b7} {:.2} KM", done.distance_m / 1000.0),
                        false,
                    );
                }
            }
            Err(e) => {
                log::warn!("auto-sync failed: {}", e);
                report_failure(app, &item, &e);
                return;
            }
        }
    }
}

/// A failed pass: pause (when retrying cannot help) or back off, and say so.
fn report_failure(app: &AppHandle, item: &Work, error: &str) {
    let issue = status::describe(error);
    {
        let mut l = live();
        if issue.permanent {
            // Retrying cannot fix it: stop until the user acts.
            l.paused = Some(issue.message.clone());
            l.backoff_until = None;
        } else {
            l.backoff_until = Some(Instant::now() + FAILURE_BACKOFF);
            l.backoff_reason = issue.message.clone();
        }
    }
    let tail = if issue.permanent {
        "Auto-sync is paused until you press Retry or change the credentials."
    } else {
        "Trying again in 15 minutes."
    };
    live::record_hint(
        false,
        format!("Auto-sync failed. {} {}", issue.message, tail),
        issue.hint,
    );
    live::set_upload_errors(item.ids(), &issue.message);
    toast(
        app,
        if issue.permanent {
            "STRAVA PAUSED \u{b7} API APP OFF"
        } else {
            "STRAVA UPLOAD FAILED"
        },
        true,
    );
}

/// Start the worker: a pass every minute, and one shortly after a session ends.
pub fn spawn_worker(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let mut tick = tokio::time::interval_at(
            tokio::time::Instant::now() + FIRST_PASS_DELAY,
            PASS_INTERVAL,
        );
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            let after_session = tokio::select! {
                _ = tick.tick() => false,
                _ = SESSION_ENDED.notified() => true,
            };
            if after_session {
                tokio::time::sleep(AFTER_SESSION_DELAY).await;
            }
            // Its own task, so a panic in one pass cannot end the worker.
            let app_c = app.clone();
            let _ = tauri::async_runtime::spawn(async move { run_pass(&app_c, None).await }).await;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn walk(ids: Vec<i64>) -> MergedSession {
        MergedSession {
            ids,
            start_time: 0,
            end_time: 0,
            duration_seconds: 0,
            distance_meters: 0.0,
            raw_distance_meters: 0.0,
            calories: 0.0,
            steps: 0.0,
            raw_steps: 0.0,
            avg_speed: 0.0,
            raw_avg_speed: 0.0,
            avg_incline: 0.0,
            neural_heat: 0.0,
            neural_ticks: 0,
            strava_id: None,
        }
    }

    #[test]
    fn work_ids_and_naming() {
        let w = Work::Walk(walk(vec![1, 2]));
        assert_eq!(w.ids(), &[1, 2]);
        assert_eq!(w.naming(3), Naming::Walk);

        let d = Work::Day(RollupDay {
            day: chrono::NaiveDate::from_ymd_opt(2026, 11, 13).unwrap(),
            ids: vec![5, 6, 7],
            walks: 2,
        });
        assert_eq!(d.ids(), &[5, 6, 7]);
        assert_eq!(d.naming(2), Naming::Rollup { walks: 2 });
    }
}
