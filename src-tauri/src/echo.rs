//! ECHO: turn a saved walk into a program. The walk's speed and incline over
//! time are split into steady stretches, smoothed into a handful of steps and
//! saved as a new program (`ECHO // <date>`) through the same save path as the
//! program editor. The program is only saved, never started: the belt still
//! needs the user's press.
//!
//! The segmentation (`echo_steps`) is pure; `program_from_session` loads the
//! walk's samples and saves the result.

use chrono::{NaiveDate, TimeZone};
use serde::Serialize;
use tauri::AppHandle;

use crate::bluetooth::ORCHESTRATOR;
use crate::engine::{OrchestratorState, STILL_KMH};
use crate::persistence;
use crate::programs::{Program, ProgramStep, MAX_NAME_CHARS, MAX_STEPS};

/// What the History button gets back: the new program, to open in the editor.
#[derive(Debug, Clone, PartialEq, Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct EchoProgram {
    pub id: u32,
    pub name: String,
    pub step_count: u32,
}

/// One reading of the walk: when, and the calibrated speed and incline.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EchoSample {
    pub t_ms: i64,
    pub speed_kmh: f32,
    pub incline_pct: f32,
}

/// The belt limits and incline calibration the steps are fitted to.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EchoLimits {
    /// Calibrated slowest speed the belt runs.
    pub min_kmh: f32,
    /// Calibrated top speed.
    pub max_kmh: f32,
    pub tilt_multiplier: f32,
    pub incline_offset: f32,
}

/// Why a walk gave no program, in words fit to show under the button.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EchoError {
    /// No samples at all (a walk saved before samples, or a simulated one).
    NoReadings,
    /// Less walking than `MIN_WALK_S`.
    TooShort,
    /// No stretch held one speed and incline for `MIN_STEP_S`.
    NoSteadyStretch,
}

impl EchoError {
    pub fn message(self) -> String {
        match self {
            Self::NoReadings => "The walk has no speed readings for ECHO.".to_string(),
            Self::TooShort => format!(
                "ECHO needs a walk of at least {} minutes.",
                MIN_WALK_S as u32 / 60
            ),
            Self::NoSteadyStretch => {
                "The walk has no steady stretch of a minute or more for ECHO.".to_string()
            }
        }
    }
}

/// Shown when the walk is still being recorded.
const STILL_RECORDING: &str = "ECHO waits for the walk to end.";
/// Shown when the walk was deleted before the click landed.
const WALK_GONE: &str = "The walk is no longer in History.";

// ---------------------------------------------------------------------------
// Limits. Every number here shapes the program a walk turns into.
// ---------------------------------------------------------------------------

/// The engine saves one sample a second; the last sample stands for this long.
const SAMPLE_S: f64 = 1.0;
/// A longer gap between samples is the app closed for an update, a lost link
/// or the pause between joined sessions, not time walked at that speed.
const MAX_SAMPLE_GAP_S: f64 = 5.0;
/// Less walking than this is a test or a false start, not a walk to repeat.
const MIN_WALK_S: f64 = 180.0;
/// The shortest step: changes shorter than a minute are the belt ramping
/// between speeds or a quick nudge, and a step that short is hard to follow
/// on the HUD.
const MIN_STEP_S: f64 = 60.0;
/// A stretch holds while the speed stays this close to its average. Deck
/// speeds are about 0.1 km/h apart, so a one or two byte wobble (neural flow,
/// a 0.1 km/h heart zone trim) stays in the stretch, while a change of 0.5 km/h
/// or more by hand starts a new one.
const SPEED_BAND_KMH: f64 = 0.25;
/// The incline moves in deck levels, usually 0.5 % or more apart; this only
/// absorbs float rounding, so any level change starts a new stretch.
const INCLINE_BAND_PCT: f64 = 0.25;
/// A handful of steps: room for a walk shaped like the seeded Interval walk
/// (11 steps), well under the editor's `MAX_STEPS`.
const MAX_ECHO_STEPS: usize = 12;
/// Step lengths are rounded to half a minute, so the editor shows round
/// numbers; the rounding is on the running total, so the program keeps the
/// walk's length.
const STEP_MINUTES: f64 = 0.5;
/// Speeds are rounded to the 0.1 km/h the program editor shows (the save then
/// snaps each to a speed the deck runs).
const SPEED_STEP_KMH: f64 = 0.1;

