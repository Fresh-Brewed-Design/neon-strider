//! FIT_TO_DECK (#225): a seeded program the user hasn't edited may trade the
//! speed of its steps above STRIDE_FIT for incline, so each step keeps about
//! the same effort at a speed the user's stride still fits the deck.
//!
//! The maths is pure (`equal_effort_grade_pct`, `fit_steps`); the offer the
//! dashboard shows comes from `offers`, and the save from
//! `OrchestratorState::fit_program_to_deck`, which only rewrites the stored
//! steps after the user confirms. Nothing here starts, resumes or moves the
//! belt: a running program keeps its own copy of the steps until it ends.

use serde::{Deserialize, Serialize};

use super::{seed_programs, Program, ProgramStep};

/// A metre per minute from km/h: the ACSM equation's speed unit.
const M_PER_MIN_PER_KMH: f32 = 1000.0 / 60.0;

/// Below this share of the old effort a step counts as losing effort to the
/// deck's top incline. 0.5 % is about what rounding the grade to a tenth of a
/// percent can cost, so a step that only lost that to rounding isn't flagged.
const EFFORT_KEPT_MIN: f32 = 0.995;

/// Oxygen cost of walking, ml/kg/min, from speed (km/h) and grade (%):
/// ACSM's Guidelines for Exercise Testing and Prescription, walking equation,
/// VO2 = 0.1 * S + 1.8 * S * G + 3.5, with S in m/min and G the grade as a
/// fraction. Valid for walking speeds (about 3 to 6 km/h), the steps a deck
/// that crowds the stride runs.
pub fn walking_vo2(kmh: f32, grade_pct: f32) -> f32 {
    let s = kmh * M_PER_MIN_PER_KMH;
    let g = grade_pct / 100.0;
    0.1 * s + 1.8 * s * g + 3.5
}

/// The grade (%) that costs as much at `fit_kmh` as `kmh` at `grade_pct`, by
/// the ACSM walking equation (`walking_vo2`) solved for G at the fit speed:
/// G = (VO2 - 3.5 - 0.1 * Sf) / (1.8 * Sf). Unclamped.
pub fn equal_effort_grade_pct(kmh: f32, grade_pct: f32, fit_kmh: f32) -> f32 {
    let sf = fit_kmh * M_PER_MIN_PER_KMH;
    let vo2 = walking_vo2(kmh, grade_pct);
    (vo2 - 3.5 - 0.1 * sf) / (1.8 * sf) * 100.0
}

/// One step's change: the old speed and grade, the new grade at the fit
/// speed, and how much of the old effort it keeps (1.0 = all).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StepFit {
    pub from_kmh: f32,
    pub from_pct: f32,
    pub to_pct: f32,
    pub kept: f32,
}

impl StepFit {
    /// The deck's top incline cut the grade short of the same effort.
    pub fn lost_effort(&self) -> bool {
        self.kept < EFFORT_KEPT_MIN
    }
}

/// Tenths, the precision the editor shows a grade at.
fn tenths(v: f32) -> f32 {
    (v * 10.0).round() / 10.0
}

/// The program's steps with each one faster than STRIDE_FIT (in tenths, as
/// `stride_fit::exceeds`) set to the fit speed and the grade that keeps its
/// effort, and the change made to each. The grade is held to the deck's top
/// incline (`max_incline_pct`): a step can't ask for more than the deck
/// climbs, so a step that needs more keeps less effort, and `StepFit::kept`
/// says how much. Duration and heart rate zone stay. Empty changes when no
/// step is above the fit.
pub fn fit_steps(
    steps: &[ProgramStep],
    fit_kmh: f32,
    max_incline_pct: f32,
) -> (Vec<ProgramStep>, Vec<StepFit>) {
    let mut changes = Vec::new();
    let fitted = steps
        .iter()
        .map(|s| {
            if !crate::calibration::stride_fit::exceeds(s.speed_kmh, Some(fit_kmh)) {
                return s.clone();
            }
            let want = equal_effort_grade_pct(s.speed_kmh, s.incline_pct, fit_kmh);
            let to_pct = tenths(want.min(max_incline_pct).max(s.incline_pct));
            let kept = walking_vo2(fit_kmh, to_pct) / walking_vo2(s.speed_kmh, s.incline_pct);
            changes.push(StepFit {
                from_kmh: s.speed_kmh,
                from_pct: s.incline_pct,
                to_pct,
                kept,
            });
            ProgramStep {
                speed_kmh: fit_kmh,
                incline_pct: to_pct,
                ..s.clone()
            }
        })
        .collect();
    (fitted, changes)
}

/// A seeded program as it was seeded: it still has its note (a save that
/// changes the steps or repeat drops it) and its steps and repeat match one
/// of `seed_programs`. Migrations 4, 7 and 8 wrote the same steps an install
/// seeds today, so a migrated copy the user never touched matches too.
pub fn is_unedited_seed(program: &Program) -> bool {
    program.note.is_some()
        && seed_programs().iter().any(|s| {
            s.steps == program.steps && s.repeat_every_minutes == program.repeat_every_minutes
        })
}

