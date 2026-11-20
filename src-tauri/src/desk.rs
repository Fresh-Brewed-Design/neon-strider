//! Desk features at runtime: the sit-too-long nudge, slow-down-on-calls,
//! walk-away safety and lights out.
//!
//! One loop, once a second. It reads the belt and the treadmill link under one
//! short lock of the orchestrator, runs the pure state machines (`seated.rs`,
//! `calls.rs`, `walkaway.rs`, `lights_out.rs`) and applies what they decide: start a remembered
//! program, ease the belt to the quiet speed and back, or pause it when the user
//! walks away. The microphone is polled on its own thread (`calls::mic`), the
//! session lock is watched on another (`platform::spawn_session_lock_watcher`).
//!
//! This file is declared from `snapshot.rs` (`#[path]`), not `lib.rs`.

use crate::bluetooth::{self, ORCHESTRATOR};
use crate::engine::{DeckAction, OrchestratorState};
use crate::programs::{self, ProgramPhase};
use crate::protocol::MachineStatus;
use crate::snapshot::{calls, eyebreak, lights_out, seated, walkaway};
use crate::util::{blocking, now_ms_u64 as now_ms};
use std::time::Duration;
use tauri::{AppHandle, Manager};

const TICK: Duration = Duration::from_secs(1);
/// How long a freshly created HUD window gets to load before a toast is sent to it.
const HUD_LOAD_GRACE: Duration = Duration::from_millis(1500);

const TOAST_QUIET: &str = "CALL DETECTED // QUIET MODE";
const TOAST_RESTORED: &str = "CALL ENDED // RESTORED";

/// The treadmill is connected and has reported at least once, so a program can
/// be started on it now.
fn treadmill_ready(connected: bool, state: &OrchestratorState) -> bool {
    connected && !state.awaiting_first_packet && !state.is_connecting
}

fn belt_running(state: &OrchestratorState) -> bool {
    state.last_status == MachineStatus::Running
}

/// The belt is walking and walk-away may pause it: running, no stop in flight,
/// and either manual or a program's walking phase that is not already paused.
fn belt_walking(state: &OrchestratorState) -> bool {
    belt_running(state)
        && !state.stop_requested
        && state
            .program
            .as_ref()
            .is_none_or(|r| r.phase == ProgramPhase::Walking && !r.paused)
}

/// Pause the belt for walk-away: the program's pause while one is walking (so
/// it resumes at the same step), otherwise the treadmill's own pause. Never a
/// stop, so the session carries on.
fn walkaway_pause(state: &mut OrchestratorState, reason: walkaway::WalkAwayReason) {
    let why = match reason {
        walkaway::WalkAwayReason::Lock => "the PC was locked",
        walkaway::WalkAwayReason::Idle => "no keyboard or mouse input",
    };
    if state.program_walking() {
        log::info!("Walk-away: pausing the program ({})", why);
        if let Err(e) = state.pause_program(std::time::Instant::now()) {
            log::warn!("Walk-away: could not pause the program: {}", e);
        }
    } else {
        log::info!("Walk-away: pausing the belt ({})", why);
        state.trigger_action(DeckAction::Pause);
    }
}

/// The target speed the call logic reads and writes: the running program's live
/// step target, or the manual one (falling back to what the deck reports).
fn belt_view(state: &OrchestratorState) -> calls::Belt {
    let program = state.program_walking();
    let target_kmh = if program {
        state.active_program().map_or(0.0, |a| a.speed_kmh)
    } else {
        state
            .manual_intent
            .map(|i| i.speed)
            .unwrap_or_else(|| state.to_physical_speed(state.hardware_speed))
    };
    calls::Belt {
        running: belt_running(state),
        program,
        target_kmh,
        min_kmh: state.min_physical_speed(),
    }
}

/// Set the target speed the same way a manual speed edit does: into the live
/// program step while one is walking (never the saved program), otherwise the
/// manual intent with its incline kept. Not a speed set by hand, so Neural flow
/// keeps its slowdown (and defers to quiet mode while it lasts).
fn apply_speed(state: &mut OrchestratorState, kmh: f32) {
    let incline = state
        .manual_intent
        .map(|i| i.incline)
        .unwrap_or(state.hardware_incline);
    state.set_intent(Some(kmh), Some(incline), false);
}

