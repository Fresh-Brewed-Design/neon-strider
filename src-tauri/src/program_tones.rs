//! Program tones (off by default, Programs page): the HUD beeps when a
//! running program changes step, plays a two-tone cue when the program starts
//! waiting for GO, and a short cue a few seconds before a step that speeds the
//! belt up. For walking at a desk with headphones on, where the belt changing
//! speed or sitting still for GO would otherwise only show as HUD text.
//!
//! Rust decides which events cue; the HUD only plays the sound. `ProgramTones`
//! is a pure machine fed once a second from `desk.rs` with the running
//! program as the snapshot shows it and whether the belt runs. Each cue bumps
//! `seq` once and stamps `at_ms`, so a cue is one event however many frames
//! carry it, and the HUD skips a stale one (a window that reloads or shows
//! again after the cue). It only reads the program and the belt: nothing here
//! starts, stops or changes the belt.
//!
//! This file is declared from `snapshot.rs` (`#[path]`), not `lib.rs`.

use crate::programs::{ActiveProgram, ProgramPhase};
use serde::{Deserialize, Serialize};
use std::sync::Mutex;
use tauri::AppHandle;

const KEY_ENABLED: &str = "program_tones";

/// The speed-up cue comes when the step has this many seconds or fewer left.
/// Five seconds is enough to finish a typed sentence and set your stride
/// before the belt speeds up, and short enough that the cue still reads as
/// part of the change. The desk loop ticks once a second and the step's time
/// left is rounded up, so it lands 4 to 5 s before the next step.
pub(crate) const SPEED_UP_LEAD_S: u32 = 5;
/// A next step at least this much faster counts as a speed-up: half a deck
/// speed step, so float noise between the live target and the step's speed
/// never counts.
const SPEED_UP_KMH: f32 = 0.05;

/// Which cue the HUD plays.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/ipc/bindings/")]
#[serde(rename_all = "snake_case")]
pub enum ProgramCue {
    /// The program moved to its next step (or into a rest) with the belt running.
    Step,
    /// The program just began waiting for GO (`ProgramWait`).
    Wait,
    /// The next step is faster; it starts in about `SPEED_UP_LEAD_S`.
    SpeedUp,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct ProgramTonesSnapshot {
    /// The HUD plays program tones.
    pub enabled: bool,
    /// Counts up once per cue; the HUD plays `cue` when it changes.
    pub seq: u32,
    /// The latest cue, if any since the app started.
    pub cue: Option<ProgramCue>,
    /// When the latest cue fired (epoch ms), so a window that only now gets
    /// the snapshot skips an old one.
    #[ts(type = "number")]
    pub at_ms: u64,
}

/// Where a program is: a new value is a new step.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Spot {
    program_id: u32,
    phase: ProgramPhase,
    step_index: u32,
    cycle: u32,
}

/// What one tick sees of the running program.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct View {
    spot: Spot,
    waiting: bool,
    paused: bool,
    /// Whole seconds left in this step or rest; None for one with no fixed end.
    left_s: Option<u32>,
    /// The live target now.
    speed_kmh: f32,
    /// The speed of the step that follows this one, when one follows in the
    /// same run: the next step, or after a rest the first step.
    next_kmh: Option<f32>,
}

impl View {
    pub fn of(p: &ActiveProgram) -> Self {
        let next_kmh = match p.phase {
            ProgramPhase::Walking => p.steps.get(p.step_index as usize + 1),
            ProgramPhase::Resting => p.steps.first(),
        }
        .map(|s| s.speed_kmh);
        View {
            spot: Spot {
                program_id: p.program_id,
                phase: p.phase,
                step_index: p.step_index,
                cycle: p.cycle,
            },
            waiting: p.wait.is_some(),
            paused: p.paused,
            left_s: p.seconds_remaining,
            speed_kmh: p.speed_kmh,
            next_kmh,
        }
    }
}

/// One second's input.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Tick {
    /// The treadmill is connected and reporting.
    pub ready: bool,
    /// The deck reports the belt running.
    pub belt_running: bool,
    pub program: Option<View>,
}

#[derive(Debug, Default)]
pub struct ProgramTones {
    last: Option<View>,
    /// The step whose speed-up cue already played.
    lead_for: Option<Spot>,
    pub seq: u32,
    pub cue: Option<ProgramCue>,
    pub at_ms: u64,
}

impl ProgramTones {
    /// Note this second's program and return the cue it fires, if any. The
    /// first view of a run is only a baseline: starting a program, a restart
    /// that restores one and switching tones on never cue by themselves.
    pub fn tick(&mut self, enabled: bool, now_ms: u64, t: &Tick) -> Option<ProgramCue> {
        let prev = self.last.take();
        self.last = t.program;
        let now = t.program?;
        let prev = prev.filter(|p| p.spot.program_id == now.spot.program_id)?;
        if !enabled || !t.ready {
            return None;
        }
        let cue = if now.waiting && !prev.waiting {
            // The belt is still and the program sits until GO.
            Some(ProgramCue::Wait)
        } else if now.spot != prev.spot {
            // A step change the belt walks through; one made while paused or
            // stopped (SKIP on a held belt) changes nothing underfoot.
            (t.belt_running && !now.paused && !now.waiting).then_some(ProgramCue::Step)
        } else if self.speed_up_due(&now, t.belt_running) {
            self.lead_for = Some(now.spot);
            Some(ProgramCue::SpeedUp)
        } else {
            None
        };
        if let Some(c) = cue {
            self.seq = self.seq.wrapping_add(1);
            self.cue = Some(c);
            self.at_ms = now_ms;
        }
        cue
    }

