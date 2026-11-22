//! Saved, named programs: the one model behind "walk X every Y" and step lists.
//!
//! A program is a list of steps (speed + incline, optionally timed) and an
//! optional repeat period. Running one drives `program_intent`; when it ends
//! (or between cycles of a repeating program) the previous state is restored.
//! A program never starts a still belt by itself: running it is the press for
//! its first step, and a later step that would start a still belt (after a
//! rest, or after a 0 km/h step) pauses the program until GO (`ProgramWait`).
//! All runtime logic is pure over `OrchestratorState` plus an explicit
//! `Instant` so it can be unit-tested without Tauri or a treadmill.

use crate::bluetooth::ORCHESTRATOR;
use crate::engine::{OrchestratorState, STILL_KMH};
use crate::persistence;
use crate::protocol::{ControlIntent, MachineStatus};
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};

pub mod fit_to_deck;

pub const KEY_PROGRAMS: &str = "programs";
const KEY_NEXT_ID: &str = "program_next_id";
const KEY_LAST_RUN: &str = "program_last_run_id";
/// The running program, saved so it survives an app restart (crash, update,
/// reboot, or a Rust rebuild under `tauri dev`).
const KEY_ACTIVE_RUN: &str = "program_active_run";
/// A saved run that was walking is dropped on startup when older than this:
/// the treadmill has almost certainly been stopped or reset in the meantime.
const RESUME_MAX_AGE_MS: i64 = 10 * 60 * 1000;
/// A saved run that held the belt still (resting, paused, waiting for GO) is
/// kept this long: restoring it can't move the belt, and a program left
/// waiting over lunch should still be there. Older ones are dropped.
const RESUME_STILL_MAX_AGE_MS: i64 = 4 * 60 * 60 * 1000;
/// Elapsed time is saved at most this often; structural changes save at once.
/// A crash or update loses at most 5 s of a step, as for the session row.
const ACTIVE_RUN_SAVE_EVERY: Duration = Duration::from_secs(5);

/// Fits a program button on the HUD and the idle-nudge card.
pub(crate) const MAX_NAME_CHARS: usize = 40;
/// A long interval program (the VO2 max one has 10 steps) with room to spare,
/// while the editor stays one screen.
pub(crate) const MAX_STEPS: usize = 20;
/// Below this a speed means "belt held still": the engine's own threshold.
const MOVING_KMH: f32 = STILL_KMH;
/// Half a minute to finish a sentence before the belt stops for a rest.
const REST_WARNING_S: f64 = 30.0;
/// A step with a heart rate zone changes the speed by this much at a time:
/// one deck step, the smallest change the belt makes.
const ZONE_TRIM_KMH: f32 = 0.1;
/// No more often than this: a pulse takes 15 to 30 s to settle after the belt
/// changes, so trimming faster would chase a reading that hasn't caught up.
const ZONE_TRIM_EVERY: Duration = Duration::from_secs(20);
/// Never further than this from the step's speed: the zone fine-tunes the
/// speed the user chose for the step, and a pulse it can't reach within
/// 1 km/h means the step doesn't suit the zone, not that the belt should run
/// at a speed the user never picked.
const ZONE_TRIM_RANGE_KMH: f32 = 1.0;
/// A zone step leaves the belt alone for this much running time after it
/// starts, resumes or takes GO, so the trim doesn't chase a pulse still
/// climbing to the new effort. After a step change in treadmill speed the
/// heart rate follows with a time constant of about 58 s (57.6 s mean, 24 men;
/// Hunt, Fankhauser and Saengsuwan 2015, docs/research.md). Two time constants
/// cover about 86 % of the change, and leave half of a 4 minute VO2 max climb
/// to trim in; three (about 3 minutes) would leave one minute.
pub(crate) const ZONE_SETTLE: Duration = Duration::from_secs(120);
/// A zone that trims incline (Labs, ZONE_TILT) stays within this many
/// percent of grade of the step's incline. By the ACSM walking equation, 1 km/h
/// at 3 to 4 km/h costs about as much as 1.5 to 2 % of grade, so 2 % matches
/// the speed trim's 1 km/h: a fine-tune of the step, not a new one.
const ZONE_INCLINE_RANGE_PCT: f32 = 2.0;
/// Incline moves one deck level at a time, no more often than this. A level
/// (about 1 % of grade on most decks) is a bigger change in effort than
/// 0.1 km/h, so it waits about one heart rate time constant (57.6 s, see
/// `ZONE_SETTLE`) for most of its effect to show before the next.
const ZONE_INCLINE_TRIM_EVERY: Duration = Duration::from_secs(60);
/// Above STRIDE_FIT a zone raises incline where it would have raised speed
/// (#227), up to this many percent of grade past the incline the zone may
/// reach by itself. It stands in for the speed trim's 1 km/h, which costs
/// about 2 % of grade by the ACSM walking equation (see
/// `ZONE_INCLINE_RANGE_PCT`), so the extra climb does no more than the speed
/// would have. Always held to the deck's top level too.
const ZONE_FIT_INCLINE_PCT: f32 = ZONE_INCLINE_RANGE_PCT;
/// A zone's lowest allowed bound, in % of maximum heart rate: below 30 % is
/// resting heart rate for most people, which walking can't hold down to.
const ZONE_MIN_PCT: f32 = 30.0;
/// The highest: a zone can't go above the maximum heart rate itself.
const ZONE_MAX_PCT: f32 = 100.0;
/// A cool-down step that ends on heart rate (#233) ends once its minutes are
/// done and the pulse has stayed under the target this long. After a change
/// in treadmill speed the pulse follows with a time constant of about 58 s
/// (see `ZONE_SETTLE`), so a pulse truly falling through the target keeps
/// falling and half a time constant costs little; a single low reading
/// (wrist sensors read 1 to 7 bpm off on walks, Thiebaud et al. 2018,
/// docs/research.md) can't end the step.
pub(crate) const PULSE_END_HOLD: Duration = Duration::from_secs(30);
/// At most this much of one tick counts toward `PULSE_END_HOLD`. Sensors send
/// about one reading a second and the tick is 1 Hz, so a reading speaks for
/// about the second before it; a longer gap (the app closed, then restored)
/// says nothing about the pulse in between.
const PULSE_TICK_MAX_S: f64 = 2.0;
/// The lowest cool-down pulse target, in % of maximum heart rate: under 50 %,
/// deep in the ACSM very light band (under 57 %), an easy walk may never get
/// there and the step would walk on until SKIP.
pub(crate) const PULSE_END_MIN_PCT: f32 = 50.0;
/// The highest: 77 % and up is the ACSM vigorous band (Garber et al. 2011),
/// so a target over 76 % would end a cool-down that never cooled down.
pub(crate) const PULSE_END_MAX_PCT: f32 = 76.0;
/// Every COOL_DOWN step's target unless the user changes it (the editor
/// ticks it on a new cool-down, the seeded VO2 max carries it): inside the
/// ACSM light band (57 to 63 %), a few bpm clear of moderate (64 %), and
/// under the 4x4's 70 % rests. One value whatever comes before: a harder
/// program makes the fall slower, not its end higher (docs/research.md).
/// 114 bpm at the default 190 bpm max.
pub(crate) const PULSE_END_DEFAULT_PCT: f32 = 60.0;
/// A start at the treadmill counts once the deck has reported the belt moving
/// (Starting or Running) for this long without a break, after reporting it
/// still. The deck answers a status poll every 600 ms, so this spans at least
/// two frames in a row: one stray frame can't pass for a press. It is well
/// inside the deck's own start countdown (about 3 s of Starting), so the
/// program's speed is set by the time the belt turns.
pub(crate) const DECK_GO_DEBOUNCE: Duration = Duration::from_millis(1000);
/// An app start (the handshake's Start) counts as in flight this long, until
/// the deck answers it: motion in that window is the app's own start.
const APP_START_IN_FLIGHT: Duration = crate::link::START_TIMEOUT;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct Program {
    /// 0 when creating (ProgramSave assigns the real id).
    pub id: u32,
    pub name: String,
    pub steps: Vec<ProgramStep>,
    /// Start a new cycle this many minutes after the previous one started.
    pub repeat_every_minutes: Option<f32>,
    /// One sentence on why the program is built this way (docs/research.md).
    /// Set by Rust for the seeded programs only; a save that changes the steps
    /// or the repeat drops it, since it no longer describes the program.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub note: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct ProgramStep {
    /// None = runs until ended; only valid as the last step.
    pub duration_minutes: Option<f32>,
    /// Calibrated (physical) speed; 0 holds the belt still.
    pub speed_kmh: f32,
    /// Calibrated incline percentage as shown to the user.
    pub incline_pct: f32,
    /// Hold this heart rate zone while the step walks: the speed is trimmed
    /// around `speed_kmh` to keep the pulse inside it. None = plain step.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub zone: Option<HeartZone>,
    /// What the step is for, shown on the HUD, the Live page and the tray as
    /// `INTERVAL 3/5`. None (every step saved before #235) shows the step
    /// number only, as before. A name: changing it keeps the program's note.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub kind: Option<StepKind>,
    /// A COOL_DOWN step only (#233): once its minutes are done it walks on
    /// until the pulse has stayed under this for `PULSE_END_HOLD`. With no
    /// heart rate reading it ends on its minutes. None = a timed step.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub pulse_end: Option<PulseEnd>,
}

/// Where a cool-down step's pulse must get to before the step ends: under
/// `below_pct` % of the maximum heart rate, the same maximum zones use.
/// Stored as % (like `HeartZone`), so a new maximum moves the target with it.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct PulseEnd {
    pub below_pct: f32,
}

impl PulseEnd {
    /// The target a COOL_DOWN step gets unless the user changes it:
    /// `PULSE_END_DEFAULT_PCT`, the same for every cool-down whatever comes
    /// before it (docs/research.md).
    pub(crate) const fn default_target() -> Self {
        PulseEnd {
            below_pct: PULSE_END_DEFAULT_PCT,
        }
    }

    /// The target in whole beats per minute for a maximum heart rate, rounded
    /// as the editor shows it; a reading at or under it counts as down, so
    /// the number on screen is the one to reach.
    fn target_bpm(self, max_bpm: u32) -> u32 {
        (self.below_pct / 100.0 * max_bpm as f32).round() as u32
    }
}

/// What a program step is for (#235). Only a name: no kind changes how a
/// step drives the belt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub enum StepKind {
    WarmUp,
    Walk,
    Interval,
    Rest,
    CoolDown,
}

impl StepKind {
    /// Every kind, in the editor's order.
    pub const ALL: [StepKind; 5] = [
        StepKind::WarmUp,
        StepKind::Walk,
        StepKind::Interval,
        StepKind::Rest,
        StepKind::CoolDown,
    ];

    /// The short code the HUD, the Live page and the tray show.
    pub fn code(self) -> &'static str {
        match self {
            StepKind::WarmUp => "WARM_UP",
            StepKind::Walk => "WALK",
            StepKind::Interval => "INTERVAL",
            StepKind::Rest => "REST",
            StepKind::CoolDown => "COOL_DOWN",
        }
    }
}

/// The running step's kind and its place among the program's steps of that
/// kind: the 3rd of 5 intervals is `nth` 3 `of` 5, labelled `INTERVAL 3/5`.
/// A kind the program has once is labelled by its code alone (`WARM_UP`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct StepKindSpot {
    pub kind: StepKind,
    /// 1-based, counting only steps of this kind.
    pub nth: u32,
    pub of: u32,
    pub label: String,
}

impl StepKindSpot {
    /// Step `index` of `steps`, or None when it has no kind (or no such step).
    pub fn of_step(steps: &[ProgramStep], index: usize) -> Option<Self> {
        let kind = steps.get(index)?.kind?;
        let same = |s: &&ProgramStep| s.kind == Some(kind);
        let nth = steps[..=index].iter().filter(same).count() as u32;
        let of = steps.iter().filter(same).count() as u32;
        let label = if of > 1 {
            format!("{} {nth}/{of}", kind.code())
        } else {
            kind.code().to_string()
        };
        Some(Self {
            kind,
            nth,
            of,
            label,
        })
    }
}

/// A heart rate zone as percentages of the maximum heart rate, e.g. VO2 max
/// at 85 to 95.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct HeartZone {
    pub low_pct: f32,
    pub high_pct: f32,
    /// What the zone trims while the Labs switch ZONE_TILT is on. None (every
    /// zone saved before #126, and what a save stores for speed) trims speed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub trim: Option<ZoneTrim>,
}

/// What a heart rate zone moves to hold the pulse in it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub enum ZoneTrim {
    /// Speed only, within 1 km/h of the step's speed (the default).
    Speed,
    /// Incline only, within 2 % of the step's incline; the speed stays put.
    Incline,
    /// Incline first, then speed once the incline is at its limit.
    Both,
}

impl ZoneTrim {
    /// How far from the step's incline the zone moves the incline by itself:
    /// none for a speed-only zone.
    fn own_incline_range_pct(self) -> f32 {
        match self {
            ZoneTrim::Speed => 0.0,
            ZoneTrim::Incline | ZoneTrim::Both => ZONE_INCLINE_RANGE_PCT,
        }
    }
}

impl HeartZone {
    /// What the zone trims, with ZONE_TILT on (`incline_allowed`); off,
    /// every zone trims speed only, as before #126.
    fn trims(self, incline_allowed: bool) -> ZoneTrim {
        match self.trim {
            Some(t) if incline_allowed => t,
            _ => ZoneTrim::Speed,
        }
    }

    /// The zone's bounds in beats per minute for a maximum heart rate.
    fn bounds_bpm(self, max_bpm: u32) -> (f32, f32) {
        let max = max_bpm as f32;
        (self.low_pct / 100.0 * max, self.high_pct / 100.0 * max)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub enum ProgramPhase {
    Walking,
    /// Only for repeating programs, between cycles.
    Resting,
}

/// Why a program is paused waiting for GO: its next step would start a belt
/// that is still, and the belt never starts without a press in that moment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub enum ProgramWait {
    /// A repeating program's rest is over; the next cycle waits.
    RestComplete,
    /// A walking step follows a 0 km/h step.
    HoldComplete,
    /// After a restart or reconnect the belt was found still mid-step.
    BeltStill,
}

impl ProgramWait {
    /// The HUD toast when the wait begins (the HUD and dashboard show the same).
    pub fn headline(self) -> &'static str {
        match self {
            ProgramWait::RestComplete => "REST COMPLETE // PRESS GO",
            ProgramWait::HoldComplete => "HOLD COMPLETE // PRESS GO",
            ProgramWait::BeltStill => "BELT STILL // PRESS GO",
        }
    }
}

/// What one heart rate zone trim changes.
#[derive(Debug, Clone, Copy, PartialEq)]
enum ZoneLever {
    /// The step's live speed, km/h.
    Speed(f32),
    /// The step's live incline, as a hardware level.
    Incline(u8),
}

/// A HUD toast from the program tick.
#[derive(Debug, Clone, PartialEq)]
pub enum ProgramToast {
    /// A rest ends in 30 s (the toast offers SKIP REST).
    RestEndingSoon(String),
    /// The program just started waiting for GO (the panel's GO is the action).
    WaitingForGo(ProgramWait),
    /// A start at the treadmill was taken as the program's GO (no button:
    /// the program is already walking again).
    DeckGo(String),
}

impl ProgramToast {
    /// The toast for a start at the treadmill that resumed `name`, e.g.
    /// `DECK START // WORKDAY RESUMED`. The name is uppercased as in the 30 s
    /// warning; the HUD wraps a long one.
    pub fn deck_go(name: &str) -> Self {
        ProgramToast::DeckGo(format!("DECK START // {} RESUMED", name.to_uppercase()))
    }

