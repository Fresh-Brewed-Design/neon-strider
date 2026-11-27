//! DAILY_UPLINK: upload today's walks now as one Strava activity, with
//! auto-sync off and before the rollup time. The rules are pure
//! (`strava_plan::plan_today`, the daily rollup's joining, title and duplicate
//! rules); this is the reading, the cached plan the snapshot shows and the press.

use super::api::NOT_CONNECTED;
use super::live::{self, live, Pending};
use super::settings;
use super::upload::{self, plural, UPLOAD_LOCK};
use crate::persistence;
use crate::snapshot::StravaToday;
use crate::strava_plan::{self as plan, Naming, TodayBlock, TodayPlan};
use chrono::{Local, TimeZone};
use tauri::AppHandle;

/// The reason shown while no Strava account is linked.
pub(super) const NOT_LINKED: &str = "Link a Strava account to upload today.";
/// The reason shown before today's walks were first read.
pub(super) const NOT_READ: &str = "Reading today's walks.";

/// Today's walks as DAILY_UPLINK would send them now.
fn read_plan(app: &AppHandle, active: Option<i64>) -> Result<TodayPlan, String> {
    let now = Local::now();
    let today = now.date_naive();
    // A session of today ends after local midnight. On a day whose midnight
    // a DST change skips, a day back is a safe over-read (`plan_today` keeps
    // only the sessions that started today).
    let midnight_ms = today
        .and_hms_opt(0, 0, 0)
        .and_then(|t| Local.from_local_datetime(&t).earliest())
        .map_or(now.timestamp_millis() - 24 * 3_600_000, |t| {
            t.timestamp_millis()
        });
    let sessions = persistence::db_sessions_ended_after(app, midnight_ms - 1)?;
    Ok(plan::plan_today(
        &sessions,
        active,
        today,
        settings::merge_minutes(app),
    ))
}

/// The plan as the windows see it.
fn to_snapshot(p: &TodayPlan) -> StravaToday {
    StravaToday {
        walks: p.rollup.as_ref().map_or(0, |r| r.walks) as u32,
        distance_km: p.distance_m / 1000.0,
        on_strava: p.on_strava as u32,
        blocked: p.block.map(|b| b.sentence().to_string()),
        note: plan::today_note(p),
    }
}

/// Re-read today's walks for the snapshot. The running walk is laid over it
/// by the emitter (`hold_while_walking`), so none is passed here.
pub(super) fn refresh(app: &AppHandle) {
    match read_plan(app, None) {
        Ok(p) => live().today = Some(to_snapshot(&p)),
        Err(e) => log::warn!("could not read today's walks: {}", e),
    }
}

/// What the snapshot shows: the cached plan, or why it cannot be used.
pub(super) fn shown(connected: bool, cached: Option<&StravaToday>) -> StravaToday {
    let reason = |text: &str| StravaToday {
        blocked: Some(text.to_string()),
        note: None,
        ..cached.cloned().unwrap_or_default()
    };
    match cached {
        _ if !connected => reason(NOT_LINKED),
        None => reason(NOT_READ),
        Some(t) => t.clone(),
    }
}

/// While a walk runs DAILY_UPLINK waits for it, as the press would.
pub fn hold_while_walking(t: &mut StravaToday) {
    if t.blocked.as_deref() != Some(NOT_LINKED) {
        t.blocked = Some(TodayBlock::Running.sentence().to_string());
        t.note = None;
    }
}

/// The result line for a finished DAILY_UPLINK.
fn done_message(done: &upload::Done, walks: usize) -> String {
    if done.already_there {
        return done.message();
    }
    let base = format!(
        "Today uploaded as one activity: {} walk{}, {:.2} km (activity {})",
        walks,
        plural(walks),
        done.distance_m / 1000.0,
        done.activity_id
    );
    match &done.update_error {
        Some(e) => format!("{}. Shoes and mute not applied: {}", base, e),
        None => base,
    }
}

/// DAILY_UPLINK pressed: refuse at once when there is nothing to send (the
/// reason shows under the button), else queue the upload. The outcome arrives
/// in `snapshot().last_result`.
pub async fn upload_today(app: &AppHandle) -> Result<(), String> {
    if !live().connected {
        return Err(NOT_CONNECTED.to_string());
    }
    let active = upload::active_session_id().await;
    let app_c = app.clone();
    let first = super::blocking(move || read_plan(&app_c, active)).await?;
    if let Some(block) = first.block {
        live().today = Some(to_snapshot(&first));
        return Err(block.sentence().to_string());
    }

    let pending = Pending::new();
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let _pending = pending;
        let _lock = UPLOAD_LOCK.lock().await;
        // Read again under the lock: an auto-sync pass may have sent part of
        // today meanwhile, and a session already on Strava is never sent twice.
        let active = upload::active_session_id().await;
        let app_c = app.clone();
        match super::blocking(move || read_plan(&app_c, active)).await {
            Ok(TodayPlan {
                rollup: Some(day), ..
            }) => {
                live::clear_upload_errors(&day.ids);
                match upload::upload_sessions(
                    &app,
                    &day.ids,
                    |walks| Naming::Rollup { walks },
                    true,
                )
                .await
                {
                    Ok(done) => {
                        live::clear_upload_errors(&day.ids);
                        live::record(true, done_message(&done, day.walks));
                    }
                    Err(e) => {
                        log::warn!("upload of today failed: {}", e);
                        let issue = live::record_failure("Upload failed.", &e);
                        live::set_upload_errors(&day.ids, &issue.message);
                    }
                }
            }
            Ok(p) => live::record(false, p.block.unwrap_or(TodayBlock::NoWalks).sentence()),
            Err(e) => {
                live::record_failure("Upload failed.", &e);
            }
        }
        let app_c = app.clone();
        let _ = super::blocking(move || {
            refresh(&app_c);
            Ok(())
        })
        .await;
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ready() -> StravaToday {
        StravaToday {
            walks: 2,
            distance_km: 3.2,
            on_strava: 1,
            blocked: None,
            note: Some("1 activity from today is already on Strava and stays separate.".into()),
        }
    }

    #[test]
    fn shown_says_why_before_the_first_read_and_without_a_login() {
        assert_eq!(shown(true, None).blocked.as_deref(), Some(NOT_READ));
        let off = shown(false, Some(&ready()));
        assert_eq!(off.blocked.as_deref(), Some(NOT_LINKED));
        assert_eq!(off.note, None);
        assert_eq!(off.walks, 2);
        assert_eq!(shown(true, Some(&ready())), ready());
    }

    #[test]
    fn a_running_walk_holds_the_button_unless_strava_is_not_linked() {
        let mut t = ready();
        hold_while_walking(&mut t);
        assert_eq!(t.blocked.as_deref(), Some(TodayBlock::Running.sentence()));
        assert_eq!(t.note, None);

        let mut t = shown(false, Some(&ready()));
        hold_while_walking(&mut t);
        assert_eq!(t.blocked.as_deref(), Some(NOT_LINKED));
    }

    #[test]
    fn done_message_names_the_walks_joined() {
        let done = upload::Done {
            activity_id: "42".into(),
            distance_m: 4200.0,
            already_there: false,
            update_error: None,
        };
        assert_eq!(
            done_message(&done, 3),
            "Today uploaded as one activity: 3 walks, 4.20 km (activity 42)"
        );
        assert_eq!(
            done_message(&done, 1),
            "Today uploaded as one activity: 1 walk, 4.20 km (activity 42)"
        );
        let there = upload::Done {
            already_there: true,
            ..done
        };
        assert_eq!(
            done_message(&there, 3),
            "Already on Strava (from another app)"
        );
    }
}
