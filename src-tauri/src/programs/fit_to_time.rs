//! FIT_TO_TIME (#258): run a program once at a length the user picks, for
//! example 25 min before a meeting. WARM_UP and COOL_DOWN steps keep their
//! minutes; the timed steps between them scale together to fill the rest.
//!
//! The maths is pure (`range`, `fit`); the Programs page reads the lengths
//! from `options` (`snapshot.fit_to_time`) and sends the pick with
//! `Command::ProgramRun`, so the run is still the user's press. The saved
//! program never changes: the fitted copy lives only in the running
//! `ProgramRun`.

use serde::{Deserialize, Serialize};

use super::{Program, ProgramStep, StepKind};

/// The shortest run a pick may ask for: 5 min. It is Workday's break, the
/// shortest walk any example program has, and the smallest dose of the trial
/// behind it that lowered the glucose rise after meals (Duran et al. 2023,
/// docs/research.md); 1 min an hour did not.
pub(crate) const MIN_FIT_MINUTES: f32 = 5.0;
/// The longest: 180 min, a morning at the desk. A longer walk is what a
/// program with no end is for, and the list of lengths stays short.
pub(crate) const MAX_FIT_MINUTES: f32 = 180.0;
/// A scaled step keeps at least 1 min. The deck takes a few seconds for each
/// speed change and Easy start eases a start from still over 30 s, so a
/// shorter step spends most of its time getting to its speed.
const MIN_STEP_MINUTES: f32 = 1.0;
/// With two or more steps between WARM_UP and COOL_DOWN, they keep their
/// balance: each scales to no less than a third of its saved minutes and no
/// more than three times. Past that the program is another workout (VO2 max's
/// 4 min climbs at 12 min are a steady climb, at 1 min a sprint the deck
/// can't ramp to), better saved as a program of its own. A single step in
/// between has no balance to keep, so only the length limits apply.
const MIN_SHRINK: f32 = 1.0 / 3.0;
const MAX_STRETCH: f32 = 3.0;
/// The picker offers lengths 5 min apart (plus the shortest and longest), so
/// the list stays short enough to scan; the command takes any whole minute
/// in range.
const LENGTH_STEP_MINUTES: u32 = 5;
/// Room for float sums: the saved minutes are typed to a tenth, so a range
/// end this close to a whole minute counts as that minute.
const EPS: f32 = 0.001;

/// WARM_UP and COOL_DOWN keep their minutes whatever the length.
fn keeps_minutes(step: &ProgramStep) -> bool {
    matches!(step.kind, Some(StepKind::WarmUp | StepKind::CoolDown))
}

/// An open-ended last step (it runs until END) has no minutes to scale, so it
/// takes whatever the other steps leave.
fn open_end(program: &Program) -> bool {
    program
        .steps
        .last()
        .is_some_and(|s| s.duration_minutes.is_none())
}

/// The minutes a pick may run for, `(shortest, longest)` in whole minutes,
/// or None when nothing in the program can change length (every timed step
/// is a WARM_UP or COOL_DOWN, or the limits leave no whole minute).
pub fn range(program: &Program) -> Option<(u32, u32)> {
    let (lo, hi) = if open_end(program) {
        let timed: f32 = program
            .steps
            .iter()
            .filter_map(|s| s.duration_minutes)
            .sum();
        (
            (timed + MIN_STEP_MINUTES).max(MIN_FIT_MINUTES),
            MAX_FIT_MINUTES,
        )
    } else {
        let fixed: f32 = fixed_minutes(program);
        let scaled: Vec<f32> = scaled_minutes(program).collect();
        let total: f32 = scaled.iter().sum();
        let shortest = scaled.iter().copied().fold(f32::INFINITY, f32::min);
        if scaled.is_empty() || total <= 0.0 {
            return None;
        }
        let (mut lo_factor, mut hi_factor) = (MIN_STEP_MINUTES / shortest, f32::INFINITY);
        if scaled.len() >= 2 {
            lo_factor = lo_factor.max(MIN_SHRINK);
            hi_factor = MAX_STRETCH;
        }
        (
            (fixed + total * lo_factor).max(MIN_FIT_MINUTES),
            (fixed + total * hi_factor).min(MAX_FIT_MINUTES),
        )
    };
    let lo = (lo - EPS).ceil() as u32;
    let hi = (hi + EPS).floor() as u32;
    (lo <= hi).then_some((lo, hi))
}

fn fixed_minutes(program: &Program) -> f32 {
    program
        .steps
        .iter()
        .filter(|s| keeps_minutes(s))
        .filter_map(|s| s.duration_minutes)
        .sum()
}

/// The minutes of each step that scales: timed, more than zero, and neither
/// WARM_UP nor COOL_DOWN.
fn scaled_minutes(program: &Program) -> impl Iterator<Item = f32> + '_ {
    program
        .steps
        .iter()
        .filter(|s| !keeps_minutes(s))
        .filter_map(|s| s.duration_minutes)
        .filter(|m| *m > 0.0)
}