    fn speed_up_due(&self, now: &View, belt_running: bool) -> bool {
        belt_running
            && !now.paused
            && !now.waiting
            && self.lead_for != Some(now.spot)
            && now.left_s.is_some_and(|s| s > 0 && s <= SPEED_UP_LEAD_S)
            && now
                .next_kmh
                .is_some_and(|next| next >= now.speed_kmh + SPEED_UP_KMH)
    }
}

// ---------------------------------------------------------------------------
// Setting and shared state
// ---------------------------------------------------------------------------

#[derive(Default)]
pub struct Runtime {
    pub enabled: bool,
    pub machine: ProgramTones,
}

static STATE: Mutex<Option<Runtime>> = Mutex::new(None);

/// Run `f` on the shared setting and machine.
pub fn with<R>(f: impl FnOnce(&mut Runtime) -> R) -> R {
    let mut guard = STATE.lock().unwrap_or_else(|e| e.into_inner());
    f(guard.get_or_insert_with(Runtime::default))
}

/// Load the saved switch (off when never set). Blocking; call once after the
/// DB is initialised.
pub fn init(app: &AppHandle) {
    let on = crate::persistence::parse_setting::<bool>(
        crate::persistence::read_setting(app, KEY_ENABLED).as_deref(),
    )
    .unwrap_or(false);
    with(|rt| rt.enabled = on);
}

/// Blocking (SQLite).
pub fn set_enabled(app: &AppHandle, on: bool) -> Result<(), String> {
    crate::persistence::write_setting(app, KEY_ENABLED, if on { "true" } else { "false" })?;
    with(|rt| rt.enabled = on);
    log::info!("Program tones {}", if on { "on" } else { "off" });
    Ok(())
}