/// What one pass decided, to be done once the orchestrator lock is released.
#[derive(Default)]
struct Outcome {
    started_program: Option<u32>,
    /// The toast's type and message.
    toast: Option<(&'static str, &'static str)>,
}

async fn step(
    connected: bool,
    now: u64,
    idle_ms: Option<u64>,
    machine: &mut calls::CallMachine,
) -> Outcome {
    let mut out = Outcome::default();
    let mut guard = ORCHESTRATOR.lock().await;
    let state: &mut OrchestratorState = &mut guard;

    // Sit-too-long nudge.
    let tick = seated::Tick {
        idle_ms,
        belt_running: belt_running(state),
        ready: treadmill_ready(connected, state),
        program_waiting: state.program.as_ref().is_some_and(|r| r.wait.is_some()),
    };
    let effect = seated::with(|rt| rt.machine.tick(&rt.cfg, now, &tick));
    if let Some(seated::Effect::StartProgram(id)) = effect {
        out.started_program = start_program(state, id);
    }

    // Calls.
    let cfg = calls::config();
    let input = calls::Input {
        mic_in_use: calls::mic::in_use_now(),
        belt: belt_view(state),
    };
    match machine.tick(&cfg, now, &input) {
        Some(calls::Action::Quiet { to_kmh }) => {
            log::info!(
                "Call detected: easing the belt from {:.1} to {:.1} km/h",
                input.belt.target_kmh,
                to_kmh
            );
            apply_speed(state, to_kmh);
            out.toast = Some(("CALL", TOAST_QUIET));
        }
        Some(calls::Action::Restore { to_kmh }) => {
            log::info!("Call ended: restoring {:.1} km/h", to_kmh);
            apply_speed(state, to_kmh);
            out.toast = Some(("CALL", TOAST_RESTORED));
        }
        None => {}
    }
    calls::set_quiet_active(machine.quiet_active());

    // Walk-away. After the call logic, which never restarts a paused belt.
    let input = walkaway::Input {
        connected: treadmill_ready(connected, state),
        walking: belt_walking(state),
        locked: crate::platform::session_locked(),
        idle_ms,
        call_active: input.mic_in_use,
    };
    let effect = walkaway::with(|rt| rt.machine.tick(&rt.cfg, now, &input));
    if let Some(walkaway::Effect::Pause(reason)) = effect {
        walkaway_pause(state, reason);
    }

    // Lights out. Last: it only ever stops.
    let input = lights_out::Input {
        connected: treadmill_ready(connected, state),
        display_off: crate::platform::display_off(),
        belt: lights_out::belt(state),
    };
    if let Some(effect) = lights_out::with(|rt| rt.machine.tick(rt.enabled, &input)) {
        lights_out::apply(state, effect);
        if effect == lights_out::Effect::Stop {
            // A walk-away card on the stopped belt has nothing left to resume.
            walkaway::with(|rt| rt.machine.acknowledge(now));
        }
        out.toast = Some((lights_out::TOAST_KIND, lights_out::toast(effect)));
    }
    out
}

/// Start a program on the orchestrator. Returns the id if it started.
fn start_program(state: &mut OrchestratorState, id: u32) -> Option<u32> {
    match state.run_program(id, std::time::Instant::now()) {
        Ok(()) => {
            log::info!("Nudge: started program {}", id);
            Some(id)
        }
        Err(e) => {
            log::warn!("Nudge: could not start program {}: {}", id, e);
            None
        }
    }
}

/// Send a toast to the HUD, creating the window first if there is none.
fn announce(app: &AppHandle, kind: &'static str, message: &'static str) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        if app.get_webview_window(crate::hud::HUD_LABEL).is_none() {
            crate::hud::ensure_hud_native(&app);
            // A page that is still loading has no listener yet.
            tokio::time::sleep(HUD_LOAD_GRACE).await;
        }
        crate::hud::toast(
            &app,
            serde_json::json!({ "type": kind, "message": message }),
        );
    });
}

/// Load the settings. Blocking; call once after the DB is initialised.
pub fn init(app: &AppHandle) {
    seated::init(app);
    eyebreak::init(app);
    calls::init(app);
    walkaway::init(app);
    lights_out::init(app);
}

