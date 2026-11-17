//! Rust-owned app state for the windows.
//!
//! Windows are pure views: they read one serialisable `AppSnapshot` (through
//! `get_snapshot` or the `app-state` event) and send intent through one typed
//! `Command` (through `dispatch`).
//!
//! - `types`: the snapshot and command types (exported to TypeScript)
//! - `hud`: the HUD config and the display list
//! - `stasis`: whether the HUD panel shows in full, as a strip or not at all
//! - `commands`: what each command does
//! - `emitter`: collecting the snapshot and pushing it to the windows
//!
//! The feature modules that contribute a part of the snapshot or act on it
//! (calls, desk, hud_drag, maintenance, odometer, seated, tray, walkaway,
//! weekly) are declared here rather than in `lib.rs`, so this module owns
//! their wiring.

#[path = "calls.rs"]
pub mod calls;
#[path = "desk.rs"]
pub mod desk;
#[path = "hud_drag.rs"]
pub mod hud_drag;
#[path = "maintenance.rs"]
pub mod maintenance;
#[path = "odometer.rs"]
pub mod odometer;
#[path = "seated.rs"]
pub mod seated;
#[path = "tray.rs"]
pub mod tray;
#[path = "walkaway.rs"]
pub mod walkaway;
#[path = "weekly.rs"]
pub mod weekly;

mod commands;
mod emitter;
mod hud;
mod stasis;
mod types;

pub use calls::CallsSnapshot;
pub use emitter::spawn_emitter;
pub use hud::{hud_config, on_setting_written, refresh_monitors};
pub use maintenance::{MaintenanceBasis, MaintenanceSnapshot};
pub use odometer::OdometerSnapshot;
pub use seated::SeatedSnapshot;
pub use types::*;
pub use walkaway::WalkAwaySnapshot;
pub use weekly::WeeklySummary;

// `hud_drag` saves a dragged position through this.
use hud::mutate as mutate_hud;

use crate::engine::OrchestratorState;
use crate::persistence;
use std::sync::Mutex;
use tauri::AppHandle;

pub const EVENT_APP_STATE: &str = "app-state";

/// Today's saved distance, excluding the live session, and whether the live
/// session started today (so its distance counts); see `refresh_daily_base`.
static DAILY_BASE: Mutex<(f64, bool)> = Mutex::new((0.0, false));

/// Everything `build_snapshot` needs besides the engine state. The cached
/// parts (daily base, maintenance, weekly, odometer) are read before the call.
pub struct SnapshotParts {
    pub connection: ConnectionSnapshot,
    pub hud: HudConfig,
    pub hud_monitors: Vec<HudMonitor>,
    /// Today's saved distance EXCLUDING the active session; the live session
    /// distance is added by `build_snapshot`.
    pub daily_base_m: f64,
    /// The live session started today, so `build_snapshot` adds its distance.
    /// A session belongs to the day it started, as in the saved base.
    pub live_counts_today: bool,
    pub max_incline_level: u8,
    pub strava: StravaSnapshot,
    pub maintenance: MaintenanceSnapshot,
    pub weekly: WeeklySummary,
    pub seated: SeatedSnapshot,
    pub calls: CallsSnapshot,
    pub odometer: OdometerSnapshot,
}

/// Calibrated incline percentage for a hardware level.
fn incline_pct(level: u8, state: &OrchestratorState) -> f32 {
    level as f32 * state.tilt_multiplier + state.incline_offset
}