/// FIT_TO_DECK on the Programs page: one per unedited seeded program with a
/// step above STRIDE_FIT. Rust writes the words so the dashboard only shows
/// them.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct FitToDeckOffer {
    pub program_id: u32,
    /// One line on the program's card, beside the button.
    pub summary: String,
    /// The confirm dialog's lines, one sentence each.
    pub message: String,
    /// The confirm dialog's detail lines: whether the effort holds.
    pub hint: String,
    /// The line under the button once saved.
    pub done: String,
}

fn plural_steps(n: usize) -> String {
    if n == 1 {
        "1 step".to_string()
    } else {
        format!("{n} steps")
    }
}

/// The offer for one program, if FIT_TO_DECK applies to it.
pub fn offer(program: &Program, fit_kmh: f32, max_incline_pct: f32) -> Option<FitToDeckOffer> {
    if !is_unedited_seed(program) {
        return None;
    }
    let (_, changes) = fit_steps(&program.steps, fit_kmh, max_incline_pct);
    if changes.is_empty() {
        return None;
    }
    let name = &program.name;
    let fit = format!("STRIDE_FIT {fit_kmh:.1} km/h");
    // One line per distinct change: VO2 max's four climbs are one line.
    let mut distinct: Vec<StepFit> = Vec::new();
    for c in &changes {
        if !distinct
            .iter()
            .any(|d| d.from_kmh == c.from_kmh && d.from_pct == c.from_pct)
        {
            distinct.push(*c);
        }
    }
    let mut message = vec![format!(
        "FIT_TO_DECK sets {} of {name} to {fit}.",
        plural_steps(changes.len())
    )];
    for d in &distinct {
        message.push(format!(
            "{:.1} km/h at {:.1}% becomes {fit_kmh:.1} km/h at {:.1}%.",
            d.from_kmh, d.from_pct, d.to_pct
        ));
    }
    let mut hint = Vec::new();
    let short: Vec<&StepFit> = distinct.iter().filter(|d| d.lost_effort()).collect();
    if short.is_empty() {
        hint.push(
            "The new incline keeps the same effort by the ACSM walking equation.".to_string(),
        );
    } else {
        for d in short {
            hint.push(format!(
                "The deck tops out at {max_incline_pct:.1}%, so the {:.1} km/h steps keep {:.0}% of their effort.",
                d.from_kmh,
                (d.kept * 100.0).floor()
            ));
        }
    }
    hint.push("FIT_TO_DECK never starts the program.".to_string());
    Some(FitToDeckOffer {
        program_id: program.id,
        summary: if changes.len() == 1 {
            format!("1 step runs above {fit}.")
        } else {
            format!("{} run above {fit}.", plural_steps(changes.len()))
        },
        message: message.join("\n"),
        hint: hint.join("\n"),
        done: format!("{name} now runs at {fit} with more incline."),
    })
}