/// Whole seconds, as a step's minutes.
fn to_second(minutes: f32) -> f32 {
    (minutes * 60.0).round() / 60.0
}

/// The program as it runs at `minutes`: the scaled steps rounded to whole
/// seconds, with the rounding left on the longest so the run adds up to the
/// pick. Speeds, inclines, zones and kinds stay, and a COOL_DOWN keeps its
/// pulse target, so it may still walk on past the pick until the pulse is
/// down. A repeating program runs one cycle that fills the pick and ends:
/// the length is the time the user has, and a repeat would run past it. An
/// open-ended last step gets the minutes the others leave. The id, name and
/// note stay, so the run shows and saves as the program it came from.
pub fn fit(program: &Program, minutes: u32) -> Result<Program, String> {
    let (lo, hi) = range(program).ok_or_else(|| {
        format!(
            "{} keeps every step's minutes, so its length can't change.",
            program.name
        )
    })?;
    if !(lo..=hi).contains(&minutes) {
        return Err(format!("{} runs from {lo} to {hi} min.", program.name));
    }
    let target = minutes as f32;
    let mut steps = program.steps.clone();
    if open_end(program) {
        let timed: f32 = steps.iter().filter_map(|s| s.duration_minutes).sum();
        if let Some(last) = steps.last_mut() {
            last.duration_minutes = Some(to_second(target - timed));
        }
    } else {
        let total: f32 = scaled_minutes(program).sum();
        let factor = (target - fixed_minutes(program)) / total;
        let mut longest: Option<usize> = None;
        for (i, s) in steps.iter_mut().enumerate() {
            let Some(m) = s.duration_minutes else {
                continue;
            };
            if keeps_minutes(s) || m <= 0.0 {
                continue;
            }
            s.duration_minutes = Some(to_second(m * factor));
            if longest.is_none_or(|j| m > program.steps[j].duration_minutes.unwrap_or(0.0)) {
                longest = Some(i);
            }
        }
        let sum: f32 = steps.iter().filter_map(|s| s.duration_minutes).sum();
        if let Some(j) = longest {
            if let Some(m) = steps[j].duration_minutes.as_mut() {
                *m = to_second(*m + target - sum);
            }
        }
    }
    Ok(Program {
        steps,
        repeat_every_minutes: None,
        ..program.clone()
    })
}

/// FIT_TO_TIME's picker beside a program's Run: the lengths it offers and
/// Rust's words for it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct FitToTimeOption {
    pub program_id: u32,
    /// The picker's first choice, which runs the program as saved: its
    /// length (`35 min`), or `As saved` for one that repeats or has no end.
    pub saved_label: String,
    /// Whole minutes to pick from, shortest first, without the saved length.
    pub lengths: Vec<u32>,
    /// Lines under the picker while a length is picked, one sentence each.
    pub note: String,
}

/// Minutes as the editor shows them: whole, or to a tenth.
fn minutes_text(m: f32) -> String {
    if (m - m.round()).abs() < EPS {
        format!("{} min", m.round() as u32)
    } else {
        format!("{m:.1} min")
    }
}

/// The picker for one program, if its length can change.
pub fn option(program: &Program) -> Option<FitToTimeOption> {
    let (lo, hi) = range(program)?;
    let plain = !open_end(program) && program.repeat_every_minutes.is_none();
    let saved: f32 = program
        .steps
        .iter()
        .filter_map(|s| s.duration_minutes)
        .sum();
    let saved_whole = (plain && (saved - saved.round()).abs() < EPS).then(|| saved.round() as u32);
    let mut lengths: Vec<u32> = std::iter::once(lo)
        .chain((lo..=hi).filter(|m| m % LENGTH_STEP_MINUTES == 0))
        .chain(std::iter::once(hi))
        .filter(|m| Some(*m) != saved_whole)
        .collect();
    lengths.dedup();
    let mut note = Vec::new();
    let has = |kind: StepKind| {
        program
            .steps
            .iter()
            .any(|s| s.kind == Some(kind) && s.duration_minutes.is_some())
    };
    match (has(StepKind::WarmUp), has(StepKind::CoolDown)) {
        (true, true) => note.push("WARM_UP and COOL_DOWN keep their minutes.".to_string()),
        (true, false) => note.push("WARM_UP keeps its minutes.".to_string()),
        (false, true) => note.push("COOL_DOWN keeps its minutes.".to_string()),
        (false, false) => {}
    }
    if program.steps.iter().any(|s| s.pulse_end.is_some()) {
        note.push("COOL_DOWN still walks on until the pulse is down.".to_string());
    }
    if program.repeat_every_minutes.is_some() {
        note.push("The program runs once with no repeat.".to_string());
    }
    note.push("The saved program stays as it is.".to_string());
    Some(FitToTimeOption {
        program_id: program.id,
        saved_label: if plain {
            minutes_text(saved)
        } else {
            "As saved".to_string()
        },
        lengths,
        note: note.join("\n"),
    })
}