/// Start the microphone poller and the one-second loop.
pub fn spawn(app: AppHandle) {
    calls::mic::spawn_poller();
    crate::platform::spawn_session_lock_watcher();
    tauri::async_runtime::spawn(async move {
        let mut ticker = tokio::time::interval(TICK);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        let mut machine = calls::CallMachine::default();
        loop {
            ticker.tick().await;
            // Asked before the orchestrator lock is taken (see snapshot.rs's locking rule).
            let connected = bluetooth::is_bluetooth_connected().await;
            let now = now_ms();
            let idle_ms = crate::hooks::input_idle_ms(now);
            let eye = eyebreak::Tick {
                idle_ms,
                locked: crate::platform::session_locked(),
            };
            eyebreak::with(|rt| rt.machine.tick(&rt.cfg, now, &eye));
            let out = step(connected, now, idle_ms, &mut machine).await;
            if let Some(id) = out.started_program {
                let app_c = app.clone();
                let saved = blocking(move || programs::persist_last_run(&app_c, Some(id))).await;
                if let Err(e) = saved {
                    log::warn!("Could not remember the last program: {}", e);
                }
            }
            if let Some((kind, message)) = out.toast {
                announce(&app, kind, message);
            }
        }
    });
}

// ---------------------------------------------------------------------------
// Commands from the HUD card
// ---------------------------------------------------------------------------

/// STAY on the walk-away warning card.
pub fn walkaway_stay() {
    walkaway::with(|rt| rt.machine.stay(now_ms()));
}

/// RESUME on the walk-away paused card: resume the program, or start the belt
/// again at the speed it had (the treadmill's pause keeps the manual target).
pub async fn walkaway_resume() -> Result<(), String> {
    let connected = bluetooth::is_bluetooth_connected().await;
    let mut state = ORCHESTRATOR.lock().await;
    if !treadmill_ready(connected, &state) {
        return Err("The treadmill is offline".to_string());
    }
    walkaway::with(|rt| rt.machine.acknowledge(now_ms()));
    let program_paused = state
        .program
        .as_ref()
        .is_some_and(|r| r.phase == ProgramPhase::Walking && r.paused);
    if program_paused {
        log::info!("Walk-away: resuming the program");
        state.resume_program(std::time::Instant::now())
    } else {
        log::info!("Walk-away: resuming the belt");
        state.trigger_action(DeckAction::Start);
        Ok(())
    }
}

/// END on the walk-away paused card: stop, which ends the session (and any
/// program).
pub async fn walkaway_end() -> Result<(), String> {
    walkaway::with(|rt| rt.machine.acknowledge(now_ms()));
    log::info!("Walk-away: ending the session");
    ORCHESTRATOR.lock().await.trigger_action(DeckAction::Stop);
    Ok(())
}

/// SNOOZE 15 MIN.
pub fn snooze() {
    seated::with(|rt| rt.machine.snooze(now_ms()));
}

/// Shown under the card's buttons for ENGAGE without a usable link.
const ENGAGE_OFFLINE: &str = "The treadmill is offline";

/// ENGAGE on the card, the press itself: Engage's own start on a still belt
/// (`DeckAction::Start`, which is also GO on a paused program). Nothing is
/// remembered for later: without a link it fails at once. A deck in standby
/// refuses the start in the engine, which says why (`start_result`, shown
/// under the card's buttons), and the card stays up.
fn engage(
    state: &mut OrchestratorState,
    ready: bool,
    machine: &mut seated::Seated,
    now: u64,
) -> Result<(), String> {
    if !ready {
        return Err(ENGAGE_OFFLINE.to_string());
    }
    let refused = state.start_refusal().is_some();
    state.trigger_action(DeckAction::Start);
    if !refused {
        log::info!("Nudge: Engage from the card");
        machine.engaged(now);
    }
    Ok(())
}

/// ENGAGE on the card.
pub async fn press_engage() -> Result<(), String> {
    let connected = bluetooth::is_bluetooth_connected().await;
    let mut state = ORCHESTRATOR.lock().await;
    let ready = treadmill_ready(connected, &state);
    seated::with(|rt| engage(&mut state, ready, &mut rt.machine, now_ms()))
}