fn deck_snapshot(state: &OrchestratorState, max_incline_level: u8) -> DeckSnapshot {
    let (target_speed, target_incline, _source) = state.resolve_target();
    // After a program ended on a still belt, the target shown is the speed
    // the next Engage will start it at, not the 0 that holds it still. Only
    // shown: the engine keeps following the manual intent.
    let target_speed = state.engage_preview().map_or(target_speed, |e| e.speed);
    DeckSnapshot {
        status: state.last_status,
        unknown_status_code: state.unknown_status_code,
        standby: state.deck_standby,
        speed_kmh: state.to_physical_speed(state.hardware_speed),
        raw_speed_kmh: state.hardware_speed,
        target_speed_kmh: target_speed,
        incline_level: state.hardware_incline,
        incline_pct: incline_pct(state.hardware_incline, state),
        target_incline_level: target_incline,
        target_incline_pct: incline_pct(target_incline, state),
        min_speed_kmh: state.min_speed,
        max_speed_kmh: state.to_physical_speed(state.max_speed),
        max_incline_level,
        max_incline_pct: incline_pct(max_incline_level, state),
        incline_offset_pct: state.incline_offset,
        incline_pct_per_level: state.tilt_multiplier,
    }
}

fn session_snapshot(
    state: &OrchestratorState,
    daily_base_m: f64,
    live_counts_today: bool,
) -> SessionSnapshot {
    // After a stop the engine keeps the last distance in `session` until the
    // next session starts, but by then it is already in the saved base. Only
    // count the live distance while a session is actually open, and only if
    // it started today (one that started yesterday belongs to yesterday).
    let live_m = if state.session.active_session_id.is_some() && live_counts_today {
        state.session.distance_meters
    } else {
        0.0
    };
    SessionSnapshot {
        active_id: state.session.active_session_id,
        duration_s: state.session.duration_seconds.max(0) as u64,
        distance_m: state.session.distance_meters,
        steps: state.session.steps.max(0.0).round() as u64,
        calories: state.session.calories,
        daily_distance_m: daily_base_m + live_m,
    }
}

fn neural_snapshot(state: &OrchestratorState) -> NeuralSnapshot {
    let (target_speed, _, _) = state.resolve_target();
    let (base_speed, _, _) = state.resolve_base(state.now());
    let active = state.focus_stability_enabled
        && state.neural.active()
        && base_speed > 0.0
        && target_speed < base_speed - 1e-3;
    NeuralSnapshot {
        enabled: state.focus_stability_enabled,
        factor: if active {
            target_speed / base_speed
        } else {
            1.0
        },
        active,
        base_kmh: base_speed,
        target_kmh: if active { target_speed } else { base_speed },
        keys_per_min: state.neural.keys_per_min(),
        floor_pct: state.neural_floor_pct,
        sensitivity: state.neural_sensitivity,
    }
}

/// Build a snapshot from state. Pure (no Tauri, no locks, no I/O) so it can be
/// unit-tested. `walkaway`, `update`, `traffic` and `hud_view` are left at their defaults:
/// they depend on the clock, so the emitter fills them in.
pub fn build_snapshot(state: &OrchestratorState, parts: SnapshotParts) -> AppSnapshot {
    AppSnapshot {
        connection: parts.connection,
        deck: deck_snapshot(state, parts.max_incline_level),
        session: session_snapshot(state, parts.daily_base_m, parts.live_counts_today),
        programs: state.programs.clone(),
        active_program: state.active_program(),
        neural: neural_snapshot(state),
        hud: parts.hud,
        hud_view: HudView::Full,
        hud_monitors: parts.hud_monitors,
        strava: parts.strava,
        maintenance: parts.maintenance,
        weekly: parts.weekly,
        seated: parts.seated,
        calls: parts.calls,
        odometer: parts.odometer,
        walkaway: WalkAwaySnapshot::default(),
        update: crate::update::UpdateSnapshot::default(),
        traffic: crate::traffic::TrafficSnapshot::default(),
    }
}

/// Re-read today's saved distance from SQLite, excluding `exclude` (the live
/// session, whose distance the snapshot adds itself only if it started today).
/// Blocking; call it from a blocking context. Never called per snapshot tick.
pub fn refresh_daily_base(app: &AppHandle, exclude: Option<i64>) {
    let midnight = persistence::local_midnight_ms(chrono::Local::now());
    let v = persistence::daily_distance_m(app, midnight, exclude);
    let live_counts = match exclude {
        Some(id) => started_today(persistence::session_start_ms(app, id), midnight),
        None => false,
    };
    *DAILY_BASE.lock().unwrap_or_else(|e| e.into_inner()) = (v, live_counts);
}

