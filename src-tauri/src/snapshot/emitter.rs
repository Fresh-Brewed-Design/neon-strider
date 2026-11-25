//! Collecting the live state into an `AppSnapshot`, and the loop that pushes
//! it to the windows as `app-state`.

use super::frame::{self, Delivery, Due, Frame, Windows};
use super::{
    build_snapshot, calls, daily_base, desk_sync, eyebreak, hud, lights_out, maintenance, odometer,
    pc_walk_share, program_tones, refresh_daily_base, seated, stasis, tray, tray_menu, walkaway,
    weekly, AppSnapshot, ConnectionSnapshot, SnapshotParts, EVENT_APP_STATE,
};
use crate::bluetooth::{self, ORCHESTRATOR};
use crate::programs::{self, SavedRun};
use crate::strava;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter as _, EventTarget, Manager};

/// How often the snapshot is rebuilt and compared with the last one sent.
/// A quarter second is quicker than the eye notices a lag after a press, and
/// under half the 600 ms poll, so a new frame shows almost at once.
const TICK: Duration = Duration::from_millis(250);
/// Sent at least this often even when nothing changed.
/// A window that missed an event catches up within 2 s; countdowns in the
/// snapshot are whole seconds, which the windows tick on their own between beats.
const HEARTBEAT: Duration = Duration::from_secs(2);
/// Every shown window gets the whole snapshot at least this often, cold
/// parts included. Only a window that missed a frame needs it (one whose
/// listener came up just after its first fetch), so it catches up within
/// 10 s; a full frame (20-40 KB) that often costs next to nothing.
const FULL_REFRESH: Duration = Duration::from_secs(10);
/// The link state goes through btleplug, so it is polled less often.
/// Link changes are shown within a second, which is quick enough.
const CONNECTION_POLL: Duration = Duration::from_secs(1);
/// Catches midnight for the daily total, and keeps the cached totals fresh.
/// The daily total is shown to 0.01 km, which takes longer than a minute at a walk.
const DAILY_REFRESH: Duration = Duration::from_secs(60);
/// A plugged-in display shows up in the HUD settings within a few seconds;
/// listing monitors is a system call, so not every tick.
const MONITOR_REFRESH: Duration = Duration::from_secs(3);

/// Locking rule: CONNECTED_DECK before ORCHESTRATOR, and neither held across
/// BLE I/O (`bluetooth::drop_old_link` takes the old link out before closing
/// it). Everything here takes them one at a time and never holds one across
/// an await on the other.
pub(super) async fn read_connection() -> (bool, Option<String>) {
    let connected = bluetooth::is_bluetooth_connected().await;
    let device_id = if connected {
        let guard = bluetooth::CONNECTED_DECK.lock().await;
        guard.as_ref().map(|d| d.id())
    } else {
        None
    };
    (connected, device_id)
}

