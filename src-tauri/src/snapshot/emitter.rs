//! Collecting the live state into an `AppSnapshot`, and the loop that pushes
//! it to the windows as `app-state`.

use super::{
    build_snapshot, calls, daily_base, hud, lights_out, maintenance, odometer, refresh_daily_base,
    seated, stasis, tray, walkaway, weekly, AppSnapshot, ConnectionSnapshot, SnapshotParts,
    EVENT_APP_STATE,
};
use crate::bluetooth::{self, ORCHESTRATOR};
use crate::programs::{self, SavedRun};
use crate::strava;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter as _, Manager};

/// How often the snapshot is rebuilt and compared with the last one sent.
const TICK: Duration = Duration::from_millis(250);
/// Sent at least this often even when nothing changed.
const HEARTBEAT: Duration = Duration::from_secs(2);
/// The link state goes through btleplug, so it is polled less often.
const CONNECTION_POLL: Duration = Duration::from_secs(1);
/// Catches midnight for the daily total, and keeps the cached totals fresh.
const DAILY_REFRESH: Duration = Duration::from_secs(60);
const MONITOR_REFRESH: Duration = Duration::from_secs(3);

/// Locking rule: `bluetooth::connect_to` takes CONNECTED_DECK
/// and then ORCHESTRATOR. Everything here takes them one at a time and never
/// holds one across an await on the other.
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
    let state = ORCHESTRATOR.lock().await;
    let (link, last_error) = crate::link::current(connected, state.is_connecting);
    let connection = ConnectionSnapshot {
        simulated: connected && crate::bluetooth::is_simulated_id(device_id.as_deref()),
        connected,
        connecting: state.is_connecting,
        device_id,
        has_saved_device: crate::link::has_saved_device(),
        link,
        last_error,
        start_pending: state.start_requested_at.is_some(),
        start_result: crate::link::start_result(),
        connect_dialog_at_ms: crate::link::connect_dialog_at_ms(),
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
    let hud = hud::hud_config();
    let hud_monitors = hud::monitors(&hud);
    let (daily_base_m, live_counts_today) = daily_base();
    let mut snapshot = build_snapshot(
        &state,
        SnapshotParts {
            connection,
            hud,
            hud_monitors,
            daily_base_m,
            live_counts_today,
            max_incline_level: state.max_incline,
            strava: strava::snapshot(),
            maintenance: maintenance::snapshot(live_m, live_s),
            weekly: weekly::snapshot(),
            seated: seated::snapshot(&state.programs),
            calls: calls::snapshot(),
            odometer: odometer::snapshot(live_m, live_s, live_open),
        },
    );
    let now_ms = crate::util::now_ms_u64();
    snapshot.walkaway = walkaway::snapshot(now_ms);
    snapshot.update = crate::update::snapshot(now_ms);
    snapshot.traffic = crate::traffic::snapshot();
    snapshot.lights_out = lights_out::snapshot();
    snapshot.calibration = crate::calibration::snapshot(state.hardware_speed);
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
    last_windows: usize,
    connection: Option<(Instant, bool, Option<String>)>,
    last_daily_refresh: Instant,
    last_active_id: Option<i64>,
    last_monitor_refresh: Option<Instant>,
    last_run_save: Option<(Option<SavedRun>, Instant)>,
    /// Tray tooltip and icon; throttles itself.
    tray: tray::Updater,
}

impl Loop {
    fn new(app: AppHandle) -> Self {
        Self {
            app,
            last_sent: None,
            last_emit: Instant::now(),
            last_windows: 0,
            connection: None,
            last_daily_refresh: Instant::now(),
            last_active_id: None,
            last_monitor_refresh: None,
            last_run_save: None,
            tray: tray::Updater::default(),
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

    /// Keep the running program on disk so a restart can resume it.
    async fn save_program_run(&mut self) {
        let now_ms = crate::util::now_ms();
        let current = ORCHESTRATOR.lock().await.saved_run(now_ms);
        programs::persist_active_run(&self.app, current, &mut self.last_run_save);
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
            // After the refresh, so the card judges the current week even when
            // the session starts just after Monday 00:00.
            if session_started {
                weekly::on_session_start();
            }
        });
    }

    /// A due belt service, the last-week card (shown on a new week's first
    /// walk), the sit-too-long card, the walk-away cards and the update card all show on the
    /// HUD, so the HUD window must exist. It is never destroyed when they go away: an empty HUD is
    /// invisible and click-through.
    fn ensure_hud_for_cards(&self, snapshot: &AppSnapshot) {
        let card = snapshot.maintenance.due
            || snapshot.weekly.show_card
            || snapshot.seated.show_card
            || snapshot.walkaway.card != walkaway::WalkAwayCard::None
            || snapshot.update.hud_card;
        if card && self.app.get_webview_window("hud").is_none() {
            crate::ensure_hud_native(&self.app);
        }
    }

    /// Send the snapshot when it changed, when a window appeared (it has no
    /// state yet), or when the heartbeat is due.
    fn emit(&mut self, snapshot: AppSnapshot) {
        let windows = self.app.webview_windows().len();
        if windows == 0 {
            self.last_windows = 0;
            return;
        }
        let new_window = windows > self.last_windows;
        self.last_windows = windows;

        let changed = self.last_sent.as_ref() != Some(&snapshot);
        if new_window || changed || self.last_emit.elapsed() >= HEARTBEAT {
            if let Err(e) = self.app.emit(EVENT_APP_STATE, &snapshot) {
                log::warn!("emit failed: {}", e);
            }
            self.last_emit = Instant::now();
            self.last_sent = Some(snapshot);
        }
    }

    async fn tick(&mut self) {
        let (connected, device_id) = self.connection().await;
        self.save_program_run().await;
        self.refresh_monitors().await;
        let snapshot = assemble(connected, device_id).await;
        self.refresh_totals(snapshot.session.active_id);
        self.ensure_hud_for_cards(&snapshot);
        self.tray
            .apply(&self.app, &snapshot, crate::util::now_ms_u64());
        self.emit(snapshot);
    }
}

/// Push `app-state` to all windows when the snapshot changes (checked every
/// 250 ms) and at least every 2 s as a heartbeat. Windows that mount after an
/// emit pick the state up with `get_snapshot` or the next heartbeat.
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