/// A session started at or after local midnight belongs to today, the same
/// rule as the saved base. If its start can't be read, count it.
fn started_today(start_ms: Option<i64>, midnight_ms: i64) -> bool {
    start_ms.is_none_or(|s| s >= midnight_ms)
}

/// (today's saved distance excluding the live session, whether the live
/// session counts toward today).
fn daily_base() -> (f64, bool) {
    *DAILY_BASE.lock().unwrap_or_else(|e| e.into_inner())
}

/// Load the HUD config and the cached totals. Call once after the DB is initialised.
pub fn init(app: &AppHandle) {
    hud::load(app);
    refresh_daily_base(app, None);
    maintenance::init(app);
    weekly::init(app);
    odometer::init(app);
    desk::init(app);
}

#[tauri::command]
pub async fn get_snapshot() -> AppSnapshot {
    let (connected, device_id) = emitter::read_connection().await;
    emitter::assemble(connected, device_id).await
}

#[tauri::command]
pub async fn dispatch(app: AppHandle, command: Command) -> Result<(), String> {
    commands::run(app, command).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::link::LinkState;
    use crate::persistence::AppConfig;
    use crate::programs;
    use crate::protocol::CalibrationPoint;
    use crate::strava_status::StravaActivity;
    use std::time::{Duration, Instant};

    fn conn() -> ConnectionSnapshot {
        ConnectionSnapshot {
            connected: true,
            connecting: false,
            device_id: None,
            has_saved_device: true,
            link: LinkState::Connected,
            last_error: None,
            start_pending: false,
            start_result: None,
            connect_dialog_at_ms: None,
            simulated: false,
        }
    }

    fn strava_snap() -> StravaSnapshot {
        StravaSnapshot {
            has_credentials: false,
            connected: false,
            athlete_name: None,
            auto_sync: false,
            daily_rollup: false,
            shift_end: "18:00".to_string(),
            syncing: false,
            activity: StravaActivity::Idle,
            uploading_session_ids: Vec::new(),
            upload_errors: Default::default(),
            saved_session_ids: Vec::new(),
            last_result: None,
            needs_reconnect: false,
            gear: Vec::new(),
            gear_id: None,
            gear_loading: false,
            mute_mode: MuteMode::All,
            mute_under_minutes: 20,
            title_template: String::new(),
            rollup_title_template: String::new(),
            description_template: String::new(),
        }
    }

    fn maint_snap() -> MaintenanceSnapshot {
        MaintenanceSnapshot {
            last_service_ms: 0,
            distance_since_km: 0.0,
            interval_km: 200.0,
            basis: MaintenanceBasis::Distance,
            hours_since: 0.0,
            interval_hours: 50.0,
            days_since: 0,
            interval_days: 90,
            due: false,
            progress: 0.0,
        }
    }

    fn snap(state: &OrchestratorState, base: f64, max_incline: u8) -> AppSnapshot {
        snap_live(state, base, max_incline, true)
    }

    fn snap_live(
        state: &OrchestratorState,
        base: f64,
        max_incline: u8,
        live_counts_today: bool,
    ) -> AppSnapshot {
        build_snapshot(
            state,
            SnapshotParts {
                connection: conn(),
                hud: HudConfig::default(),
                hud_monitors: Vec::new(),
                daily_base_m: base,
                live_counts_today,
                max_incline_level: max_incline,
                strava: strava_snap(),
                maintenance: maint_snap(),
                weekly: WeeklySummary::default(),
                seated: SeatedSnapshot::default(),
                calls: CallsSnapshot::default(),
                odometer: OdometerSnapshot::default(),
            },
        )
    }

    #[test]
    fn incline_percent_is_level_times_tilt_plus_offset() {
        let state = OrchestratorState {
            tilt_multiplier: 1.5,
            incline_offset: 0.5,
            hardware_incline: 4,
            manual_intent: Some(crate::protocol::ControlIntent {
                speed: 3.0,
                incline: 6,
            }),
            last_manual_interaction: Instant::now(),
            ..OrchestratorState::default()
        };

        let s = snap(&state, 0.0, 15);
        assert_eq!(s.deck.incline_level, 4);
        assert_eq!(s.deck.incline_pct, 6.5);
        assert_eq!(s.deck.target_incline_level, 6);
        assert_eq!(s.deck.target_incline_pct, 9.5);
        assert_eq!(s.deck.max_incline_level, 15);
        assert_eq!(s.deck.max_incline_pct, 23.0);
        assert_eq!(s.deck.incline_offset_pct, 0.5);
        assert_eq!(s.deck.incline_pct_per_level, 1.5);
        assert_eq!(s.deck.target_speed_kmh, 3.0);
    }

    #[test]
    fn speeds_are_calibrated() {
        let mut state = OrchestratorState::default();
        let config = AppConfig {
            calibration_points: vec![
                CalibrationPoint {
                    speed: 1.0,
                    stride_length_cm: 70.0,
                    distance_scale: 2.0,
                },
                CalibrationPoint {
                    speed: 10.0,
                    stride_length_cm: 70.0,
                    distance_scale: 2.0,
                },
            ],
            max_speed: 12.0,
            ..AppConfig::default()
        };
        state.sync_config(&config);
        state.hardware_speed = 6.0;

        let s = snap(&state, 0.0, 15);
        assert_eq!(s.deck.raw_speed_kmh, 6.0);
        assert_eq!(s.deck.speed_kmh, 3.0);
        assert_eq!(s.deck.max_speed_kmh, 6.0);
    }

    #[test]
    fn daily_distance_adds_live_session_only_while_active() {
        let mut state = OrchestratorState::default();
        state.session.distance_meters = 250.5;
        state.session.steps = 10.6;

        // No open session: the leftover distance is already in the saved base.
        let idle = snap(&state, 1000.0, 15);
        assert_eq!(idle.session.daily_distance_m, 1000.0);
        assert_eq!(idle.session.distance_m, 250.5);
        assert_eq!(idle.session.steps, 11);

        state.session.active_session_id = Some(7);
        let live = snap(&state, 1000.0, 15);
        assert_eq!(live.session.active_id, Some(7));
        assert_eq!(live.session.daily_distance_m, 1250.5);
    }

    /// A walk that started before midnight belongs to yesterday: after the
    /// rollover it adds nothing to today's total, so stopping it (when the
    /// base is re-read with nothing excluded) doesn't make the total drop.
    #[test]
    fn daily_distance_skips_a_live_session_that_started_yesterday() {
        let mut state = OrchestratorState::default();
        state.session.distance_meters = 2500.0;
        state.session.active_session_id = Some(7);

        let yesterdays = snap_live(&state, 400.0, 15, false);
        assert_eq!(yesterdays.session.daily_distance_m, 400.0);
        assert_eq!(
            yesterdays.session.distance_m, 2500.0,
            "the walk still shows"
        );

        let todays = snap_live(&state, 400.0, 15, true);
        assert_eq!(todays.session.daily_distance_m, 2900.0);

        // Stopped: the base no longer excludes it and it isn't today's.
        state.session.active_session_id = None;
        assert_eq!(
            snap_live(&state, 400.0, 15, false).session.daily_distance_m,
            400.0
        );
    }

    #[test]
    fn a_session_started_at_midnight_counts_toward_today() {
        let midnight = 1_700_000_000_000;
        assert!(started_today(Some(midnight), midnight));
        assert!(started_today(Some(midnight + 1), midnight));
        assert!(!started_today(Some(midnight - 1), midnight));
        assert!(started_today(None, midnight), "unreadable start counts");
    }

    #[test]
    fn deck_snapshot_exposes_incline_conversion() {
        let state = OrchestratorState {
            tilt_multiplier: 0.5,
            incline_offset: 1.5,
            ..OrchestratorState::default()
        };
        let s = snap(&state, 0.0, 15);
        assert_eq!(s.deck.incline_pct_per_level, 0.5);
        assert_eq!(s.deck.incline_offset_pct, 1.5);
        // The two numbers reproduce the percentages the snapshot reports.
        assert_eq!(
            s.deck.max_incline_pct,
            15.0 * s.deck.incline_pct_per_level + s.deck.incline_offset_pct
        );
    }

    #[test]
    fn snapshot_carries_programs_and_active_program() {
        let mut state = OrchestratorState {
            programs: programs::classic_programs(),
            ..OrchestratorState::default()
        };
        let idle = snap(&state, 0.0, 15);
        assert_eq!(idle.programs.len(), 3);
        assert_eq!(idle.active_program, None);

        state.run_program(2, Instant::now()).unwrap();
        let s = snap(&state, 0.0, 15);
        let a = s.active_program.unwrap();
        assert_eq!((a.program_id, a.cycle, a.step_count), (2, 1, 1));
        assert_eq!(a.seconds_remaining, Some(300));
        assert_eq!(a.name, "Workday");
    }

    /// A program run from a belt walking at 3.5 km/h (level 2), paused, then
    /// ended: the belt stays still and 3.5 waits for the next Engage.
    fn ended_on_a_still_belt() -> OrchestratorState {
        let t0 = Instant::now();
        let mut state = OrchestratorState {
            programs: programs::classic_programs(),
            last_status: crate::protocol::MachineStatus::Running,
            manual_intent: Some(crate::protocol::ControlIntent {
                speed: 3.5,
                incline: 2,
            }),
            ..OrchestratorState::default()
        };
        state.run_program(1, t0).unwrap();
        state.pause_program(t0).unwrap();
        state.last_status = crate::protocol::MachineStatus::Paused;
        assert!(state.end_program(true, t0));
        assert_eq!(state.engage_speed, Some(3.5));
        // Ten minutes on: the manual lockout from the pause and END is over.
        state.set_test_now(t0 + Duration::from_secs(600));
        state
    }

    /// What the engine sends over `secs` seconds of 50 ms ticks from the
    /// state's clock.
    fn engine_sends(state: &mut OrchestratorState, secs: u64) -> Vec<crate::engine::PacketKind> {
        let mut engine = crate::engine::ProtocolEngine::new();
        engine.has_synced_reality = true;
        state.awaiting_first_packet = false;
        let t0 = state.now();
        let mut sent = vec![];
        for i in 0..secs * 20 {
            let now = t0 + Duration::from_millis(50 * i);
            state.set_test_now(now);
            if let Some(p) = engine.determine_next_packet(state, now) {
                state.last_cmd_time = now;
                sent.push(p.kind);
            }
        }
        sent
    }

    fn starts_belt(sent: &[crate::engine::PacketKind]) -> bool {
        use crate::engine::PacketKind;
        sent.iter().any(|k| {
            matches!(
                k,
                PacketKind::HandshakeReady | PacketKind::HandshakeStart | PacketKind::ControlSet
            )
        })
    }

    /// Was `pin_after_end_on_a_still_belt_the_target_reads_zero`.
    #[test]
    fn after_end_on_a_still_belt_the_target_is_the_engage_speed() {
        let mut state = ended_on_a_still_belt();
        assert!(!state.hold_auto_start, "nothing else holds starts back");
        let s = snap(&state, 0.0, 15);
        assert_eq!(s.deck.target_speed_kmh, 3.5);
        assert_eq!(s.deck.target_incline_level, 2);
        // Only shown: the engine holds the belt still.
        assert!(state.resolve_target().0 < 0.05);
        let sent = engine_sends(&mut state, 10);
        assert!(!starts_belt(&sent), "{sent:?}");
        assert_eq!(state.engage_speed, Some(3.5));
        assert_eq!(snap(&state, 0.0, 15).deck.target_speed_kmh, 3.5);
        // Engage starts it at that speed.
        state.trigger_action(crate::engine::DeckAction::Start);
        assert_eq!(state.manual_intent.map(|m| m.speed), Some(3.5));
    }

    #[test]
    fn the_engage_speed_shows_only_on_a_still_belt_with_no_program() {
        let mut state = ended_on_a_still_belt();
        state.last_status = crate::protocol::MachineStatus::Starting;
        assert_eq!(snap(&state, 0.0, 15).deck.target_speed_kmh, 0.0);
        state.last_status = crate::protocol::MachineStatus::Stopped;
        state.min_speed = 4.0;
        assert_eq!(
            snap(&state, 0.0, 15).deck.target_speed_kmh,
            4.0,
            "Engage starts at no less than the minimum"
        );
    }

    #[test]
    fn a_speed_nudge_from_the_engage_speed_is_a_speed_set_by_hand() {
        for (action, want) in [
            (crate::engine::DeckAction::SpeedUp, 3.6),
            (crate::engine::DeckAction::SpeedDown, 3.4),
        ] {
            let mut state = ended_on_a_still_belt();
            state.hold_auto_start = true;
            state.trigger_action(action);
            let m = state.manual_intent.unwrap();
            assert!((m.speed - want).abs() < 0.01, "{action:?}: {m:?}");
            assert_eq!(m.incline, 2);
            assert_eq!(state.engage_speed, None);
            assert!(!state.hold_auto_start);
            assert!((snap(&state, 0.0, 15).deck.target_speed_kmh - want).abs() < 0.01);
            // It starts the belt, as a speed set by hand on a still belt does.
            let sent = engine_sends(&mut state, 3);
            assert!(
                sent.contains(&crate::engine::PacketKind::HandshakeStart),
                "{action:?}: {sent:?}"
            );
        }
    }

    #[test]
    fn a_tilt_nudge_from_the_engage_speed_does_not_start_the_belt() {
        let mut state = ended_on_a_still_belt();
        state.trigger_action(crate::engine::DeckAction::InclineUp);
        assert_eq!(state.manual_intent.map(|m| m.speed), Some(0.0));
        let sent = engine_sends(&mut state, 10);
        assert!(!starts_belt(&sent), "{sent:?}");
        let s = snap(&state, 0.0, 15);
        assert_eq!(s.deck.target_speed_kmh, 3.5);
        // The tilt shown is the one Engage will use.
        assert_eq!(s.deck.target_incline_level, 3);
        assert_eq!(state.engage_speed, Some(3.5));
    }

    /// A tilt change after END on a still belt does what it does on any still
    /// belt: the same packets (none that start it) as one with no program
    /// behind it, by nudge or by slider.
    #[test]
    fn a_tilt_change_after_end_matches_one_on_a_plain_still_belt() {
        use crate::protocol::{ControlIntent, MachineStatus};
        for status in [MachineStatus::Paused, MachineStatus::Stopped] {
            for by_slider in [false, true] {
                let change = |state: &mut OrchestratorState| {
                    if by_slider {
                        state.set_intent(None, Some(5), true);
                    } else {
                        state.trigger_action(crate::engine::DeckAction::InclineUp);
                    }
                };
                let mut ended = ended_on_a_still_belt();
                ended.last_status = status;
                change(&mut ended);
                let mut plain = OrchestratorState {
                    last_status: status,
                    manual_intent: Some(ControlIntent {
                        speed: 0.0,
                        incline: 2,
                    }),
                    ..OrchestratorState::default()
                };
                change(&mut plain);
                assert_eq!(ended.manual_intent, plain.manual_intent);
                let sent = engine_sends(&mut ended, 10);
                assert!(!starts_belt(&sent), "{status:?} {by_slider}: {sent:?}");
                assert_eq!(sent, engine_sends(&mut plain, 10), "{status:?} {by_slider}");
                let want = if by_slider { 5 } else { 3 };
                let s = snap(&ended, 0.0, 15);
                assert_eq!(s.deck.target_incline_level, want);
                assert_eq!(s.deck.target_speed_kmh, 3.5);
            }
        }
    }

    /// From `state` on a still belt, press Engage and let the deck answer:
    /// the first control packet the engine sends once the belt runs.
    fn first_control_after_engage(state: &mut OrchestratorState) -> crate::engine::Packet {
        use crate::engine::PacketKind;
        let mut engine = crate::engine::ProtocolEngine::new();
        engine.has_synced_reality = true;
        state.awaiting_first_packet = false;
        state.trigger_action(crate::engine::DeckAction::Start);
        let t0 = state.now();
        for i in 0..200 {
            let now = t0 + Duration::from_millis(50 * i);
            state.set_test_now(now);
            let Some(p) = engine.determine_next_packet(state, now) else {
                continue;
            };
            state.last_cmd_time = now;
            match p.kind {
                PacketKind::HandshakeStart => {
                    state.last_status = crate::protocol::MachineStatus::Running;
                }
                PacketKind::ControlSet => return p,
                _ => {}
            }
        }
        panic!("no control packet after Engage");
    }

    fn control_bytes(state: &OrchestratorState, speed: f32, incline: u8) -> Vec<u8> {
        let (s, i) = crate::engine::calculate_hardware_bytes(
            speed,
            incline,
            &state.calibration_points,
            state.max_speed,
        );
        assert_eq!(i, incline);
        crate::protocol::control_packet(crate::protocol::Control::Target, s, i)
    }

    #[test]
    fn engage_after_a_tilt_nudge_starts_at_the_tilt_shown() {
        let mut state = ended_on_a_still_belt();
        state.trigger_action(crate::engine::DeckAction::InclineUp);
        let shown = snap(&state, 0.0, 15).deck;
        assert_eq!(
            (shown.target_speed_kmh, shown.target_incline_level),
            (3.5, 3)
        );
        let p = first_control_after_engage(&mut state);
        assert_eq!(p.bytes, control_bytes(&state, 3.5, 3));
        assert_eq!(state.engage_speed, None, "used once");
    }

    #[test]
    fn engage_with_no_nudge_starts_at_the_speed_and_tilt_from_before() {
        let mut state = ended_on_a_still_belt();
        let p = first_control_after_engage(&mut state);
        assert_eq!(p.bytes, control_bytes(&state, 3.5, 2));
    }

    #[test]
    fn a_speed_nudge_after_a_tilt_nudge_starts_at_the_tilt_shown() {
        let mut state = ended_on_a_still_belt();
        state.trigger_action(crate::engine::DeckAction::InclineUp);
        state.trigger_action(crate::engine::DeckAction::SpeedUp);
        let m = state.manual_intent.unwrap();
        assert!((m.speed - 3.6).abs() < 0.01, "{m:?}");
        assert_eq!(m.incline, 3);
    }

    #[test]
    fn snapshot_passes_the_parts_through() {
        let state = OrchestratorState {
            focus_stability_enabled: true,
            ..OrchestratorState::default()
        };
        let s = snap(&state, 0.0, 15);
        assert_eq!(s.connection, conn());
        assert_eq!(s.strava, strava_snap());
        assert_eq!(s.maintenance, maint_snap());
        assert!(s.neural.enabled);
        assert!(!s.neural.active);
        assert_eq!(s.neural.factor, 1.0);
        assert_eq!(s.neural.target_kmh, s.neural.base_kmh);
        assert_eq!(s.walkaway, WalkAwaySnapshot::default());
    }
}