const _: () = assert!(MAX_ECHO_STEPS <= MAX_STEPS);

// ---------------------------------------------------------------------------
// Segmentation
// ---------------------------------------------------------------------------

/// A run of walking at about one speed and incline: its length and its
/// time-weighted averages.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Stretch {
    secs: f64,
    speed: f64,
    incline: f64,
}

impl Stretch {
    /// Fold `other` in, keeping time-weighted averages.
    fn absorb(&mut self, other: &Stretch) {
        let total = self.secs + other.secs;
        if total <= 0.0 {
            return;
        }
        self.speed = (self.speed * self.secs + other.speed * other.secs) / total;
        self.incline = (self.incline * self.secs + other.incline * other.secs) / total;
        self.secs = total;
    }

    fn alike(&self, other: &Stretch) -> bool {
        (self.speed - other.speed).abs() <= SPEED_BAND_KMH
            && (self.incline - other.incline).abs() <= INCLINE_BAND_PCT
    }

    /// How far apart two stretches are, to pick which ones to join. 1 km/h
    /// and 1 % weigh the same; only the order matters.
    fn distance(&self, other: &Stretch) -> f64 {
        (self.speed - other.speed).abs() + (self.incline - other.incline).abs()
    }
}

/// The walked time per sample, leaving out every still one (before the belt
/// started, after it stopped, and pauses between).
fn walked(samples: &[EchoSample]) -> Vec<Stretch> {
    samples
        .iter()
        .enumerate()
        .filter(|(_, s)| s.speed_kmh.is_finite() && s.speed_kmh >= STILL_KMH)
        .map(|(i, s)| {
            let dt = samples
                .get(i + 1)
                .map(|next| (next.t_ms - s.t_ms) as f64 / 1000.0)
                .unwrap_or(SAMPLE_S)
                .clamp(0.0, MAX_SAMPLE_GAP_S);
            let incline = if s.incline_pct.is_finite() {
                s.incline_pct.max(0.0)
            } else {
                0.0
            };
            Stretch {
                secs: dt,
                speed: s.speed_kmh as f64,
                incline: incline as f64,
            }
        })
        .filter(|s| s.secs > 0.0)
        .collect()
}

/// Split the walked time into stretches that each stay within the bands.
fn split(walked: &[Stretch]) -> Vec<Stretch> {
    let mut out: Vec<Stretch> = Vec::new();
    for s in walked {
        match out.last_mut() {
            Some(cur) if cur.alike(s) => cur.absorb(s),
            _ => out.push(*s),
        }
    }
    out
}

/// Join stretches `i` and `i + 1` into one.
fn join(st: &mut Vec<Stretch>, i: usize) {
    let next = st.remove(i + 1);
    st[i].absorb(&next);
}

/// Fold the short stretch `short` into its neighbour `into`. A steady
/// neighbour takes the time and keeps its own speed and incline, so the belt
/// ramping into and out of a step doesn't pull the step's speed; two short
/// ones average.
fn fold(st: &mut Vec<Stretch>, short: usize, into: usize) {
    let s = st.remove(short);
    let into = if into > short { into - 1 } else { into };
    let target = &mut st[into];
    if target.secs >= MIN_STEP_S {
        target.secs += s.secs;
    } else {
        target.absorb(&s);
    }
}

/// Fold every stretch shorter than `MIN_STEP_S` into its nearer neighbour,
/// shortest first.
fn merge_short(st: &mut Vec<Stretch>) {
    while st.len() > 1 {
        let Some((i, _)) = st
            .iter()
            .enumerate()
            .filter(|(_, s)| s.secs < MIN_STEP_S)
            .min_by(|a, b| a.1.secs.total_cmp(&b.1.secs))
        else {
            break;
        };
        let left = i.checked_sub(1).map(|l| st[l].distance(&st[i]));
        let right = st.get(i + 1).map(|r| r.distance(&st[i]));
        match (left, right) {
            (Some(l), Some(r)) if l <= r => fold(st, i, i - 1),
            (Some(_), None) => fold(st, i, i - 1),
            _ => fold(st, i, i + 1),
        }
    }
}

/// Join neighbours that ended up alike after merging.
fn merge_alike(st: &mut Vec<Stretch>) {
    let mut i = 0;
    while i + 1 < st.len() {
        if st[i].alike(&st[i + 1]) {
            join(st, i);
        } else {
            i += 1;
        }
    }
}