/// Every picker, in the programs' order.
pub fn options(programs: &[Program]) -> Vec<FitToTimeOption> {
    programs.iter().filter_map(option).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::programs::{seed_programs, PulseEnd};

    fn seeded(name: &str) -> Program {
        seed_programs()
            .into_iter()
            .find(|p| p.name == name)
            .unwrap()
    }

    fn total(p: &Program) -> f32 {
        p.steps.iter().filter_map(|s| s.duration_minutes).sum()
    }

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 0.01
    }

    fn step(minutes: Option<f32>, kind: Option<StepKind>) -> ProgramStep {
        ProgramStep {
            duration_minutes: minutes,
            speed_kmh: 4.0,
            incline_pct: 0.0,
            zone: None,
            kind,
            pulse_end: None,
        }
    }

    fn program(steps: Vec<ProgramStep>, repeat: Option<f32>) -> Program {
        Program {
            id: 9,
            name: "Mine".to_string(),
            steps,
            repeat_every_minutes: repeat,
            note: None,
        }
    }

    #[test]
    fn warm_up_and_cool_down_keep_their_minutes_and_the_middle_fills_the_rest() {
        // Interval walk: 5 min warm-up and ten 3 min steps, 35 min.
        let walk = seeded("Interval walk");
        let fitted = fit(&walk, 25).unwrap();
        assert!(close(total(&fitted), 25.0), "{}", total(&fitted));
        assert_eq!(fitted.steps[0], walk.steps[0], "WARM_UP as saved");
        for (old, new) in walk.steps.iter().zip(&fitted.steps).skip(1) {
            assert!(
                close(new.duration_minutes.unwrap(), 2.0),
                "20 min over ten steps"
            );
            assert_eq!(new.speed_kmh, old.speed_kmh);
            assert_eq!(new.incline_pct, old.incline_pct);
            assert_eq!(new.kind, old.kind);
        }
        assert_eq!(fitted.id, walk.id);
        assert_eq!(fitted.name, walk.name);
        assert_eq!(fitted.note, walk.note);
    }

    #[test]
    fn the_cool_down_keeps_its_pulse_rule_and_its_minutes() {
        // VO2 max: 5 warm-up, four 4 min climbs, four 3 min rests, 3 cool-down.
        let vo2 = seeded("VO2 max");
        let fitted = fit(&vo2, 26).unwrap();
        let (old, new) = (vo2.steps.last().unwrap(), fitted.steps.last().unwrap());
        assert_eq!(new, old, "minutes and pulse_end as saved");
        assert_eq!(new.pulse_end, Some(PulseEnd::default_target()));
        assert!(close(total(&fitted), 26.0));
        // 18 min between, 28 saved: each climb and rest at 18/28.
        assert!(close(
            fitted.steps[3].duration_minutes.unwrap(),
            4.0 * 18.0 / 28.0
        ));
        assert!(close(
            fitted.steps[4].duration_minutes.unwrap(),
            3.0 * 18.0 / 28.0
        ));
        assert_eq!(fitted.steps[3].zone, vo2.steps[3].zone, "the zone stays");
    }

    #[test]
    fn the_shortest_and_longest_lengths() {
        // VO2 max: 8 fixed, 28 between, its 3 min rests stop at 1 min (a
        // third, which is also MIN_SHRINK): 8 + 28/3 = 17.3, so 18. Three times
        // is 8 + 84 = 92.
        assert_eq!(range(&seeded("VO2 max")), Some((18, 92)));
        // Interval walk: 5 fixed, 30 between: 15 to 95.
        assert_eq!(range(&seeded("Interval walk")), Some((15, 95)));
        // One step to scale: only the 5 and 180 min limits.
        assert_eq!(range(&seeded("Brisk")), Some((5, 180)));
        assert_eq!(range(&seeded("Post-meal")), Some((5, 180)));
        let vo2 = seeded("VO2 max");
        assert!(fit(&vo2, 17).is_err());
        assert!(fit(&vo2, 93).is_err());
        assert!(close(total(&fit(&vo2, 18).unwrap()), 18.0));
        let long = fit(&vo2, 92).unwrap();
        assert!(
            close(long.steps[1].duration_minutes.unwrap(), 12.0),
            "three times"
        );
        let err = fit(&seeded("Brisk"), 181).unwrap_err();
        assert_eq!(err, "Brisk runs from 5 to 180 min.");
        assert!(fit(&seeded("Brisk"), 4).is_err());
    }

    #[test]
    fn a_short_middle_step_sets_the_shortest_length() {
        // 2 min and 10 min between: the 2 min step stops at 1, half, so
        // 12 * 0.5 = 6 min (above a third).
        let p = program(vec![step(Some(2.0), None), step(Some(10.0), None)], None);
        assert_eq!(range(&p), Some((6, 36)));
    }

    #[test]
    fn a_repeating_program_runs_one_cycle_that_fills_the_length() {
        // Workday: 5 min every 30.
        let workday = seeded("Workday");
        assert_eq!(range(&workday), Some((5, 180)));
        let fitted = fit(&workday, 25).unwrap();
        assert_eq!(fitted.repeat_every_minutes, None, "ends on time");
        assert!(close(total(&fitted), 25.0));
        let o = option(&workday).unwrap();
        assert_eq!(o.saved_label, "As saved");
        assert!(o.lengths.contains(&5), "one 5 min cycle is a pick");
        assert!(o.note.contains("runs once with no repeat"));
    }

    #[test]
    fn an_open_ended_step_takes_what_the_others_leave() {
        let focus = seeded("Focus");
        assert_eq!(range(&focus), Some((5, 180)));
        let fitted = fit(&focus, 25).unwrap();
        assert!(close(fitted.steps[0].duration_minutes.unwrap(), 25.0));
        // A timed warm-up and walk keep their minutes before an open end.
        let p = program(
            vec![
                step(Some(5.0), Some(StepKind::WarmUp)),
                step(Some(10.0), Some(StepKind::Walk)),
                step(None, Some(StepKind::Walk)),
            ],
            None,
        );
        assert_eq!(range(&p), Some((16, 180)));
        let fitted = fit(&p, 40).unwrap();
        assert_eq!(fitted.steps[1].duration_minutes, Some(10.0));
        assert!(close(fitted.steps[2].duration_minutes.unwrap(), 25.0));
        assert_eq!(option(&p).unwrap().saved_label, "As saved");
    }

    #[test]
    fn nothing_to_scale_offers_nothing() {
        let p = program(
            vec![
                step(Some(5.0), Some(StepKind::WarmUp)),
                step(Some(0.0), Some(StepKind::Walk)),
                step(Some(5.0), Some(StepKind::CoolDown)),
            ],
            None,
        );
        assert_eq!(range(&p), None);
        assert!(option(&p).is_none());
        assert!(fit(&p, 10).unwrap_err().contains("can't change"));
        // A middle at three times that still can't reach 5 min.
        let tiny = program(vec![step(Some(0.5), None), step(Some(0.5), None)], None);
        assert_eq!(range(&tiny), None);
    }

    #[test]
    fn the_rounding_lands_on_the_pick() {
        // Three 1.3 min steps to 7 min: 2.333... each, in whole seconds.
        let p = program(
            vec![
                step(Some(1.3), None),
                step(Some(1.3), None),
                step(Some(1.3), None),
            ],
            None,
        );
        let fitted = fit(&p, 7).unwrap();
        assert!(close(total(&fitted), 7.0), "{}", total(&fitted));
        for s in &fitted.steps {
            let secs = s.duration_minutes.unwrap() * 60.0;
            assert!((secs - secs.round()).abs() < 0.01, "whole seconds");
        }
    }

    #[test]
    fn the_picker_lists_5_min_lengths_and_skips_the_saved_one() {
        let o = option(&seeded("Interval walk")).unwrap();
        assert_eq!(o.saved_label, "35 min");
        assert_eq!(o.lengths.first(), Some(&15));
        assert_eq!(o.lengths.last(), Some(&95));
        assert!(
            !o.lengths.contains(&35),
            "the saved length is the first choice"
        );
        assert!(o.lengths.contains(&25));
        assert!(o.lengths.iter().all(|m| m % 5 == 0));
        assert_eq!(
            o.note,
            "WARM_UP keeps its minutes.\nThe saved program stays as it is."
        );
        let vo2 = option(&seeded("VO2 max")).unwrap();
        assert!(vo2
            .note
            .starts_with("WARM_UP and COOL_DOWN keep their minutes."));
        assert_eq!(vo2.lengths.first(), Some(&18), "the shortest itself");
        assert_eq!(vo2.lengths.last(), Some(&92), "and the longest");
        assert!(vo2.note.contains("walks on until the pulse is down"));
        let ids: Vec<u32> = options(&seed_programs())
            .iter()
            .map(|o| o.program_id)
            .collect();
        assert_eq!(ids, vec![1, 2, 3, 5, 6, 7], "every example program");
    }

    #[test]
    fn fitting_leaves_the_saved_program_alone() {
        let walk = seeded("Interval walk");
        let copy = walk.clone();
        let _ = fit(&walk, 20).unwrap();
        assert_eq!(walk, copy);
    }
}