/// Every FIT_TO_DECK offer, in the programs' order. None while STRIDE_FIT is
/// unset.
pub fn offers(
    programs: &[Program],
    fit_kmh: Option<f32>,
    max_incline_pct: f32,
) -> Vec<FitToDeckOffer> {
    let Some(fit) = fit_kmh else {
        return vec![];
    };
    programs
        .iter()
        .filter_map(|p| offer(p, fit, max_incline_pct))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seeded(name: &str) -> Program {
        seed_programs()
            .into_iter()
            .find(|p| p.name == name)
            .unwrap()
    }

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 0.05
    }

    #[test]
    fn the_acsm_walking_equation() {
        // 6.0 km/h is 100 m/min: 10 + 0 + 3.5 flat, plus 18 at 10 %.
        assert!(close(walking_vo2(6.0, 0.0), 13.5));
        assert!(close(walking_vo2(6.0, 10.0), 31.5));
    }

    #[test]
    fn the_grade_that_keeps_the_effort_at_the_fit_speed() {
        // Interval walk's brisk 6.0 km/h at 5 % (22.5 ml/kg/min) at 4.5 km/h
        // (75 m/min): (22.5 - 3.5 - 7.5) / 135 = 8.5 %.
        let g = equal_effort_grade_pct(6.0, 5.0, 4.5);
        assert!(close(g, 8.52), "{g}");
        assert!(close(walking_vo2(4.5, g), walking_vo2(6.0, 5.0)));
        // Same speed: same grade.
        assert!(close(equal_effort_grade_pct(4.0, 3.0, 4.0), 3.0));
    }

    #[test]
    fn only_steps_above_the_fit_change_and_keep_their_zone() {
        let vo2 = seeded("VO2 max");
        let (steps, changes) = fit_steps(&vo2.steps, 4.5, 18.75);
        assert_eq!(changes.len(), 4, "the four 6.0 km/h climbs");
        for (old, new) in vo2.steps.iter().zip(&steps) {
            if old.speed_kmh > 4.5 {
                assert_eq!(new.speed_kmh, 4.5);
                assert!(close(new.incline_pct, 15.2), "{}", new.incline_pct);
                assert_eq!(new.zone, old.zone);
                assert_eq!(new.duration_minutes, old.duration_minutes);
            } else {
                assert_eq!(new, old, "4.5 km/h and under stay");
            }
        }
        assert!(changes.iter().all(|c| !c.lost_effort()));
    }

    #[test]
    fn held_to_the_deck_top_incline_and_says_what_is_lost() {
        let vo2 = seeded("VO2 max");
        let (steps, changes) = fit_steps(&vo2.steps, 4.5, 12.0);
        assert!(steps.iter().all(|s| s.incline_pct <= 12.0));
        let c = changes[0];
        assert_eq!(c.to_pct, 12.0);
        assert!(c.lost_effort());
        // 7.5 + 16.2 + 3.5 = 27.2 of 31.5: 86 %.
        assert!(close(c.kept * 100.0, 86.3), "{}", c.kept);
        let o = offer(&vo2, 4.5, 12.0).unwrap();
        assert!(o.hint.contains("keep 86% of their effort"), "{}", o.hint);
    }

    #[test]
    fn a_fit_at_or_above_every_step_changes_nothing() {
        let walk = seeded("Interval walk");
        let (steps, changes) = fit_steps(&walk.steps, 6.0, 18.75);
        assert!(changes.is_empty());
        assert_eq!(steps, walk.steps);
        assert_eq!(offer(&walk, 6.0, 18.75), None);
    }

    #[test]
    fn offered_only_on_an_unedited_seeded_program() {
        let walk = seeded("Interval walk");
        let o = offer(&walk, 4.5, 18.75).unwrap();
        assert_eq!(o.program_id, walk.id);
        assert_eq!(o.summary, "5 steps run above STRIDE_FIT 4.5 km/h.");
        assert_eq!(
            o.message,
            "FIT_TO_DECK sets 5 steps of Interval walk to STRIDE_FIT 4.5 km/h.\n6.0 km/h at 5.0% becomes 4.5 km/h at 8.5%."
        );
        assert!(o.hint.starts_with("The new incline keeps the same effort"));
        // Edited: the save drops the note.
        let mut edited = walk.clone();
        edited.note = None;
        assert_eq!(offer(&edited, 4.5, 18.75), None);
        // A step changed, note and all.
        let mut changed = walk.clone();
        changed.steps[0].duration_minutes = Some(6.0);
        assert_eq!(offer(&changed, 4.5, 18.75), None);
        // A user's own program with the same name.
        let mine = Program {
            id: 40,
            note: None,
            ..walk.clone()
        };
        assert_eq!(offer(&mine, 4.5, 18.75), None);
    }

    #[test]
    fn no_offers_while_stride_fit_is_unset() {
        let list = seed_programs();
        assert!(offers(&list, None, 18.75).is_empty());
        let names: Vec<u32> = offers(&list, Some(4.5), 18.75)
            .iter()
            .map(|o| o.program_id)
            .collect();
        // Brisk (5.0), VO2 max (6.0) and Interval walk (6.0); Post-meal at 4.0 stays.
        assert_eq!(names, vec![5, 6, 7]);
        assert_eq!(
            offers(&list, Some(4.5), 18.75)[0].summary,
            "1 step runs above STRIDE_FIT 4.5 km/h.",
            "Brisk's one step"
        );
    }

    /// The command: saves the fitted steps once, keeps the note, and refuses
    /// a program with no offer, a second fit, or an unset STRIDE_FIT.
    #[test]
    fn the_command_fits_an_offered_program_once() {
        use crate::engine::OrchestratorState;
        let mut s = OrchestratorState {
            programs: seed_programs(),
            max_incline: 12,
            ..Default::default()
        };
        assert_eq!(s.top_incline_pct(), 12.0);
        assert_eq!(s.fit_to_deck_offers(None), vec![]);
        assert!(s.fit_program_to_deck(6, None).is_err(), "unset");
        assert!(
            s.fit_program_to_deck(1, Some(4.5)).is_err(),
            "Post-meal: none above"
        );
        s.fit_program_to_deck(6, Some(4.5)).unwrap();
        let vo2 = s.programs.iter().find(|p| p.id == 6).unwrap();
        assert!(vo2.steps.iter().all(|st| st.speed_kmh <= 4.5));
        assert!(vo2.steps.iter().all(|st| st.incline_pct <= 12.0));
        assert!(vo2.note.is_some(), "the note stays");
        assert!(s.fit_program_to_deck(6, Some(4.5)).is_err(), "once only");
        let left: Vec<u32> = s
            .fit_to_deck_offers(Some(4.5))
            .iter()
            .map(|o| o.program_id)
            .collect();
        assert_eq!(left, vec![5, 7]);
        assert!(s.program.is_none(), "nothing started");
    }

    #[test]
    fn fitting_twice_offers_nothing_the_second_time() {
        let walk = seeded("Interval walk");
        let (steps, _) = fit_steps(&walk.steps, 4.5, 18.75);
        let fitted = Program { steps, ..walk };
        assert!(!is_unedited_seed(&fitted));
    }
}