pub(super) async fn assemble(connected: bool, device_id: Option<String>) -> AppSnapshot {
    // The parts that read only their own module's state, gathered before the
    // belt lock (#274): the larger ones (programs aside) are cloned here, so the
    // worker and the status listener never wait on them.
    let hud = hud::hud_config();
    let hud_monitors = hud::monitors(&hud);
    let daily = daily_base();
    let strava = strava::snapshot();
    let weekly = weekly::snapshot();
    let eye_break = eyebreak::snapshot();
    let program_tones = program_tones::snapshot();
    let calls = calls::snapshot();
    let now_ms = crate::util::now_ms_u64();
    let wall_clock = desk_sync::wall_clock();
    let pc_walk_share = pc_walk_share::snapshot();
    let speed_drift_config = crate::speed_drift::config();
    let today_timeline = pc_walk_share::today_timeline();
    let walkaway = walkaway::snapshot(now_ms);
    let update = crate::update::snapshot(now_ms);
    let traffic = crate::traffic::snapshot();
    let lights_out = lights_out::snapshot();
    let ghost_pod = crate::ghost_pod::snapshot();
    let typing_speed = crate::typing_speed::snapshot();
    let mut heart_rate = crate::heart_rate::snapshot();
    let guided_task = crate::guided_task::snapshot();
    let has_saved_device = crate::link::has_saved_device();
    let start_result = crate::link::start_result();
    let connect_dialog_at_ms = crate::link::connect_dialog_at_ms();

    let (mut snapshot, deck_silent) = {
        let state = ORCHESTRATOR.lock().await;
        let (link, last_error) = crate::link::current(connected, state.is_connecting);
        let connection = ConnectionSnapshot {
            simulated: connected && crate::bluetooth::is_simulated_id(device_id.as_deref()),
            connected,
            connecting: state.is_connecting,
            device_id,
            has_saved_device,
            link,
            last_error,
            start_pending: state.start_requested_at.is_some(),
            start_result,
            connect_dialog_at_ms,
        };
        // A stopped session's leftover numbers are already in the saved totals.
        let live_open = state.session.active_session_id.is_some();
        let (live_m, live_s) = if live_open {
            (
                state.session.distance_meters,
                state.session.duration_seconds.max(0),
            )
        } else {
            (0.0, 0)
        };
        let deck_silent = state
            .last_frame_at
            .is_some_and(|t| t.elapsed() >= stasis::SILENCE);
        let mut snapshot = build_snapshot(
            &state,
            SnapshotParts {
                connection,
                hud,
                hud_monitors,
                daily,
                max_incline_level: state.max_incline,
                strava,
                maintenance: maintenance::snapshot(live_m, live_s),
                weekly,
                seated: seated::snapshot(&state.programs),
                eye_break,
                calls,
                odometer: odometer::snapshot(live_m, live_s, live_open),
            },
        );
        snapshot.speed_drift = state.speed_drift_snapshot(&speed_drift_config, now_ms);
        snapshot.calibration = crate::calibration::snapshot(&state);
        // The zone shows on PULSE_RATE only with a sensor picked: without one
        // there is no reading to place in it and the zone trims nothing.
        heart_rate.zone = match heart_rate.sensor {
            Some(_) => state.zone_band(heart_rate.bpm),
            None => None,
        };
        // A cool-down's pulse target the same way: with no sensor the step ends
        // on its minutes, and there is nothing to show against it.
        heart_rate.cool_down = match heart_rate.sensor {
            Some(_) => state.pulse_target(heart_rate.bpm),
            None => None,
        };
        (snapshot, deck_silent)
    };
    snapshot.desk_sync = desk_sync::snapshot(snapshot.session.daily_distance_m);
    snapshot.wall_clock = wall_clock;
    snapshot.pc_walk_share = pc_walk_share;
    snapshot.today_timeline = today_timeline;
    snapshot.walkaway = walkaway;
    snapshot.update = update;
    snapshot.traffic = traffic;
    snapshot.lights_out = lights_out;
    snapshot.program_tones = program_tones;
    snapshot.presenting = super::presenting::snapshot();
    // After every card is decided, before the HUD view and the card check.
    super::presenting::hold_cards(&mut snapshot);
    // The first-connect offer only shows while the treadmill is connected.
    snapshot.calibration.offer &= snapshot.connection.connected;
    snapshot.ghost_pod = ghost_pod;
    snapshot.typing_speed = typing_speed;
    snapshot.heart_rate = heart_rate;
    snapshot.guided_task = guided_task;
    snapshot.platform.permissions = crate::platform::permissions();
    snapshot.shortcuts = crate::shortcuts::snapshot();
    // Recomputed every tick, so the end of the grace after a stop reaches the
    // HUD with the next on-change emit (within a tick).
    snapshot.hud_view = stasis::view(&snapshot, deck_silent, Instant::now());
    snapshot
}

/// What the emitter loop remembers between ticks.
struct Loop {
    app: AppHandle,
    last_sent: Option<AppSnapshot>,
    last_emit: Instant,
    last_full: Instant,
    /// Which windows hold the frames sent so far.
    windows: Windows,
    connection: Option<(Instant, bool, Option<String>)>,
    last_daily_refresh: Instant,
    last_active_id: Option<i64>,
    last_monitor_refresh: Option<Instant>,
    last_run_save: Option<(Option<SavedRun>, Instant)>,
    /// Tray tooltip and icon; throttles itself.
    tray: tray::Updater,
    /// Tray menu; throttles itself.
    tray_menu: tray_menu::Updater,
    /// A HUD card showed on the last tick.
    card_up: bool,
}