    pub fn message(&self) -> &str {
        match self {
            ProgramToast::RestEndingSoon(m) | ProgramToast::DeckGo(m) => m,
            ProgramToast::WaitingForGo(w) => w.headline(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct ActiveProgram {
    pub program_id: u32,
    pub name: String,
    pub phase: ProgramPhase,
    /// While Resting, step_index/speed_kmh/incline_pct describe the first step
    /// of the next cycle.
    pub step_index: u32,
    pub step_count: u32,
    /// In this phase; None for an open-ended step.
    pub seconds_remaining: Option<u32>,
    /// 1-based.
    pub cycle: u32,
    pub paused: bool,
    /// Set while the program is paused waiting for GO before its next step
    /// starts the belt (`paused` is true too). The step timer doesn't run, and
    /// nothing ends the wait but GO, SKIP or END.
    pub wait: Option<ProgramWait>,
    /// Current live target (includes manual nudges made during the step).
    pub speed_kmh: f32,
    pub incline_pct: f32,
    /// The walking step's kind and its count within that kind; None while
    /// resting or for a step with no kind.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub kind: Option<StepKindSpot>,
    /// Set while a cool-down step's minutes are done and it walks on until
    /// the pulse is under this many bpm (#233); `seconds_remaining` is none
    /// then, since nothing but the pulse, SKIP or END ends it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub pulse_to_bpm: Option<u32>,
}

/// What the treadmill was doing when the program took over.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PreviousState {
    pub status: MachineStatus,
    /// Manual intent to put back; Some only if the belt was moving.
    pub moving: Option<ControlIntent>,
}

#[derive(Debug, Clone)]
pub struct ProgramRun {
    pub program: Program,
    pub phase: ProgramPhase,
    pub step_index: usize,
    pub cycle: u32,
    pub paused: bool,
    /// Paused waiting for GO (see `ActiveProgram::wait`).
    pub wait: Option<ProgramWait>,
    /// Seconds spent in the current phase (only counted while it should be).
    pub elapsed_s: f64,
    pub last_tick: Instant,
    /// Live target (hardware incline level). Nudges edit this, not the program.
    pub target: ControlIntent,
    pub previous: PreviousState,
    pub rest_warned: bool,
    /// Restored after an app restart and not yet confirmed by the deck. The
    /// reconnect clears all intents, which would otherwise read as "stopped
    /// at the treadmill" and end the program; a restored run re-applies its
    /// target instead, once.
    pub restored: bool,
    /// For a restored run: when it was saved, before the app closed. If the
    /// deck is found still running on the reconnect, the belt never stopped,
    /// so the time in between was walked and counts toward the step.
    pub saved_at: Option<Instant>,
    /// Watches for a start at the treadmill while the run holds the belt still
    /// (paused, waiting for GO, or resting). Not saved.
    pub deck_start: DeckStartWatch,
    /// The deck has reported the belt moving since the run last started or
    /// resumed walking, so a Paused report after it is a pause of that walk
    /// (`notice_deck_pause`), not the deck still answering a GO. Not saved.
    pub deck_moved: bool,
    /// When a heart rate zone may trim again (the speed or incline was last
    /// trimmed, plus that lever's wait). Not saved.
    pub zone_next_trim: Option<Instant>,
    /// Seconds the belt has run in this step since it started, resumed or
    /// took GO; a zone trims nothing until `ZONE_SETTLE` has passed. Not
    /// saved: a restored run waits again.
    pub zone_settle_s: f64,
    /// Seconds of fresh heart rate readings in a row under the step's
    /// `pulse_end` target while the belt runs; a missing reading, one at or
    /// over the target, a pause or a new step starts it again. Not saved.
    pub pulse_under_s: f64,
    /// The step's minutes are done and the pulse is keeping it going: when it
    /// ends, no time carries into the next step. Not saved.
    pub pulse_held: bool,
}

/// What the deck has reported since the run began holding the belt still: a
/// start at the treadmill is the belt reported still, then moving for
/// `DECK_GO_DEBOUNCE`, without the app having sent a start.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct DeckStartWatch {
    /// The deck has reported the belt still since the hold began (so a pause
    /// still winding down never counts).
    still_seen: bool,
    /// Since when it has reported the belt moving again after that.
    moving_since: Option<Instant>,
}

impl ProgramRun {
    /// Length of the current phase in seconds; None for an open-ended step.
    fn phase_len_s(&self) -> Option<f64> {
        match self.phase {
            ProgramPhase::Walking => self
                .program
                .steps
                .get(self.step_index)
                .and_then(|s| s.duration_minutes)
                .map(|m| m as f64 * 60.0),
            ProgramPhase::Resting => {
                let repeat = self.program.repeat_every_minutes.unwrap_or(0.0) as f64;
                Some(((repeat - total_minutes(&self.program) as f64) * 60.0).max(0.0))
            }
        }
    }

    /// Walking, on a 0 km/h step: the program holds the belt still.
    pub fn on_hold_step(&self) -> bool {
        self.phase == ProgramPhase::Walking
            && self
                .program
                .steps
                .get(self.step_index)
                .is_some_and(|s| s.speed_kmh < MOVING_KMH)
    }

    /// The next time the program holds the belt still while it walks: the
    /// seconds of walking until then, and how long the still stretch lasts
    /// (None: until ended). A 0 km/h step is one, and so is the rest after
    /// this cycle when the rest leaves the belt still (nothing was walking
    /// before the program). On a 0 km/h step it is (0, what is left of it).
    /// None while not walking, when an open-ended step comes first, or when
    /// the program just ends (its end is not a rest). The step timer runs only
    /// while the belt walks, so this is walking time.
    pub fn next_still_s(&self) -> Option<(f64, Option<f64>)> {
        if self.phase != ProgramPhase::Walking {
            return None;
        }
        let left_here = self
            .phase_len_s()
            .map(|len| (len - self.elapsed_s).max(0.0));
        if self.on_hold_step() {
            return Some((0.0, left_here));
        }
        let mut walk = left_here?;
        let steps = &self.program.steps;
        for step in steps.get(self.step_index + 1..).unwrap_or(&[]) {
            let len = step.duration_minutes.map(|m| m as f64 * 60.0);
            if step.speed_kmh < MOVING_KMH {
                return Some((walk, len));
            }
            walk += len?;
        }
        let repeat = self.program.repeat_every_minutes? as f64;
        let rest_s = (repeat - total_minutes(&self.program) as f64) * 60.0;
        (rest_s >= 1.0 && self.previous.moving.is_none()).then_some((walk, Some(rest_s)))
    }

    /// While the program holds the belt still: how long until it wants the
    /// belt moving again. None while waiting for GO or on an open-ended
    /// 0 km/h step (until a press), and when it doesn't hold the belt still.
    pub fn still_left_s(&self) -> Option<f64> {
        if self.wait.is_some() {
            return None;
        }
        let left = self
            .phase_len_s()
            .map(|len| (len - self.elapsed_s).max(0.0));
        match self.phase {
            ProgramPhase::Resting => left,
            ProgramPhase::Walking if self.on_hold_step() => left,
            ProgramPhase::Walking => None,
        }
    }
}

// ---------------------------------------------------------------------------
// Pure helpers
// ---------------------------------------------------------------------------

fn total_minutes(program: &Program) -> f32 {
    program
        .steps
        .iter()
        .filter_map(|s| s.duration_minutes)
        .sum()
}

/// Hardware incline level for a calibrated percentage:
/// round((pct - offset) / tilt), clamped to 0..=max_level. A tilt of ~0 can't
/// be inverted, so it yields level 0.
fn incline_pct_to_level(pct: f32, tilt_multiplier: f32, incline_offset: f32, max_level: u8) -> u8 {
    if !pct.is_finite() || !tilt_multiplier.is_finite() || tilt_multiplier.abs() < 0.01 {
        return 0;
    }
    let level = ((pct - incline_offset) / tilt_multiplier).round();
    level.clamp(0.0, max_level as f32) as u8
}

fn level_to_incline_pct(level: u8, tilt_multiplier: f32, incline_offset: f32) -> f32 {
    level as f32 * tilt_multiplier + incline_offset
}

/// Validate a program for saving. `min_speed` / `max_speed` are the calibrated
/// belt limits. The error is a short message fit to show the user.
pub(crate) fn validate_program(p: &Program, min_speed: f32, max_speed: f32) -> Result<(), String> {
    let name_len = p.name.trim().chars().count();
    if name_len == 0 {
        return Err("Name is required".to_string());
    }
    if name_len > MAX_NAME_CHARS {
        return Err(format!(
            "Name must be {} characters or fewer",
            MAX_NAME_CHARS
        ));
    }
    if p.steps.is_empty() {
        return Err("Add at least one step".to_string());
    }
    if p.steps.len() > MAX_STEPS {
        return Err(format!("A program can have at most {} steps", MAX_STEPS));
    }
    let ok = |v: f32| v.is_finite() && v >= 0.0;
    let last = p.steps.len() - 1;
    for (i, s) in p.steps.iter().enumerate() {
        let n = i + 1;
        if let Some(d) = s.duration_minutes {
            if !ok(d) {
                return Err(format!("Step {}: duration must be zero or more", n));
            }
        } else if i != last {
            return Err(format!(
                "Step {}: only the last step can run until ended",
                n
            ));
        }
        if !ok(s.speed_kmh) {
            return Err(format!("Step {}: speed must be zero or more", n));
        }
        // The calibrated floor is rarely a round number; a speed typed as the
        // floor shown (to 0.01 km/h) is not below it.
        // 0.005 is half the 0.01 km/h the floor is shown to.
        if s.speed_kmh != 0.0 && (s.speed_kmh < min_speed - 0.005 || s.speed_kmh > max_speed) {
            return Err(format!(
                "Step {}: speed must be 0 or between {:.1} and {:.1} km/h",
                n, min_speed, max_speed
            ));
        }
        if !ok(s.incline_pct) {
            return Err(format!("Step {}: incline must be zero or more", n));
        }
        if let Some(pe) = s.pulse_end {
            if s.kind != Some(StepKind::CoolDown) {
                return Err(format!(
                    "Step {}: only a COOL_DOWN step can end on heart rate",
                    n
                ));
            }
            if s.duration_minutes.is_none() {
                return Err(format!("Step {}: a heart rate end needs minutes", n));
            }
            if s.speed_kmh == 0.0 {
                return Err(format!(
                    "Step {}: a heart rate end needs a walking speed",
                    n
                ));
            }
            if s.zone.is_some() {
                return Err(format!("Step {}: pick a heart zone or a heart rate end", n));
            }
            if !pe.below_pct.is_finite()
                || pe.below_pct < PULSE_END_MIN_PCT
                || pe.below_pct > PULSE_END_MAX_PCT
            {
                return Err(format!(
                    "Step {}: the heart rate end must be {} to {} % of maximum heart rate",
                    n, PULSE_END_MIN_PCT, PULSE_END_MAX_PCT
                ));
            }
        }
        if let Some(z) = s.zone {
            if s.speed_kmh == 0.0 {
                return Err(format!("Step {}: a zone needs a walking speed", n));
            }
            if !(z.low_pct.is_finite() && z.high_pct.is_finite())
                || z.low_pct < ZONE_MIN_PCT
                || z.high_pct > ZONE_MAX_PCT
                || z.low_pct >= z.high_pct
            {
                return Err(format!(
                    "Step {}: zone must be between 30 and 100 % of maximum heart rate, low below high",
                    n
                ));
            }
        }
    }
    if let Some(r) = p.repeat_every_minutes {
        if !ok(r) {
            return Err("Repeat interval must be zero or more".to_string());
        }
        if p.steps[last].duration_minutes.is_none() {
            return Err("A step that runs until ended can't repeat".to_string());
        }
        // A cycle starts every `r` minutes and the rest is what is left after
        // the steps, so a spacing no longer than the walk would leave no rest
        // (Workday: the spacing must be longer than the break).
        if r <= total_minutes(p) {
            return Err("Repeat interval must be longer than the program".to_string());
        }
    }
    Ok(())
}

/// The examples a new install starts with. Values and sources are in
/// docs/research.md. Existing installs were moved to these by
/// `persistence::migration_3_research_programs` and
/// `persistence::migration_8_seeded_programs`, which keep their own copies;
/// `migration_10_step_kinds` gives unedited ones the step kinds and
/// `migration_11_cool_down_pulse` VO2 max's cool-down its pulse target.
/// The ids match what a v1.6 install ends up with after every migration
/// (id 4 was Micro-break, merged into Workday), so a migrated install and
/// a new one hold the same list.
pub fn seed_programs() -> Vec<Program> {
    let program =
        |id: u32, name: &str, step: ProgramStep, repeat: Option<f32>, note: &str| Program {
            id,
            name: name.to_string(),
            steps: vec![step],
            repeat_every_minutes: repeat,
            note: Some(note.to_string()),
        };
    // Every one-step example is a plain walk.
    let step = |minutes: Option<f32>, speed_kmh: f32, incline_pct: f32| ProgramStep {
        duration_minutes: minutes,
        speed_kmh,
        incline_pct,
        zone: None,
        kind: Some(StepKind::Walk),
        pulse_end: None,
    };
    vec![
        program(
            1,
            "Post-meal",
            step(Some(10.0), 4.0, 5.0),
            None,
            NOTE_POST_MEAL,
        ),
        program(
            2,
            "Workday",
            step(Some(WORKDAY_MINUTES), WORKDAY_KMH, WORKDAY_INCLINE_PCT),
            Some(WORKDAY_EVERY_MINUTES),
            NOTE_WORKDAY,
        ),
        program(3, "Focus", step(None, 2.0, 0.0), None, NOTE_FOCUS),
        program(5, "Brisk", step(Some(30.0), 5.0, 0.0), None, NOTE_BRISK),
        vo2_max_program(6),
        interval_walk_program(7),
    ]
}

/// Workday's two settings are the program's own fields: the break length
/// is its one step's minutes and the spacing is `repeat_every_minutes`, both
/// edited like any program's, with speed and incline. The defaults are
/// Duran et al. (2023)'s best dose: 5 minutes of light walking every 30 was
/// the only one of their four doses that lowered the glucose rise after
/// meals. The limits are the ones every program has (`validate_program`):
/// the spacing must be longer than the break, since the rest between breaks
/// is the spacing minus the break.
const WORKDAY_MINUTES: f32 = 5.0;
const WORKDAY_EVERY_MINUTES: f32 = 30.0;
/// Light walking, about 3.7 METs by the ACSM equation, in line with the
/// trials' light breaks (docs/research.md).
const WORKDAY_KMH: f32 = 3.0;
const WORKDAY_INCLINE_PCT: f32 = 5.0;

/// The Norwegian 4x4 for the deck: warm-up, four 4-minute climbs each followed
/// by 3 minutes of easy walking, cool-down. The deck tops out at 8 km/h and
/// 10 %, so the hard part is the incline (docs/research.md). Each climb holds
/// 85 to 95 % of maximum heart rate, the intensity the 4x4 is prescribed at:
/// with a heart rate reading the zone trims the climb's speed within 1 km/h,
/// and without one the climb runs its set speed. Existing installs get the
/// program from `persistence::migration_4_vo2_max_program`, the zones from
/// `migration_7_vo2_max_zones` and the cool-down's pulse target from
/// `migration_11_cool_down_pulse`, which keep their own copies.
fn vo2_max_program(id: u32) -> Program {
    let step = |minutes: f32, speed_kmh: f32, incline_pct: f32, zone, kind| ProgramStep {
        duration_minutes: Some(minutes),
        speed_kmh,
        incline_pct,
        zone,
        kind: Some(kind),
        pulse_end: None,
    };
    let climb_zone = Some(HeartZone {
        low_pct: 85.0,
        high_pct: 95.0,
        trim: None,
    });
    let mut steps = vec![step(5.0, 4.5, 0.0, None, StepKind::WarmUp)];
    for _ in 0..4 {
        steps.push(step(4.0, 6.0, 10.0, climb_zone, StepKind::Interval));
        steps.push(step(3.0, 4.0, 0.0, None, StepKind::Rest));
    }
    // The cool-down walks on past its minutes until the pulse is down, at
    // the one target every cool-down gets (docs/research.md).
    steps.push(ProgramStep {
        pulse_end: Some(PulseEnd::default_target()),
        ..step(3.0, 3.0, 0.0, None, StepKind::CoolDown)
    });
    Program {
        id,
        name: "VO2 max".to_string(),
        steps,
        repeat_every_minutes: None,
        note: Some(NOTE_VO2_MAX.to_string()),
    }
}

/// Interval walking training (IWT, "Japanese walking"; Nemoto et al. 2007):
/// a warm-up, then five rounds of 3 brisk minutes and 3 easy ones, 35 minutes
/// in all. Every step moves the belt and nothing repeats, so no step starts a
/// still belt: running it is the one press, like VO2 max. Existing installs
/// get it from `persistence::migration_8_seeded_programs`,
/// which keeps its own copy.
fn interval_walk_program(id: u32) -> Program {
    let step = |minutes: f32, speed_kmh: f32, incline_pct: f32, kind| ProgramStep {
        duration_minutes: Some(minutes),
        speed_kmh,
        incline_pct,
        zone: None,
        kind: Some(kind),
        pulse_end: None,
    };
    let mut steps = vec![step(
        INTERVAL_WARMUP_MINUTES,
        INTERVAL_WARMUP_KMH,
        0.0,
        StepKind::WarmUp,
    )];
    // The easy minutes are the rests between brisk ones; the last easy round
    // is a rest too, so the rounds count 1/5 to 5/5 in step.
    for _ in 0..INTERVAL_ROUNDS {
        steps.push(step(
            INTERVAL_MINUTES,
            INTERVAL_BRISK_KMH,
            INTERVAL_BRISK_INCLINE_PCT,
            StepKind::Interval,
        ));
        steps.push(step(
            INTERVAL_MINUTES,
            INTERVAL_EASY_KMH,
            0.0,
            StepKind::Rest,
        ));
    }
    Program {
        id,
        name: "Interval walk".to_string(),
        steps,
        repeat_every_minutes: None,
        note: Some(NOTE_INTERVAL_WALK.to_string()),
    }
}

/// A short warm-up at an ordinary walking pace before the first brisk round.
const INTERVAL_WARMUP_MINUTES: f32 = 5.0;
const INTERVAL_WARMUP_KMH: f32 = 4.0;
/// IWT asks for 5 or more rounds a day; five is the trial's minimum dose.
const INTERVAL_ROUNDS: usize = 5;
/// Equal 3 minute blocks, brisk then easy, as in the trial.
const INTERVAL_MINUTES: f32 = 3.0;
/// Brisk: about 6.4 METs by the ACSM equation, inside the 6 to 7 METs aimed
/// at for a fast walk and well under VO2 max's 9. The deck tops out at
/// 8 km/h, so a fast walk gets its effort from a moderate incline.
const INTERVAL_BRISK_KMH: f32 = 6.0;
const INTERVAL_BRISK_INCLINE_PCT: f32 = 5.0;
/// Easy: about 2.7 METs, flat, a slow walk to recover.
const INTERVAL_EASY_KMH: f32 = 3.5;

pub const NOTE_VO2_MAX: &str =
    "Four hard climbs train VO2 max, and the last walk ends on heart rate.";
pub const NOTE_POST_MEAL: &str = "Walking soon after a meal blunts the rise in blood sugar.";
pub const NOTE_WORKDAY: &str = "Five easy minutes every half hour keep the body online.";
pub const NOTE_FOCUS: &str = "A slow, flat walk keeps typing and thinking steady.";
pub const NOTE_BRISK: &str = "Thirty brisk minutes count toward a weekly 150 of moderate activity.";
pub const NOTE_INTERVAL_WALK: &str =
    "Five rounds alternate three brisk minutes with three easy ones.";

/// Test fixture: the three programs v1.6 seeded (Post-meal 10 min at 4.0 km/h
/// and 10 %, Workday 5 min every 30 at 3.0 km/h and 5 %, Focus at 2.5 km/h and
/// 5 % until ended), with ids 1 to 3. The engine and program tests are written
/// against these, so they don't move when the seeds do.
#[cfg(test)]
pub(crate) fn classic_programs() -> Vec<Program> {
    let program = |id, name: &str, minutes, speed_kmh, incline_pct, repeat| Program {
        id,
        name: name.to_string(),
        steps: vec![ProgramStep {
            duration_minutes: minutes,
            speed_kmh,
            incline_pct,
            zone: None,
            kind: None,
            pulse_end: None,
        }],
        repeat_every_minutes: repeat,
        note: None,
    };
    vec![
        program(1, "Post-meal", Some(10.0), 4.0, 10.0, None),
        program(2, "Workday", Some(5.0), 3.0, 5.0, Some(30.0)),
        program(3, "Focus", None, 2.5, 5.0, None),
    ]
}

pub struct LoadedPrograms {
    pub programs: Vec<Program>,
    pub next_id: u32,
    /// True when the key had never been written and the examples were created.
    pub seeded: bool,
}

/// Decide what to load from the raw stored values. Seeds the examples only
/// when the `programs` key is absent; a present-but-unreadable value is NOT
/// reseeded (that would overwrite the user's data on the next save).
fn initial_programs(stored: Option<&str>, stored_next_id: Option<&str>) -> LoadedPrograms {
    let (programs, seeded) = match stored {
        None => (seed_programs(), true),
        Some(raw) => match serde_json::from_str::<Vec<Program>>(raw) {
            Ok(list) => (list, false),
            Err(e) => {
                log::warn!("Stored programs unreadable, starting empty: {}", e);
                (Vec::new(), false)
            }
        },
    };
    let floor = programs.iter().map(|p| p.id).max().unwrap_or(0) + 1;
    let next_id = stored_next_id
        .and_then(|s| s.trim().trim_matches('"').parse::<u32>().ok())
        .unwrap_or(floor)
        .max(floor);
    LoadedPrograms {
        programs,
        next_id,
        seeded,
    }
}

// ---------------------------------------------------------------------------
// Persistence
// ---------------------------------------------------------------------------

pub fn persist(app: &tauri::AppHandle, programs: &[Program], next_id: u32) -> Result<(), String> {
    let json = serde_json::to_string(programs).map_err(|e| e.to_string())?;
    persistence::write_setting(app, KEY_PROGRAMS, &json)?;
    persistence::write_setting(app, KEY_NEXT_ID, &next_id.to_string())
}

pub fn persist_last_run(app: &tauri::AppHandle, id: Option<u32>) -> Result<(), String> {
    let v = id.map(|i| i.to_string()).unwrap_or_default();
    persistence::write_setting(app, KEY_LAST_RUN, &v)
}

/// Load programs into the orchestrator at startup (after init_db).
pub async fn init(app: &tauri::AppHandle) {
    let stored = persistence::read_setting(app, KEY_PROGRAMS);
    let stored_next = persistence::read_setting(app, KEY_NEXT_ID);
    let loaded = initial_programs(stored.as_deref(), stored_next.as_deref());
    if loaded.seeded {
        if let Err(e) = persist(app, &loaded.programs, loaded.next_id) {
            log::error!("Failed to persist seeded programs: {}", e);
        }
    }
    let last_run = persistence::read_setting(app, KEY_LAST_RUN)
        .and_then(|s| s.trim().trim_matches('"').parse::<u32>().ok());

    let saved: Option<SavedRun> = persistence::read_setting(app, KEY_ACTIVE_RUN)
        .and_then(|s| serde_json::from_str::<Option<SavedRun>>(&s).ok())
        .flatten();

    let mut state = ORCHESTRATOR.lock().await;
    state.programs = loaded.programs;
    state.program_next_id = loaded.next_id;
    state.program_last_run_id = last_run;

    if let Some(saved) = saved {
        let now_ms = crate::util::now_ms();
        let now = state.now();
        match state.restore_run(&saved, now_ms, now) {
            Ok(()) => log::info!("Resumed program {} after restart", saved.program_id),
            Err(why) => log::info!("Not resuming program {}: {}", saved.program_id, why),
        }
    }
}

// ---------------------------------------------------------------------------
// Runtime
// ---------------------------------------------------------------------------

impl OrchestratorState {
    /// A program is in a Walking phase (running or paused). Manual nudges are
    /// routed to the live step target while this holds.
    pub fn program_walking(&self) -> bool {
        matches!(&self.program, Some(r) if r.phase == ProgramPhase::Walking)
    }

    /// Neural flow eases any running belt, manual or program, so key presses
    /// are counted exactly while it is enabled and the belt is running.
    pub fn neural_monitor_wanted(&self) -> bool {
        self.focus_stability_enabled && self.last_status == MachineStatus::Running
    }

    fn step_intent(&self, step: &ProgramStep) -> ControlIntent {
        ControlIntent {
            speed: step.speed_kmh,
            incline: self.snap_incline_level(incline_pct_to_level(
                step.incline_pct,
                self.tilt_multiplier,
                self.incline_offset,
                self.max_incline,
            )),
        }
    }

    fn capture_previous(&self) -> PreviousState {
        let moving = matches!(
            self.last_status,
            MachineStatus::Running | MachineStatus::Starting
        );
        let moving = if moving {
            Some(
                self.manual_intent
                    .filter(|i| i.speed >= MOVING_KMH)
                    .unwrap_or_else(|| ControlIntent {
                        speed: self
                            .to_physical_speed(self.hardware_speed)
                            .max(self.min_physical_speed()),
                        incline: self.hardware_incline,
                    }),
            )
        } else {
            None
        };
        PreviousState {
            status: self.last_status,
            moving,
        }
    }

    /// Hand control back: a belt that was moving gets its speed/incline back,
    /// a stopped or paused one gets speed 0 (the engine pauses it). `still`:
    /// the belt isn't moving now (the program was paused, waiting for GO, or
    /// on a 0 km/h step). Then nothing moves: the speed from before waits in
    /// `engage_speed` for the next Engage or RE-JACK.
    fn restore_previous(&mut self, prev: PreviousState, still: bool, now: Instant) {
        self.program_intent = None;
        match prev.moving {
            Some(intent) if still => {
                log::info!("Program ended on a still belt: it stays still until Engage");
                self.manual_intent = Some(ControlIntent {
                    speed: 0.0,
                    incline: intent.incline,
                });
                self.engage_speed = Some(intent.speed);
            }
            Some(intent) => {
                self.manual_intent = Some(intent);
                self.kick_resume();
            }
            None => {
                let incline = self.manual_intent.map(|m| m.incline).unwrap_or(0);
                self.manual_intent = Some(ControlIntent {
                    speed: 0.0,
                    incline,
                });
            }
        }
        self.last_manual_interaction = now;
    }

    /// Mirror intent for the current run into `program_intent` (speed 0 while paused).
    fn set_program_intent_from(&mut self, run: &ProgramRun) {
        if run.phase != ProgramPhase::Walking {
            return;
        }
        self.program_intent = Some(ControlIntent {
            speed: if run.paused { 0.0 } else { run.target.speed },
            incline: run.target.incline,
        });
    }

    /// resolve_target lets manual intent win for `MANUAL_LOCKOUT` after the last manual
    /// interaction; push that timestamp into the past so the program applies now.
    fn prioritize_program(&mut self, now: Instant) {
        self.last_manual_interaction = now
            .checked_sub(crate::engine::MANUAL_LOCKOUT + Duration::from_secs(1))
            .unwrap_or(now);
    }

    /// A paused deck only auto-resumes when the target exceeds the engine's
    /// last_physical_speed, which is stale after an auto-pause. Prime the same
    /// start handshake the engine's auto-resume uses. Never on a connection
    /// that is holding automatic starts (`hold_auto_start`).
    fn kick_resume(&mut self) {
        if self.last_status == MachineStatus::Paused
            && !self.stop_requested
            && !self.hold_auto_start
            && self.handshake_step == crate::protocol::HandshakeStep::Idle
            && self.resolve_target().0 >= MOVING_KMH
        {
            self.handshake_step = crate::protocol::HandshakeStep::Priming;
        }
    }

    /// The belt is still, or on its way there: the deck doesn't report it
    /// moving, or what the app asks for right now holds it still.
    fn belt_still(&self, now: Instant) -> bool {
        !self.last_status.is_moving() || self.resolve_base(now).0 < MOVING_KMH
    }

    /// The program's next step would start a still belt, and nobody pressed
    /// anything: pause it there until GO, the same way a program waits on a
    /// connection that holds automatic starts (`hold_auto_start`, which this
    /// sets too). The caller decides what the step timer reads.
    fn wait_for_go(&mut self, run: &mut ProgramRun, why: ProgramWait, now: Instant) {
        log::info!(
            "Program {} would start a still belt ({:?}): waiting for GO",
            run.program.id,
            why
        );
        run.paused = true;
        run.wait = Some(why);
        run.last_tick = now;
        run.deck_start = DeckStartWatch::default();
        run.deck_moved = false;
        self.set_program_intent_from(run);
        self.manual_intent = Some(ControlIntent {
            speed: 0.0,
            incline: run.target.incline,
        });
        self.last_manual_interaction = now;
        self.hold_auto_start = true;
    }

    /// SKIP took a still belt onto a walking step: that press is the user
    /// asking for the belt, so it starts like GO.
    fn start_on_press(&mut self, now: Instant) {
        self.engage_speed = None;
        self.prioritize_program(now);
        self.arm_start_watch(now);
        self.kick_resume();
    }

    /// A start at the treadmill (its own buttons or remote) while the program
    /// holds the belt still is a press. On a paused program, one waiting for
    /// GO included, it is the program's GO: the program resumes its step
    /// (`resume_program`) and sets the step's speed on the moving belt. During
    /// a rest the belt becomes a manual walk at the deck's speed
    /// (`adopt_physical_start`, which this lets act at once instead of after
    /// the manual lockout of the rest's own pause).
    ///
    /// It counts only when the deck reported the belt still after the hold
    /// began, then moving for `DECK_GO_DEBOUNCE`, and the app sent no start
    /// in the meantime. The engine calls this before deciding what to send;
    /// it returns true while a start is being confirmed, and the engine then
    /// leaves the belt alone instead of pausing it.
    /// A pause at the treadmill (its own buttons or remote, or the deck's
    /// Pause sent from a shortcut) while the program walks a moving step: the
    /// program pauses with it, the same as its own pause. Its step timer
    /// stops, the HUD and dashboard offer GO, and GO (or a start at the
    /// treadmill, `watch_for_deck_start`) resumes the step. Without this the
    /// program walked on against a paused belt, and GO had nothing to resume.
    ///
    /// It counts only once the deck has reported the belt moving since the
    /// run last started or resumed (`ProgramRun::deck_moved`): until then a
    /// Paused report is the deck not yet answering GO. A reconnect that holds
    /// automatic starts is left to the program's own wait (`BeltStill`), and a
    /// run restored after a restart to its re-apply. Nothing is sent: the deck
    /// is paused already, and the program's paused target is zero.
    pub(crate) fn notice_deck_pause(&mut self, now: Instant) {
        if self.awaiting_first_packet || self.stop_requested || self.hold_auto_start {
            return;
        }
        let moving = self.last_status.is_moving();
        let deck_paused = self.last_status == MachineStatus::Paused;
        let Some(run) = self.program.as_mut() else {
            return;
        };
        // A 0 km/h step pauses the belt itself: that pause is the program's.
        if run.phase != ProgramPhase::Walking
            || run.paused
            || run.restored
            || run.target.speed < MOVING_KMH
        {
            run.deck_moved = false;
            return;
        }
        if moving {
            run.deck_moved = true;
            return;
        }
        if !deck_paused || !run.deck_moved {
            return;
        }
        log::info!(
            "Belt paused at the treadmill: pausing program {}",
            run.program.id
        );
        if let Err(e) = self.pause_program(now) {
            log::warn!("Pause at the treadmill: {}", e);
        }
    }

    pub(crate) fn watch_for_deck_start(&mut self, now: Instant) -> bool {
        let moving = self.last_status.is_moving();
        let app_start = self.handshake_step != crate::protocol::HandshakeStep::Idle
            || self
                .start_sent_at
                .is_some_and(|t| now.saturating_duration_since(t) < APP_START_IN_FLIGHT);
        if moving || !app_start {
            // Answered (or given up on): later motion isn't the app's start.
            self.start_sent_at = None;
        }
        if self.awaiting_first_packet || self.stop_requested {
            return false;
        }
        let Some(run) = self.program.as_mut() else {
            return false;
        };
        let holding = match run.phase {
            ProgramPhase::Walking => run.paused,
            ProgramPhase::Resting => true,
        };
        if !holding {
            return false;
        }
        let watch = &mut run.deck_start;
        if !moving {
            watch.still_seen = true;
            watch.moving_since = None;
            return false;
        }
        if !watch.still_seen {
            return false;
        }
        if app_start {
            // The app's own start (Engage or GO in flight): not a press here.
            *watch = DeckStartWatch::default();
            return false;
        }
        let since = *watch.moving_since.get_or_insert(now);
        if now.saturating_duration_since(since) < DECK_GO_DEBOUNCE {
            return true;
        }
        *watch = DeckStartWatch::default();
        let id = run.program.id;
        match run.phase {
            ProgramPhase::Walking => {
                log::info!("Belt started at the treadmill: GO for program {}", id);
                let toast = ProgramToast::deck_go(&run.program.name);
                match self.resume_program(now) {
                    // One toast for this GO: the watch starts over, and only
                    // a new pause can arm it again.
                    Ok(()) => self.program_toast = Some(toast),
                    Err(e) => log::warn!("Start at the treadmill: {}", e),
                }
            }
            ProgramPhase::Resting => {
                log::info!(
                    "Belt started at the treadmill during program {}'s rest: a manual walk",
                    id
                );
                // The press is newer than the rest's own pause.
                if self.manual_intent.is_none_or(|i| i.speed < MOVING_KMH) {
                    self.last_manual_interaction = now
                        .checked_sub(crate::engine::MANUAL_LOCKOUT)
                        .unwrap_or(now);
                }
            }
        }
        false
    }

    /// The program toast raised since the last call, if any (the worker sends
    /// it to the HUD right after the engine's tick).
    pub(crate) fn take_program_toast(&mut self) -> Option<ProgramToast> {
        self.program_toast.take()
    }

    /// Start a program immediately, replacing any running one without restoring
    /// in between (the replaced run's captured previous state is kept).
    pub fn run_program(&mut self, id: u32, now: Instant) -> Result<(), String> {
        let program = self
            .programs
            .iter()
            .find(|p| p.id == id)
            .cloned()
            .ok_or_else(|| "Program not found".to_string())?;
        let first = program
            .steps
            .first()
            .ok_or_else(|| "Program has no steps".to_string())?;
        let target = self.step_intent(first);
        let previous = match &self.program {
            Some(r) if r.phase == ProgramPhase::Walking => r.previous,
            _ => self.capture_previous(),
        };
        let run = ProgramRun {
            program,
            phase: ProgramPhase::Walking,
            step_index: 0,
            cycle: 1,
            paused: false,
            wait: None,
            elapsed_s: 0.0,
            last_tick: now,
            target,
            previous,
            rest_warned: false,
            restored: false,
            saved_at: None,
            deck_start: DeckStartWatch::default(),
            deck_moved: false,
            zone_next_trim: None,
            zone_settle_s: 0.0,
            pulse_under_s: 0.0,
            pulse_held: false,
        };
        self.set_program_intent_from(&run);
        self.program = Some(run);
        self.program_last_run_id = Some(id);
        self.prioritize_program(now);
        // Running a program is the user asking for the belt; it replaces the
        // speed a previous program left for the next Engage.
        self.hold_auto_start = false;
        self.engage_speed = None;
        // Starting a program on a stopped or paused deck is a start: watch for the answer.
        if target.speed >= MOVING_KMH {
            self.arm_start_watch(now);
        }
        self.kick_resume();
        log::info!("Run program {}", id);
        Ok(())
    }

    /// End the running program. `restore` puts the previous state back (not
    /// wanted for a deck Stop). Returns whether one was running.
    pub fn end_program(&mut self, restore: bool, now: Instant) -> bool {
        let still = self.belt_still(now);
        let Some(run) = self.program.take() else {
            return false;
        };
        match run.phase {
            ProgramPhase::Walking if restore => self.restore_previous(run.previous, still, now),
            ProgramPhase::Walking => self.program_intent = None,
            // Already restored when the rest began; don't clobber what the user did since.
            ProgramPhase::Resting => {}
        }
        log::info!("Program ended (restore={})", restore);
        true
    }

    pub fn pause_program(&mut self, now: Instant) -> Result<(), String> {
        let run = self
            .program
            .as_mut()
            .ok_or_else(|| "No program is running".to_string())?;
        if run.phase != ProgramPhase::Walking {
            return Err("Nothing to pause while resting".to_string());
        }
        if run.paused {
            return Ok(());
        }
        run.paused = true;
        run.last_tick = now;
        run.deck_start = DeckStartWatch::default();
        run.deck_moved = false;
        // The pulse falls while the belt is still; a zone waits again, and a
        // cool-down's pulse must stay down again once the belt walks.
        run.zone_settle_s = 0.0;
        run.pulse_under_s = 0.0;
        let incline = run.target.incline;
        let run = run.clone();
        self.set_program_intent_from(&run);
        self.manual_intent = Some(ControlIntent {
            speed: 0.0,
            incline,
        });
        self.last_manual_interaction = now;
        Ok(())
    }

    pub fn resume_program(&mut self, now: Instant) -> Result<(), String> {
        let run = self
            .program
            .as_mut()
            .ok_or_else(|| "No program is running".to_string())?;
        if run.phase != ProgramPhase::Walking {
            return Err("Nothing to resume while resting".to_string());
        }
        if !run.paused {
            return Ok(());
        }
        run.paused = false;
        run.wait = None;
        run.last_tick = now;
        // GO is a step up in effort from a still belt: a zone waits for the
        // pulse again, however short the pause was.
        run.zone_settle_s = 0.0;
        run.pulse_under_s = 0.0;
        let run = run.clone();
        self.set_program_intent_from(&run);
        self.prioritize_program(now);
        // GO is the user asking for the belt: like Engage, watch for the deck
        // to answer (a deck asleep after a long wait says so).
        self.hold_auto_start = false;
        self.engage_speed = None;
        if run.target.speed >= MOVING_KMH {
            self.arm_start_watch(now);
        }
        self.kick_resume();
        Ok(())
    }

    /// Next step; from the last step to Resting (repeating) or finish; from
    /// Resting straight to the next cycle. SKIP is a press: onto a walking
    /// step from a still belt (out of a rest, or past a 0 km/h step) it starts
    /// the belt. A paused program stays paused.
    pub fn skip_program(&mut self, now: Instant) -> Result<(), String> {
        let run = self
            .program
            .take()
            .ok_or_else(|| "No program is running".to_string())?;
        self.program = self.advance(run, 0.0, true, now);
        Ok(())
    }

    /// Move to the phase after the current one, carrying `leftover` seconds.
    /// None means the program finished (and the previous state was restored).
    /// `pressed`: SKIP, not the step timer. Without a press, a step that would
    /// start a still belt waits for GO instead (`wait_for_go`).
    fn advance(
        &mut self,
        mut run: ProgramRun,
        leftover: f64,
        pressed: bool,
        now: Instant,
    ) -> Option<ProgramRun> {
        let still = self.belt_still(now);
        // Every new step is a new effort: its zone waits for the pulse, and a
        // cool-down counts its pulse from the step's start.
        run.zone_settle_s = 0.0;
        run.pulse_under_s = 0.0;
        run.pulse_held = false;
        match run.phase {
            ProgramPhase::Walking => {
                if run.step_index + 1 < run.program.steps.len() {
                    run.step_index += 1;
                    run.target = self.step_intent(&run.program.steps[run.step_index]);
                    run.elapsed_s = leftover;
                    let starts = still && run.target.speed >= MOVING_KMH;
                    if run.paused {
                        // Still paused (by the user, or waiting for GO): a hold
                        // step has nothing to wait for.
                        if run.target.speed < MOVING_KMH {
                            run.wait = None;
                        }
                        self.set_program_intent_from(&run);
                    } else if starts && !pressed {
                        run.elapsed_s = 0.0;
                        self.wait_for_go(&mut run, ProgramWait::HoldComplete, now);
                    } else {
                        self.set_program_intent_from(&run);
                        if starts {
                            self.start_on_press(now);
                        }
                    }
                    log::debug!("Step {}", run.step_index + 1);
                    Some(run)
                } else if run.program.repeat_every_minutes.is_some() {
                    run.phase = ProgramPhase::Resting;
                    run.step_index = 0;
                    run.target = self.step_intent(&run.program.steps[0]);
                    run.paused = false;
                    run.wait = None;
                    run.elapsed_s = leftover;
                    run.rest_warned = false;
                    run.deck_start = DeckStartWatch::default();
                    self.restore_previous(run.previous, still, now);
                    log::info!("Resting after cycle {}", run.cycle);
                    Some(run)
                } else {
                    self.restore_previous(run.previous, still, now);
                    log::info!("Program complete");
                    None
                }
            }
            ProgramPhase::Resting => {
                run.cycle += 1;
                run.previous = self.capture_previous();
                run.phase = ProgramPhase::Walking;
                run.step_index = 0;
                run.target = self.step_intent(&run.program.steps[0]);
                run.paused = false;
                run.wait = None;
                run.elapsed_s = leftover;
                let starts = still && run.target.speed >= MOVING_KMH;
                if starts && !pressed {
                    // The rest ran out on a still belt: the next cycle waits
                    // for GO, its step timer at zero.
                    run.elapsed_s = 0.0;
                    self.wait_for_go(&mut run, ProgramWait::RestComplete, now);
                } else {
                    self.set_program_intent_from(&run);
                    self.prioritize_program(now);
                    if starts {
                        self.start_on_press(now);
                    } else {
                        self.kick_resume();
                    }
                }
                log::info!("Cycle {}", run.cycle);
                Some(run)
            }
        }
    }

    /// 1 Hz tick. Returns HUD toast text: once when a rest is 30 s from
    /// ending, and once when the program starts waiting for GO.
    pub fn tick_program(&mut self, now: Instant) -> Option<ProgramToast> {
        let mut run = self.program.take()?;
        let waiting_before = run.wait.is_some();

        // Something outside the program (deck stop, connection reset) wiped the
        // intents: the program is over; don't restart the belt. A run restored
        // after a restart expects one such wipe from the reconnect.
        if run.phase == ProgramPhase::Walking && self.program_intent.is_none() {
            if run.restored {
                self.set_program_intent_from(&run);
                self.prioritize_program(now);
                self.kick_resume();
                log::info!("Re-applied restored program after reconnect");
            } else {
                log::warn!("Intents cleared externally; ending program");
                return None;
            }
        }
        // The deck has reported since the reconnect and the target stands.
        if run.restored && !self.awaiting_first_packet && self.program_intent.is_some() {
            run.restored = false;
            // Found still counting (the belt never stopped, as after an
            // update): the time the app was closed counts, so the step timer
            // keeps to the wall clock.
            let saved_at = run.saved_at.take();
            let counting_now = run.phase == ProgramPhase::Walking
                && !run.paused
                && (run.target.speed < MOVING_KMH || self.last_status == MachineStatus::Running);
            if let Some(at) = saved_at.filter(|at| counting_now && *at < run.last_tick) {
                log::info!(
                    "Restored program found running: counting the {:.0} s the app was closed",
                    now.saturating_duration_since(at).as_secs_f64()
                );
                run.last_tick = at;
            }
        }

        // The belt is still on a connection that holds automatic starts
        // (after a restart or reconnect), but the program wants it moving: the
        // program waits, paused, for GO instead of starting the belt itself.
        if run.phase == ProgramPhase::Walking
            && !run.paused
            && run.target.speed >= MOVING_KMH
            && self.hold_auto_start
            && !self.awaiting_first_packet
            && !self.last_status.is_moving()
        {
            // The step timer keeps what it had.
            self.wait_for_go(&mut run, ProgramWait::BeltStill, now);
        }

        let dt = now.saturating_duration_since(run.last_tick).as_secs_f64();
        run.last_tick = now;

        // Walk time only counts while the belt is actually running (not while
        // starting up, paused, or stopped), unless the step holds the belt
        // still anyway. Rest time always counts.
        let counting = match run.phase {
            ProgramPhase::Resting => true,
            ProgramPhase::Walking => {
                !run.paused
                    && (run.target.speed < MOVING_KMH || self.last_status == MachineStatus::Running)
            }
        };
        if counting {
            run.elapsed_s += dt;
        }
        // Only time with the belt running settles the pulse to the step.
        if run.phase == ProgramPhase::Walking
            && !run.paused
            && self.last_status == MachineStatus::Running
        {
            run.zone_settle_s += dt;
        }
        self.count_pulse_under(&mut run, dt);

        for _ in 0..64 {
            let Some(len) = run.phase_len_s() else { break };
            if run.elapsed_s < len {
                break;
            }
            if self.pulse_keeps_step(&run) {
                if !run.pulse_held {
                    log::info!("Cool-down minutes done: walking on until heart rate is down");
                }
                run.pulse_held = true;
                break;
            }
            // A step the pulse kept going ends now, not at its minutes: none
            // of the extra time belongs to the next step.
            let leftover = if run.pulse_held {
                0.0
            } else {
                run.elapsed_s - len
            };
            run = self.advance(run, leftover, false, now)?;
        }

        if let (false, Some(why)) = (waiting_before, run.wait) {
            self.program = Some(run);
            return Some(ProgramToast::WaitingForGo(why));
        }

        self.trim_speed_to_zone(&mut run, now);

        let mut warning = None;
        if run.phase == ProgramPhase::Resting && !run.rest_warned {
            if let Some(len) = run.phase_len_s() {
                // A rest shorter than the warning window can't be warned "30 s before".
                if len > REST_WARNING_S && len - run.elapsed_s <= REST_WARNING_S {
                    run.rest_warned = true;
                    warning = Some(ProgramToast::RestEndingSoon(format!(
                        "{} IN 30S",
                        run.program.name.to_uppercase()
                    )));
                }
            }
        }
        self.program = Some(run);
        warning
    }

    /// The walking step's cool-down pulse target in whole bpm (#233), or None
    /// for a step without one, and while resting.
    fn pulse_end_bpm(&self, run: &ProgramRun) -> Option<u32> {
        if run.phase != ProgramPhase::Walking {
            return None;
        }
        let end = run.program.steps.get(run.step_index)?.pulse_end?;
        Some(end.target_bpm(self.max_heart_rate))
    }

    /// Count the seconds in a row a cool-down step's pulse has been at or
    /// under its target with the belt running. Anything else starts the
    /// count again: no fresh reading, a reading over the target, a belt that
    /// isn't running (paused, starting, waiting for GO). Heart rate only
    /// decides when the step ends; it never moves the belt.
    fn count_pulse_under(&self, run: &mut ProgramRun, dt: f64) {
        let running =
            !run.paused && run.wait.is_none() && self.last_status == MachineStatus::Running;
        let down = match (self.pulse_end_bpm(run), self.heart_bpm) {
            (Some(target), Some(bpm)) => running && bpm <= target,
            _ => false,
        };
        if down {
            run.pulse_under_s += dt.min(PULSE_TICK_MAX_S);
        } else {
            run.pulse_under_s = 0.0;
        }
    }

    /// A cool-down step whose minutes are done keeps walking: while the user
    /// has it paused (GO carries on with it), or while a fresh reading says
    /// the pulse has not yet stayed down for `PULSE_END_HOLD`. With no reading
    /// (no sensor, or one that dropped out) the minutes end it, as before.
    /// No upper limit: SKIP and END end it at any time.
    fn pulse_keeps_step(&self, run: &ProgramRun) -> bool {
        if self.pulse_end_bpm(run).is_none() {
            return false;
        }
        if run.paused && run.pulse_held {
            return true;
        }
        self.heart_bpm.is_some() && run.pulse_under_s < PULSE_END_HOLD.as_secs_f64()
    }

    /// PULSE_RATE's cool-down target while a walking step has one (its
    /// minimum included), with `bpm` placed against it: Rust's label and
    /// sentence, and the seconds the pulse must still stay down.
    pub fn pulse_target(&self, bpm: Option<u32>) -> Option<crate::heart_rate::PulseTarget> {
        let run = self.program.as_ref()?;
        let target = self.pulse_end_bpm(run)?;
        let hold_left = PULSE_END_HOLD.as_secs_f64() - run.pulse_under_s;
        let hold_s = (hold_left > 0.0).then(|| hold_left.ceil() as u32);
        Some(crate::heart_rate::pulse_target(target, bpm, hold_s))
    }

    /// Hold the step's heart rate zone: above it ease the belt a notch, below
    /// it work it harder, within a range around the step's speed and incline.
    /// Heart rate only trims a belt that is already running: it never
    /// starts, resumes or stops it, and a missing reading changes nothing.
    /// A step waits `ZONE_SETTLE` of running first, so the pulse has caught
    /// up with the step. By the zone's `trims`: speed by 0.1 km/h, incline by
    /// one deck level, or both, incline first away from the step's settings
    /// and speed first back toward them. With STRIDE_FIT set (#227) a trim
    /// that would take the speed above it raises the incline instead, and
    /// easing off above it lowers the speed first, then takes back that extra
    /// incline (`fit_lever`); unset, nothing changes.
    fn trim_speed_to_zone(&mut self, run: &mut ProgramRun, now: Instant) {
        if run.phase != ProgramPhase::Walking
            || run.paused
            || run.wait.is_some()
            || self.last_status != MachineStatus::Running
        {
            return;
        }
        let Some(step) = run.program.steps.get(run.step_index).cloned() else {
            return;
        };
        let (Some(zone), Some(bpm)) = (step.zone, self.heart_bpm) else {
            return;
        };
        if step.speed_kmh < MOVING_KMH
            || run.zone_settle_s < ZONE_SETTLE.as_secs_f64()
            || run.zone_next_trim.is_some_and(|at| now < at)
        {
            return;
        }
        let (low, high) = zone.bounds_bpm(self.max_heart_rate);
        let bpm = bpm as f32;
        let harder = if bpm > high {
            false
        } else if bpm < low {
            true
        } else {
            return;
        };
        let trims = zone.trims(self.zone_incline_trim);
        let speed = self.zone_speed(&step, run.target.speed, harder);
        let incline = match trims {
            ZoneTrim::Speed => None,
            ZoneTrim::Incline | ZoneTrim::Both => {
                self.zone_incline_level(&step, run.target.incline, harder, ZONE_INCLINE_RANGE_PCT)
            }
        };
        // Back toward the step's own speed first; away from it only once
        // the incline can't go further.
        let speed_returns = (harder && run.target.speed < step.speed_kmh - 0.001)
            || (!harder && run.target.speed > step.speed_kmh + 0.001);
        let lever = match trims {
            ZoneTrim::Speed => speed.map(ZoneLever::Speed),
            ZoneTrim::Incline => incline.map(ZoneLever::Incline),
            ZoneTrim::Both if speed_returns => speed
                .map(ZoneLever::Speed)
                .or(incline.map(ZoneLever::Incline)),
            ZoneTrim::Both => incline
                .map(ZoneLever::Incline)
                .or(speed.map(ZoneLever::Speed)),
        };
        let lever = self.fit_lever(&step, run, trims, harder, lever);
        match lever {
            Some(ZoneLever::Speed(kmh)) => {
                log::info!("Heart rate zone: {} bpm, speed {:.1} km/h", bpm, kmh);
                run.target.speed = kmh;
                run.zone_next_trim = Some(now + ZONE_TRIM_EVERY);
            }
            Some(ZoneLever::Incline(level)) => {
                log::info!("Heart rate zone: {} bpm, incline level {}", bpm, level);
                run.target.incline = level;
                run.zone_next_trim = Some(now + ZONE_INCLINE_TRIM_EVERY);
            }
            // At its limits: look again after the speed trim's wait.
            None => {
                run.zone_next_trim = Some(now + ZONE_TRIM_EVERY);
                return;
            }
        }
        self.set_program_intent_from(run);
    }

    /// STRIDE_FIT's say in a zone's trim (#227): `lever` as the zone picked
    /// it, kept as is while STRIDE_FIT is unset. Working harder, a speed above
    /// STRIDE_FIT becomes one incline level up instead (none at the limit, so
    /// the speed never goes past it). Easing off, a speed above STRIDE_FIT
    /// comes down first, then incline above what the zone reaches by itself
    /// (the extra a harder trim added), before the zone's own choice.
    fn fit_lever(
        &self,
        step: &ProgramStep,
        run: &ProgramRun,
        trims: ZoneTrim,
        harder: bool,
        lever: Option<ZoneLever>,
    ) -> Option<ZoneLever> {
        use crate::calibration::stride_fit::exceeds;
        let fit = self.stride_fit_kmh;
        if fit.is_none() {
            return lever;
        }
        let fit_range = trims.own_incline_range_pct() + ZONE_FIT_INCLINE_PCT;
        if harder {
            return match lever {
                Some(ZoneLever::Speed(kmh)) if exceeds(kmh, fit) => self
                    .zone_incline_level(step, run.target.incline, true, fit_range)
                    .map(ZoneLever::Incline),
                other => other,
            };
        }
        if exceeds(run.target.speed, fit) {
            if let Some(kmh) = self.zone_speed(step, run.target.speed, false) {
                return Some(ZoneLever::Speed(kmh));
            }
        }
        let pct =
            |level: u8| level_to_incline_pct(level, self.tilt_multiplier, self.incline_offset);
        let base = pct(self.step_intent(step).incline);
        // 0.001 % is float slack, far under a level.
        if pct(run.target.incline) > base + trims.own_incline_range_pct() + 0.001 {
            if let Some(level) = self.zone_incline_level(step, run.target.incline, false, fit_range)
            {
                return Some(ZoneLever::Incline(level));
            }
        }
        lever
    }

    /// The speed one trim takes `current` to (`harder`: up), within
    /// `ZONE_TRIM_RANGE_KMH` of the step's speed and the belt's limits. None
    /// at the limit, or when the speed was set by hand outside the range: the
    /// zone leaves that alone rather than pull it back by up to a km/h at once.
    fn zone_speed(&self, step: &ProgramStep, current: f32, harder: bool) -> Option<f32> {
        // Step speeds are calibrated km/h, so the belt's limits are too.
        let floor = (step.speed_kmh - ZONE_TRIM_RANGE_KMH).max(self.min_physical_speed());
        let ceiling =
            (step.speed_kmh + ZONE_TRIM_RANGE_KMH).min(self.to_physical_speed(self.top_hw_speed()));
        // 0.001 km/h is float slack, far under the deck's 0.1 step.
        if current < floor - 0.001 || current > ceiling + 0.001 {
            return None;
        }
        let delta = if harder {
            ZONE_TRIM_KMH
        } else {
            -ZONE_TRIM_KMH
        };
        let speed = (current + delta).clamp(floor, ceiling);
        ((speed - current).abs() >= 0.001).then_some(speed)
    }

    /// The incline level one trim takes `current` to (`harder`: steeper): the
    /// next level the deck runs (DECK_PROFILE, up to the top level) within
    /// `range_pct` of the step's incline (`ZONE_INCLINE_RANGE_PCT`, wider
    /// above STRIDE_FIT). None at the limit, on a deck whose levels don't
    /// change the grade, or when the incline was set by hand outside the
    /// range (left alone, like the speed).
    fn zone_incline_level(
        &self,
        step: &ProgramStep,
        current: u8,
        harder: bool,
        range_pct: f32,
    ) -> Option<u8> {
        if !self.tilt_multiplier.is_finite() || self.tilt_multiplier.abs() < 0.01 {
            return None;
        }
        let pct =
            |level: u8| level_to_incline_pct(level, self.tilt_multiplier, self.incline_offset);
        let base = pct(self.step_intent(step).incline);
        // 0.001 % is float slack, far under a level.
        let within = |level: u8| (pct(level) - base).abs() <= range_pct + 0.001;
        if !within(current) {
            return None;
        }
        let mut levels = self.deck_incline_levels();
        if levels.is_empty() {
            levels = (0..=self.top_incline_level()).collect();
        }
        let now_pct = pct(current);
        let candidates = levels.into_iter().filter(|l| within(*l));
        if harder {
            candidates
                .filter(|l| pct(*l) > now_pct)
                .min_by(|a, b| pct(*a).total_cmp(&pct(*b)))
        } else {
            candidates
                .filter(|l| pct(*l) < now_pct)
                .max_by(|a, b| pct(*a).total_cmp(&pct(*b)))
        }
    }

    /// Adjust the live step target (not saved to the program). Returns false
    /// when no program is walking, so the caller edits the manual intent.
    pub fn adjust_program_target(&mut self, speed: Option<f32>, incline: Option<u8>) -> bool {
        if !self.program_walking() {
            return false;
        }
        // Taking the speed down to zero by hand is a pause, not a step edited
        // to 0 km/h: the step keeps its own speed, so Engage (GO) resumes it.
        // A step target of zero would pause the belt again right after Engage.
        if speed.is_some_and(|s| s < STILL_KMH) && self.pause_by_zero_speed(incline) {
            return true;
        }
        let Some(run) = self.program.as_mut() else {
            return false;
        };
        if let Some(s) = speed {
            run.target.speed = s;
        }
        if let Some(i) = incline {
            run.target.incline = i;
        }
        let run = run.clone();
        self.set_program_intent_from(&run);
        true
    }

    /// The hand set the walking step's speed to zero: pause the program and
    /// put the step's own speed back as its target. False when the step is
    /// itself a 0 km/h step (nothing to resume to) or no program is walking.
    fn pause_by_zero_speed(&mut self, incline: Option<u8>) -> bool {
        let Some(run) = self.program.as_ref() else {
            return false;
        };
        let Some(step) = run.program.steps.get(run.step_index) else {
            return false;
        };
        if step.speed_kmh < STILL_KMH {
            return false;
        }
        let step_speed = step.speed_kmh;
        if let Some(run) = self.program.as_mut() {
            run.target.speed = step_speed;
            if let Some(i) = incline {
                run.target.incline = i;
            }
        }
        self.pause_program(self.now()).is_ok()
    }

    pub fn active_program(&self) -> Option<ActiveProgram> {
        let run = self.program.as_ref()?;
        // A cool-down the pulse keeps going has no time left to count down.
        let pulse_to_bpm = self.pulse_end_bpm(run).filter(|_| run.pulse_held);
        let seconds_remaining = run
            .phase_len_s()
            .filter(|_| pulse_to_bpm.is_none())
            .map(|len| (len - run.elapsed_s).max(0.0).ceil() as u32);
        Some(ActiveProgram {
            program_id: run.program.id,
            name: run.program.name.clone(),
            phase: run.phase,
            step_index: run.step_index as u32,
            step_count: run.program.steps.len() as u32,
            seconds_remaining,
            cycle: run.cycle,
            paused: run.paused,
            wait: run.wait,
            speed_kmh: run.target.speed,
            incline_pct: level_to_incline_pct(
                run.target.incline,
                self.tilt_multiplier,
                self.incline_offset,
            ),
            kind: match run.phase {
                ProgramPhase::Walking => StepKindSpot::of_step(&run.program.steps, run.step_index),
                ProgramPhase::Resting => None,
            },
            pulse_to_bpm,
        })
    }

    /// The current step's heart rate zone in bpm while it is a walking step
    /// with a zone (paused or not), for PULSE_RATE: the chart's band, and
    /// whether `bpm` is under, in or over it, by the same bounds the trim uses.
    pub fn zone_band(&self, bpm: Option<u32>) -> Option<crate::heart_rate::ZoneBand> {
        let run = self.program.as_ref()?;
        if run.phase != ProgramPhase::Walking {
            return None;
        }
        let zone = run.program.steps.get(run.step_index)?.zone?;
        let (low, high) = zone.bounds_bpm(self.max_heart_rate);
        let band = crate::heart_rate::zone_band(low, high, bpm);
        Some(match self.zone_settle_left_s(run) {
            Some(left) => band.settling(left),
            None => band,
        })
    }

    /// Whole seconds a walking, unpaused zone step still waits for the pulse
    /// to settle before it trims; none once it trims (or can't: paused).
    fn zone_settle_left_s(&self, run: &ProgramRun) -> Option<u32> {
        if run.paused || run.wait.is_some() {
            return None;
        }
        let left = ZONE_SETTLE.as_secs_f64() - run.zone_settle_s;
        (left > 0.0).then(|| left.ceil() as u32)
    }

    /// Create (id 0) or replace a program after validating it.
    pub fn save_program(&mut self, mut program: Program) -> Result<(), String> {
        program.name = program.name.trim().to_string();
        let min_speed = self.min_physical_speed();
        let max_speed = self.to_physical_speed(self.top_hw_speed());
        // Each step at a speed the deck runs (DECK_PROFILE); 0 stays 0.
        for step in &mut program.steps {
            // 0.005: half the 0.01 km/h the floor is shown to, as in `validate`.
            if step.speed_kmh >= min_speed - 0.005 && step.speed_kmh <= max_speed {
                step.speed_kmh = self.snap_speed(step.speed_kmh);
            }
            // Speed is the default trim: stored as none, as before #126, so
            // a zone saved unchanged compares equal and keeps the note.
            if let Some(z) = step.zone.as_mut() {
                if z.trim == Some(ZoneTrim::Speed) {
                    z.trim = None;
                }
            }
        }
        validate_program(&program, min_speed, max_speed)?;
        if program.id == 0 {
            // Notes are Rust's; a new program has none.
            program.note = None;
            let floor = self.programs.iter().map(|p| p.id).max().unwrap_or(0) + 1;
            program.id = self.program_next_id.max(floor);
            self.program_next_id = program.id + 1;
            self.programs.push(program.clone());
        } else {
            let slot = self
                .programs
                .iter_mut()
                .find(|p| p.id == program.id)
                .ok_or_else(|| "Program not found".to_string())?;
            // The note survives a rename, not a change to what the program does.
            // A step's kind is a name too, so changing one keeps it.
            let unkinded = |steps: &[ProgramStep]| -> Vec<ProgramStep> {
                steps
                    .iter()
                    .map(|s| ProgramStep {
                        kind: None,
                        ..s.clone()
                    })
                    .collect()
            };
            let same_plan = unkinded(&slot.steps) == unkinded(&program.steps)
                && slot.repeat_every_minutes == program.repeat_every_minutes;
            program.note = if same_plan { slot.note.clone() } else { None };
            *slot = program.clone();
        }
        Ok(())
    }

    /// The deck's top incline in percent (DECK_PROFILE's ceiling or the max
    /// incline setting, whichever is lower): what FIT_TO_DECK may climb to.
    pub fn top_incline_pct(&self) -> f32 {
        level_to_incline_pct(
            self.top_incline_level(),
            self.tilt_multiplier,
            self.incline_offset,
        )
    }

    /// FIT_TO_DECK (#225): the dashboard's offers, one per unedited seeded
    /// program with a step above STRIDE_FIT. None while it is unset.
    pub fn fit_to_deck_offers(&self, fit_kmh: Option<f32>) -> Vec<fit_to_deck::FitToDeckOffer> {
        fit_to_deck::offers(&self.programs, fit_kmh, self.top_incline_pct())
    }

    /// FIT_TO_DECK, once the user confirmed: the program's steps above
    /// STRIDE_FIT run at it with the incline that keeps their effort, held to
    /// the deck's top incline. Refused unless the program still has an offer
    /// (unedited seed, a step above the fit), so a stale confirm changes
    /// nothing. The note stays: the program keeps its plan, at a speed the
    /// deck fits. Only the stored steps change; a running program keeps its
    /// own copy until it ends, and nothing starts or moves the belt.
    pub fn fit_program_to_deck(&mut self, id: u32, fit_kmh: Option<f32>) -> Result<(), String> {
        let fit = fit_kmh.ok_or("Set STRIDE_FIT in System, Calibration first.")?;
        let max_pct = self.top_incline_pct();
        let slot = self
            .programs
            .iter_mut()
            .find(|p| p.id == id)
            .ok_or_else(|| "Program not found".to_string())?;
        if fit_to_deck::offer(slot, fit, max_pct).is_none() {
            return Err(format!(
                "FIT_TO_DECK only fits a seeded program you haven't edited, with a step above STRIDE_FIT {fit:.1} km/h."
            ));
        }
        let (steps, changes) = fit_to_deck::fit_steps(&slot.steps, fit, max_pct);
        slot.steps = steps;
        log::info!(
            "FIT_TO_DECK: {} steps of program {} set to {:.1} km/h",
            changes.len(),
            id,
            fit
        );
        Ok(())
    }

    /// Delete a program, ending it first (with restore) if it is running.
    pub fn delete_program(&mut self, id: u32, now: Instant) -> Result<(), String> {
        let idx = self
            .programs
            .iter()
            .position(|p| p.id == id)
            .ok_or_else(|| "Program not found".to_string())?;
        if matches!(&self.program, Some(r) if r.program.id == id) {
            self.end_program(true, now);
        }
        self.programs.remove(idx);
        if self.program_last_run_id == Some(id) {
            self.program_last_run_id = None;
        }
        Ok(())
    }

    /// Alt+End: end the running program, otherwise run the last-run one.
    /// Returns the id that was started, if any.
    pub fn toggle_program(&mut self, now: Instant) -> Option<u32> {
        if self.end_program(true, now) {
            return None;
        }
        let id = self.program_last_run_id?;
        self.run_program(id, now).ok().map(|_| id)
    }
}

// ---------------------------------------------------------------------------
// Surviving restarts
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SavedRun {
    pub program_id: u32,
    pub phase: ProgramPhase,
    pub step_index: usize,
    pub cycle: u32,
    pub paused: bool,
    /// Absent in runs saved before waiting for GO existed.
    #[serde(default)]
    pub wait: Option<ProgramWait>,
    pub elapsed_s: f64,
    pub target: ControlIntent,
    pub previous: PreviousState,
    pub rest_warned: bool,
    pub saved_at_ms: i64,
}

/// What a saved run's shape is, apart from the ticking clock fields.
type RunShape = (
    u32,
    ProgramPhase,
    usize,
    u32,
    bool,
    Option<ProgramWait>,
    ControlIntent,
);

impl SavedRun {
    /// Everything except the ticking clock fields, to tell a real change (step,
    /// phase, pause, wait, nudge) from elapsed time passing.
    pub fn shape(&self) -> RunShape {
        (
            self.program_id,
            self.phase,
            self.step_index,
            self.cycle,
            self.paused,
            self.wait,
            self.target,
        )
    }

    /// The run holds the belt still (resting, paused, or waiting for GO), so
    /// putting it back can't move anything until a press.
    fn holds_belt_still(&self) -> bool {
        self.phase == ProgramPhase::Resting || self.paused
    }
}

impl OrchestratorState {
    pub fn saved_run(&self, now_ms: i64) -> Option<SavedRun> {
        let r = self.program.as_ref()?;
        Some(SavedRun {
            program_id: r.program.id,
            phase: r.phase,
            step_index: r.step_index,
            cycle: r.cycle,
            paused: r.paused,
            wait: r.wait,
            elapsed_s: r.elapsed_s,
            target: r.target,
            previous: r.previous,
            rest_warned: r.rest_warned,
            saved_at_ms: now_ms,
        })
    }

    /// Put a saved run back. Rejected when it is stale, its program was
    /// deleted, or its step no longer exists. Rest time advances by the time
    /// the app was down; walk time only once the deck is found still running
    /// on the reconnect (see `tick_program`). A run that holds the belt still
    /// keeps for hours (a program left waiting for GO over lunch is still
    /// there); a walking one only for minutes.
    pub(crate) fn restore_run(
        &mut self,
        saved: &SavedRun,
        now_ms: i64,
        now: Instant,
    ) -> Result<(), String> {
        let age = now_ms - saved.saved_at_ms;
        let max_age = if saved.holds_belt_still() {
            RESUME_STILL_MAX_AGE_MS
        } else {
            RESUME_MAX_AGE_MS
        };
        if !(0..=max_age).contains(&age) {
            return Err(format!("saved {} s ago", age / 1000));
        }
        let program = self
            .programs
            .iter()
            .find(|p| p.id == saved.program_id)
            .cloned()
            .ok_or_else(|| "program no longer exists".to_string())?;
        if saved.step_index >= program.steps.len() {
            return Err("program changed since".to_string());
        }
        let elapsed_s = match saved.phase {
            ProgramPhase::Resting => saved.elapsed_s + age as f64 / 1000.0,
            ProgramPhase::Walking => saved.elapsed_s,
        };
        let run = ProgramRun {
            program,
            phase: saved.phase,
            step_index: saved.step_index,
            cycle: saved.cycle,
            paused: saved.paused,
            wait: saved.wait,
            elapsed_s,
            last_tick: now,
            target: saved.target,
            previous: saved.previous,
            rest_warned: saved.rest_warned,
            restored: true,
            saved_at: now.checked_sub(Duration::from_millis(age as u64)),
            deck_start: DeckStartWatch::default(),
            deck_moved: false,
            zone_next_trim: None,
            zone_settle_s: 0.0,
            pulse_under_s: 0.0,
            pulse_held: false,
        };
        self.set_program_intent_from(&run);
        self.program = Some(run);
        self.prioritize_program(now);
        Ok(())
    }
}

/// Save the running program when it changes shape, or every few seconds
/// while it runs; clear it once nothing is running. `last` remembers the
/// previous save.
pub fn persist_active_run(
    app: &tauri::AppHandle,
    current: Option<SavedRun>,
    last: &mut Option<(Option<SavedRun>, Instant)>,
) {
    let due = match (last.as_ref(), current.as_ref()) {
        (None, _) => true,
        (Some((None, _)), None) => false,
        (Some((Some(p), at)), Some(c)) => {
            p.shape() != c.shape() || at.elapsed() >= ACTIVE_RUN_SAVE_EVERY
        }
        _ => true,
    };
    if !due {
        return;
    }
    let value = match &current {
        Some(run) => serde_json::to_string(run).unwrap_or_default(),
        None => "null".to_string(),
    };
    if let Err(e) = persistence::write_setting(app, KEY_ACTIVE_RUN, &value) {
        log::warn!("Could not save the running program: {}", e);
    }
    *last = Some((current, Instant::now()));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn secs(n: u64) -> Duration {
        Duration::from_secs(n)
    }

    fn state() -> OrchestratorState {
        OrchestratorState {
            programs: classic_programs(),
            program_next_id: 4,
            ..Default::default()
        }
    }

    /// Tick once per second from `from` (exclusive) to `to` (inclusive), in
    /// seconds after `t0`, collecting warnings.
    fn run_ticks(s: &mut OrchestratorState, t0: Instant, from: u64, to: u64) -> Vec<(u64, String)> {
        let mut warnings = Vec::new();
        for t in (from + 1)..=to {
            s.set_test_now(t0 + secs(t));
            if let Some(w) = s.tick_program(t0 + secs(t)) {
                warnings.push((t, w.message().to_string()));
            }
        }
        warnings
    }

    fn moving(s: &mut OrchestratorState, speed: f32, incline: u8) {
        s.last_status = MachineStatus::Running;
        s.manual_intent = Some(ControlIntent { speed, incline });
    }

    fn manual(s: &OrchestratorState) -> ControlIntent {
        s.manual_intent.expect("manual intent")
    }

    #[test]
    fn post_meal_counts_only_while_running_then_restores_moving_belt() {
        let mut s = state();
        moving(&mut s, 3.0, 2);
        let t0 = Instant::now();
        s.run_program(1, t0).unwrap();
        assert_eq!(
            s.program_intent,
            Some(ControlIntent {
                speed: 4.0,
                incline: 10
            })
        );
        assert_eq!(s.active_program().unwrap().seconds_remaining, Some(600));

        // Spinning up / paused / stopped do not eat walk time.
        s.last_status = MachineStatus::Starting;
        run_ticks(&mut s, t0, 0, 60);
        s.last_status = MachineStatus::Paused;
        run_ticks(&mut s, t0, 60, 120);
        assert_eq!(s.active_program().unwrap().seconds_remaining, Some(600));

        s.last_status = MachineStatus::Running;
        run_ticks(&mut s, t0, 120, 120 + 599);
        assert!(s.program.is_some());
        assert_eq!(s.active_program().unwrap().seconds_remaining, Some(1));
        run_ticks(&mut s, t0, 719, 720);
        assert!(s.program.is_none());
        assert_eq!(s.program_intent, None);
        assert_eq!(
            manual(&s),
            ControlIntent {
                speed: 3.0,
                incline: 2
            }
        );
    }

    #[test]
    fn post_meal_from_stopped_deck_restores_speed_zero() {
        let mut s = state();
        s.last_status = MachineStatus::Stopped;
        let t0 = Instant::now();
        s.run_program(1, t0).unwrap();
        s.last_status = MachineStatus::Running;
        run_ticks(&mut s, t0, 0, 600);
        assert!(s.program.is_none());
        assert_eq!(s.program_intent, None);
        assert_eq!(manual(&s).speed, 0.0);

        // A previously paused deck restores the same way.
        let mut s = state();
        s.last_status = MachineStatus::Paused;
        s.run_program(1, t0).unwrap();
        s.last_status = MachineStatus::Running;
        run_ticks(&mut s, t0, 0, 600);
        assert_eq!(manual(&s).speed, 0.0);
    }

    #[test]
    fn run_makes_program_win_immediately() {
        let mut s = state();
        moving(&mut s, 3.0, 2);
        s.last_manual_interaction = Instant::now(); // fresh manual input
        s.run_program(1, Instant::now()).unwrap();
        let (speed, incline, source) = s.resolve_target();
        assert_eq!(
            (speed, incline, source),
            (4.0, 10, crate::engine::TargetSource::Program)
        );
    }

    #[test]
    fn workday_cycles_rest_and_warns_once() {
        let mut s = state();
        moving(&mut s, 2.0, 0);
        let t0 = Instant::now();
        s.run_program(2, t0).unwrap();
        let a = s.active_program().unwrap();
        assert_eq!(
            (a.phase, a.cycle, a.seconds_remaining),
            (ProgramPhase::Walking, 1, Some(300))
        );

        // Walk 5 min -> Resting, previous (2.0 km/h) restored.
        let mut warnings = run_ticks(&mut s, t0, 0, 300);
        let a = s.active_program().unwrap();
        assert_eq!(a.phase, ProgramPhase::Resting);
        assert_eq!(a.cycle, 1);
        assert_eq!(a.seconds_remaining, Some(1500));
        assert_eq!(s.program_intent, None);
        assert_eq!(manual(&s).speed, 2.0);
        // While resting the dashboard sees the next cycle's first step.
        assert_eq!((a.speed_kmh, a.incline_pct), (3.0, 5.0));

        // Warning fires once, 30 s before the rest ends.
        warnings.extend(run_ticks(&mut s, t0, 300, 1799));
        assert_eq!(warnings, vec![(1770, "WORKDAY IN 30S".to_string())]);
        assert_eq!(s.active_program().unwrap().phase, ProgramPhase::Resting);

        // Cycle 2 starts at 30 min.
        run_ticks(&mut s, t0, 1799, 1800);
        let a = s.active_program().unwrap();
        assert_eq!(
            (a.phase, a.cycle, a.seconds_remaining),
            (ProgramPhase::Walking, 2, Some(300))
        );
        assert_eq!(
            s.program_intent,
            Some(ControlIntent {
                speed: 3.0,
                incline: 5
            })
        );

        // The second rest warns again, once.
        let w = run_ticks(&mut s, t0, 1800, 1800 + 300 + 1500);
        assert_eq!(w.len(), 1);
        assert_eq!(s.active_program().unwrap().cycle, 3);
    }

    #[test]
    fn workday_recaptures_previous_state_each_cycle() {
        let mut s = state();
        s.last_status = MachineStatus::Stopped;
        let t0 = Instant::now();
        s.run_program(2, t0).unwrap();
        s.last_status = MachineStatus::Running;
        run_ticks(&mut s, t0, 0, 300);
        assert_eq!(manual(&s).speed, 0.0);
        // User starts walking at 2.5 during the rest.
        moving(&mut s, 2.5, 1);
        run_ticks(&mut s, t0, 300, 1800);
        assert_eq!(s.active_program().unwrap().cycle, 2);
        run_ticks(&mut s, t0, 1800, 2100);
        assert_eq!(
            manual(&s),
            ControlIntent {
                speed: 2.5,
                incline: 1
            }
        );
    }

    #[test]
    fn focus_never_finishes_and_end_restores() {
        let mut s = state();
        moving(&mut s, 3.5, 1);
        let t0 = Instant::now();
        s.run_program(3, t0).unwrap();
        assert_eq!(s.active_program().unwrap().seconds_remaining, None);
        for chunk in 0..10u64 {
            run_ticks(&mut s, t0, chunk * 1000, (chunk + 1) * 1000);
        }
        assert!(s.program.is_some());
        assert_eq!(s.active_program().unwrap().seconds_remaining, None);
        assert_eq!(
            s.program_intent,
            Some(ControlIntent {
                speed: 2.5,
                incline: 5
            })
        );

        assert!(s.end_program(true, t0 + secs(10_001)));
        assert!(s.program.is_none());
        assert_eq!(s.program_intent, None);
        assert_eq!(
            manual(&s),
            ControlIntent {
                speed: 3.5,
                incline: 1
            }
        );
        assert!(!s.end_program(true, t0));
    }

    #[test]
    fn pause_freezes_time_and_resume_reapplies() {
        let mut s = state();
        moving(&mut s, 3.0, 0);
        let t0 = Instant::now();
        s.run_program(1, t0).unwrap();
        run_ticks(&mut s, t0, 0, 100);
        s.pause_program(t0 + secs(100)).unwrap();
        assert!(s.active_program().unwrap().paused);
        assert_eq!(manual(&s).speed, 0.0);
        assert_eq!(s.program_intent.unwrap().speed, 0.0);

        run_ticks(&mut s, t0, 100, 400);
        assert_eq!(s.active_program().unwrap().seconds_remaining, Some(500));

        s.resume_program(t0 + secs(400)).unwrap();
        assert!(!s.active_program().unwrap().paused);
        assert_eq!(s.program_intent.unwrap().speed, 4.0);
        run_ticks(&mut s, t0, 400, 460);
        assert_eq!(s.active_program().unwrap().seconds_remaining, Some(440));

        s.program = None;
        assert!(s.pause_program(t0).is_err());
    }

    #[test]
    fn resume_kicks_a_paused_deck() {
        let mut s = state();
        s.last_status = MachineStatus::Running;
        let t0 = Instant::now();
        s.run_program(1, t0).unwrap();
        s.pause_program(t0 + secs(5)).unwrap();
        s.last_status = MachineStatus::Paused;
        // The engine's clock reads the same instant, past the manual lockout.
        s.set_test_now(t0 + secs(10));
        s.resume_program(t0 + secs(10)).unwrap();
        assert_eq!(s.handshake_step, crate::protocol::HandshakeStep::Priming);
    }

    #[test]
    fn next_still_finds_a_hold_step_or_a_still_rest() {
        let step = |m: Option<f32>, kmh: f32| ProgramStep {
            duration_minutes: m,
            speed_kmh: kmh,
            incline_pct: 0.0,
            zone: None,
            kind: None,
            pulse_end: None,
        };
        let mut s = state();
        s.programs.push(Program {
            id: 4,
            name: "Stand".to_string(),
            steps: vec![
                step(Some(2.0), 3.0),
                step(Some(3.0), 4.0),
                step(Some(1.0), 0.0),
                step(Some(2.0), 3.0),
            ],
            repeat_every_minutes: Some(20.0),
            note: None,
        });
        let t0 = Instant::now();
        s.last_status = MachineStatus::Stopped;
        s.run_program(4, t0).unwrap();
        s.last_status = MachineStatus::Running;
        run_ticks(&mut s, t0, 0, 60);
        let run = s.program.as_ref().unwrap();
        // One minute of step 1 left, then 3 minutes, then the 1-minute hold.
        assert_eq!(run.next_still_s(), Some((240.0, Some(60.0))));
        assert_eq!(run.still_left_s(), None);
        run_ticks(&mut s, t0, 60, 330);
        let run = s.program.as_ref().unwrap();
        assert!(run.on_hold_step());
        assert_eq!(run.next_still_s(), Some((0.0, Some(30.0))));
        assert_eq!(run.still_left_s(), Some(30.0));

        // Workday from a still deck: the 25-minute rest is next.
        let mut s = state();
        s.last_status = MachineStatus::Stopped;
        s.run_program(2, t0).unwrap();
        s.last_status = MachineStatus::Running;
        run_ticks(&mut s, t0, 0, 100);
        let run = s.program.as_ref().unwrap();
        assert_eq!(run.next_still_s(), Some((200.0, Some(1500.0))));
        let s = workday_resting_on_a_still_belt(t0);
        let run = s.program.as_ref().unwrap();
        assert_eq!(run.next_still_s(), None);
        assert_eq!(run.still_left_s(), Some(1500.0));

        // Over a manual walk the rest carries the walk on: not a still rest.
        let mut s = state();
        moving(&mut s, 2.0, 0);
        s.run_program(2, t0).unwrap();
        assert_eq!(s.program.as_ref().unwrap().next_still_s(), None);
        // Nor is the end of a program that doesn't repeat.
        let mut s = state();
        s.last_status = MachineStatus::Stopped;
        s.run_program(1, t0).unwrap();
        assert_eq!(s.program.as_ref().unwrap().next_still_s(), None);
    }

    #[test]
    fn skip_walks_through_steps_rest_and_cycles() {
        let mut s = state();
        s.programs.push(Program {
            id: 4,
            name: "Intervals".to_string(),
            steps: vec![
                ProgramStep {
                    duration_minutes: Some(2.0),
                    speed_kmh: 3.0,
                    incline_pct: 0.0,
                    zone: None,
                    kind: None,
                    pulse_end: None,
                },
                ProgramStep {
                    duration_minutes: Some(2.0),
                    speed_kmh: 5.0,
                    incline_pct: 2.0,
                    zone: None,
                    kind: None,
                    pulse_end: None,
                },
            ],
            repeat_every_minutes: Some(10.0),
            note: None,
        });
        moving(&mut s, 2.0, 0);
        let t0 = Instant::now();
        s.run_program(4, t0).unwrap();

        s.skip_program(t0).unwrap();
        let a = s.active_program().unwrap();
        assert_eq!(
            (a.step_index, a.step_count, a.speed_kmh, a.incline_pct),
            (1, 2, 5.0, 2.0)
        );
        assert_eq!(
            s.program_intent,
            Some(ControlIntent {
                speed: 5.0,
                incline: 2
            })
        );

        s.skip_program(t0).unwrap(); // last step -> Resting
        let a = s.active_program().unwrap();
        assert_eq!(a.phase, ProgramPhase::Resting);
        assert_eq!(a.seconds_remaining, Some(360));
        assert_eq!(s.program_intent, None);
        assert_eq!(manual(&s).speed, 2.0);

        s.skip_program(t0).unwrap(); // Resting -> next cycle
        let a = s.active_program().unwrap();
        assert_eq!(
            (a.phase, a.cycle, a.step_index),
            (ProgramPhase::Walking, 2, 0)
        );

        // Non-repeating: skipping the last step finishes and restores.
        let mut s = state();
        moving(&mut s, 3.0, 1);
        s.run_program(1, t0).unwrap();
        s.skip_program(t0).unwrap();
        assert!(s.program.is_none());
        assert_eq!(
            manual(&s),
            ControlIntent {
                speed: 3.0,
                incline: 1
            }
        );
    }

    #[test]
    fn stop_ends_without_restore() {
        let mut s = state();
        moving(&mut s, 3.0, 1);
        s.run_program(1, Instant::now()).unwrap();
        s.trigger_action(crate::engine::DeckAction::Stop);
        assert!(s.program.is_none());
        assert_eq!(s.program_intent, None);
        assert_eq!(s.manual_intent, None);
        assert!(s.stop_requested);
    }

    #[test]
    fn external_intent_wipe_ends_program_without_restarting() {
        let mut s = state();
        moving(&mut s, 3.0, 1);
        let t0 = Instant::now();
        s.run_program(1, t0).unwrap();
        s.program_intent = None; // e.g. stop_session on a deck stop
        s.manual_intent = None;
        assert_eq!(s.tick_program(t0 + secs(1)), None);
        assert!(s.program.is_none());
        assert_eq!(s.manual_intent, None);
    }

    #[test]
    fn nudges_adjust_the_live_step_not_manual_intent() {
        let mut s = state();
        moving(&mut s, 3.0, 2);
        s.run_program(1, Instant::now()).unwrap();

        s.trigger_action(crate::engine::DeckAction::SpeedUp);
        let live = s.program_intent.unwrap();
        assert!(live.speed > 4.0 && live.speed < 4.3, "got {}", live.speed);
        assert_eq!(
            manual(&s),
            ControlIntent {
                speed: 3.0,
                incline: 2
            }
        );

        s.trigger_action(crate::engine::DeckAction::InclineUp);
        assert_eq!(s.program_intent.unwrap().incline, 11);
        s.trigger_action(crate::engine::DeckAction::InclineDown);
        s.trigger_action(crate::engine::DeckAction::InclineDown);
        assert_eq!(s.program_intent.unwrap().incline, 9);

        assert!(s.adjust_program_target(Some(4.5), Some(7)));
        let a = s.active_program().unwrap();
        assert_eq!((a.speed_kmh, a.incline_pct), (4.5, 7.0));
        assert_eq!(
            manual(&s),
            ControlIntent {
                speed: 3.0,
                incline: 2
            }
        );

        // The saved program is untouched.
        assert_eq!(s.programs[0].steps[0].speed_kmh, 4.0);

        // Restore returns to the manual state, not the nudged target.
        s.end_program(true, Instant::now());
        assert_eq!(
            manual(&s),
            ControlIntent {
                speed: 3.0,
                incline: 2
            }
        );
        // And outside a program nudges are manual again.
        assert!(!s.adjust_program_target(Some(1.0), None));
    }

    #[test]
    fn zero_speed_step_counts_even_though_the_belt_pauses() {
        let mut s = state();
        s.programs.push(Program {
            id: 4,
            name: "Hold".to_string(),
            steps: vec![ProgramStep {
                duration_minutes: Some(1.0),
                speed_kmh: 0.0,
                incline_pct: 0.0,
                zone: None,
                kind: None,
                pulse_end: None,
            }],
            repeat_every_minutes: None,
            note: None,
        });
        s.last_status = MachineStatus::Paused;
        let t0 = Instant::now();
        s.run_program(4, t0).unwrap();
        run_ticks(&mut s, t0, 0, 60);
        assert!(s.program.is_none());
    }

    #[test]
    fn replacing_a_run_keeps_the_original_previous_state() {
        let mut s = state();
        moving(&mut s, 3.0, 1);
        let t0 = Instant::now();
        s.run_program(1, t0).unwrap();
        s.run_program(3, t0 + secs(5)).unwrap();
        assert_eq!(s.program.as_ref().unwrap().program.id, 3);
        s.end_program(true, t0 + secs(6));
        assert_eq!(
            manual(&s),
            ControlIntent {
                speed: 3.0,
                incline: 1
            }
        );
    }

    #[test]
    fn toggle_ends_or_runs_last() {
        let mut s = state();
        let t0 = Instant::now();
        assert_eq!(s.toggle_program(t0), None); // nothing last-run yet
        s.program_last_run_id = Some(2);
        assert_eq!(s.toggle_program(t0), Some(2));
        assert!(s.program.is_some());
        assert_eq!(s.toggle_program(t0), None);
        assert!(s.program.is_none());
    }

    /// Steps are calibrated km/h, so they're checked against the calibrated
    /// floor, and a program started on a moving belt keeps at least that.
    #[test]
    fn programs_use_the_calibrated_speed_floor() {
        let mut s = state();
        s.sync_config(&crate::engine::slow_floor_config());
        let floor = s.min_physical_speed();
        assert!((floor - 1.2).abs() < 1e-4, "{floor}");
        let mut p = s.programs[0].clone();
        p.id = 0;
        p.steps[0].speed_kmh = 0.8; // over hw 0.6, under the belt's 1.2 km/h
        let err = s.save_program(p.clone()).unwrap_err();
        assert!(err.contains("between 1.2 and"), "{err}");
        p.steps[0].speed_kmh = 1.2;
        s.save_program(p).unwrap();

        // The deck reports its floor byte with no manual intent: the speed
        // to return to is the calibrated floor, not the raw 0.6.
        s.last_status = MachineStatus::Running;
        s.hardware_speed = 0.3;
        s.manual_intent = None;
        let prev = s.capture_previous();
        assert_eq!(prev.moving.map(|i| i.speed), Some(floor));
    }

    #[test]
    fn save_assigns_ids_and_delete_ends_running() {
        let mut s = state();
        s.programs[0].note = Some(NOTE_POST_MEAL.to_string());
        let mut p = s.programs[0].clone();
        p.id = 0;
        p.name = "  New  ".to_string();
        s.save_program(p).unwrap();
        assert_eq!(s.programs.last().unwrap().id, 4);
        assert_eq!(s.programs.last().unwrap().name, "New");
        assert_eq!(s.programs.last().unwrap().note, None, "notes are Rust's");
        assert_eq!(s.program_next_id, 5);

        // A rename keeps the note (the editor sends none); a new pace drops it.
        let mut edit = s.programs[0].clone();
        edit.name = "Renamed".to_string();
        edit.note = None;
        s.save_program(edit).unwrap();
        assert_eq!(s.programs[0].name, "Renamed");
        assert_eq!(s.programs[0].note.as_deref(), Some(NOTE_POST_MEAL));
        let mut edit = s.programs[0].clone();
        edit.steps[0].speed_kmh = 4.5;
        s.save_program(edit).unwrap();
        assert_eq!(s.programs[0].note, None);
        let mut ghost = s.programs[0].clone();
        ghost.id = 99;
        assert!(s.save_program(ghost).is_err());

        moving(&mut s, 3.0, 0);
        s.run_program(2, Instant::now()).unwrap();
        s.delete_program(2, Instant::now()).unwrap();
        assert!(s.program.is_none());
        assert!(s.programs.iter().all(|p| p.id != 2));
        assert_eq!(s.program_last_run_id, None);
        assert!(s.delete_program(2, Instant::now()).is_err());
    }

    fn base() -> Program {
        Program {
            id: 0,
            name: "Test".to_string(),
            steps: vec![ProgramStep {
                duration_minutes: Some(5.0),
                speed_kmh: 3.0,
                incline_pct: 2.0,
                zone: None,
                kind: None,
                pulse_end: None,
            }],
            repeat_every_minutes: None,
            note: None,
        }
    }

    fn err(p: &Program) -> String {
        validate_program(p, 0.6, 12.0).unwrap_err()
    }

    #[test]
    fn validation_accepts_seeds_and_rejects_each_invalid_case() {
        for p in seed_programs() {
            validate_program(&p, 0.6, 12.0).unwrap();
            // Also inside the deck's own 1.0 to 8.0 km/h, with one short sentence.
            validate_program(&p, 1.0, 8.0).unwrap();
            let note = p.note.as_deref().unwrap();
            assert!(note.len() <= 70 && note.ends_with('.'), "{}", note);
            assert_eq!(note.matches(". ").count(), 0, "{}", note);
            // The writing rules: no dashes or semicolons, no deictic words.
            assert!(
                !note.contains(['\u{2014}', '\u{2013}', ';', '-']),
                "{}",
                note
            );
            for word in ["this", "that", "it", "here", "below"] {
                assert!(
                    !note
                        .split([' ', ',', '.'])
                        .any(|w| w.eq_ignore_ascii_case(word)),
                    "{}",
                    note
                );
            }
        }
        validate_program(&base(), 0.6, 12.0).unwrap();

        let mut p = base();
        p.name = "   ".to_string();
        assert!(err(&p).contains("Name"));
        p.name = "x".repeat(41);
        assert!(err(&p).contains("40"));
        p.name = "x".repeat(40);
        validate_program(&p, 0.6, 12.0).unwrap();

        let mut p = base();
        p.steps.clear();
        assert!(err(&p).contains("at least one"));
        let mut p = base();
        p.steps = vec![base().steps[0].clone(); 21];
        assert!(err(&p).contains("20"));
        p.steps.truncate(20);
        validate_program(&p, 0.6, 12.0).unwrap();

        for bad in [f32::NAN, f32::INFINITY, -1.0] {
            let mut p = base();
            p.steps[0].duration_minutes = Some(bad);
            assert!(err(&p).contains("duration"));
            let mut p = base();
            p.steps[0].speed_kmh = bad;
            assert!(err(&p).contains("speed"));
            let mut p = base();
            p.steps[0].incline_pct = bad;
            assert!(err(&p).contains("incline"));
            let mut p = base();
            p.repeat_every_minutes = Some(bad);
            assert!(err(&p).contains("Repeat"));
        }

        let mut p = base();
        p.steps[0].speed_kmh = 0.3;
        assert!(err(&p).contains("0.6"));
        p.steps[0].speed_kmh = 12.5;
        assert!(err(&p).contains("12.0"));
        p.steps[0].speed_kmh = 0.0;
        validate_program(&p, 0.6, 12.0).unwrap();

        let mut p = base();
        p.steps.insert(
            0,
            ProgramStep {
                duration_minutes: None,
                speed_kmh: 3.0,
                incline_pct: 0.0,
                zone: None,
                kind: None,
                pulse_end: None,
            },
        );
        assert!(err(&p).contains("Step 1"));

        let mut p = base();
        p.steps[0].duration_minutes = None;
        p.repeat_every_minutes = Some(30.0);
        assert!(err(&p).contains("until ended"));

        let mut p = base();
        p.repeat_every_minutes = Some(5.0);
        assert!(err(&p).contains("longer"));
        p.repeat_every_minutes = Some(4.0);
        assert!(err(&p).contains("longer"));
        p.repeat_every_minutes = Some(5.5);
        validate_program(&p, 0.6, 12.0).unwrap();
    }

    #[test]
    fn incline_conversion_uses_tilt_and_offset() {
        // pct = level * tilt + offset
        assert_eq!(incline_pct_to_level(10.0, 1.0, 0.0, 15), 10);
        assert_eq!(incline_pct_to_level(9.5, 1.5, 0.5, 15), 6);
        assert_eq!(level_to_incline_pct(6, 1.5, 0.5), 9.5);
        assert_eq!(incline_pct_to_level(0.0, 1.5, 0.5, 15), 0); // below offset clamps up
        assert_eq!(incline_pct_to_level(99.0, 1.0, 0.0, 12), 12);
        assert_eq!(incline_pct_to_level(5.0, 0.0, 0.0, 15), 0); // tilt ~0 guard
        assert_eq!(incline_pct_to_level(5.0, 0.005, 0.0, 15), 0);
        assert_eq!(incline_pct_to_level(f32::NAN, 1.0, 0.0, 15), 0);

        let mut s = state();
        s.tilt_multiplier = 0.5;
        s.incline_offset = 1.0;
        moving(&mut s, 3.0, 0);
        s.run_program(1, Instant::now()).unwrap(); // 10% -> (10-1)/0.5 = 18 -> clamp 15
        assert_eq!(s.program_intent.unwrap().incline, 15);
        assert_eq!(s.active_program().unwrap().incline_pct, 8.5);
    }

    /// A calibration change mid-program keeps the deck at the step's level and
    /// shows that level in the new calibration; the next step's percentage is
    /// converted with the new numbers.
    #[test]
    fn a_tilt_change_mid_program_keeps_the_step_level() {
        let mut s = state();
        s.programs[0].steps.push(ProgramStep {
            duration_minutes: Some(5.0),
            speed_kmh: 4.0,
            incline_pct: 4.0,
            zone: None,
            kind: None,
            pulse_end: None,
        });
        moving(&mut s, 3.0, 0);
        let t0 = Instant::now();
        s.run_program(1, t0).unwrap(); // 10% at tilt 1.0 -> level 10
        assert_eq!(s.program_intent.unwrap().incline, 10);

        s.sync_config(&crate::persistence::AppConfig {
            tilt_multiplier: 0.5,
            incline_offset: 1.0,
            ..Default::default()
        });
        s.tick_program(t0 + secs(1));
        assert_eq!(s.program_intent.unwrap().incline, 10);
        assert_eq!(s.active_program().unwrap().incline_pct, 6.0); // 10 * 0.5 + 1

        s.skip_program(t0 + secs(2)).unwrap(); // 4% -> (4 - 1) / 0.5 = 6
        assert_eq!(s.program_intent.unwrap().incline, 6);
        assert_eq!(s.active_program().unwrap().incline_pct, 4.0);
    }

    #[test]
    fn seeding_only_when_the_key_is_absent() {
        let l = initial_programs(None, None);
        assert!(l.seeded);
        assert_eq!(l.programs, seed_programs());
        assert_eq!(l.next_id, 8);

        // Present (even empty) is never reseeded.
        let l = initial_programs(Some("[]"), None);
        assert!(!l.seeded);
        assert!(l.programs.is_empty());
        assert_eq!(l.next_id, 1);

        let json = serde_json::to_string(&seed_programs()[..2]).unwrap();
        let l = initial_programs(Some(&json), Some("9"));
        assert!(!l.seeded);
        assert_eq!(l.programs.len(), 2);
        assert_eq!(l.next_id, 9);

        // next_id can't fall behind an existing id.
        assert_eq!(initial_programs(Some(&json), Some("1")).next_id, 3);

        // Garbage is not reseeded (that would overwrite it on next save).
        let l = initial_programs(Some("not json"), None);
        assert!(!l.seeded);
        assert!(l.programs.is_empty());
    }

    #[test]
    fn program_json_is_snake_case() {
        let json = serde_json::to_value(&seed_programs()[1]).unwrap();
        assert_eq!(json["repeat_every_minutes"], 30.0);
        assert_eq!(json["steps"][0]["duration_minutes"], 5.0);
        assert_eq!(json["steps"][0]["speed_kmh"], 3.0);
        assert_eq!(json["steps"][0]["incline_pct"], 5.0);
        assert_eq!(
            serde_json::to_string(&ProgramPhase::Resting).unwrap(),
            "\"resting\""
        );
    }

    #[test]
    fn stored_program_with_a_legacy_schedule_key_still_loads() {
        let json = r#"{"id":7,"name":"Old","steps":[{"duration_minutes":10.0,"speed_kmh":3.0,"incline_pct":0.0}],"repeat_every_minutes":null,"schedule":{"time":"09:00","days":["mon","tue"],"enabled":true}}"#;
        let p: Program = serde_json::from_str(json).unwrap();
        assert_eq!(p.id, 7);
        assert_eq!(p.name, "Old");
        assert!(validate_program(&p, 0.6, 10.0).is_ok());
    }

    fn saved_from(s: &OrchestratorState, now_ms: i64) -> SavedRun {
        s.saved_run(now_ms).expect("a program is running")
    }

    #[test]
    fn running_program_survives_a_restart() {
        let t0 = Instant::now();
        let mut before = state();
        before.last_status = MachineStatus::Running;
        before.run_program(1, t0).unwrap();
        before.tick_program(t0 + secs(120));
        let saved = saved_from(&before, 1_000_000);

        let mut after = state();
        after.restore_run(&saved, 1_000_000 + 30_000, t0).unwrap();
        let run = after.program.as_ref().unwrap();
        assert_eq!(run.program.id, 1);
        assert!(
            (run.elapsed_s - 120.0).abs() < 1.0,
            "walk time does not advance while down"
        );
        assert!(after.program_intent.is_some());
    }

    #[test]
    fn restored_program_reapplies_once_after_reconnect_then_obeys_a_deck_stop() {
        let t0 = Instant::now();
        let mut before = state();
        before.last_status = MachineStatus::Running;
        before.run_program(1, t0).unwrap();
        let saved = saved_from(&before, 0);

        let mut s = state();
        s.restore_run(&saved, 1_000, t0).unwrap();
        // Reconnect wipes intents.
        s.program_intent = None;
        s.tick_program(t0 + secs(1));
        assert!(
            s.program.is_some() && s.program_intent.is_some(),
            "re-applied"
        );

        // Deck reports in; the restore is now confirmed.
        s.awaiting_first_packet = false;
        s.last_status = MachineStatus::Running;
        s.tick_program(t0 + secs(2));
        assert!(!s.program.as_ref().unwrap().restored);

        // A later wipe is a real stop at the treadmill.
        s.program_intent = None;
        s.tick_program(t0 + secs(3));
        assert!(s.program.is_none());
    }

    #[test]
    fn stale_deleted_or_changed_runs_are_not_restored() {
        let t0 = Instant::now();
        let mut before = state();
        before.run_program(1, t0).unwrap();
        let saved = saved_from(&before, 0);

        let mut s = state();
        assert!(s.restore_run(&saved, RESUME_MAX_AGE_MS + 1, t0).is_err());
        assert!(s.restore_run(&saved, -5, t0).is_err());

        let mut gone = state();
        gone.programs.retain(|p| p.id != 1);
        assert!(gone.restore_run(&saved, 1_000, t0).is_err());

        let mut changed = saved.clone();
        changed.step_index = 99;
        assert!(s.restore_run(&changed, 1_000, t0).is_err());
    }

    #[test]
    fn rest_time_keeps_counting_while_the_app_is_down() {
        let t0 = Instant::now();
        let mut s = state();
        let mut saved = {
            let mut before = state();
            before.run_program(2, t0).unwrap();
            saved_from(&before, 0)
        };
        saved.phase = ProgramPhase::Resting;
        saved.elapsed_s = 60.0;
        s.restore_run(&saved, 90_000, t0).unwrap();
        assert!((s.program.as_ref().unwrap().elapsed_s - 150.0).abs() < 0.01);
    }

    use crate::engine::{PacketKind, ProtocolEngine};

    /// Drive the engine like the protocol worker: 50 ms ticks, the program
    /// tick once a second. Returns the kinds of packet sent.
    fn drive(
        s: &mut OrchestratorState,
        engine: &mut ProtocolEngine,
        t0: Instant,
        secs_n: u64,
    ) -> Vec<PacketKind> {
        let mut out = vec![];
        for i in 0..secs_n * 20 {
            let now = t0 + Duration::from_millis(50 * i);
            s.set_test_now(now);
            if i % 20 == 0 {
                s.tick_program(now);
            }
            if let Some(p) = engine.determine_next_packet(s, now) {
                s.last_cmd_time = now;
                out.push(p.kind);
            }
        }
        out
    }

    fn moves_belt(sent: &[PacketKind]) -> bool {
        sent.iter().any(|k| {
            matches!(
                k,
                PacketKind::HandshakeReady | PacketKind::HandshakeStart | PacketKind::ControlSet
            )
        })
    }

    /// The app restarts (an update) `down_ms` after saving the run, the
    /// reconnect clears everything, and the deck's first frame comes in.
    fn restart_with(
        saved: &SavedRun,
        down_ms: i64,
        status: MachineStatus,
        hw: f32,
        incline: u8,
        t0: Instant,
    ) -> OrchestratorState {
        let mut s = state();
        s.restore_run(saved, saved.saved_at_ms + down_ms, t0)
            .unwrap();
        s.reset_connection();
        assert!(
            s.program_intent.is_none(),
            "the reconnect wiped the intents"
        );
        s.last_status = status;
        s.hardware_speed = hw;
        s.hardware_incline = incline;
        s.awaiting_first_packet = false;
        s
    }

    /// Post-meal (10 min at 4.0 km/h, 10 %) two minutes in, saved at 1_000_000 ms.
    fn post_meal_two_minutes_in(t0: Instant) -> SavedRun {
        let mut before = state();
        before.last_status = MachineStatus::Running;
        before.run_program(1, t0).unwrap();
        before.tick_program(t0 + secs(120));
        saved_from(&before, 1_000_000)
    }

    #[test]
    fn an_update_mid_program_carries_on_without_touching_the_belt() {
        let t0 = Instant::now();
        let saved = post_meal_two_minutes_in(t0);
        assert!(!saved.paused);
        let level = saved.target.incline;
        // Closed for 20 s; the belt kept going at the program's speed.
        let mut s = restart_with(&saved, 20_000, MachineStatus::Running, 4.0, level, t0);
        let mut engine = ProtocolEngine::new();
        let sent = drive(&mut s, &mut engine, t0, 10);
        assert!(
            sent.iter().all(|k| *k == PacketKind::PollStatus),
            "{sent:?}"
        );

        let a = s.active_program().unwrap();
        assert!(!a.paused, "still running");
        assert_eq!(a.step_index, 0);
        // 120 s walked before, 20 s while closed, about 10 s since: the step
        // timer keeps to the wall clock.
        let elapsed = s.program.as_ref().unwrap().elapsed_s;
        assert!((elapsed - 149.0).abs() < 1.5, "{elapsed}");
        assert_eq!(s.program_intent.map(|i| i.speed), Some(4.0));
        assert!(!s.hold_auto_start);
    }

    #[test]
    fn time_closed_can_finish_a_step_and_the_next_one_applies() {
        let t0 = Instant::now();
        let mut before = state();
        before.programs.push(Program {
            id: 9,
            name: "Two".to_string(),
            steps: vec![
                ProgramStep {
                    duration_minutes: Some(1.0),
                    speed_kmh: 3.0,
                    incline_pct: 0.0,
                    zone: None,
                    kind: None,
                    pulse_end: None,
                },
                ProgramStep {
                    duration_minutes: Some(5.0),
                    speed_kmh: 4.0,
                    incline_pct: 0.0,
                    zone: None,
                    kind: None,
                    pulse_end: None,
                },
            ],
            repeat_every_minutes: None,
            note: None,
        });
        before.last_status = MachineStatus::Running;
        before.run_program(9, t0).unwrap();
        before.tick_program(t0 + secs(50));
        let saved = saved_from(&before, 1_000_000);

        let mut s = state();
        s.programs = before.programs.clone();
        s.restore_run(&saved, 1_000_000 + 30_000, t0).unwrap();
        s.reset_connection();
        s.last_status = MachineStatus::Running;
        s.hardware_speed = 3.0;
        s.awaiting_first_packet = false;
        s.tick_program(t0);
        let a = s.active_program().unwrap();
        assert_eq!(a.step_index, 1, "50 s + 30 s closed is past the 60 s step");
        assert_eq!(s.program_intent.map(|i| i.speed), Some(4.0));
    }

    #[test]
    fn a_restored_program_on_a_still_belt_comes_back_paused_and_waits_for_go() {
        for status in [MachineStatus::Paused, MachineStatus::Stopped] {
            let t0 = Instant::now();
            let saved = post_meal_two_minutes_in(t0);
            let mut s = restart_with(&saved, 20_000, status, 0.0, saved.target.incline, t0);
            let mut engine = ProtocolEngine::new();
            let sent = drive(&mut s, &mut engine, t0, 10);
            assert!(!moves_belt(&sent), "{status:?}: {sent:?}");
            let a = s.active_program().unwrap();
            assert!(a.paused, "{status:?}: waits for GO");
            assert_eq!(a.wait, Some(ProgramWait::BeltStill), "{status:?}");
            let elapsed = s.program.as_ref().unwrap().elapsed_s;
            assert!((elapsed - 120.0).abs() < 1.0, "{status:?}: {elapsed}");

            // GO starts it, right after the 10 s driven above.
            let later = t0 + secs(10);
            s.set_test_now(later);
            s.last_cmd_time = later;
            s.resume_program(later).unwrap();
            let sent = drive(&mut s, &mut engine, later, 2);
            assert!(
                sent.contains(&PacketKind::HandshakeStart),
                "{status:?}: {sent:?}"
            );
        }
    }

    #[test]
    fn a_resting_program_restored_on_a_still_belt_waits_for_go_at_its_next_cycle() {
        let t0 = Instant::now();
        let mut before = state();
        before.run_program(2, t0).unwrap();
        let mut saved = saved_from(&before, 1_000_000);
        saved.phase = ProgramPhase::Resting;
        // Workday rests 25 min; 3 s of it left when saved.
        saved.elapsed_s = 25.0 * 60.0 - 3.0;
        let mut s = restart_with(&saved, 1_000, MachineStatus::Paused, 0.0, 0, t0);
        let mut engine = ProtocolEngine::new();
        let sent = drive(&mut s, &mut engine, t0, 10);
        assert!(!moves_belt(&sent), "{sent:?}");
        let a = s.active_program().unwrap();
        assert_eq!(
            (a.phase, a.cycle, a.paused, a.wait),
            (
                ProgramPhase::Walking,
                2,
                true,
                Some(ProgramWait::RestComplete)
            )
        );
    }

    // -----------------------------------------------------------------------
    // Still-belt starts: what each path does when the belt is not moving.
    // -----------------------------------------------------------------------

    /// A 1-minute 0 km/h hold, then 2 minutes at 3.0 km/h.
    fn hold_then_walk() -> Program {
        Program {
            id: 5,
            name: "Hold then walk".to_string(),
            steps: vec![
                ProgramStep {
                    duration_minutes: Some(1.0),
                    speed_kmh: 0.0,
                    incline_pct: 0.0,
                    zone: None,
                    kind: None,
                    pulse_end: None,
                },
                ProgramStep {
                    duration_minutes: Some(2.0),
                    speed_kmh: 3.0,
                    incline_pct: 0.0,
                    zone: None,
                    kind: None,
                    pulse_end: None,
                },
            ],
            repeat_every_minutes: None,
            note: None,
        }
    }

    /// Workday from a stopped deck, walked into its rest; the belt pauses.
    fn workday_resting_on_a_still_belt(t0: Instant) -> OrchestratorState {
        let mut s = state();
        s.last_status = MachineStatus::Stopped;
        s.run_program(2, t0).unwrap();
        s.last_status = MachineStatus::Running;
        run_ticks(&mut s, t0, 0, 300);
        assert_eq!(s.active_program().unwrap().phase, ProgramPhase::Resting);
        assert_eq!(manual(&s).speed, 0.0);
        s.last_status = MachineStatus::Paused;
        s
    }

    /// Was `pin_rest_end_on_a_still_belt_starts_the_next_cycle`: the next
    /// cycle now waits for GO instead of starting the belt.
    #[test]
    fn rest_end_on_a_still_belt_waits_for_go() {
        let t0 = Instant::now();
        let mut s = workday_resting_on_a_still_belt(t0);
        let toasts = run_ticks(&mut s, t0, 300, 1800);
        let a = s.active_program().unwrap();
        assert_eq!(
            (a.phase, a.cycle, a.paused, a.wait),
            (
                ProgramPhase::Walking,
                2,
                true,
                Some(ProgramWait::RestComplete)
            )
        );
        assert_eq!(a.seconds_remaining, Some(300), "the step timer waits too");
        assert_eq!(s.program_intent.map(|i| i.speed), Some(0.0));
        assert_eq!(manual(&s).speed, 0.0);
        assert_eq!(s.handshake_step, crate::protocol::HandshakeStep::Idle);
        assert!(s.hold_auto_start);
        // One toast when the rest ends (after the 30 s warning), not repeated.
        assert_eq!(
            toasts,
            vec![
                (1770, "WORKDAY IN 30S".to_string()),
                (1800, "REST COMPLETE // PRESS GO".to_string())
            ]
        );
        assert!(run_ticks(&mut s, t0, 1800, 1900).is_empty());
    }

    /// Was `pin_a_walking_step_after_a_zero_step_starts_the_belt`.
    #[test]
    fn a_walking_step_after_a_zero_step_waits_for_go() {
        let t0 = Instant::now();
        let mut s = state();
        s.programs.push(hold_then_walk());
        s.last_status = MachineStatus::Paused;
        s.run_program(5, t0).unwrap();
        let toasts = run_ticks(&mut s, t0, 0, 70);
        let a = s.active_program().unwrap();
        assert_eq!(
            (a.step_index, a.paused, a.wait),
            (1, true, Some(ProgramWait::HoldComplete))
        );
        assert_eq!(a.seconds_remaining, Some(120));
        assert_eq!(s.program_intent.map(|i| i.speed), Some(0.0));
        assert_eq!(s.handshake_step, crate::protocol::HandshakeStep::Idle);
        assert_eq!(toasts, vec![(60, "HOLD COMPLETE // PRESS GO".to_string())]);
    }

    #[test]
    fn a_step_change_between_two_moving_speeds_stays_automatic() {
        let mut s = state();
        s.programs[0].steps.push(ProgramStep {
            duration_minutes: Some(5.0),
            speed_kmh: 5.0,
            incline_pct: 0.0,
            zone: None,
            kind: None,
            pulse_end: None,
        });
        s.last_status = MachineStatus::Running;
        let t0 = Instant::now();
        s.run_program(1, t0).unwrap();
        assert!(run_ticks(&mut s, t0, 0, 601).is_empty());
        let a = s.active_program().unwrap();
        assert_eq!((a.step_index, a.paused, a.wait), (1, false, None));
        assert_eq!(s.program_intent.map(|i| i.speed), Some(5.0));
    }

    #[test]
    fn a_rest_on_a_moving_belt_carries_on_into_the_next_cycle() {
        // The user was walking at 2.0 before the program: the belt keeps
        // moving through the rest, so the next cycle is a speed change.
        let mut s = state();
        moving(&mut s, 2.0, 0);
        let t0 = Instant::now();
        s.run_program(2, t0).unwrap();
        run_ticks(&mut s, t0, 0, 1800);
        let a = s.active_program().unwrap();
        assert_eq!((a.cycle, a.paused, a.wait), (2, false, None));
        assert_eq!(s.program_intent.map(|i| i.speed), Some(3.0));
    }

    #[test]
    fn waiting_for_go_never_times_out_and_go_starts_the_belt() {
        let t0 = Instant::now();
        let mut s = workday_resting_on_a_still_belt(t0);
        run_ticks(&mut s, t0, 300, 1800);
        // Lunch: three hours of ticks, nothing moves and nothing ends.
        let mut engine = ProtocolEngine::new();
        engine.has_synced_reality = true;
        s.awaiting_first_packet = false;
        let sent = drive(&mut s, &mut engine, t0 + secs(1801), 3 * 3600);
        assert!(!moves_belt(&sent), "{sent:?}");
        let a = s.active_program().unwrap();
        assert_eq!(
            (a.cycle, a.wait, a.seconds_remaining),
            (2, Some(ProgramWait::RestComplete), Some(300))
        );

        // GO, after the lunch driven above.
        let later = t0 + secs(1801 + 3 * 3600);
        s.set_test_now(later);
        s.last_cmd_time = later;
        s.resume_program(later).unwrap();
        let a = s.active_program().unwrap();
        assert_eq!((a.paused, a.wait), (false, None));
        assert!(!s.hold_auto_start);
        let sent = drive(&mut s, &mut engine, later, 2);
        assert!(sent.contains(&PacketKind::HandshakeStart), "{sent:?}");
    }

    #[test]
    fn walking_through_skip_end_and_engage_from_waiting() {
        let t0 = Instant::now();
        // SKIP while waiting skips the cycle: back to resting, nothing moves.
        let mut s = workday_resting_on_a_still_belt(t0);
        run_ticks(&mut s, t0, 300, 1800);
        s.skip_program(t0 + secs(1801)).unwrap();
        let a = s.active_program().unwrap();
        assert_eq!((a.phase, a.wait), (ProgramPhase::Resting, None));
        assert_eq!(s.handshake_step, crate::protocol::HandshakeStep::Idle);

        // END while waiting ends it; the belt stays still.
        let mut s = workday_resting_on_a_still_belt(t0);
        run_ticks(&mut s, t0, 300, 1800);
        assert!(s.end_program(true, t0 + secs(1801)));
        assert_eq!(manual(&s).speed, 0.0);
        assert_eq!(s.handshake_step, crate::protocol::HandshakeStep::Idle);
    }

    #[test]
    fn a_waiting_run_survives_a_restart_still_waiting() {
        let t0 = Instant::now();
        let mut s = workday_resting_on_a_still_belt(t0);
        run_ticks(&mut s, t0, 300, 1800);
        let saved = saved_from(&s, 1_000_000);
        assert_eq!(saved.wait, Some(ProgramWait::RestComplete));
        // Back three hours later (lunch, a reboot).
        let three_h = 3 * 3600 * 1000;
        let mut s = restart_with(&saved, three_h, MachineStatus::Paused, 0.0, 0, t0);
        let mut engine = ProtocolEngine::new();
        let sent = drive(&mut s, &mut engine, t0, 10);
        assert!(!moves_belt(&sent), "{sent:?}");
        let a = s.active_program().unwrap();
        assert_eq!(
            (a.cycle, a.paused, a.wait),
            (2, true, Some(ProgramWait::RestComplete))
        );
        // Older than the limit it is dropped, and nothing is running.
        let mut s = state();
        assert!(s
            .restore_run(&saved, saved.saved_at_ms + RESUME_STILL_MAX_AGE_MS + 1, t0)
            .is_err());
        assert!(s.program.is_none());
    }

    #[test]
    fn a_waiting_program_outlives_the_treadmill_switching_itself_off() {
        let t0 = Instant::now();
        let mut s = workday_resting_on_a_still_belt(t0);
        run_ticks(&mut s, t0, 300, 1800);
        // The deck powers off while idle: the link drops and comes back.
        s.reset_connection();
        assert!(s.program_intent.is_none());
        s.awaiting_first_packet = false;
        s.last_status = MachineStatus::Stopped;
        let mut engine = ProtocolEngine::new();
        let sent = drive(&mut s, &mut engine, t0 + secs(5000), 10);
        assert!(!moves_belt(&sent), "{sent:?}");
        let a = s.active_program().unwrap();
        assert_eq!((a.cycle, a.wait), (2, Some(ProgramWait::RestComplete)));

        // A stop at the treadmill (intents wiped with the link up) still ends it.
        s.program_intent = None;
        s.tick_program(t0 + secs(5011));
        assert!(s.program.is_none());
    }

    #[test]
    fn go_watches_for_the_deck_to_answer_like_engage() {
        let t0 = Instant::now();
        let mut s = workday_resting_on_a_still_belt(t0);
        run_ticks(&mut s, t0, 300, 1800);
        // The watch from running the program was answered long ago.
        s.start_requested_at = None;
        s.resume_program(t0 + secs(1801)).unwrap();
        assert!(s.start_requested_at.is_some());
    }

    #[test]
    fn a_run_saved_before_waiting_existed_still_loads() {
        let json = r#"{"program_id":2,"phase":"walking","step_index":0,"cycle":1,"paused":false,"elapsed_s":12.0,"target":{"speed":3.0,"incline":5},"previous":{"status":"Running","moving":null},"rest_warned":false,"saved_at_ms":5}"#;
        let saved: SavedRun = serde_json::from_str(json).unwrap();
        assert_eq!(saved.wait, None);
    }

    const BEFORE: ControlIntent = ControlIntent {
        speed: 3.0,
        incline: 1,
    };

    /// Post-meal run from a belt walking at 3.0 km/h, then paused.
    fn post_meal_paused(t0: Instant) -> OrchestratorState {
        let mut s = state();
        moving(&mut s, BEFORE.speed, BEFORE.incline);
        s.run_program(1, t0).unwrap();
        s.pause_program(t0 + secs(10)).unwrap();
        s.last_status = MachineStatus::Paused;
        // The manual lockout from the pause has passed.
        s.last_manual_interaction = t0;
        s
    }

    /// Nothing moves the belt for a while; then Engage starts it at the
    /// speed from before the program.
    fn stays_still_until_engage(s: &mut OrchestratorState, t0: Instant) {
        assert_eq!(s.handshake_step, crate::protocol::HandshakeStep::Idle);
        assert_eq!(
            manual(s),
            ControlIntent {
                speed: 0.0,
                incline: 1
            }
        );
        assert_eq!(s.engage_speed, Some(BEFORE.speed));
        let mut engine = ProtocolEngine::new();
        engine.has_synced_reality = true;
        s.awaiting_first_packet = false;
        let sent = drive(s, &mut engine, t0 + secs(100), 30);
        assert!(!moves_belt(&sent), "{sent:?}");

        s.trigger_action(crate::engine::DeckAction::ToggleStart);
        assert_eq!(manual(s), BEFORE);
        assert_eq!(s.engage_speed, None);
        assert_eq!(s.handshake_step, crate::protocol::HandshakeStep::Priming);
    }

    /// Was `pin_end_on_a_paused_program_restores_the_moving_speed_and_resumes`.
    #[test]
    fn end_on_a_paused_program_leaves_the_belt_paused_until_engage() {
        let t0 = Instant::now();
        let mut s = post_meal_paused(t0);
        assert!(s.end_program(true, t0 + secs(20)));
        assert!(s.program.is_none());
        stays_still_until_engage(&mut s, t0);
    }

    /// Was `pin_skip_on_the_last_step_while_paused_restores_and_resumes`.
    #[test]
    fn skip_on_the_last_step_while_paused_leaves_the_belt_paused_until_engage() {
        let t0 = Instant::now();
        let mut s = post_meal_paused(t0);
        s.skip_program(t0 + secs(20)).unwrap();
        assert!(s.program.is_none());
        stays_still_until_engage(&mut s, t0);
    }

    #[test]
    fn end_while_the_belt_is_moving_restores_the_speed_and_keeps_moving() {
        let t0 = Instant::now();
        let mut s = state();
        moving(&mut s, BEFORE.speed, BEFORE.incline);
        s.run_program(1, t0).unwrap();
        assert!(s.end_program(true, t0 + secs(20)));
        assert_eq!(manual(&s), BEFORE);
        assert_eq!(s.engage_speed, None);
    }

    #[test]
    fn end_mid_pause_with_the_deck_still_reporting_running_keeps_the_pause() {
        // HOLD then END within a second: the deck hasn't reported the pause
        // yet, but the belt is on its way to still and stays there.
        let t0 = Instant::now();
        let mut s = state();
        moving(&mut s, BEFORE.speed, BEFORE.incline);
        s.run_program(1, t0).unwrap();
        s.pause_program(t0).unwrap();
        assert!(s.end_program(true, t0));
        assert_eq!(manual(&s).speed, 0.0);
        assert_eq!(s.engage_speed, Some(BEFORE.speed));
        assert!(s.resolve_target().0 < MOVING_KMH);
    }

    #[test]
    fn skipping_into_a_rest_while_paused_does_not_start_the_belt() {
        let t0 = Instant::now();
        let mut s = state();
        moving(&mut s, BEFORE.speed, BEFORE.incline);
        s.run_program(2, t0).unwrap();
        s.pause_program(t0 + secs(10)).unwrap();
        s.last_status = MachineStatus::Paused;
        s.last_manual_interaction = t0;
        s.skip_program(t0 + secs(20)).unwrap();
        assert_eq!(s.active_program().unwrap().phase, ProgramPhase::Resting);
        assert_eq!(manual(&s).speed, 0.0);
        assert_eq!(s.handshake_step, crate::protocol::HandshakeStep::Idle);
    }

    #[test]
    fn a_program_ending_on_a_zero_step_leaves_the_belt_still() {
        let t0 = Instant::now();
        let mut s = state();
        s.programs.push(Program {
            id: 6,
            name: "Walk then hold".to_string(),
            steps: vec![
                ProgramStep {
                    duration_minutes: Some(1.0),
                    speed_kmh: 3.0,
                    incline_pct: 0.0,
                    zone: None,
                    kind: None,
                    pulse_end: None,
                },
                ProgramStep {
                    duration_minutes: Some(1.0),
                    speed_kmh: 0.0,
                    incline_pct: 0.0,
                    zone: None,
                    kind: None,
                    pulse_end: None,
                },
            ],
            repeat_every_minutes: None,
            note: None,
        });
        moving(&mut s, BEFORE.speed, BEFORE.incline);
        s.run_program(6, t0).unwrap();
        run_ticks(&mut s, t0, 0, 60);
        s.last_status = MachineStatus::Paused;
        run_ticks(&mut s, t0, 60, 121);
        assert!(s.program.is_none());
        assert_eq!(manual(&s).speed, 0.0);
        assert_eq!(s.engage_speed, Some(BEFORE.speed));
        assert_eq!(s.handshake_step, crate::protocol::HandshakeStep::Idle);
    }

    #[test]
    fn a_speed_set_by_hand_or_a_stop_drops_the_engage_speed() {
        let t0 = Instant::now();
        let mut s = post_meal_paused(t0);
        s.end_program(true, t0 + secs(20));
        s.set_intent(Some(2.0), None, true);
        assert_eq!(s.engage_speed, None);

        let mut s = post_meal_paused(t0);
        s.end_program(true, t0 + secs(20));
        s.trigger_action(crate::engine::DeckAction::Stop);
        assert_eq!(s.engage_speed, None);
    }

    #[test]
    fn pin_skip_during_a_rest_starts_the_next_cycle() {
        let t0 = Instant::now();
        let mut s = workday_resting_on_a_still_belt(t0);
        s.set_test_now(t0 + secs(400));
        s.skip_program(t0 + secs(400)).unwrap();
        let a = s.active_program().unwrap();
        assert_eq!(
            (a.phase, a.cycle, a.paused),
            (ProgramPhase::Walking, 2, false)
        );
        assert_eq!(s.handshake_step, crate::protocol::HandshakeStep::Priming);
    }

    #[test]
    fn skip_is_a_press_even_on_a_connection_holding_starts() {
        // After a reconnect (hold on), SKIP out of a rest or past a hold step
        // still starts the belt: the user pressed it.
        let t0 = Instant::now();
        let mut s = workday_resting_on_a_still_belt(t0);
        s.hold_auto_start = true;
        s.set_test_now(t0 + secs(400));
        s.skip_program(t0 + secs(400)).unwrap();
        assert!(!s.hold_auto_start);
        assert_eq!(s.active_program().unwrap().wait, None);
        assert_eq!(s.handshake_step, crate::protocol::HandshakeStep::Priming);

        let mut s = state();
        s.programs.push(hold_then_walk());
        s.last_status = MachineStatus::Paused;
        s.run_program(5, t0).unwrap();
        s.hold_auto_start = true;
        s.set_test_now(t0 + secs(10));
        s.skip_program(t0 + secs(10)).unwrap();
        let a = s.active_program().unwrap();
        assert_eq!((a.step_index, a.paused, a.wait), (1, false, None));
        assert_eq!(s.handshake_step, crate::protocol::HandshakeStep::Priming);
    }

    /// Was `pin_engage_on_a_paused_program_starts_the_belt_at_min_speed`
    /// (which started the belt under a paused program, at min speed, only for
    /// the program to pause it again once the manual lockout ran out). Engage
    /// and the start shortcut are now the program's GO.
    #[test]
    fn engage_on_a_paused_program_is_its_go() {
        for action in [
            crate::engine::DeckAction::Start,
            crate::engine::DeckAction::ToggleStart,
        ] {
            let t0 = Instant::now();
            let mut s = workday_resting_on_a_still_belt(t0);
            run_ticks(&mut s, t0, 300, 1800);
            assert!(s.active_program().unwrap().wait.is_some());
            s.trigger_action(action);
            let a = s.active_program().unwrap();
            assert_eq!((a.paused, a.wait), (false, None), "{action:?}");
            assert_eq!(s.program_intent.map(|i| i.speed), Some(3.0));
            assert_eq!(manual(&s).speed, 0.0, "{action:?}: not min speed");
            assert!(!s.hold_auto_start);
        }
    }

    /// Was `pin_a_resting_run_saved_over_ten_minutes_ago_is_dropped`: a run
    /// that holds the belt still now keeps for hours; a walking one still for
    /// ten minutes.
    #[test]
    fn speed_taken_to_zero_by_hand_pauses_the_program_and_engage_resumes_it() {
        let t0 = Instant::now();
        let mut s = state();
        moving(&mut s, BEFORE.speed, BEFORE.incline);
        s.run_program(1, t0).unwrap();
        let step_speed = s.program_intent.map(|i| i.speed).unwrap();
        assert!(step_speed > 1.0);

        assert!(s.adjust_program_target(Some(0.0), None));
        let a = s.active_program().unwrap();
        assert!(a.paused, "zero by hand pauses the program");
        assert_eq!(s.program.as_ref().unwrap().target.speed, step_speed);

        s.trigger_action(crate::engine::DeckAction::Start);
        let a = s.active_program().unwrap();
        assert!(!a.paused, "Engage is GO");
        assert_eq!(s.program_intent.map(|i| i.speed), Some(step_speed));
        // Nothing left that would pause the belt again once the lockout ends.
        assert!(s.resolve_target().0 >= step_speed - 0.01);
    }

    #[test]
    fn a_still_run_keeps_for_hours_and_a_walking_one_for_minutes() {
        let t0 = Instant::now();
        let mut s = workday_resting_on_a_still_belt(t0);
        let saved = saved_from(&s, 0);
        assert!(s.restore_run(&saved, RESUME_MAX_AGE_MS + 1, t0).is_ok());
        assert!(s.restore_run(&saved, RESUME_STILL_MAX_AGE_MS, t0).is_ok());
        assert!(s
            .restore_run(&saved, RESUME_STILL_MAX_AGE_MS + 1, t0)
            .is_err());

        let walking = post_meal_two_minutes_in(t0);
        let at = walking.saved_at_ms;
        assert!(s
            .restore_run(&walking, at + RESUME_MAX_AGE_MS + 1, t0)
            .is_err());
        let mut paused = walking.clone();
        paused.paused = true;
        assert!(s
            .restore_run(&paused, at + RESUME_MAX_AGE_MS + 1, t0)
            .is_ok());
    }

    #[test]
    fn a_program_started_after_a_reconnect_starts_the_belt() {
        let t0 = Instant::now();
        let mut s = state();
        s.reset_connection();
        s.awaiting_first_packet = false;
        s.last_status = MachineStatus::Paused;
        s.run_program(1, t0).unwrap();
        let sent = drive(&mut s, &mut ProtocolEngine::new(), t0, 2);
        assert!(sent.contains(&PacketKind::HandshakeStart), "{sent:?}");
    }

    // -----------------------------------------------------------------------
    // A start at the treadmill (its own buttons or remote).
    // -----------------------------------------------------------------------

    fn pauses_belt(sent: &[PacketKind]) -> bool {
        sent.iter().any(|k| {
            matches!(
                k,
                PacketKind::HandshakePausePrime | PacketKind::HandshakePause
            )
        })
    }

    /// A synced engine, with the deck reporting the belt still for `secs_n`
    /// seconds from `t`.
    fn still_for(s: &mut OrchestratorState, t: Instant, secs_n: u64) -> ProtocolEngine {
        let mut engine = ProtocolEngine::new();
        engine.has_synced_reality = true;
        s.awaiting_first_packet = false;
        s.hardware_speed = 0.0;
        // On the test clock, so the command gap and the poll can pass.
        s.last_cmd_time = t;
        s.last_poll_time = t;
        let sent = drive(s, &mut engine, t, secs_n);
        assert!(!moves_belt(&sent), "{sent:?}");
        engine
    }

    /// The user starts the belt at the treadmill at `t`: the deck counts down
    /// (Starting) for 3 s, then runs at its own 1.0 km/h for `run_s` seconds.
    fn start_at_the_treadmill(
        s: &mut OrchestratorState,
        engine: &mut ProtocolEngine,
        t: Instant,
        run_s: u64,
    ) -> Vec<PacketKind> {
        s.last_status = MachineStatus::Starting;
        s.hardware_speed = 0.0;
        let mut sent = drive(s, engine, t, 3);
        s.last_status = MachineStatus::Running;
        s.hardware_speed = 1.0;
        sent.extend(drive(s, engine, t + secs(3), run_s));
        sent
    }

    /// Anything the app sends to start the belt itself: the start handshake.
    fn app_starts(sent: &[PacketKind]) -> bool {
        sent.iter()
            .any(|k| matches!(k, PacketKind::HandshakeReady | PacketKind::HandshakeStart))
    }

    /// The GO from the treadmill: the program carries on at its step, on the
    /// belt the deck started, with a speed change and no pause or start.
    fn assert_go_from_the_treadmill(s: &OrchestratorState, sent: &[PacketKind], speed: f32) {
        assert!(!pauses_belt(sent), "{sent:?}");
        assert!(!app_starts(sent), "{sent:?}");
        assert!(sent.contains(&PacketKind::ControlSet), "{sent:?}");
        let a = s.active_program().unwrap();
        assert_eq!(
            (a.phase, a.paused, a.wait),
            (ProgramPhase::Walking, false, None)
        );
        assert_eq!(s.program_intent.map(|i| i.speed), Some(speed));
        assert_eq!(s.resolve_target().2, crate::engine::TargetSource::Program);
        assert!(!s.hold_auto_start);
        assert_eq!(s.engage_speed, None);
        assert_eq!(s.handshake_step, crate::protocol::HandshakeStep::Idle);
    }

    /// Engine ticks (50 ms) for `n` ticks from `t`, without the program tick.
    fn engine_ticks(
        s: &mut OrchestratorState,
        engine: &mut ProtocolEngine,
        t: Instant,
        n: u64,
    ) -> Vec<PacketKind> {
        let mut sent = vec![];
        for i in 0..n {
            let now = t + Duration::from_millis(50 * i);
            if let Some(p) = engine.determine_next_packet(s, now) {
                s.last_cmd_time = now;
                sent.push(p.kind);
            }
        }
        sent
    }

    /// Was `pin_a_start_at_the_treadmill_on_a_paused_program_is_paused_again`.
    #[test]
    fn a_start_at_the_treadmill_on_a_paused_program_is_its_go() {
        let t0 = Instant::now();
        let mut s = post_meal_paused(t0);
        let mut engine = still_for(&mut s, t0 + secs(20), 5);
        let sent = start_at_the_treadmill(&mut s, &mut engine, t0 + secs(25), 5);
        assert_go_from_the_treadmill(&s, &sent, 4.0);
        // The step timer runs again with the belt.
        let left = s.active_program().unwrap().seconds_remaining.unwrap();
        run_ticks(&mut s, t0, 32, 62);
        assert_eq!(
            s.active_program().unwrap().seconds_remaining,
            Some(left - 30)
        );
    }

    /// Was `pin_a_start_at_the_treadmill_while_waiting_for_go_is_paused_again`.
    #[test]
    fn a_start_at_the_treadmill_while_waiting_for_go_is_its_go() {
        let t0 = Instant::now();
        let mut s = workday_resting_on_a_still_belt(t0);
        run_ticks(&mut s, t0, 300, 1800);
        assert!(s.hold_auto_start);
        let mut engine = still_for(&mut s, t0 + secs(1801), 5);
        let sent = start_at_the_treadmill(&mut s, &mut engine, t0 + secs(1806), 5);
        assert_go_from_the_treadmill(&s, &sent, 3.0);
        assert_eq!(s.active_program().unwrap().cycle, 2);
    }

    #[test]
    fn a_start_at_the_treadmill_waits_out_the_debounce_without_fighting() {
        let t0 = Instant::now();
        let mut s = post_meal_paused(t0);
        let mut engine = still_for(&mut s, t0 + secs(20), 5);
        // Moving, but not yet for DECK_GO_DEBOUNCE: still paused, and the app
        // sends the belt nothing but polls (no pause, no zero speed).
        s.last_status = MachineStatus::Running;
        s.hardware_speed = 1.0;
        let sent = engine_ticks(&mut s, &mut engine, t0 + secs(25), 19);
        assert!(
            sent.iter().all(|k| *k == PacketKind::PollStatus),
            "{sent:?}"
        );
        assert!(s.active_program().unwrap().paused);
        let sent = drive(
            &mut s,
            &mut engine,
            t0 + secs(25) + Duration::from_millis(950),
            2,
        );
        assert_go_from_the_treadmill(&s, &sent, 4.0);
    }

    #[test]
    fn a_stray_moving_frame_is_not_a_press() {
        let t0 = Instant::now();
        let mut s = post_meal_paused(t0);
        let mut engine = still_for(&mut s, t0 + secs(20), 5);
        // Moving for 600 ms (one frame), then still again.
        s.last_status = MachineStatus::Running;
        s.hardware_speed = 1.0;
        let mut sent = engine_ticks(&mut s, &mut engine, t0 + secs(25), 12);
        s.last_status = MachineStatus::Paused;
        s.hardware_speed = 0.0;
        sent.extend(drive(&mut s, &mut engine, t0 + secs(26), 5));
        assert!(!moves_belt(&sent) && !pauses_belt(&sent), "{sent:?}");
        assert!(s.active_program().unwrap().paused);
        // A real start afterwards still counts.
        let sent = start_at_the_treadmill(&mut s, &mut engine, t0 + secs(31), 5);
        assert_go_from_the_treadmill(&s, &sent, 4.0);
    }

    #[test]
    fn a_pause_still_winding_down_is_not_a_press() {
        // HOLD on a walking belt: the deck reports it running for a while
        // before it reports the pause. That is never GO.
        let t0 = Instant::now();
        let mut s = state();
        moving(&mut s, BEFORE.speed, BEFORE.incline);
        s.hardware_speed = 4.0;
        s.run_program(1, t0).unwrap();
        let mut engine = ProtocolEngine::new();
        engine.has_synced_reality = true;
        s.awaiting_first_packet = false;
        drive(&mut s, &mut engine, t0, 3);
        s.pause_program(t0 + secs(3)).unwrap();
        s.last_manual_interaction = t0;
        let mut sent = drive(&mut s, &mut engine, t0 + secs(3), 3);
        // The deck reports the pause between two pause handshakes.
        let mut t = t0 + secs(6);
        while s.handshake_step != crate::protocol::HandshakeStep::Idle {
            sent.extend(engine_ticks(&mut s, &mut engine, t, 1));
            t += Duration::from_millis(50);
        }
        assert!(pauses_belt(&sent), "{sent:?}");
        assert!(s.active_program().unwrap().paused);
        s.last_status = MachineStatus::Paused;
        s.hardware_speed = 0.0;
        let sent = drive(&mut s, &mut engine, t + secs(1), 5);
        assert!(!moves_belt(&sent), "{sent:?}");
        assert!(s.active_program().unwrap().paused);
    }

    #[test]
    fn the_apps_own_start_in_flight_is_not_a_press() {
        // GO, then HOLD before the deck answers: the deck then starts on the
        // app's Start. That is not a press at the treadmill, so HOLD wins.
        let t0 = Instant::now();
        let mut s = post_meal_paused(t0);
        let mut engine = still_for(&mut s, t0 + secs(20), 2);
        s.resume_program(t0 + secs(22)).unwrap();
        let sent = drive(&mut s, &mut engine, t0 + secs(22), 1);
        assert!(sent.contains(&PacketKind::HandshakeStart), "{sent:?}");
        s.pause_program(t0 + secs(23)).unwrap();
        s.last_manual_interaction = t0;
        let sent = start_at_the_treadmill(&mut s, &mut engine, t0 + secs(23), 5);
        assert!(pauses_belt(&sent), "{sent:?}");
        assert!(s.active_program().unwrap().paused);
    }

    #[test]
    fn the_deck_go_toast_names_the_program() {
        let t = ProgramToast::deck_go("Post-meal");
        assert_eq!(t.message(), "DECK START // POST-MEAL RESUMED");
        assert_eq!(
            ProgramToast::deck_go("Workday").message(),
            "DECK START // WORKDAY RESUMED"
        );
    }

    #[test]
    fn a_start_at_the_treadmill_taken_as_go_toasts_once() {
        let t0 = Instant::now();
        let mut s = post_meal_paused(t0);
        let mut engine = still_for(&mut s, t0 + secs(20), 5);
        assert_eq!(s.take_program_toast(), None);
        // Moving, not yet for the debounce: no GO, no toast.
        s.last_status = MachineStatus::Running;
        s.hardware_speed = 1.0;
        engine_ticks(&mut s, &mut engine, t0 + secs(25), 10);
        assert_eq!(s.take_program_toast(), None);
        let sent = start_at_the_treadmill(&mut s, &mut engine, t0 + secs(26), 5);
        assert_go_from_the_treadmill(&s, &sent, 4.0);
        assert_eq!(
            s.take_program_toast(),
            Some(ProgramToast::deck_go("Post-meal"))
        );
        // Not repeated while the program walks on.
        drive(&mut s, &mut engine, t0 + secs(34), 10);
        assert_eq!(s.take_program_toast(), None);
    }

    #[test]
    fn a_start_at_the_treadmill_while_waiting_for_go_toasts_the_program() {
        let t0 = Instant::now();
        let mut s = workday_resting_on_a_still_belt(t0);
        run_ticks(&mut s, t0, 300, 1800);
        let mut engine = still_for(&mut s, t0 + secs(1801), 5);
        start_at_the_treadmill(&mut s, &mut engine, t0 + secs(1806), 5);
        assert_eq!(
            s.take_program_toast().map(|t| t.message().to_string()),
            Some("DECK START // WORKDAY RESUMED".to_string())
        );
    }

    #[test]
    fn an_in_app_go_and_a_rest_walk_raise_no_deck_toast() {
        // GO in the app has its own feedback.
        let t0 = Instant::now();
        let mut s = post_meal_paused(t0);
        let mut engine = still_for(&mut s, t0 + secs(20), 2);
        s.resume_program(t0 + secs(22)).unwrap();
        let sent = drive(&mut s, &mut engine, t0 + secs(22), 1);
        assert!(sent.contains(&PacketKind::HandshakeStart), "{sent:?}");
        s.last_status = MachineStatus::Running;
        s.hardware_speed = 4.0;
        drive(&mut s, &mut engine, t0 + secs(23), 5);
        assert_eq!(s.take_program_toast(), None);

        // A start at the treadmill during a rest is a manual walk, quietly.
        let mut s = workday_resting_on_a_still_belt(t0);
        let mut engine = still_for(&mut s, t0 + secs(400), 5);
        start_at_the_treadmill(&mut s, &mut engine, t0 + secs(405), 5);
        assert_eq!(s.active_program().unwrap().phase, ProgramPhase::Resting);
        assert_eq!(s.take_program_toast(), None);
    }

    /// Unchanged from the pin: the press already won here.
    #[test]
    fn a_start_at_the_treadmill_during_a_rest_is_a_manual_walk() {
        let t0 = Instant::now();
        let mut s = workday_resting_on_a_still_belt(t0);
        let mut engine = still_for(&mut s, t0 + secs(400), 5);
        let sent = start_at_the_treadmill(&mut s, &mut engine, t0 + secs(405), 5);
        assert!(!pauses_belt(&sent), "{sent:?}");
        assert_eq!(s.active_program().unwrap().phase, ProgramPhase::Resting);
        assert!((manual(&s).speed - 1.0).abs() < 0.05, "{:?}", manual(&s));
        // The rest runs out on a moving belt: the next cycle carries on.
        run_ticks(&mut s, t0, 413, 1800);
        let a = s.active_program().unwrap();
        assert_eq!((a.phase, a.cycle, a.wait), (ProgramPhase::Walking, 2, None));
        assert_eq!(s.program_intent.map(|i| i.speed), Some(3.0));
    }

    /// Was `pin_a_start_at_the_treadmill_right_as_a_rest_begins_is_paused`.
    #[test]
    fn a_start_at_the_treadmill_right_as_a_rest_begins_is_a_manual_walk_too() {
        // The rest began at 300 s; the user starts the belt at the treadmill a
        // second later, inside the 5 s manual lockout of the rest's own
        // pause. The press is newer: it wins.
        let t0 = Instant::now();
        let mut s = workday_resting_on_a_still_belt(t0);
        let mut engine = still_for(&mut s, t0 + secs(300), 1);
        let sent = start_at_the_treadmill(&mut s, &mut engine, t0 + secs(301), 5);
        assert!(!pauses_belt(&sent), "{sent:?}");
        assert_eq!(s.active_program().unwrap().phase, ProgramPhase::Resting);
        assert!((manual(&s).speed - 1.0).abs() < 0.05, "{:?}", manual(&s));
    }

    /// A 20 minute step at 5 km/h holding 85 to 95 % of 200 bpm (170 to 190).
    fn zone_state(t0: Instant) -> OrchestratorState {
        let mut s = state();
        s.max_heart_rate = 200;
        s.programs.push(Program {
            id: 9,
            name: "Zone".to_string(),
            steps: vec![ProgramStep {
                duration_minutes: Some(20.0),
                speed_kmh: 5.0,
                incline_pct: 0.0,
                zone: Some(HeartZone {
                    low_pct: 85.0,
                    high_pct: 95.0,
                    trim: None,
                }),
                kind: None,
                pulse_end: None,
            }],
            repeat_every_minutes: None,
            note: None,
        });
        moving(&mut s, 3.0, 0);
        s.run_program(9, t0).unwrap();
        // Past the settle, so the trim works from the first tick.
        s.program.as_mut().unwrap().zone_settle_s = ZONE_SETTLE.as_secs_f64();
        s
    }

    /// `zone_state` at the very start of its step, before the pulse settles.
    fn fresh_zone_state(t0: Instant) -> OrchestratorState {
        let mut s = zone_state(t0);
        s.program.as_mut().unwrap().zone_settle_s = 0.0;
        s
    }

    const SETTLE_S: u64 = ZONE_SETTLE.as_secs();

    /// A zone step holds its speed while the pulse settles, then trims.
    #[test]
    fn a_zone_waits_for_the_pulse_to_settle_then_trims() {
        let t0 = Instant::now();
        let mut s = fresh_zone_state(t0);
        s.heart_bpm = Some(150);
        run_ticks(&mut s, t0, 0, SETTLE_S - 1);
        assert_eq!(program_speed(&s), 5.0, "held for the settle");
        let band = s.zone_band(Some(150)).unwrap();
        assert_eq!(band.settle_s, Some(1));
        assert_eq!(band.label, "ZONE // SETTLING");
        run_ticks(&mut s, t0, SETTLE_S - 1, SETTLE_S);
        assert_eq!(program_speed(&s), 5.0 + ZONE_TRIM_KMH, "trims once settled");
        assert_eq!(s.zone_band(Some(150)).unwrap().settle_s, None);
        assert_eq!(s.zone_band(Some(150)).unwrap().label, "ZONE // UNDER");
    }

    /// Only running time settles: a belt that isn't running yet (counting
    /// down, say) doesn't count toward the wait.
    #[test]
    fn the_settle_counts_only_running_time() {
        let t0 = Instant::now();
        let mut s = fresh_zone_state(t0);
        s.heart_bpm = Some(195);
        s.last_status = MachineStatus::Starting;
        run_ticks(&mut s, t0, 0, 200);
        s.last_status = MachineStatus::Running;
        run_ticks(&mut s, t0, 200, 200 + SETTLE_S - 1);
        assert_eq!(program_speed(&s), 5.0);
        run_ticks(&mut s, t0, 200 + SETTLE_S - 1, 200 + SETTLE_S);
        assert_eq!(program_speed(&s), 5.0 - ZONE_TRIM_KMH);
    }

    /// A pause and GO, and each new step, start the wait again.
    #[test]
    fn a_pause_and_go_or_a_new_step_waits_again() {
        let t0 = Instant::now();
        let mut s = zone_state(t0);
        s.heart_bpm = Some(195);
        run_ticks(&mut s, t0, 0, 1);
        assert_eq!(program_speed(&s), 5.0 - ZONE_TRIM_KMH);
        s.pause_program(t0 + secs(2)).unwrap();
        run_ticks(&mut s, t0, 2, 10);
        s.resume_program(t0 + secs(10)).unwrap();
        assert_eq!(
            s.zone_band(Some(195)).unwrap().settle_s,
            Some(SETTLE_S as u32)
        );
        run_ticks(&mut s, t0, 10, 10 + SETTLE_S - 1);
        assert_eq!(program_speed(&s), 5.0 - ZONE_TRIM_KMH, "held after GO");
        run_ticks(&mut s, t0, 10 + SETTLE_S - 1, 10 + SETTLE_S);
        assert_eq!(program_speed(&s), 5.0 - 2.0 * ZONE_TRIM_KMH);

        // Into a second zone step: the wait starts over.
        let mut s = zone_state(t0);
        let step = s.program.as_ref().unwrap().program.steps[0].clone();
        s.program.as_mut().unwrap().program.steps.push(step);
        s.heart_bpm = Some(195);
        run_ticks(&mut s, t0, 0, 20 * 60);
        assert_eq!(s.active_program().unwrap().step_index, 1);
        assert!(s.zone_band(Some(195)).unwrap().settle_s.is_some());
        let speed = program_speed(&s);
        run_ticks(&mut s, t0, 20 * 60, 20 * 60 + SETTLE_S - 2);
        assert_eq!(program_speed(&s), speed, "the second step waits too");
    }

    /// A paused zone step shows no countdown: nothing would trim it anyway.
    #[test]
    fn a_paused_zone_step_shows_no_settle() {
        let t0 = Instant::now();
        let mut s = fresh_zone_state(t0);
        s.pause_program(t0 + secs(1)).unwrap();
        assert_eq!(s.zone_band(Some(150)).unwrap().settle_s, None);
    }

    /// A 60 minute step at 4.0 km/h and 5 % (level 5 at 1 % a level) holding
    /// 85 to 95 % of 200 bpm, trimming `trim`, with ZONE_TILT on and past the
    /// settle.
    fn incline_zone_state(t0: Instant, trim: ZoneTrim) -> OrchestratorState {
        let mut s = state();
        s.max_heart_rate = 200;
        s.zone_incline_trim = true;
        s.programs.push(Program {
            id: 9,
            name: "Tilt".to_string(),
            steps: vec![ProgramStep {
                duration_minutes: Some(60.0),
                speed_kmh: 4.0,
                incline_pct: 5.0,
                zone: Some(HeartZone {
                    low_pct: 85.0,
                    high_pct: 95.0,
                    trim: Some(trim),
                }),
                kind: None,
                pulse_end: None,
            }],
            repeat_every_minutes: None,
            note: None,
        });
        moving(&mut s, 3.0, 0);
        s.run_program(9, t0).unwrap();
        s.program.as_mut().unwrap().zone_settle_s = ZONE_SETTLE.as_secs_f64();
        s
    }

    /// Every (speed, incline) the program asks for, in order, ticking each
    /// second from `from` to `to`.
    fn trims_seen(s: &mut OrchestratorState, t0: Instant, from: u64, to: u64) -> Vec<(f32, u8)> {
        let mut seen = vec![(program_speed(s), program_incline(s))];
        for t in from..to {
            run_ticks(s, t0, t, t + 1);
            let now = (program_speed(s), program_incline(s));
            if seen.last() != Some(&now) {
                seen.push(now);
            }
        }
        seen
    }

    /// STRIDE_FIT unset: a speed zone under its band raises the speed, as
    /// before #227.
    #[test]
    fn without_stride_fit_a_zone_raises_the_speed() {
        let t0 = Instant::now();
        let mut s = zone_state(t0);
        s.stride_fit_kmh = None;
        s.heart_bpm = Some(150);
        run_ticks(&mut s, t0, 0, 1);
        assert_eq!(program_speed(&s), 5.0 + ZONE_TRIM_KMH);
        assert_eq!(program_incline(&s), 0);
    }

    /// At STRIDE_FIT a speed zone that needs more effort climbs instead: one
    /// level a minute, at most 2 % above the step, and the speed stays.
    #[test]
    fn at_stride_fit_a_speed_zone_raises_the_incline_instead() {
        let t0 = Instant::now();
        let mut s = zone_state(t0);
        s.stride_fit_kmh = Some(5.0);
        s.heart_bpm = Some(150);
        let seen = trims_seen(&mut s, t0, 0, 600);
        assert_eq!(seen, vec![(5.0, 0), (5.0, 1), (5.0, 2)], "{seen:?}");
        let t = s.program.as_ref().unwrap().zone_next_trim;
        assert!(t.is_some(), "still looking, at the limit");
    }

    /// Below STRIDE_FIT the speed still rises, up to the fit and no further.
    #[test]
    fn a_speed_zone_rises_to_stride_fit_then_climbs() {
        let t0 = Instant::now();
        let mut s = zone_state(t0);
        s.stride_fit_kmh = Some(5.2);
        s.heart_bpm = Some(150);
        let seen = trims_seen(&mut s, t0, 0, 600);
        let top = seen.iter().map(|(v, _)| *v).fold(0.0, f32::max);
        assert!((top - 5.2).abs() < 1e-4, "{seen:?}");
        let first_climb = seen.iter().position(|(_, i)| *i > 0).unwrap();
        assert!(
            (seen[first_climb].0 - 5.2).abs() < 1e-4,
            "climbs only once at the fit: {seen:?}"
        );
    }

    /// The climb that stands in for speed stops at the deck's top level.
    #[test]
    fn the_climb_above_stride_fit_keeps_to_the_top_level() {
        let t0 = Instant::now();
        let mut s = zone_state(t0);
        s.max_incline = 1;
        s.stride_fit_kmh = Some(5.0);
        s.heart_bpm = Some(150);
        let seen = trims_seen(&mut s, t0, 0, 600);
        assert_eq!(seen, vec![(5.0, 0), (5.0, 1)], "{seen:?}");
    }

    /// Easing off above STRIDE_FIT: the speed comes down to the fit first,
    /// then the extra incline, then the zone's own speed trim.
    #[test]
    fn easing_above_stride_fit_lowers_the_speed_first() {
        let t0 = Instant::now();
        let mut s = zone_state(t0);
        s.stride_fit_kmh = Some(4.5);
        {
            let run = s.program.as_mut().unwrap();
            run.target.incline = 2;
        }
        let run = s.program.clone().unwrap();
        s.set_program_intent_from(&run);
        s.heart_bpm = Some(195);
        let seen = trims_seen(&mut s, t0, 0, 1000);
        let first_tilt = seen.iter().position(|(_, i)| *i < 2).unwrap();
        assert!(
            (seen[first_tilt].0 - 4.5).abs() < 1e-4,
            "the speed reached the fit before the incline moved: {seen:?}"
        );
        let flat = seen.iter().position(|(_, i)| *i == 0).unwrap();
        assert!(
            seen[..flat].iter().all(|(v, _)| *v > 4.5 - 1e-4),
            "the extra incline went before the speed went under the fit: {seen:?}"
        );
        let (low, incline) = *seen.last().unwrap();
        assert!(
            (low - 4.0).abs() < 1e-4,
            "then down to the step less 1 km/h: {seen:?}"
        );
        assert_eq!(incline, 0);
    }

    /// ZONE_TILT both: incline within its own 2 % first, then, at the fit,
    /// up to 2 % more in place of the speed; the speed never passes the fit.
    #[test]
    fn both_at_stride_fit_climbs_past_its_own_range_in_place_of_speed() {
        let t0 = Instant::now();
        let mut s = incline_zone_state(t0, ZoneTrim::Both);
        s.stride_fit_kmh = Some(4.0);
        s.heart_bpm = Some(150);
        let seen = trims_seen(&mut s, t0, 0, 1200);
        assert!(
            seen.iter().all(|(v, _)| (*v - 4.0).abs() < 1e-4),
            "{seen:?}"
        );
        assert_eq!(program_incline(&s), 9, "5 % plus 2 % plus 2 %: {seen:?}");
    }

    fn program_incline(s: &OrchestratorState) -> u8 {
        s.program_intent.expect("program intent").incline
    }

    /// Incline only: one level a minute, within 2 % of the step's incline,
    /// and the speed stays put.
    #[test]
    fn an_incline_zone_moves_one_level_a_minute_within_its_range() {
        let t0 = Instant::now();
        let mut s = incline_zone_state(t0, ZoneTrim::Incline);
        assert_eq!(program_incline(&s), 5);
        s.heart_bpm = Some(195);
        run_ticks(&mut s, t0, 0, 1);
        assert_eq!(program_incline(&s), 4);
        run_ticks(&mut s, t0, 1, 60);
        assert_eq!(program_incline(&s), 4, "a minute between levels");
        run_ticks(&mut s, t0, 60, 61);
        assert_eq!(program_incline(&s), 3);
        run_ticks(&mut s, t0, 61, 600);
        assert_eq!(program_incline(&s), 3, "2 % below the step at most");
        assert_eq!(program_speed(&s), 4.0, "the speed stays put");
        s.heart_bpm = Some(150);
        run_ticks(&mut s, t0, 600, 1200);
        assert_eq!(program_incline(&s), 7, "2 % above the step at most");
        assert_eq!(program_speed(&s), 4.0);
    }

    /// Only levels the deck runs (DECK_PROFILE), and never above its top.
    #[test]
    fn an_incline_zone_keeps_to_the_levels_the_deck_runs() {
        use crate::deck_profile::Outcome;
        let t0 = Instant::now();
        let mut s = incline_zone_state(t0, ZoneTrim::Incline);
        s.deck_profile.learn_level(4, Outcome::Took(5));
        s.deck_profile.learn_level(9, Outcome::Took(6));
        assert_eq!(s.deck_incline_levels(), vec![0, 1, 2, 3, 5, 6]);
        s.heart_bpm = Some(150);
        run_ticks(&mut s, t0, 0, 600);
        assert_eq!(program_incline(&s), 6, "the deck's top level");
        s.heart_bpm = Some(195);
        let mut seen = vec![program_incline(&s)];
        for t in 600..1400 {
            run_ticks(&mut s, t0, t, t + 1);
            seen.push(program_incline(&s));
        }
        seen.dedup();
        assert_eq!(seen, vec![6, 5, 3], "level 4 is skipped");
    }

    /// With ZONE_TILT off, a step set to trim incline trims speed, as
    /// before #126.
    #[test]
    fn with_the_labs_switch_off_every_zone_trims_speed() {
        let t0 = Instant::now();
        let mut s = incline_zone_state(t0, ZoneTrim::Incline);
        s.zone_incline_trim = false;
        s.heart_bpm = Some(195);
        run_ticks(&mut s, t0, 0, 1);
        assert_eq!(program_speed(&s), 4.0 - ZONE_TRIM_KMH);
        assert_eq!(program_incline(&s), 5);
    }

    /// Both: incline first away from the step, speed once the incline is at
    /// its limit; back the other way, speed first.
    #[test]
    fn both_moves_the_incline_first_and_unwinds_the_speed_first() {
        let t0 = Instant::now();
        let mut s = incline_zone_state(t0, ZoneTrim::Both);
        s.heart_bpm = Some(195);
        run_ticks(&mut s, t0, 0, 120);
        assert_eq!((program_incline(&s), program_speed(&s)), (3, 4.0));
        run_ticks(&mut s, t0, 120, 200);
        assert_eq!(program_incline(&s), 3);
        assert!(program_speed(&s) < 4.0, "then the speed");
        let eased = program_speed(&s);
        s.heart_bpm = Some(150);
        // Speed back up toward the step's 4.0 first, the incline held.
        run_ticks(&mut s, t0, 200, 260);
        assert!(program_speed(&s) > eased);
        assert!(program_speed(&s) < 4.0);
        assert_eq!(program_incline(&s), 3);
        run_ticks(&mut s, t0, 260, 900);
        assert_eq!(program_incline(&s), 7, "then the incline, to its limit");
        assert!(program_speed(&s) > 4.0, "and the speed after it");
    }

    /// An incline set by hand outside the range is the user's: left alone.
    /// A paused step never trims it either.
    #[test]
    fn an_incline_zone_leaves_a_hand_set_incline_and_a_paused_belt_alone() {
        let t0 = Instant::now();
        let mut s = incline_zone_state(t0, ZoneTrim::Incline);
        assert!(s.adjust_program_target(None, Some(10)));
        s.heart_bpm = Some(195);
        run_ticks(&mut s, t0, 0, 300);
        assert_eq!(program_incline(&s), 10);

        let mut s = incline_zone_state(t0, ZoneTrim::Incline);
        s.pause_program(t0 + secs(1)).unwrap();
        s.heart_bpm = Some(150);
        run_ticks(&mut s, t0, 1, 600);
        assert_eq!(s.active_program().unwrap().incline_pct, 5.0);
        assert!(s.active_program().unwrap().paused);
        assert_eq!(program_speed(&s), 0.0, "still paused");
    }

    /// A save stores speed as no trim, so old and new zones compare equal,
    /// and keeps incline and both.
    #[test]
    fn a_save_stores_speed_as_the_default_trim() {
        let mut s = state();
        let step = |trim| ProgramStep {
            duration_minutes: Some(5.0),
            speed_kmh: 4.0,
            incline_pct: 0.0,
            zone: Some(HeartZone {
                low_pct: 60.0,
                high_pct: 70.0,
                trim,
            }),
            kind: None,
            pulse_end: None,
        };
        let program = Program {
            id: 0,
            name: "T".to_string(),
            steps: vec![
                step(Some(ZoneTrim::Speed)),
                step(Some(ZoneTrim::Incline)),
                step(Some(ZoneTrim::Both)),
                step(None),
            ],
            repeat_every_minutes: None,
            note: None,
        };
        s.save_program(program).unwrap();
        let saved = s.programs.last().unwrap();
        let trims: Vec<_> = saved.steps.iter().map(|s| s.zone.unwrap().trim).collect();
        assert_eq!(
            trims,
            vec![None, Some(ZoneTrim::Incline), Some(ZoneTrim::Both), None]
        );
        // A zone saved before #126 reads as speed, and writes back the same.
        let old = r#"{"duration_minutes":5.0,"speed_kmh":4.0,"incline_pct":0.0,"zone":{"low_pct":60.0,"high_pct":70.0}}"#;
        let read: ProgramStep = serde_json::from_str(old).unwrap();
        assert_eq!(read.zone.unwrap().trim, None);
        assert_eq!(serde_json::to_string(&read).unwrap(), old);
        let tilt: ProgramStep = serde_json::from_str(
            r#"{"duration_minutes":5.0,"speed_kmh":4.0,"incline_pct":0.0,"zone":{"low_pct":60.0,"high_pct":70.0,"trim":"incline"}}"#,
        )
        .unwrap();
        assert_eq!(tilt.zone.unwrap().trim, Some(ZoneTrim::Incline));
    }

    #[test]
    fn the_chart_band_is_the_running_zone_in_bpm() {
        let t0 = Instant::now();
        assert_eq!(state().zone_band(Some(150)), None);
        let mut s = zone_state(t0);
        let band = s.zone_band(None).expect("band");
        assert_eq!((band.low_bpm, band.high_bpm), (170, 190));
        assert_eq!(band.position, None);
        // A step without a zone has no band.
        s.program.as_mut().unwrap().program.steps[0].zone = None;
        assert_eq!(s.zone_band(Some(150)), None);
        // Nor does a rest between cycles.
        let mut s = zone_state(t0);
        s.program.as_mut().unwrap().phase = ProgramPhase::Resting;
        assert_eq!(s.zone_band(Some(150)), None);
    }

    #[test]
    fn the_zone_position_agrees_with_the_trim() {
        use crate::heart_rate::ZonePosition;
        let t0 = Instant::now();
        // 85 to 95 % of 190 is 161.5 to 180.5 bpm: 161 is under, 181 over.
        let mut s = zone_state(t0);
        s.max_heart_rate = 190;
        let at = |s: &OrchestratorState, bpm| s.zone_band(Some(bpm)).and_then(|b| b.position);
        assert_eq!(at(&s, 161), Some(ZonePosition::Under));
        assert_eq!(at(&s, 162), Some(ZonePosition::In));
        assert_eq!(at(&s, 180), Some(ZonePosition::In));
        assert_eq!(at(&s, 181), Some(ZonePosition::Over));
        // The trim slows the belt exactly when the HUD says over.
        s.heart_bpm = Some(181);
        run_ticks(&mut s, t0, 0, 21);
        assert!(program_speed(&s) < 5.0, "over trims the speed down");
        // And holds it in the zone.
        let held = program_speed(&s);
        s.heart_bpm = Some(180);
        run_ticks(&mut s, t0, 21, 80);
        assert_eq!(program_speed(&s), held);
        // A paused step still shows where the pulse is.
        let mut s = zone_state(t0);
        s.program.as_mut().unwrap().paused = true;
        assert_eq!(at(&s, 150), Some(ZonePosition::Under));
    }

    fn program_speed(s: &OrchestratorState) -> f32 {
        s.program_intent.expect("program intent").speed
    }

    #[test]
    fn a_zone_slows_the_belt_above_it_and_speeds_it_up_below_it() {
        let t0 = Instant::now();
        let mut s = zone_state(t0);
        s.heart_bpm = Some(195);
        run_ticks(&mut s, t0, 0, 21);
        assert!(program_speed(&s) < 5.0);
        let slowed = program_speed(&s);
        s.heart_bpm = Some(150);
        run_ticks(&mut s, t0, 21, 80);
        assert!(program_speed(&s) > slowed);
        // Inside the zone nothing changes.
        let held = program_speed(&s);
        s.heart_bpm = Some(180);
        run_ticks(&mut s, t0, 80, 200);
        assert_eq!(program_speed(&s), held);
    }

    #[test]
    fn a_zone_trims_slowly_and_within_a_range_of_the_step() {
        let t0 = Instant::now();
        let mut s = zone_state(t0);
        s.heart_bpm = Some(195);
        run_ticks(&mut s, t0, 0, 19);
        assert_eq!(program_speed(&s), 5.0 - ZONE_TRIM_KMH);
        run_ticks(&mut s, t0, 19, 900);
        assert!((program_speed(&s) - (5.0 - ZONE_TRIM_RANGE_KMH)).abs() < 0.001);
    }

    #[test]
    fn a_zone_changes_nothing_without_a_reading_or_a_running_belt() {
        let t0 = Instant::now();
        let mut s = zone_state(t0);
        s.heart_bpm = None;
        run_ticks(&mut s, t0, 0, 60);
        assert_eq!(program_speed(&s), 5.0);
        s.heart_bpm = Some(120);
        s.last_status = MachineStatus::Paused;
        run_ticks(&mut s, t0, 60, 120);
        assert_eq!(program_speed(&s), 5.0);
    }

    #[test]
    fn heart_rate_never_moves_a_paused_program() {
        let t0 = Instant::now();
        let mut s = zone_state(t0);
        s.pause_program(t0 + secs(1)).unwrap();
        s.heart_bpm = Some(120);
        run_ticks(&mut s, t0, 1, 120);
        assert!(s.active_program().unwrap().paused);
        assert_eq!(s.active_program().unwrap().speed_kmh, 5.0);
    }

    #[test]
    fn zones_are_checked_when_saving() {
        let step = |speed_kmh: f32, low_pct: f32, high_pct: f32| ProgramStep {
            duration_minutes: Some(5.0),
            speed_kmh,
            incline_pct: 0.0,
            zone: Some(HeartZone {
                low_pct,
                high_pct,
                trim: None,
            }),
            kind: None,
            pulse_end: None,
        };
        let program = |s: ProgramStep| Program {
            id: 0,
            name: "Z".to_string(),
            steps: vec![s],
            repeat_every_minutes: None,
            note: None,
        };
        assert!(validate_program(&program(step(4.0, 85.0, 95.0)), 0.6, 12.0).is_ok());
        assert!(validate_program(&program(step(4.0, 95.0, 85.0)), 0.6, 12.0).is_err());
        assert!(validate_program(&program(step(4.0, 85.0, 120.0)), 0.6, 12.0).is_err());
        assert!(validate_program(&program(step(0.0, 85.0, 95.0)), 0.6, 12.0).is_err());
        // The bounds themselves are allowed; below 30 %, equal or not a number is not.
        assert!(validate_program(&program(step(4.0, 30.0, 100.0)), 0.6, 12.0).is_ok());
        for (low, high) in [
            (29.9, 60.0),
            (80.0, 80.0),
            (f32::NAN, 90.0),
            (60.0, f32::INFINITY),
        ] {
            let e = validate_program(&program(step(4.0, low, high)), 0.6, 12.0).unwrap_err();
            assert!(e.starts_with("Step 1: zone must be"), "{low} {high}: {e}");
        }
    }

    /// The seeded VO2 max holds 85 to 95 % on its four climbs and nowhere else.
    #[test]
    fn the_seeded_vo2_max_holds_its_climbs_in_the_zone() {
        let p = seed_programs()
            .into_iter()
            .find(|p| p.name == "VO2 max")
            .unwrap();
        assert_eq!(p.id, 6);
        assert_eq!(p.steps.len(), 10);
        for s in &p.steps {
            let climb = s.incline_pct > 0.0;
            assert_eq!(
                s.zone,
                climb.then_some(HeartZone {
                    low_pct: 85.0,
                    high_pct: 95.0,
                    trim: None,
                }),
                "{:?}",
                s
            );
        }
        assert_eq!(p.steps.iter().filter(|s| s.zone.is_some()).count(), 4);
        // At the default 190 bpm that is 161.5 to 180.5 bpm.
        let (low, high) = p.steps[1].zone.unwrap().bounds_bpm(190);
        assert!((low - 161.5).abs() < 0.01 && (high - 180.5).abs() < 0.01);
    }

    /// A save keeps each step's zone, and a rename with the zones intact keeps
    /// the seeded note (the editor used to drop the zone, which lost both).
    #[test]
    fn a_save_keeps_zones_and_a_rename_keeps_the_note() {
        let mut s = state();
        let seeded = seed_programs()
            .into_iter()
            .find(|p| p.name == "VO2 max")
            .unwrap();
        s.programs.push(seeded.clone());
        let mut edit = seeded.clone();
        edit.name = "My 4x4".to_string();
        edit.note = None;
        s.save_program(edit).unwrap();
        let saved = s.programs.iter().find(|p| p.id == seeded.id).unwrap();
        assert_eq!(saved.steps, seeded.steps);
        assert_eq!(saved.note.as_deref(), Some(NOTE_VO2_MAX));

        let mut edit = saved.clone();
        edit.steps[1].zone = Some(HeartZone {
            low_pct: 70.0,
            high_pct: 80.0,
            trim: None,
        });
        s.save_program(edit).unwrap();
        let saved = s.programs.iter().find(|p| p.id == seeded.id).unwrap();
        assert_eq!(saved.steps[1].zone.unwrap().low_pct, 70.0);
        assert_eq!(saved.note, None, "a new zone changes what the program does");
    }

    // -----------------------------------------------------------------------
    // Step kinds (#235).
    // -----------------------------------------------------------------------

    fn kinded(kind: Option<StepKind>, speed_kmh: f32) -> ProgramStep {
        ProgramStep {
            duration_minutes: Some(2.0),
            speed_kmh,
            incline_pct: 0.0,
            zone: None,
            kind,
            pulse_end: None,
        }
    }

    /// A step saved before #235 has no `kind` and loads as none; a step
    /// with none saves without the field, so old and new JSON match.
    #[test]
    fn a_step_without_a_kind_loads_and_saves_as_before() {
        let old = r#"{"duration_minutes":5.0,"speed_kmh":3.0,"incline_pct":5.0}"#;
        let step: ProgramStep = serde_json::from_str(old).unwrap();
        assert_eq!(step.kind, None);
        assert_eq!(serde_json::to_string(&step).unwrap(), old);
        let warm: ProgramStep = serde_json::from_str(
            r#"{"duration_minutes":5.0,"speed_kmh":3.0,"incline_pct":5.0,"kind":"warm_up"}"#,
        )
        .unwrap();
        assert_eq!(warm.kind, Some(StepKind::WarmUp));
        let codes: Vec<String> = StepKind::ALL
            .iter()
            .map(|k| serde_json::to_string(k).unwrap())
            .collect();
        assert_eq!(
            codes,
            [
                r#""warm_up""#,
                r#""walk""#,
                r#""interval""#,
                r#""rest""#,
                r#""cool_down""#
            ]
        );
        let labels: Vec<&str> = StepKind::ALL.iter().map(|k| k.code()).collect();
        assert_eq!(labels, ["WARM_UP", "WALK", "INTERVAL", "REST", "COOL_DOWN"]);
    }

    /// The count is within the step's kind: the 3rd interval of 5 is
    /// `INTERVAL 3/5` wherever it falls in the list; a kind the program has
    /// once shows its code alone; a step with no kind has no label.
    #[test]
    fn a_kind_counts_its_own_steps() {
        let p = seed_programs()
            .into_iter()
            .find(|p| p.name == "Interval walk")
            .unwrap();
        let spot = |i| StepKindSpot::of_step(&p.steps, i).unwrap();
        assert_eq!(spot(0).label, "WARM_UP");
        assert_eq!((spot(0).nth, spot(0).of), (1, 1));
        assert_eq!(spot(5).label, "INTERVAL 3/5");
        assert_eq!(spot(5).kind, StepKind::Interval);
        assert_eq!(spot(10).label, "REST 5/5");
        assert_eq!(StepKindSpot::of_step(&p.steps, 11), None);
        let mixed = [kinded(None, 3.0), kinded(Some(StepKind::Walk), 3.0)];
        assert_eq!(StepKindSpot::of_step(&mixed, 0), None);
        assert_eq!(StepKindSpot::of_step(&mixed, 1).unwrap().label, "WALK");
    }

    /// The running program carries its walking step's kind; a rest between
    /// cycles carries none, and a step with no kind shows as before.
    #[test]
    fn the_running_program_shows_its_step_kind() {
        let mut s = state();
        s.programs.push(Program {
            id: 4,
            name: "Kinds".to_string(),
            steps: vec![
                kinded(Some(StepKind::Interval), 3.0),
                kinded(None, 4.0),
                kinded(Some(StepKind::Interval), 5.0),
            ],
            repeat_every_minutes: Some(10.0),
            note: None,
        });
        moving(&mut s, 2.0, 0);
        let t0 = Instant::now();
        s.run_program(4, t0).unwrap();
        let label = |s: &OrchestratorState| s.active_program().unwrap().kind.map(|k| k.label);
        assert_eq!(label(&s).as_deref(), Some("INTERVAL 1/2"));
        s.skip_program(t0).unwrap();
        assert_eq!(label(&s), None);
        s.skip_program(t0).unwrap();
        assert_eq!(label(&s).as_deref(), Some("INTERVAL 2/2"));
        s.skip_program(t0).unwrap(); // last step -> Resting
        assert_eq!(s.active_program().unwrap().phase, ProgramPhase::Resting);
        assert_eq!(label(&s), None);
    }

    /// A kind is a name: changing one keeps the seeded note, as a rename does.
    #[test]
    fn changing_a_kind_keeps_the_note() {
        let mut s = state();
        let seeded = seed_programs()
            .into_iter()
            .find(|p| p.name == "Interval walk")
            .unwrap();
        s.programs.push(seeded.clone());
        let mut edit = seeded.clone();
        edit.note = None;
        edit.steps[10].kind = Some(StepKind::CoolDown);
        edit.steps[0].kind = None;
        s.save_program(edit).unwrap();
        let saved = s.programs.iter().find(|p| p.id == seeded.id).unwrap();
        assert_eq!(saved.steps[10].kind, Some(StepKind::CoolDown));
        assert_eq!(saved.steps[0].kind, None);
        assert_eq!(saved.note.as_deref(), Some(NOTE_INTERVAL_WALK));
    }

    /// Every seeded step has a kind.
    #[test]
    fn every_seeded_step_has_a_kind() {
        for p in seed_programs() {
            assert!(p.steps.iter().all(|s| s.kind.is_some()), "{}", p.name);
        }
    }

    // -----------------------------------------------------------------------
    // A pause at the treadmill (#158).
    // -----------------------------------------------------------------------

    /// Post-meal walking at 4.0 km/h on a synced engine for 3 s, then paused
    /// at the treadmill and reported Paused for 5 s.
    fn post_meal_paused_at_the_treadmill(t0: Instant) -> (OrchestratorState, ProtocolEngine) {
        let mut s = state();
        moving(&mut s, BEFORE.speed, BEFORE.incline);
        s.hardware_speed = 4.0;
        s.run_program(1, t0).unwrap();
        let mut engine = ProtocolEngine::new();
        engine.has_synced_reality = true;
        s.awaiting_first_packet = false;
        drive(&mut s, &mut engine, t0, 3);
        assert!(!s.active_program().unwrap().paused);
        s.last_status = MachineStatus::Paused;
        s.hardware_speed = 0.0;
        let sent = drive(&mut s, &mut engine, t0 + secs(3), 5);
        assert!(!moves_belt(&sent) && !pauses_belt(&sent), "{sent:?}");
        (s, engine)
    }

    #[test]
    fn a_pause_at_the_treadmill_pauses_the_walking_program() {
        let t0 = Instant::now();
        let (mut s, mut engine) = post_meal_paused_at_the_treadmill(t0);
        let a = s.active_program().unwrap();
        assert_eq!(
            (a.phase, a.paused, a.wait),
            (ProgramPhase::Walking, true, None)
        );
        assert_eq!(a.step_index, 0);
        assert_eq!(s.program_intent.map(|i| i.speed), Some(0.0));
        // The step timer holds, and nothing restarts the belt.
        let left = a.seconds_remaining;
        let sent = drive(&mut s, &mut engine, t0 + secs(8), 60);
        assert!(!moves_belt(&sent), "{sent:?}");
        assert_eq!(s.active_program().unwrap().seconds_remaining, left);
        assert!(s.active_program().unwrap().paused);
    }

    #[test]
    fn go_after_a_pause_at_the_treadmill_resumes_the_step() {
        let t0 = Instant::now();
        let (mut s, mut engine) = post_meal_paused_at_the_treadmill(t0);
        s.resume_program(t0 + secs(8)).unwrap();
        assert_eq!(s.handshake_step, crate::protocol::HandshakeStep::Priming);
        let sent = drive(&mut s, &mut engine, t0 + secs(8), 2);
        assert!(app_starts(&sent), "{sent:?}");
        // The deck still reports Paused until it answers: GO stands.
        let a = s.active_program().unwrap();
        assert_eq!((a.paused, a.step_index), (false, 0));
        assert_eq!(s.program_intent.map(|i| i.speed), Some(4.0));
    }

    #[test]
    fn engage_after_a_pause_at_the_treadmill_is_the_programs_go() {
        let t0 = Instant::now();
        let (mut s, _) = post_meal_paused_at_the_treadmill(t0);
        s.set_test_now(t0 + secs(8));
        s.trigger_action(crate::engine::DeckAction::Start);
        let a = s.active_program().unwrap();
        assert_eq!((a.paused, a.wait), (false, None));
        assert_eq!(s.resolve_target().2, crate::engine::TargetSource::Program);
    }

    #[test]
    fn a_start_at_the_treadmill_after_its_pause_is_the_programs_go() {
        let t0 = Instant::now();
        let (mut s, mut engine) = post_meal_paused_at_the_treadmill(t0);
        // The deck counts down, then resumes at the speed it paused at.
        s.last_status = MachineStatus::Starting;
        let mut sent = drive(&mut s, &mut engine, t0 + secs(8), 3);
        s.last_status = MachineStatus::Running;
        s.hardware_speed = 4.0;
        sent.extend(drive(&mut s, &mut engine, t0 + secs(11), 5));
        assert!(!app_starts(&sent) && !pauses_belt(&sent), "{sent:?}");
        let a = s.active_program().unwrap();
        assert_eq!((a.paused, a.wait), (false, None));
        assert_eq!(s.program_intent.map(|i| i.speed), Some(4.0));
        assert_eq!(
            s.take_program_toast(),
            Some(ProgramToast::deck_go("Post-meal"))
        );
    }

    /// Run on a paused deck: the deck reports Paused until it answers the
    /// program's start. That is not a pause at the treadmill.
    #[test]
    fn a_paused_report_before_the_belt_moved_is_not_a_pause() {
        let t0 = Instant::now();
        let mut s = state();
        s.last_status = MachineStatus::Paused;
        s.manual_intent = Some(ControlIntent {
            speed: 0.0,
            incline: 0,
        });
        let mut engine = still_for(&mut s, t0, 1);
        s.run_program(1, t0 + secs(1)).unwrap();
        let sent = drive(&mut s, &mut engine, t0 + secs(1), 2);
        assert!(app_starts(&sent), "{sent:?}");
        assert!(!s.active_program().unwrap().paused);
    }

    // -----------------------------------------------------------------------
    // Cool-down ending on heart rate (#233).
    // -----------------------------------------------------------------------

    /// 5 min at 5 km/h, then a 5 min COOL_DOWN at 3 km/h that ends at or
    /// under 60 % of 200 bpm (120), then `after` if given. Running on a belt
    /// that walked 3.0 km/h before.
    fn cool_down_state(t0: Instant, after: Option<ProgramStep>) -> OrchestratorState {
        let mut s = state();
        s.max_heart_rate = 200;
        let walk = ProgramStep {
            duration_minutes: Some(5.0),
            speed_kmh: 5.0,
            incline_pct: 0.0,
            zone: None,
            kind: Some(StepKind::Walk),
            pulse_end: None,
        };
        let cool = ProgramStep {
            duration_minutes: Some(5.0),
            speed_kmh: 3.0,
            incline_pct: 0.0,
            zone: None,
            kind: Some(StepKind::CoolDown),
            pulse_end: Some(PulseEnd { below_pct: 60.0 }),
        };
        let mut steps = vec![walk, cool];
        steps.extend(after);
        s.programs.push(Program {
            id: 9,
            name: "Cool".to_string(),
            steps,
            repeat_every_minutes: None,
            note: None,
        });
        moving(&mut s, 3.0, 0);
        s.run_program(9, t0).unwrap();
        s
    }

    fn step_now(s: &OrchestratorState) -> Option<u32> {
        s.active_program().map(|a| a.step_index)
    }

    #[test]
    fn a_pulse_already_down_ends_the_cool_down_on_its_minutes() {
        let t0 = Instant::now();
        let mut s = cool_down_state(t0, None);
        s.heart_bpm = Some(110);
        run_ticks(&mut s, t0, 0, 599);
        assert_eq!(step_now(&s), Some(1));
        assert_eq!(s.active_program().unwrap().seconds_remaining, Some(1));
        run_ticks(&mut s, t0, 599, 600);
        // The last step ended the program the normal way: the belt goes back
        // to its speed from before.
        assert!(s.program.is_none());
        assert_eq!(s.program_intent, None);
        assert_eq!(manual(&s).speed, 3.0);
    }

    #[test]
    fn a_high_pulse_keeps_the_cool_down_walking_until_it_stays_down() {
        let t0 = Instant::now();
        let mut s = cool_down_state(t0, None);
        s.heart_bpm = Some(140);
        // No upper cap: well past its minutes it still walks, at its speed.
        run_ticks(&mut s, t0, 0, 1800);
        let a = s.active_program().unwrap();
        assert_eq!(a.step_index, 1);
        assert_eq!((a.pulse_to_bpm, a.seconds_remaining), (Some(120), None));
        assert_eq!(s.program_intent.map(|i| i.speed), Some(3.0));
        let t = s.pulse_target(s.heart_bpm).unwrap();
        assert_eq!((t.target_bpm, t.label.as_str()), (120, "PULSE // HIGH"));

        // At the target counts as down; 30 s of it ends the step.
        s.heart_bpm = Some(120);
        run_ticks(&mut s, t0, 1800, 1829);
        assert!(s.program.is_some());
        assert_eq!(s.pulse_target(Some(120)).unwrap().hold_s, Some(1));
        run_ticks(&mut s, t0, 1829, 1830);
        assert!(s.program.is_none());
        assert_eq!(manual(&s).speed, 3.0);
    }

    #[test]
    fn a_short_dip_or_a_reading_over_the_target_starts_the_hold_again() {
        let t0 = Instant::now();
        let mut s = cool_down_state(t0, None);
        s.heart_bpm = Some(140);
        run_ticks(&mut s, t0, 0, 700);
        s.heart_bpm = Some(115);
        run_ticks(&mut s, t0, 700, 720);
        s.heart_bpm = Some(121);
        run_ticks(&mut s, t0, 720, 721);
        s.heart_bpm = Some(115);
        run_ticks(&mut s, t0, 721, 750);
        assert!(s.program.is_some(), "the hold started over at 721");
        run_ticks(&mut s, t0, 750, 751);
        assert!(s.program.is_none());
    }

    #[test]
    fn with_no_reading_the_cool_down_ends_on_its_minutes() {
        let t0 = Instant::now();
        let mut s = cool_down_state(t0, None);
        s.heart_bpm = None;
        run_ticks(&mut s, t0, 0, 599);
        assert_eq!(step_now(&s), Some(1));
        assert_eq!(s.pulse_target(None).unwrap().label, "PULSE // WAIT");
        run_ticks(&mut s, t0, 599, 600);
        assert!(s.program.is_none());
    }

    #[test]
    fn a_sensor_that_drops_out_mid_step_hands_back_to_the_minutes() {
        let t0 = Instant::now();
        let mut s = cool_down_state(t0, None);
        // Drops out during the minimum: the minutes still end the step.
        s.heart_bpm = Some(140);
        run_ticks(&mut s, t0, 0, 400);
        s.heart_bpm = None;
        run_ticks(&mut s, t0, 400, 600);
        assert!(s.program.is_none());

        // Drops out while the pulse holds the step past its minutes: the
        // minutes are done, so the step ends at once.
        let mut s = cool_down_state(t0, None);
        s.heart_bpm = Some(140);
        run_ticks(&mut s, t0, 0, 700);
        assert_eq!(s.active_program().unwrap().pulse_to_bpm, Some(120));
        s.heart_bpm = None;
        run_ticks(&mut s, t0, 700, 701);
        assert!(s.program.is_none());
    }

    #[test]
    fn skip_and_end_still_end_a_cool_down_the_pulse_keeps_going() {
        let t0 = Instant::now();
        let mut s = cool_down_state(t0, None);
        s.heart_bpm = Some(140);
        run_ticks(&mut s, t0, 0, 700);
        s.skip_program(t0 + secs(700)).unwrap();
        assert!(s.program.is_none());
        assert_eq!(manual(&s).speed, 3.0);

        let mut s = cool_down_state(t0, None);
        s.heart_bpm = Some(140);
        run_ticks(&mut s, t0, 0, 700);
        assert!(s.end_program(true, t0 + secs(700)));
        assert_eq!(manual(&s).speed, 3.0);
    }

    #[test]
    fn the_step_after_a_held_cool_down_gets_its_full_minutes() {
        let t0 = Instant::now();
        let after = ProgramStep {
            duration_minutes: Some(2.0),
            speed_kmh: 4.0,
            incline_pct: 0.0,
            zone: None,
            kind: None,
            pulse_end: None,
        };
        let mut s = cool_down_state(t0, Some(after));
        s.heart_bpm = Some(140);
        run_ticks(&mut s, t0, 0, 900);
        s.heart_bpm = Some(100);
        run_ticks(&mut s, t0, 900, 930);
        let a = s.active_program().unwrap();
        assert_eq!((a.step_index, a.seconds_remaining), (2, Some(120)));
        assert_eq!(a.pulse_to_bpm, None);
        // Between two moving speeds: automatic, like any timed step.
        assert_eq!(s.program_intent.map(|i| i.speed), Some(4.0));
    }

    #[test]
    fn a_pause_keeps_a_held_cool_down_and_its_hold_starts_again_after_go() {
        let t0 = Instant::now();
        let mut s = cool_down_state(t0, None);
        s.heart_bpm = Some(140);
        run_ticks(&mut s, t0, 0, 700);
        s.heart_bpm = Some(110);
        run_ticks(&mut s, t0, 700, 720);
        s.pause_program(t0 + secs(720)).unwrap();
        s.last_status = MachineStatus::Paused;
        // Paused, with or without a reading, the step waits for GO.
        run_ticks(&mut s, t0, 720, 800);
        s.heart_bpm = None;
        run_ticks(&mut s, t0, 800, 900);
        let a = s.active_program().unwrap();
        assert_eq!((a.step_index, a.paused), (1, true));
        // Heart rate never resumes the belt.
        assert_eq!(s.program_intent.map(|i| i.speed), Some(0.0));

        s.heart_bpm = Some(110);
        s.resume_program(t0 + secs(900)).unwrap();
        s.last_status = MachineStatus::Running;
        run_ticks(&mut s, t0, 900, 929);
        assert!(s.program.is_some(), "a full 30 s again after GO");
        run_ticks(&mut s, t0, 929, 930);
        assert!(s.program.is_none());
    }

    #[test]
    fn the_cool_down_target_shows_while_the_step_runs_and_follows_the_max() {
        let t0 = Instant::now();
        let mut s = cool_down_state(t0, None);
        assert_eq!(s.pulse_target(Some(150)), None, "the walk step has none");
        s.heart_bpm = Some(150);
        run_ticks(&mut s, t0, 0, 310);
        let t = s.pulse_target(Some(150)).unwrap();
        assert_eq!((t.target_bpm, t.down), (120, Some(false)));
        // During the minimum the countdown shows, not the pulse wait.
        let a = s.active_program().unwrap();
        assert_eq!((a.pulse_to_bpm, a.seconds_remaining), (None, Some(290)));
        // 60 % of 190 is 114, as the editor shows it.
        s.max_heart_rate = 190;
        assert_eq!(s.pulse_target(None).unwrap().target_bpm, 114);
    }

    #[test]
    fn a_heart_rate_end_is_only_for_a_timed_walking_cool_down() {
        let ok = |step: ProgramStep| {
            validate_program(
                &Program {
                    id: 0,
                    name: "Cool".to_string(),
                    steps: vec![step],
                    repeat_every_minutes: None,
                    note: None,
                },
                0.6,
                12.0,
            )
        };
        let cool = ProgramStep {
            duration_minutes: Some(5.0),
            speed_kmh: 3.0,
            incline_pct: 0.0,
            zone: None,
            kind: Some(StepKind::CoolDown),
            pulse_end: Some(PulseEnd {
                below_pct: PULSE_END_DEFAULT_PCT,
            }),
        };
        ok(cool.clone()).unwrap();
        let with = |f: &dyn Fn(&mut ProgramStep)| {
            let mut s = cool.clone();
            f(&mut s);
            ok(s).unwrap_err()
        };
        assert!(with(&|s| s.kind = Some(StepKind::Walk)).contains("COOL_DOWN"));
        assert!(with(&|s| s.kind = None).contains("COOL_DOWN"));
        assert!(with(&|s| s.duration_minutes = None).contains("minutes"));
        assert!(with(&|s| s.speed_kmh = 0.0).contains("walking speed"));
        assert!(with(&|s| {
            s.zone = Some(HeartZone {
                low_pct: 50.0,
                high_pct: 60.0,
                trim: None,
            })
        })
        .contains("heart zone or"));
        assert!(with(&|s| s.pulse_end = Some(PulseEnd { below_pct: 49.0 })).contains("50 to 76"));
        assert!(with(&|s| s.pulse_end = Some(PulseEnd { below_pct: 77.0 })).contains("50 to 76"));
        assert!(with(&|s| s.pulse_end = Some(PulseEnd {
            below_pct: f32::NAN
        }))
        .contains("50 to 76"));
    }

    /// A step saved before #233 has no `pulse_end` and loads as none; a
    /// step with none saves without the field, so old and new JSON match.
    #[test]
    fn a_step_without_a_heart_rate_end_loads_and_saves_as_before() {
        let old =
            r#"{"duration_minutes":3.0,"speed_kmh":3.0,"incline_pct":0.0,"kind":"cool_down"}"#;
        let step: ProgramStep = serde_json::from_str(old).unwrap();
        assert_eq!(step.pulse_end, None);
        assert!(!serde_json::to_string(&step).unwrap().contains("pulse_end"));
        let new = r#"{"duration_minutes":3.0,"speed_kmh":3.0,"incline_pct":0.0,"kind":"cool_down","pulse_end":{"below_pct":60.0}}"#;
        let step: ProgramStep = serde_json::from_str(new).unwrap();
        assert_eq!(step.pulse_end, Some(PulseEnd { below_pct: 60.0 }));
    }
}