/// Join the closest neighbours until at most `MAX_ECHO_STEPS` are left.
fn cap(st: &mut Vec<Stretch>) {
    while st.len() > MAX_ECHO_STEPS {
        let i = (0..st.len() - 1)
            .min_by(|&a, &b| {
                st[a]
                    .distance(&st[a + 1])
                    .total_cmp(&st[b].distance(&st[b + 1]))
            })
            .unwrap_or(0);
        join(st, i);
    }
}

fn round_to(value: f64, step: f64) -> f64 {
    (value / step).round() * step
}

/// A speed for a step: to the editor's 0.1 km/h, within the belt's limits.
fn step_speed(kmh: f64, limits: &EchoLimits) -> f32 {
    let rounded = (round_to(kmh, SPEED_STEP_KMH) * 10.0).round() as f32 / 10.0;
    if rounded < limits.min_kmh {
        limits.min_kmh
    } else if rounded > limits.max_kmh {
        limits.max_kmh
    } else {
        rounded
    }
}

/// An incline for a step: the nearest deck level's percent, to 0.1 %.
fn step_incline(pct: f64, limits: &EchoLimits) -> f32 {
    let tilt = limits.tilt_multiplier as f64;
    let offset = limits.incline_offset as f64;
    let snapped = if tilt.is_finite() && tilt.abs() >= 0.01 {
        let level = ((pct - offset) / tilt).round().max(0.0);
        level * tilt + offset
    } else {
        // No usable calibration: half a percent is as fine as a deck goes.
        round_to(pct, 0.5)
    };
    ((snapped * 10.0).round() / 10.0).max(0.0) as f32
}

/// The steps for a walk's samples (oldest first), or why there are none.
pub fn echo_steps(
    samples: &[EchoSample],
    limits: &EchoLimits,
) -> Result<Vec<ProgramStep>, EchoError> {
    if samples.is_empty() {
        return Err(EchoError::NoReadings);
    }
    let walked = walked(samples);
    if walked.iter().map(|s| s.secs).sum::<f64>() < MIN_WALK_S {
        return Err(EchoError::TooShort);
    }
    let mut st = split(&walked);
    if !st.iter().any(|s| s.secs >= MIN_STEP_S) {
        return Err(EchoError::NoSteadyStretch);
    }
    merge_short(&mut st);
    merge_alike(&mut st);
    cap(&mut st);

    let mut steps: Vec<ProgramStep> = Vec::with_capacity(st.len());
    let (mut elapsed_s, mut prev_end_min) = (0.0, 0.0);
    for s in &st {
        elapsed_s += s.secs;
        let end_min = round_to(elapsed_s / 60.0, STEP_MINUTES);
        let minutes = (end_min - prev_end_min) as f32;
        prev_end_min = end_min;
        let speed = step_speed(s.speed, limits);
        let incline = step_incline(s.incline, limits);
        match steps.last_mut() {
            // Two neighbours that round to the same step are one step.
            Some(last) if last.speed_kmh == speed && last.incline_pct == incline => {
                last.duration_minutes = last.duration_minutes.map(|m| m + minutes);
            }
            _ if minutes <= 0.0 => {}
            _ => steps.push(ProgramStep {
                duration_minutes: Some(minutes),
                speed_kmh: speed,
                incline_pct: incline,
                zone: None,
                kind: None,
                pulse_end: None,
            }),
        }
    }
    if steps.is_empty() {
        return Err(EchoError::NoSteadyStretch);
    }
    Ok(steps)
}

/// `ECHO // <date>`, with ` (2)`, ` (3)` and so on when a program already
/// has the name, so nothing is overwritten.
pub fn echo_name(date: NaiveDate, taken: &[&str]) -> String {
    let base = format!("ECHO // {}", date.format("%Y-%m-%d"));
    let free = |name: &str| !taken.iter().any(|t| t.trim().eq_ignore_ascii_case(name));
    if free(&base) {
        return base;
    }
    (2u32..)
        .map(|n| format!("{base} ({n})"))
        .find(|name| free(name))
        .unwrap_or(base)
}