impl Loop {
    fn new(app: AppHandle) -> Self {
        Self {
            app,
            last_sent: None,
            last_emit: Instant::now(),
            last_full: Instant::now(),
            windows: Windows::default(),
            connection: None,
            last_daily_refresh: Instant::now(),
            last_active_id: None,
            last_monitor_refresh: None,
            last_run_save: None,
            tray: tray::Updater::default(),
            tray_menu: tray_menu::Updater::default(),
            card_up: false,
        }
    }

    /// The link state, re-read at most once per `CONNECTION_POLL`.
    async fn connection(&mut self) -> (bool, Option<String>) {
        let stale = self
            .connection
            .as_ref()
            .is_none_or(|(t, _, _)| t.elapsed() >= CONNECTION_POLL);
        if stale {
            let (connected, device_id) = read_connection().await;
            self.connection = Some((Instant::now(), connected, device_id));
        }
        self.connection
            .as_ref()
            .map(|(_, c, d)| (*c, d.clone()))
            .unwrap_or((false, None))
    }

    /// Keep the running program on disk so a restart can resume it. The
    /// write runs off the async threads (#273), and only when one is due.
    async fn save_program_run(&mut self) {
        let now_ms = crate::util::now_ms();
        let current = ORCHESTRATOR.lock().await.saved_run(now_ms);
        if !programs::active_run_save_due(self.last_run_save.as_ref(), current.as_ref()) {
            return;
        }
        let app = self.app.clone();
        let mut last = self.last_run_save.take();
        let saved = crate::util::blocking(move || {
            programs::persist_active_run(&app, current, &mut last);
            Ok(last)
        })
        .await;
        // A failed hand-off leaves no record, so the next tick saves again.
        self.last_run_save = saved.unwrap_or(None);
    }

    /// Write DECK_PROFILE when the deck taught it something (or it was
    /// forgotten), off the async threads.
    async fn save_deck_profile(&mut self) {
        let (key, json) = {
            let mut state = ORCHESTRATOR.lock().await;
            let Some(key) = state.deck_profile_key.clone() else {
                return;
            };
            let Some(json) = state.deck_profile.take_unsaved() else {
                return;
            };
            (key, json)
        };
        let app = self.app.clone();
        let _detached = tauri::async_runtime::spawn_blocking(move || {
            let result = match json {
                Some(json) => crate::persistence::write_setting(&app, &key, &json),
                None => crate::persistence::delete_setting(&app, &key),
            };
            if let Err(e) = result {
                log::warn!("could not save the deck profile: {}", e);
            }
        });
    }

    /// Displays change rarely; look every few seconds, off the async threads
    /// because the query goes through the main thread.
    async fn refresh_monitors(&mut self) {
        if self
            .last_monitor_refresh
            .is_some_and(|t| t.elapsed() < MONITOR_REFRESH)
        {
            return;
        }
        self.last_monitor_refresh = Some(Instant::now());
        let app = self.app.clone();
        let _ = tauri::async_runtime::spawn_blocking(move || hud::refresh_monitors(&app)).await;
    }

    /// Re-read the cached totals from the database when a session starts or
    /// ends (start, resume-on-connect, stop) and once a minute (midnight
    /// rollover). Runs off-thread; the snapshot does not wait for it.
    fn refresh_totals(&mut self, active_id: Option<i64>) {
        let session_changed = active_id != self.last_active_id;
        if !session_changed && self.last_daily_refresh.elapsed() < DAILY_REFRESH {
            return;
        }
        let session_started = session_changed && active_id.is_some();
        self.last_active_id = active_id;
        self.last_daily_refresh = Instant::now();
        let app = self.app.clone();
        let _detached = tauri::async_runtime::spawn_blocking(move || {
            refresh_daily_base(&app, active_id);
            maintenance::refresh(&app, active_id);
            odometer::refresh(&app, active_id);
            // The weekly summary changes when a session starts or ends, and
            // when the local week rolls over (a cheap check, no query).
            if session_changed || weekly::rolled_over() {
                weekly::refresh(&app, active_id);
            }
            if session_changed {
                desk_sync::refresh(&app, active_id);
                crate::typing_speed::refresh(&app);
            }
            // After the refresh, so the card judges the current week even when
            // the session starts just after Monday 00:00.
            if session_started {
                weekly::on_session_start();
            }
        });
    }