/// For the snapshot. No I/O.
pub fn snapshot() -> ProgramTonesSnapshot {
    with(|rt| ProgramTonesSnapshot {
        enabled: rt.enabled,
        seq: rt.machine.seq,
        cue: rt.machine.cue,
        at_ms: rt.machine.at_ms,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::programs::{ProgramStep, ProgramWait};

    fn step(minutes: f32, kmh: f32) -> ProgramStep {
        ProgramStep {
            duration_minutes: Some(minutes),
            speed_kmh: kmh,
            incline_pct: 0.0,
            zone: None,
            kind: None,
            pulse_end: None,
        }
    }

    /// A run of 3.0, 4.5 and 3.5 km/h steps, at `index` with `left` seconds to go.
    fn program(index: u32, left: Option<u32>) -> ActiveProgram {
        let steps = vec![step(5.0, 3.0), step(5.0, 4.5), step(5.0, 3.5)];
        ActiveProgram {
            program_id: 7,
            name: "Test".into(),
            phase: ProgramPhase::Walking,
            step_index: index,
            step_count: steps.len() as u32,
            seconds_remaining: left,
            cycle: 1,
            paused: false,
            wait: None,
            speed_kmh: steps[index as usize].speed_kmh,
            incline_pct: 0.0,
            kind: None,
            pulse_to_bpm: None,
            steps,
            repeat_every_minutes: None,
            program_seconds_remaining: None,
            pass_note: None,
        }
    }

    fn walking(p: &ActiveProgram) -> Tick {
        Tick {
            ready: true,
            belt_running: true,
            program: Some(View::of(p)),
        }
    }

    /// Feed the ticks in order with tones on; the cues that fired.
    fn run(m: &mut ProgramTones, ticks: &[Tick]) -> Vec<ProgramCue> {
        ticks
            .iter()
            .enumerate()
            .filter_map(|(i, t)| m.tick(true, 1_000 * i as u64, t))
            .collect()
    }

    #[test]
    fn a_step_change_with_the_belt_running_cues_once() {
        let mut m = ProgramTones::default();
        let cues = run(
            &mut m,
            &[
                walking(&program(1, Some(120))),
                walking(&program(2, Some(300))),
                walking(&program(2, Some(299))),
                walking(&program(2, Some(298))),
            ],
        );
        assert_eq!(cues, vec![ProgramCue::Step]);
        assert_eq!(m.seq, 1, "one event, one count, whatever the frames");
        assert_eq!(m.cue, Some(ProgramCue::Step));
        assert_eq!(m.at_ms, 1_000);
    }

    #[test]
    fn starting_a_program_or_the_first_tick_is_only_a_baseline() {
        let mut m = ProgramTones::default();
        let none = Tick {
            ready: true,
            belt_running: true,
            program: None,
        };
        // A restored run seen first mid-wait, and a run started from nothing.
        let mut waiting = program(1, Some(300));
        waiting.paused = true;
        waiting.wait = Some(ProgramWait::BeltStill);
        assert!(run(&mut m, &[walking(&waiting)]).is_empty());
        let mut m = ProgramTones::default();
        assert!(run(&mut m, &[none, walking(&program(0, Some(300)))]).is_empty());
        assert_eq!(m.seq, 0);
    }

    #[test]
    fn a_wait_for_go_cues_once_with_the_belt_still() {
        let mut rest = program(2, Some(10));
        rest.phase = ProgramPhase::Resting;
        rest.speed_kmh = 0.0;
        let mut ready = program(0, Some(300));
        ready.cycle = 2;
        ready.paused = true;
        ready.wait = Some(ProgramWait::RestComplete);
        let still = |p: &ActiveProgram| Tick {
            ready: true,
            belt_running: false,
            program: Some(View::of(p)),
        };
        let mut m = ProgramTones::default();
        let cues = run(
            &mut m,
            &[still(&rest), still(&ready), still(&ready), still(&ready)],
        );
        assert_eq!(cues, vec![ProgramCue::Wait]);
        assert_eq!(m.seq, 1);
    }

    #[test]
    fn a_faster_next_step_cues_a_few_seconds_ahead_once() {
        let mut m = ProgramTones::default();
        let ticks: Vec<Tick> = (0..=8)
            .rev()
            .map(|left| walking(&program(0, Some(left + 1))))
            .collect();
        // 9 s left down to 1 s: the cue at 5 s, once.
        assert_eq!(run(&mut m, &ticks), vec![ProgramCue::SpeedUp]);
        assert_eq!(m.cue, Some(ProgramCue::SpeedUp));
        assert_eq!(m.at_ms, 4_000, "at the tick with 5 s left");
    }

    #[test]
    fn a_slower_or_equal_next_step_has_no_lead_cue() {
        let mut m = ProgramTones::default();
        // Step 1 (4.5) goes down to step 2 (3.5); the last step has none after it.
        let ticks: Vec<Tick> = (1..=8)
            .rev()
            .map(|left| walking(&program(1, Some(left))))
            .chain((1..=8).rev().map(|left| walking(&program(2, Some(left)))))
            .collect();
        assert_eq!(run(&mut m, &ticks), vec![ProgramCue::Step]);
    }

    #[test]
    fn a_rest_with_the_belt_moving_cues_before_a_faster_next_cycle() {
        let mut rest = program(0, Some(4));
        rest.phase = ProgramPhase::Resting;
        rest.speed_kmh = 2.5;
        let mut m = ProgramTones::default();
        let mut earlier = rest.clone();
        earlier.seconds_remaining = Some(30);
        assert_eq!(
            run(&mut m, &[walking(&earlier), walking(&rest)]),
            vec![ProgramCue::SpeedUp]
        );
    }

    #[test]
    fn nothing_cues_with_tones_off() {
        let mut m = ProgramTones::default();
        let mut ready = program(1, Some(300));
        ready.paused = true;
        ready.wait = Some(ProgramWait::HoldComplete);
        let ticks = [
            walking(&program(0, Some(9))),
            walking(&program(0, Some(5))),
            walking(&program(1, Some(300))),
            walking(&ready),
        ];
        for (i, t) in ticks.iter().enumerate() {
            assert_eq!(m.tick(false, i as u64, t), None);
        }
        assert_eq!(m.seq, 0);
        // Switched on afterwards, nothing old plays.
        assert_eq!(m.tick(true, 9, &walking(&ready)), None);
    }

    #[test]
    fn nothing_cues_on_a_paused_or_stopped_belt() {
        let mut paused = program(0, Some(5));
        paused.paused = true;
        let mut skipped = program(1, Some(300));
        skipped.paused = true;
        let stopped = |p: &ActiveProgram| Tick {
            ready: true,
            belt_running: false,
            program: Some(View::of(p)),
        };
        let mut m = ProgramTones::default();
        // Paused inside the lead window, then SKIP while paused.
        let cues = run(
            &mut m,
            &[
                walking(&program(0, Some(30))),
                walking(&paused),
                walking(&skipped),
            ],
        );
        assert!(cues.is_empty());
        // The belt stopped (not running) through a lead window and a step change.
        let mut m = ProgramTones::default();
        let cues = run(
            &mut m,
            &[
                stopped(&program(0, Some(30))),
                stopped(&program(0, Some(5))),
                stopped(&program(1, Some(300))),
            ],
        );
        assert!(cues.is_empty());
        // Offline: no cue either.
        let mut m = ProgramTones::default();
        let offline = |p: &ActiveProgram| Tick {
            ready: false,
            belt_running: true,
            program: Some(View::of(p)),
        };
        assert!(run(
            &mut m,
            &[
                offline(&program(0, Some(9))),
                offline(&program(1, Some(300)))
            ]
        )
        .is_empty());
    }

    #[test]
    fn a_new_program_is_a_new_baseline() {
        let mut m = ProgramTones::default();
        let mut other = program(2, Some(300));
        other.program_id = 8;
        assert!(run(&mut m, &[walking(&program(0, Some(300))), walking(&other)]).is_empty());
    }
}