/// Build the program for a walk that started at `start_ms` and save it into
/// `state` (not yet to disk). Returns what the button shows.
fn save_echo(
    state: &mut OrchestratorState,
    start_ms: i64,
    samples: &[EchoSample],
) -> Result<EchoProgram, String> {
    let limits = EchoLimits {
        min_kmh: state.min_physical_speed(),
        max_kmh: state.to_physical_speed(state.top_hw_speed()),
        tilt_multiplier: state.tilt_multiplier,
        incline_offset: state.incline_offset,
    };
    let steps = echo_steps(samples, &limits).map_err(EchoError::message)?;
    let date = chrono::Local
        .timestamp_millis_opt(start_ms)
        .earliest()
        .map(|d| d.date_naive())
        .unwrap_or_default();
    let taken: Vec<&str> = state.programs.iter().map(|p| p.name.as_str()).collect();
    let name = echo_name(date, &taken);
    debug_assert!(name.chars().count() <= MAX_NAME_CHARS);
    state.save_program(Program {
        id: 0,
        name,
        steps,
        repeat_every_minutes: None,
        note: None,
    })?;
    let saved = state
        .programs
        .last()
        .ok_or_else(|| "Program not saved".to_string())?;
    Ok(EchoProgram {
        id: saved.id,
        name: saved.name.clone(),
        step_count: saved.steps.len() as u32,
    })
}

/// The walk's start and its samples as `EchoSample`s, oldest first, from every
/// session of a (possibly joined) History row.
fn load_walk(
    app: &AppHandle,
    session_ids: &[i64],
    tilt_multiplier: f32,
    incline_offset: f32,
) -> Result<(i64, Vec<EchoSample>), String> {
    let sessions = persistence::db_sessions_by_ids(app, session_ids)?;
    let start_ms = sessions
        .first()
        .map(|s| s.start_time)
        .ok_or_else(|| WALK_GONE.to_string())?;
    let mut samples = Vec::new();
    for s in &sessions {
        let Some(id) = s.id else { continue };
        for sample in persistence::db_get_samples(app, id)? {
            let t_ms = sample.timestamp.unwrap_or(0);
            samples.push(EchoSample {
                t_ms,
                speed_kmh: sample.speed,
                incline_pct: persistence::sample_incline_pct(
                    sample.incline,
                    t_ms,
                    tilt_multiplier,
                    incline_offset,
                ),
            });
        }
    }
    samples.sort_by_key(|s| s.t_ms);
    Ok((start_ms, samples))
}

/// ECHO on a History row: save the walk (its sessions' ids) as a new program
/// and return it. Never starts the belt.
#[tauri::command]
pub async fn program_from_session(
    app: AppHandle,
    session_ids: Vec<i64>,
) -> Result<EchoProgram, String> {
    if session_ids.is_empty() {
        return Err(WALK_GONE.to_string());
    }
    let (active, tilt, offset) = {
        let state = ORCHESTRATOR.lock().await;
        (
            state.session.active_session_id,
            state.tilt_multiplier,
            state.incline_offset,
        )
    };
    if active.is_some_and(|a| session_ids.contains(&a)) {
        return Err(STILL_RECORDING.to_string());
    }
    let app_c = app.clone();
    let (start_ms, samples) =
        crate::util::blocking(move || load_walk(&app_c, &session_ids, tilt, offset)).await?;
    let mut saved = None;
    crate::snapshot::commands::change_programs(&app, |state| {
        saved = Some(save_echo(state, start_ms, &samples)?);
        Ok(())
    })
    .await?;
    let saved = saved.ok_or_else(|| "Program not saved".to_string())?;
    log::info!(
        "ECHO saved program {} with {} steps",
        saved.id,
        saved.step_count
    );
    Ok(saved)
}

#[cfg(test)]
mod tests {
    use super::*;

    const LIMITS: EchoLimits = EchoLimits {
        min_kmh: 0.6,
        max_kmh: 8.0,
        tilt_multiplier: 1.0,
        incline_offset: 0.0,
    };

    /// A walk built from (seconds, speed, incline) pieces, one sample a second.
    fn walk(pieces: &[(u32, f32, f32)]) -> Vec<EchoSample> {
        let mut out = Vec::new();
        let mut t = 1_800_000_000_000_i64;
        for &(secs, speed, incline) in pieces {
            for _ in 0..secs {
                out.push(EchoSample {
                    t_ms: t,
                    speed_kmh: speed,
                    incline_pct: incline,
                });
                t += 1000;
            }
        }
        out
    }