/// A program button on the card: start it now when the treadmill is connected,
/// otherwise remember it for when it connects.
pub async fn press_program(app: &AppHandle, id: u32) -> Result<(), String> {
    let connected = bluetooth::is_bluetooth_connected().await;
    let started = {
        let mut state = ORCHESTRATOR.lock().await;
        if !state.programs.iter().any(|p| p.id == id) {
            return Err("Program not found".to_string());
        }
        let ready = treadmill_ready(connected, &state);
        let effect = seated::with(|rt| rt.machine.press(id, ready, now_ms()));
        match effect {
            Some(seated::Effect::StartProgram(id)) => {
                state.run_program(id, std::time::Instant::now())?;
                Some(id)
            }
            None => {
                log::info!(
                    "Nudge: remembered program {} until the treadmill connects",
                    id
                );
                None
            }
        }
    };
    if let Some(id) = started {
        let app_c = app.clone();
        blocking(move || programs::persist_last_run(&app_c, Some(id))).await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::HandshakeStep;

    const T0: u64 = 1_800_000_000_000;
    const MIN: u64 = 60_000;

    /// A machine with the card up: 16 minutes of typing, belt off, at 15 minutes.
    fn card_up() -> (seated::Seated, seated::Config) {
        let cfg = seated::Config {
            minutes: 15,
            ..seated::Config::default()
        };
        let mut m = seated::Seated::default();
        let mut t = 0;
        while t <= 16 * MIN {
            m.tick(
                &cfg,
                T0 + t,
                &seated::Tick {
                    idle_ms: Some(0),
                    belt_running: false,
                    ready: true,
                    program_waiting: false,
                },
            );
            t += 1000;
        }
        assert!(m.show_card());
        (m, cfg)
    }

    /// A still deck on a fresh connection: the hold is on until a press.
    fn still_deck() -> OrchestratorState {
        OrchestratorState {
            hold_auto_start: true,
            ..OrchestratorState::default()
        }
    }

    #[test]
    fn engage_on_the_card_is_the_engage_press() {
        let (mut m, _) = card_up();
        let mut card = still_deck();
        engage(&mut card, true, &mut m, T0 + 16 * MIN).unwrap();

        // The HUD's Engage on a still belt (`toggle_start`), and the start
        // shortcut: the same start.
        for action in [DeckAction::ToggleStart, DeckAction::Start] {
            let mut hud = still_deck();
            hud.trigger_action(action);
            assert_eq!(card.handshake_step, hud.handshake_step);
            assert_eq!(card.manual_intent, hud.manual_intent);
            assert_eq!(card.hold_auto_start, hud.hold_auto_start);
            assert_eq!(
                card.start_requested_at.is_some(),
                hud.start_requested_at.is_some()
            );
        }
        assert_eq!(card.handshake_step, HandshakeStep::Priming);
        assert!(!card.hold_auto_start, "the press releases the hold");
        assert!(!m.show_card(), "the card steps aside for the walk");
    }

    #[test]
    fn engage_without_a_link_starts_nothing_and_says_so() {
        let (mut m, _) = card_up();
        let mut state = still_deck();
        let r = engage(&mut state, false, &mut m, T0 + 16 * MIN);
        assert_eq!(r, Err(ENGAGE_OFFLINE.to_string()));
        assert_eq!(state.handshake_step, HandshakeStep::Idle);
        assert_eq!(state.manual_intent, None);
        assert!(state.hold_auto_start);
        assert!(m.show_card(), "the card stays to be pressed again");
        assert_eq!(m.pending_program_id(), None, "nothing is kept for later");
    }

    #[test]
    fn engage_on_a_deck_in_standby_is_refused_and_the_card_stays() {
        let (mut m, _) = card_up();
        let mut state = OrchestratorState {
            deck_standby: true,
            ..still_deck()
        };
        engage(&mut state, true, &mut m, T0 + 16 * MIN).unwrap();
        assert_eq!(state.handshake_step, HandshakeStep::Idle);
        assert!(state.start_requested_at.is_none());
        assert!(state.start_blocked, "nothing starts it once it wakes");
        assert!(m.show_card());
    }
}