    /// A due belt service, the last-week card (shown on a new week's first
    /// walk), the sit-too-long card, the eye-break cards, the walk-away cards and the update card all show on the
    /// HUD, so the HUD window must exist. It is never destroyed when they go away: an empty HUD is
    /// invisible and click-through.
    /// A HUD hidden from the tray menu comes back when a card appears.
    fn ensure_hud_for_cards(&mut self, snapshot: &AppSnapshot) {
        let card = snapshot.maintenance.hud_card
            || snapshot.weekly.show_card
            || snapshot.seated.show_card
            || snapshot.eye_break.phase != eyebreak::EyeBreakPhase::Idle
            || snapshot.walkaway.card != walkaway::WalkAwayCard::None
            || snapshot.update.hud_card;
        if card && self.app.get_webview_window("hud").is_none() {
            crate::ensure_hud_native(&self.app);
        } else if card && !self.card_up && !crate::hud::shown(&self.app) {
            crate::hud::set_shown(&self.app, true);
        }
        self.card_up = card;
    }

    /// Each window and whether it shows. Asked off the async threads: on
    /// Windows the answer comes from the main thread.
    async fn shown_windows(&self) -> Vec<(String, bool)> {
        let windows: Vec<_> = self.app.webview_windows().into_iter().collect();
        if windows.is_empty() {
            return Vec::new();
        }
        tauri::async_runtime::spawn_blocking(move || {
            windows
                .into_iter()
                .map(|(label, w)| {
                    // Unknown counts as shown: a frame too many beats a window left stale.
                    let shown = w.is_visible().unwrap_or(true);
                    (label, shown)
                })
                .collect()
        })
        .await
        .unwrap_or_default()
    }

    /// Send each shown window what it lacks (#274): the hot parts when the
    /// snapshot changed or the heartbeat is due, everything when a cold part
    /// changed, the window just showed or appeared, or the full refresh is
    /// due. Hidden windows get nothing.
    fn emit(&mut self, snapshot: AppSnapshot, windows: &[(String, bool)]) {
        let changed = self.last_sent.as_ref() != Some(&snapshot);
        let due = Due {
            changed,
            cold_changed: changed
                && self
                    .last_sent
                    .as_ref()
                    .is_none_or(|last| !frame::cold_equal(last, &snapshot)),
            heartbeat: self.last_emit.elapsed() >= HEARTBEAT,
            refresh: self.last_full.elapsed() >= FULL_REFRESH,
        };
        let plan = self.windows.plan(windows, due);
        let mut sent = false;
        for (label, send) in plan {
            let full = match send {
                Delivery::Nothing => continue,
                Delivery::Hot => false,
                Delivery::Full => true,
            };
            let frame = Frame {
                snapshot: &snapshot,
                full,
            };
            let target = EventTarget::webview_window(label.as_str());
            if let Err(e) = self.app.emit_to(target, EVENT_APP_STATE, frame) {
                log::warn!("emit to {} failed: {}", label, e);
                self.windows.lost(&label);
            }
            sent = true;
        }
        if sent {
            self.last_emit = Instant::now();
        }
        if due.refresh {
            self.last_full = Instant::now();
        }
        // Every window still in sync now holds this snapshot: it was sent
        // what changed, or nothing changed.
        self.last_sent = Some(snapshot);
    }

    async fn tick(&mut self) {
        let (connected, device_id) = self.connection().await;
        self.save_program_run().await;
        self.save_deck_profile().await;
        self.refresh_monitors().await;
        // The guided-task safety timeout ends here, within a tick of its time.
        if crate::guided_task::expire() {
            crate::dashboard::on_guided_pin_changed(&self.app);
        }
        let snapshot = assemble(connected, device_id).await;
        self.refresh_totals(snapshot.session.active_id);
        self.ensure_hud_for_cards(&snapshot);
        let now_ms = crate::util::now_ms_u64();
        self.tray.apply(&self.app, &snapshot, now_ms);
        self.tray_menu.apply(&self.app, &snapshot, now_ms);
        let windows = self.shown_windows().await;
        self.emit(snapshot, &windows);
    }
}

/// Push `app-state` to the shown windows when the snapshot changes (checked
/// every 250 ms) and at least every 2 s as a heartbeat, the cold parts only
/// when they change (`frame.rs`). Windows that mount after an emit pick the
/// state up with `get_snapshot` or the full frame sent when they show.
pub fn spawn_emitter(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let mut ticker = tokio::time::interval(TICK);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        let mut state = Loop::new(app);
        loop {
            ticker.tick().await;
            state.tick().await;
        }
    });
}