    /// A belt ramp from `from` to `to` km/h over `secs` seconds.
    fn ramp(secs: u32, from: f32, to: f32, incline: f32) -> Vec<(u32, f32, f32)> {
        (0..secs)
            .map(|i| (1, from + (to - from) * i as f32 / secs as f32, incline))
            .collect()
    }

    fn plan(steps: &[ProgramStep]) -> Vec<(f32, f32, f32)> {
        steps
            .iter()
            .map(|s| (s.duration_minutes.unwrap(), s.speed_kmh, s.incline_pct))
            .collect()
    }

    #[test]
    fn steady_walk_is_one_step_without_the_still_ends() {
        let mut pieces = vec![(20, 0.0, 0.0)];
        pieces.extend(ramp(6, 0.6, 4.0, 2.0));
        // A small wobble, like neural flow easing off by a byte.
        for _ in 0..30 {
            pieces.push((50, 4.0, 2.0));
            pieces.push((10, 3.9, 2.0));
        }
        pieces.extend(ramp(4, 4.0, 0.6, 2.0));
        pieces.push((40, 0.0, 2.0));
        let steps = echo_steps(&walk(&pieces), &LIMITS).unwrap();
        assert_eq!(plan(&steps), vec![(30.0, 4.0, 2.0)]);
        assert!(steps.iter().all(|s| s.zone.is_none()));
    }

    #[test]
    fn intervals_keep_each_round() {
        let mut pieces = vec![(300, 3.5, 0.0)];
        for _ in 0..3 {
            pieces.extend(ramp(8, 3.5, 6.0, 0.0));
            pieces.push((172, 6.0, 5.0));
            pieces.extend(ramp(8, 6.0, 3.5, 0.0));
            pieces.push((172, 3.5, 0.0));
        }
        let steps = echo_steps(&walk(&pieces), &LIMITS).unwrap();
        let p = plan(&steps);
        assert_eq!(p.len(), 7, "{p:?}");
        assert_eq!((p[0].1, p[0].2), (3.5, 0.0));
        for round in 0..3 {
            assert_eq!(
                (p[1 + 2 * round].1, p[1 + 2 * round].2),
                (6.0, 5.0),
                "{p:?}"
            );
            assert_eq!(
                (p[2 + 2 * round].1, p[2 + 2 * round].2),
                (3.5, 0.0),
                "{p:?}"
            );
        }
        // The ramps fold into the steps, and the length is the walk's.
        let total: f32 = p.iter().map(|s| s.0).sum();
        assert_eq!(total, 23.0);
        assert!(p.iter().all(|s| s.0 >= 1.0 && (s.0 * 2.0).fract() == 0.0));
    }

    #[test]
    fn a_short_walk_is_refused() {
        let samples = walk(&[(30, 0.0, 0.0), (170, 4.0, 0.0), (60, 0.0, 0.0)]);
        assert_eq!(echo_steps(&samples, &LIMITS), Err(EchoError::TooShort));
        assert_eq!(echo_steps(&[], &LIMITS), Err(EchoError::NoReadings));
        // A belt that never moved is too short as well.
        let still = walk(&[(600, 0.0, 0.0)]);
        assert_eq!(echo_steps(&still, &LIMITS), Err(EchoError::TooShort));
    }

    #[test]
    fn an_all_ramp_walk_has_no_steady_stretch() {
        let mut pieces = ramp(240, 1.0, 6.0, 0.0);
        pieces.extend(ramp(240, 6.0, 1.0, 0.0));
        let samples = walk(&pieces);
        assert_eq!(
            echo_steps(&samples, &LIMITS),
            Err(EchoError::NoSteadyStretch)
        );
    }

    #[test]
    fn an_incline_change_starts_a_step() {
        let samples = walk(&[(600, 4.0, 0.0), (600, 4.0, 5.0), (300, 4.0, 2.0)]);
        let steps = echo_steps(&samples, &LIMITS).unwrap();
        assert_eq!(
            plan(&steps),
            vec![(10.0, 4.0, 0.0), (10.0, 4.0, 5.0), (5.0, 4.0, 2.0)]
        );
    }

    #[test]
    fn inclines_snap_to_deck_levels() {
        let limits = EchoLimits {
            tilt_multiplier: 0.8,
            incline_offset: 0.2,
            ..LIMITS
        };
        // Level 3 is 2.6 %, level 6 is 5.0 %.
        let samples = walk(&[(300, 4.0, 2.6), (300, 4.0, 5.0)]);
        let steps = echo_steps(&samples, &limits).unwrap();
        assert_eq!(plan(&steps), vec![(5.0, 4.0, 2.6), (5.0, 4.0, 5.0)]);
    }

    #[test]
    fn pauses_and_gaps_are_not_walked_time() {
        // A 10-minute pause and a 2-hour gap between joined sessions.
        let mut samples = walk(&[(300, 3.0, 0.0), (600, 0.0, 0.0), (300, 3.0, 0.0)]);
        let later = walk(&[(300, 5.0, 0.0)]);
        let last = samples.last().unwrap().t_ms;
        samples.extend(later.into_iter().map(|mut s| {
            s.t_ms = last + 2 * 3600 * 1000 + s.t_ms - 1_800_000_000_000;
            s
        }));
        let steps = echo_steps(&samples, &LIMITS).unwrap();
        assert_eq!(plan(&steps), vec![(10.0, 3.0, 0.0), (5.0, 5.0, 0.0)]);
    }

    #[test]
    fn many_changes_are_capped_to_a_handful() {
        let mut pieces = Vec::new();
        for i in 0..30 {
            pieces.push((90, 2.0 + (i % 7) as f32 * 0.5, (i % 3) as f32));
        }
        let steps = echo_steps(&walk(&pieces), &LIMITS).unwrap();
        assert!(steps.len() <= MAX_ECHO_STEPS, "{}", steps.len());
        let total: f32 = steps.iter().map(|s| s.duration_minutes.unwrap()).sum();
        assert_eq!(total, 45.0);
    }

    #[test]
    fn speeds_stay_within_the_belt() {
        let limits = EchoLimits {
            min_kmh: 0.73,
            max_kmh: 5.0,
            ..LIMITS
        };
        let samples = walk(&[(300, 0.7, 0.0), (300, 6.0, 0.0)]);
        let steps = echo_steps(&samples, &limits).unwrap();
        assert_eq!(plan(&steps), vec![(5.0, 0.73, 0.0), (5.0, 5.0, 0.0)]);
    }

    #[test]
    fn names_never_overwrite() {
        let d = NaiveDate::from_ymd_opt(2026, 11, 21).unwrap();
        assert_eq!(echo_name(d, &["Workday"]), "ECHO // 2026-11-21");
        assert_eq!(
            echo_name(d, &["ECHO // 2026-11-21", "echo // 2026-11-21 (2)"]),
            "ECHO // 2026-11-21 (3)"
        );
    }

    #[test]
    fn save_adds_a_program_that_is_not_started() {
        let mut state = OrchestratorState::default();
        state
            .save_program(Program {
                id: 0,
                name: "ECHO // 2026-11-21".to_string(),
                steps: vec![ProgramStep {
                    duration_minutes: Some(5.0),
                    speed_kmh: 3.0,
                    incline_pct: 0.0,
                    zone: None,
                    kind: None,
                    pulse_end: None,
                }],
                repeat_every_minutes: None,
                note: None,
            })
            .unwrap();
        let before = state.programs.len();
        // Midday, so the local date is the 21st in any time zone the app runs in.
        let start = chrono::Local
            .with_ymd_and_hms(2026, 11, 21, 12, 0, 0)
            .unwrap()
            .timestamp_millis();
        let samples = walk(&[(600, 3.0, 0.0), (600, 4.5, 0.0)]);
        let saved = save_echo(&mut state, start, &samples).unwrap();
        assert_eq!(saved.name, "ECHO // 2026-11-21 (2)");
        assert_eq!(saved.step_count, 2);
        assert_eq!(state.programs.len(), before + 1);
        let p = state.programs.last().unwrap();
        assert_eq!(p.id, saved.id);
        assert_eq!(p.repeat_every_minutes, None);
        assert!(p.steps.iter().all(|s| s.duration_minutes.is_some()));
        assert!(state.program.is_none(), "ECHO never runs the program");

        let short = walk(&[(60, 3.0, 0.0)]);
        assert_eq!(
            save_echo(&mut state, start, &short),
            Err(EchoError::TooShort.message())
        );
        assert_eq!(state.programs.len(), before + 1);
    }
}
