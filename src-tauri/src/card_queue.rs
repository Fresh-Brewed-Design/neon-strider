//! The HUD's cards, one at a time (#421).
//!
//! Each card is still decided by its own machine (`walkaway`, `eyebreak`,
//! `seated`, `weekly`, `maintenance`, `update`). This module picks the one the
//! HUD shows (`snapshot.hud_card`), by rank:
//!
//! 1. belt safety: walk-away, then the eye-break resume card;
//! 2. a program waiting for GO (its prompt lives on the panel, so no card);
//! 3. the eye break (heads-up, DEFERRED while a walk runs, and the break);
//! 4. ATROPHY_WARNING (sit-too-long);
//! 5. last week;
//! 6. the belt service;
//! 7. the update offer.
//!
//! A break already under way keeps its card through a GO prompt: the eyes are
//! closed and the tones are playing, so the GO waits on the panel until it ends.
//!
//! A waiting card is dropped, not shown next, when the answer to the one before
//! it removed its reason. ATROPHY_WARNING goes once a start was asked (GO,
//! RE-JACK, ENGAGE; refused or not) or the belt moves, before it ever showed;
//! it folds into the strip's readout instead. An eye-break heads-up set for a
//! program's rest leaves the queue while the belt walks there, and while a
//! belt starts.
//!
//! A card that can wait (anything but belt safety and a break under way) shows
//! only after a short grace: after the presenting hold ends, and after another
//! card left. So held cards come back one by one, and a press on one card
//! never lands on the next.
//!
//! ATROPHY_WARNING shows once (#422): unanswered for a minute of showing, it
//! folds into a yellow seated-time readout on the HUD (`seated.folded`) and
//! stays there until its machine lets it go (a walk, time away, the switch).
//!
//! The eye-break machine is told to wait (`eye_break_waits`) while a card
//! above it or a grace holds its card back, the same way the presenting hold
//! does, so no break begins unseen and its heads-up keeps its full minute.
//!
//! Nothing here touches the belt: it only picks what the HUD shows.
//!
//! This file is declared from `snapshot.rs` (`#[path]`), not `lib.rs`.

use super::eyebreak::EyeBreakPhase;
use super::walkaway::WalkAwayCard;
use super::AppSnapshot;
use crate::protocol::MachineStatus;
use serde::{Deserialize, Serialize};
use std::sync::Mutex;

const SECOND: u64 = 1000;
/// A card that can wait shows this long after the hold ended or another card
/// left. Long enough that a second click meant for the card before misses the
/// next one; short enough that the next one reads as the queue moving on.
const GRACE_MS: u64 = 5 * SECOND;
/// ATROPHY_WARNING folds into the strip after showing this long unanswered.
const FOLD_MS: u64 = 60 * SECOND;
/// A start asked this recently removed the reason for a waiting
/// ATROPHY_WARNING, even if the deck refused it: the user is at the treadmill.
/// As long as `seated`'s quiet time after its own ENGAGE.
const START_ASK_MS: u64 = 2 * 60 * SECOND;
/// A longer gap between two looks is a stall or sleep: no showing time counted.
const MAX_STEP_MS: u64 = 10 * SECOND;

/// The one card the HUD shows.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/ipc/bindings/")]
#[serde(rename_all = "snake_case")]
pub enum HudCard {
    #[default]
    None,
    /// `walkaway.card`: the warning or the paused card.
    WalkAway,
    /// `eye_break.phase`: the heads-up, the break or the resume card.
    EyeBreak,
    /// ATROPHY_WARNING, the sit-too-long card.
    Seated,
    /// LAST WEEK.
    Weekly,
    /// BELT SERVICE DUE.
    Maintenance,
    /// UPDATE READY.
    Update,
}

impl HudCard {
    /// Belt safety never waits for a grace.
    fn waits(self, eye: EyeBreakPhase) -> bool {
        match self {
            HudCard::None | HudCard::WalkAway => false,
            // The resume card holds the belt; a break under way is running.
            HudCard::EyeBreak => matches!(eye, EyeBreakPhase::Upcoming | EyeBreakPhase::Waiting),
            _ => true,
        }
    }
}

/// What the queue looks at, each time the snapshot is built.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Input {
    pub now_ms: u64,
    /// The presenting hold (`presenting.holding`).
    pub holding: bool,
    pub walkaway: bool,
    pub eye: EyeBreakPhase,
    /// The heads-up is set for a program's rest (`eye_break.at_rest`).
    pub eye_at_rest: bool,
    /// A program waits for GO.
    pub program_go: bool,
    /// The belt runs.
    pub belt_running: bool,
    /// The deck counts down to a start, or a start is in flight.
    pub belt_starting: bool,
    /// When a start was last asked (epoch ms), refused or not.
    pub start_asked_ms: Option<u64>,
    /// Each machine's own answer: the card is due.
    pub seated: bool,
    pub weekly: bool,
    pub maintenance: bool,
    pub update: bool,
}

/// What the HUD shows.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Decision {
    pub card: HudCard,
    /// ATROPHY_WARNING is due but folded into the strip's readout.
    pub seated_folded: bool,
    /// The eye-break machine holds its heads-up and break back.
    pub eye_break_waits: bool,
}

/// Where ATROPHY_WARNING is in its showing, while its machine has it due.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
enum SeatedShowing {
    /// Due and not shown yet.
    #[default]
    Fresh,
    /// Shown this long so far (unanswered).
    Shown(u64),
    /// Folded into the readout: no card until its machine lets it go.
    Folded,
}

#[derive(Debug, Default)]
pub struct Queue {
    last_ms: Option<u64>,
    shown: HudCard,
    /// When the shown card last went (another may wait out the grace).
    left_ms: Option<u64>,
    was_holding: bool,
    hold_ended_ms: Option<u64>,
    start_asked_ms: Option<u64>,
    seated: SeatedShowing,
}

fn within(now: u64, since: Option<u64>, ms: u64) -> bool {
    since.is_some_and(|t| now.saturating_sub(t) < ms)
}

impl Queue {
    /// Pure: the card to show now, from the machines' answers and what the
    /// queue remembers.
    pub fn step(&mut self, i: &Input) -> Decision {
        let now = i.now_ms;
        let step = self.last_ms.map_or(0, |prev| now.saturating_sub(prev));
        self.last_ms = Some(now);
        let dt = if step > MAX_STEP_MS { 0 } else { step };

        if self.was_holding && !i.holding {
            self.hold_ended_ms = Some(now);
        }
        self.was_holding = i.holding;
        if i.belt_starting {
            self.start_asked_ms = Some(now);
        }
        if let Some(at) = i.start_asked_ms {
            self.start_asked_ms = Some(self.start_asked_ms.map_or(at, |t| t.max(at)));
        }

        // ATROPHY_WARNING: forgotten once its machine lets it go; dropped as
        // moot when a start was asked (or the belt moves) before it showed.
        if !i.seated {
            self.seated = SeatedShowing::Fresh;
        } else if self.seated == SeatedShowing::Fresh
            && (i.belt_running || within(now, self.start_asked_ms, START_ASK_MS))
        {
            self.seated = SeatedShowing::Folded;
        }

        let eye_deferred = i.eye == EyeBreakPhase::Upcoming
            && ((i.eye_at_rest && i.belt_running) || i.belt_starting);
        // Highest rank first. `None` for the GO prompt: it is on the panel.
        let ranked = [
            (i.walkaway, HudCard::WalkAway),
            (i.eye == EyeBreakPhase::Complete, HudCard::EyeBreak),
            (i.eye == EyeBreakPhase::Break, HudCard::EyeBreak),
            (i.program_go, HudCard::None),
            (
                i.eye == EyeBreakPhase::Upcoming && !eye_deferred && !i.holding,
                HudCard::EyeBreak,
            ),
            // DEFERRED: due mid-walk, waiting for the walk to end. Shown while
            // the walk runs (it says when the belt will pause); the presenting
            // hold never reaches it, as `presenting::hold_cards` keeps it too.
            (i.eye == EyeBreakPhase::Waiting, HudCard::EyeBreak),
            (
                i.seated && self.seated != SeatedShowing::Folded && !i.holding,
                HudCard::Seated,
            ),
            (i.weekly && !i.holding, HudCard::Weekly),
            (i.maintenance && !i.holding, HudCard::Maintenance),
            (i.update && !i.holding, HudCard::Update),
        ];
        let top = ranked
            .iter()
            .find(|(due, _)| *due)
            .map_or(HudCard::None, |(_, card)| *card);
        // The card showing goes now (answered, or a higher one came).
        if self.shown != HudCard::None && top != self.shown {
            self.left_ms = Some(now);
        }
        let in_grace =
            within(now, self.left_ms, GRACE_MS) || within(now, self.hold_ended_ms, GRACE_MS);
        let card = if top != self.shown && top.waits(i.eye) && in_grace {
            HudCard::None
        } else {
            top
        };
        self.shown = card;

        if card == HudCard::Seated {
            let shown_for = match self.seated {
                SeatedShowing::Shown(ms) => ms + dt,
                _ => 0,
            };
            self.seated = if shown_for >= FOLD_MS {
                SeatedShowing::Folded
            } else {
                SeatedShowing::Shown(shown_for)
            };
        }
        // Folded this time: the card goes now, and the next may follow.
        let card = if card == HudCard::Seated && self.seated == SeatedShowing::Folded {
            self.left_ms = Some(now);
            self.shown = HudCard::None;
            HudCard::None
        } else {
            card
        };

        Decision {
            card,
            // No readout while the belt starts or runs: a walk lets the card go.
            seated_folded: i.seated
                && self.seated == SeatedShowing::Folded
                && !i.belt_starting
                && !i.belt_running,
            // Never a break already due (DEFERRED) or under way: holding the
            // machine then would start its wait, and its cap, again.
            eye_break_waits: card != HudCard::EyeBreak
                && !matches!(i.eye, EyeBreakPhase::Waiting | EyeBreakPhase::Break)
                && (i.walkaway || i.program_go || i.eye == EyeBreakPhase::Complete || in_grace),
        }
    }
}

/// The queue's input from a snapshot built this tick, before the presenting
/// hold hides anything (the queue applies the hold itself).
pub fn input_of(s: &AppSnapshot, now_ms: u64) -> Input {
    let connected = s.connection.connected;
    Input {
        now_ms,
        holding: s.presenting.holding,
        walkaway: s.walkaway.card != WalkAwayCard::None,
        eye: s.eye_break.phase,
        eye_at_rest: s.eye_break.at_rest,
        program_go: s.active_program.as_ref().is_some_and(|a| a.wait.is_some()),
        belt_running: connected && s.deck.status == MachineStatus::Running,
        belt_starting: connected
            && (s.deck.status == MachineStatus::Starting || s.connection.start_pending),
        start_asked_ms: s
            .connection
            .start_result
            .as_ref()
            .and_then(|r| u64::try_from(r.at_ms).ok()),
        seated: s.seated.show_card,
        weekly: s.weekly.show_card,
        maintenance: s.maintenance.hud_card,
        update: s.update.hud_card,
    }
}

static QUEUE: Mutex<Option<Queue>> = Mutex::new(None);
static EYE_BREAK_WAITS: Mutex<bool> = Mutex::new(false);

/// Pick the card for this snapshot and write it in (`hud_card`,
/// `seated.folded`). Call after every card is decided and before the
/// presenting hold hides any.
pub fn apply(s: &mut AppSnapshot, now_ms: u64) {
    let input = input_of(s, now_ms);
    let d = QUEUE
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get_or_insert_with(Queue::default)
        .step(&input);
    *EYE_BREAK_WAITS.lock().unwrap_or_else(|e| e.into_inner()) = d.eye_break_waits;
    s.hud_card = d.card;
    s.seated.folded = d.seated_folded;
}

/// ATROPHY_WARNING is folded into the strip (`desk_resume.rs` keeps it
/// across a restart).
pub fn seated_folded() -> bool {
    QUEUE
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .as_ref()
        .is_some_and(|q| q.seated == SeatedShowing::Folded)
}

/// After a restart: ATROPHY_WARNING was folded, so it stays folded and
/// doesn't come back as a whole card.
pub fn restore_seated_folded() {
    QUEUE
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get_or_insert_with(Queue::default)
        .seated = SeatedShowing::Folded;
}

/// The eye-break machine holds its heads-up and break back (the desk loop).
pub fn eye_break_waits() -> bool {
    *EYE_BREAK_WAITS.lock().unwrap_or_else(|e| e.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;

    const T0: u64 = 1_800_000_000_000;
    const S: u64 = SECOND;

    fn at(ms: u64) -> Input {
        Input {
            now_ms: T0 + ms,
            ..Input::default()
        }
    }

    /// Steps the queue once a second from `from` to `to` (ms after T0) with
    /// `input` for each time, and returns the last decision.
    fn run(q: &mut Queue, from: u64, to: u64, input: impl Fn(Input) -> Input) -> Decision {
        let mut t = from;
        let mut d = Decision::default();
        while t <= to {
            d = q.step(&input(at(t)));
            t += S;
        }
        d
    }

    fn everything(i: Input) -> Input {
        Input {
            walkaway: true,
            eye: EyeBreakPhase::Upcoming,
            seated: true,
            weekly: true,
            maintenance: true,
            update: true,
            ..i
        }
    }

    #[test]
    fn one_card_at_a_time_in_rank_order() {
        let mut q = Queue::default();
        let mut i = everything(at(0));
        assert_eq!(q.step(&i).card, HudCard::WalkAway);
        // Each answer lets the next one through, after the grace.
        let order = [
            HudCard::EyeBreak,
            HudCard::Seated,
            HudCard::Weekly,
            HudCard::Maintenance,
            HudCard::Update,
        ];
        let mut t = 0;
        for (n, next) in order.iter().enumerate() {
            match n {
                0 => i.walkaway = false,
                1 => i.eye = EyeBreakPhase::Idle,
                2 => i.seated = false,
                3 => i.weekly = false,
                _ => i.maintenance = false,
            }
            t += S;
            i.now_ms = T0 + t;
            assert_eq!(q.step(&i).card, HudCard::None, "the grace before {next:?}");
            t += GRACE_MS;
            i.now_ms = T0 + t;
            assert_eq!(q.step(&i).card, *next);
        }
    }

    #[test]
    fn belt_safety_takes_over_at_once_and_the_card_below_waits() {
        let mut q = Queue::default();
        let update = |i: Input| Input { update: true, ..i };
        assert_eq!(run(&mut q, 0, 0, update).card, HudCard::Update);
        let warn = |i: Input| Input {
            walkaway: true,
            ..update(i)
        };
        assert_eq!(run(&mut q, S, S, warn).card, HudCard::WalkAway);
        // Input cleared the warning: the update comes back after the grace.
        assert_eq!(run(&mut q, 2 * S, 3 * S, update).card, HudCard::None);
        assert_eq!(run(&mut q, 4 * S, 7 * S, update).card, HudCard::Update);
    }

    #[test]
    fn the_resume_card_outranks_go_and_the_cards_below() {
        let mut q = Queue::default();
        let i = |i: Input| Input {
            eye: EyeBreakPhase::Complete,
            program_go: true,
            seated: true,
            ..i
        };
        let d = run(&mut q, 0, 0, i);
        assert_eq!(d.card, HudCard::EyeBreak);
    }

    #[test]
    fn go_holds_the_cards_below_it_and_the_eye_break_back() {
        let mut q = Queue::default();
        let go = |i: Input| Input {
            program_go: true,
            eye: EyeBreakPhase::Upcoming,
            seated: true,
            update: true,
            ..i
        };
        let d = run(&mut q, 0, 30 * 60 * S, go);
        assert_eq!(
            d.card,
            HudCard::None,
            "the GO prompt on the panel is the one"
        );
        assert!(d.eye_break_waits, "no break begins behind the GO prompt");
        assert!(!d.seated_folded, "never shown, so nothing folded yet");
    }

    #[test]
    fn a_break_under_way_keeps_its_card_through_a_go_prompt() {
        let mut q = Queue::default();
        let d = run(&mut q, 0, 0, |i| Input {
            eye: EyeBreakPhase::Break,
            program_go: true,
            ..i
        });
        assert_eq!(d.card, HudCard::EyeBreak);
        assert!(!d.eye_break_waits);
    }

    #[test]
    fn go_drops_a_waiting_atrophy_warning() {
        let mut q = Queue::default();
        let waiting = |i: Input| Input {
            program_go: true,
            seated: true,
            ..i
        };
        run(&mut q, 0, 60 * S, waiting);
        // GO: the deck counts down, then runs.
        let starting = |i: Input| Input {
            seated: true,
            belt_starting: true,
            ..i
        };
        let d = run(&mut q, 61 * S, 63 * S, starting);
        assert_eq!(
            d.card,
            HudCard::None,
            "no flash between GO and the belt running"
        );
        assert!(!d.seated_folded, "nor a readout for the countdown");
        // The machine lets the card go once the belt runs.
        let d = run(&mut q, 64 * S, 70 * S, |i| Input {
            belt_running: true,
            ..i
        });
        assert_eq!(d.card, HudCard::None);
        assert!(!d.seated_folded);
    }

    #[test]
    fn a_refused_start_drops_a_waiting_atrophy_warning_too() {
        let mut q = Queue::default();
        // GO refused at once (the deck asleep): only the start result says so.
        let refused = |i: Input| Input {
            seated: true,
            start_asked_ms: Some(T0 + 10 * S),
            ..i
        };
        let d = run(&mut q, 10 * S, 30 * S, refused);
        assert_eq!(d.card, HudCard::None);
        assert!(d.seated_folded, "the readout stays on the strip");
        // The deck woke and the result cleared: still dropped.
        let d = run(&mut q, 31 * S, 40 * S, |i| Input { seated: true, ..i });
        assert_eq!(d.card, HudCard::None);
    }

    #[test]
    fn an_atrophy_warning_due_long_after_a_start_shows() {
        let mut q = Queue::default();
        let d = run(&mut q, 0, 0, |i| Input {
            seated: true,
            start_asked_ms: Some(T0 - START_ASK_MS),
            ..i
        });
        assert_eq!(d.card, HudCard::Seated);
    }

    #[test]
    fn a_start_does_not_fold_the_card_already_showing() {
        // ENGAGE on the card itself: a refusal keeps the card and its line.
        let mut q = Queue::default();
        run(&mut q, 0, 5 * S, |i| Input { seated: true, ..i });
        let d = run(&mut q, 6 * S, 8 * S, |i| Input {
            seated: true,
            start_asked_ms: Some(T0 + 6 * S),
            ..i
        });
        assert_eq!(d.card, HudCard::Seated);
    }

    #[test]
    fn atrophy_warning_folds_after_a_minute_shown() {
        let mut q = Queue::default();
        let seated = |i: Input| Input { seated: true, ..i };
        let d = run(&mut q, 0, FOLD_MS - S, seated);
        assert_eq!(d.card, HudCard::Seated);
        assert!(!d.seated_folded);
        let d = run(&mut q, FOLD_MS, FOLD_MS, seated);
        assert_eq!(d.card, HudCard::None, "folded into the strip");
        assert!(d.seated_folded);
        // It stays folded, however long it is due.
        let d = run(&mut q, FOLD_MS + S, 2 * 3600 * S, seated);
        assert_eq!(d.card, HudCard::None);
        assert!(d.seated_folded);
        // SNOOZE (or a walk) lets it go; due again, it shows once more.
        run(&mut q, 2 * 3600 * S + S, 2 * 3600 * S + S, |i| i);
        let d = run(&mut q, 2 * 3600 * S + 2 * S, 2 * 3600 * S + 2 * S, seated);
        assert_eq!(d.card, HudCard::Seated);
    }

    #[test]
    fn only_the_time_it_shows_counts_toward_the_fold() {
        let mut q = Queue::default();
        let seated = |i: Input| Input { seated: true, ..i };
        run(&mut q, 0, 30 * S, seated);
        // An eye break takes the slot for two and a half minutes.
        let eye = |i: Input| Input {
            eye: EyeBreakPhase::Break,
            ..seated(i)
        };
        run(&mut q, 31 * S, 180 * S, eye);
        let d = run(&mut q, 181 * S, 181 * S + GRACE_MS + 10 * S, seated);
        assert_eq!(d.card, HudCard::Seated, "30 s + 10 s shown so far");
        let d = run(
            &mut q,
            181 * S + GRACE_MS + 11 * S,
            181 * S + GRACE_MS + 30 * S,
            seated,
        );
        assert_eq!(d.card, HudCard::None);
        assert!(d.seated_folded);
    }

    #[test]
    fn the_folded_card_lets_the_next_one_through() {
        let mut q = Queue::default();
        let i = |i: Input| Input {
            seated: true,
            update: true,
            ..i
        };
        run(&mut q, 0, FOLD_MS, i);
        let d = run(&mut q, FOLD_MS + S, FOLD_MS + GRACE_MS, i);
        assert_eq!(d.card, HudCard::Update);
        assert!(d.seated_folded);
    }

    #[test]
    fn held_cards_come_back_one_by_one_after_a_grace() {
        let mut q = Queue::default();
        let held = |i: Input| Input {
            holding: true,
            ..everything(i)
        };
        let d = run(&mut q, 0, 60 * S, |i| Input {
            walkaway: false,
            ..held(i)
        });
        assert_eq!(d.card, HudCard::None, "everything held");
        assert!(
            !d.eye_break_waits,
            "the hold itself tells the eye-break machine"
        );
        let after = |i: Input| Input {
            walkaway: false,
            ..everything(i)
        };
        let d = run(&mut q, 61 * S, 61 * S + GRACE_MS - S, after);
        assert_eq!(d.card, HudCard::None, "a short grace first");
        assert!(d.eye_break_waits, "the heads-up keeps its minute");
        let d = run(&mut q, 61 * S + GRACE_MS, 61 * S + GRACE_MS, after);
        assert_eq!(d.card, HudCard::EyeBreak, "then the first, alone");
        assert!(!d.eye_break_waits);
    }

    #[test]
    fn the_hold_never_hides_belt_safety_or_a_break_under_way() {
        for (input, card) in [
            (
                Input {
                    walkaway: true,
                    ..Input::default()
                },
                HudCard::WalkAway,
            ),
            (
                Input {
                    eye: EyeBreakPhase::Complete,
                    ..Input::default()
                },
                HudCard::EyeBreak,
            ),
            (
                Input {
                    eye: EyeBreakPhase::Break,
                    ..Input::default()
                },
                HudCard::EyeBreak,
            ),
        ] {
            let mut q = Queue::default();
            let d = q.step(&Input {
                holding: true,
                now_ms: T0,
                ..input
            });
            assert_eq!(d.card, card);
        }
    }

    #[test]
    fn an_eye_break_set_for_a_rest_leaves_the_queue_while_the_belt_walks() {
        let mut q = Queue::default();
        let walking = |i: Input| Input {
            eye: EyeBreakPhase::Upcoming,
            eye_at_rest: true,
            belt_running: true,
            update: true,
            ..i
        };
        let d = run(&mut q, 0, 30 * S, walking);
        assert_eq!(
            d.card,
            HudCard::Update,
            "the deferred break is not in the way"
        );
        assert!(
            !d.eye_break_waits,
            "its machine waits for the rest by itself"
        );
        // At the stop the break begins: it takes the slot from the update.
        let d = run(&mut q, 31 * S, 31 * S, |i| Input {
            eye: EyeBreakPhase::Break,
            update: true,
            ..i
        });
        assert_eq!(d.card, HudCard::EyeBreak);
    }

    #[test]
    fn a_heads_up_steps_aside_while_the_belt_starts() {
        let mut q = Queue::default();
        let d = run(&mut q, 0, 2 * S, |i| Input {
            eye: EyeBreakPhase::Upcoming,
            belt_starting: true,
            ..i
        });
        assert_eq!(d.card, HudCard::None);
    }

    #[test]
    fn the_heads_up_waits_out_the_grace_with_its_machine_held() {
        let mut q = Queue::default();
        let seated = |i: Input| Input { seated: true, ..i };
        run(&mut q, 0, 10 * S, seated);
        // The heads-up outranks the card showing: a short gap, then it.
        let both = |i: Input| Input {
            eye: EyeBreakPhase::Upcoming,
            ..seated(i)
        };
        let d = run(&mut q, 11 * S, 11 * S, both);
        assert_eq!(d.card, HudCard::None);
        assert!(d.eye_break_waits);
        let d = run(&mut q, 12 * S, 11 * S + GRACE_MS, both);
        assert_eq!(d.card, HudCard::EyeBreak);
        assert!(!d.eye_break_waits);
    }

    #[test]
    fn a_deferred_break_shows_while_the_walk_runs_and_atrophy_warning_is_moot() {
        let mut q = Queue::default();
        // Walking: the seated machine has let its card go; the queue drops it.
        let walking = |i: Input| Input {
            eye: EyeBreakPhase::Waiting,
            belt_running: true,
            update: true,
            ..i
        };
        let d = run(&mut q, 0, 30 * S, walking);
        assert_eq!(d.card, HudCard::EyeBreak, "DEFERRED, above the update");
        assert!(!d.eye_break_waits, "a break already due is never held back");
        // A seated card due in the same breath (the belt only just started)
        // is moot: the walk under way removed its reason.
        let d = run(&mut q, 31 * S, 40 * S, |i| Input {
            seated: true,
            ..walking(i)
        });
        assert_eq!(d.card, HudCard::EyeBreak);
        assert!(!d.seated_folded, "no readout while the belt runs");
        // The walk ends: the break begins where DEFERRED was, no grace.
        let d = run(&mut q, 41 * S, 41 * S, |i| Input {
            eye: EyeBreakPhase::Break,
            update: true,
            ..i
        });
        assert_eq!(d.card, HudCard::EyeBreak);
    }

    #[test]
    fn a_deferred_break_waits_out_a_grace_without_its_machine_held() {
        let mut q = Queue::default();
        let weekly = |i: Input| Input {
            weekly: true,
            belt_running: true,
            ..i
        };
        run(&mut q, 0, 5 * S, weekly);
        let d = run(&mut q, 6 * S, 6 * S, |i| Input {
            eye: EyeBreakPhase::Waiting,
            ..weekly(i)
        });
        assert_eq!(d.card, HudCard::None, "a short gap before DEFERRED");
        assert!(!d.eye_break_waits, "its wait and cap keep running");
        let d = run(&mut q, 7 * S, 6 * S + GRACE_MS, |i| Input {
            eye: EyeBreakPhase::Waiting,
            ..weekly(i)
        });
        assert_eq!(d.card, HudCard::EyeBreak);
    }

    #[test]
    fn the_hold_keeps_a_deferred_break() {
        let mut q = Queue::default();
        let d = q.step(&Input {
            now_ms: T0,
            holding: true,
            eye: EyeBreakPhase::Waiting,
            belt_running: true,
            ..Input::default()
        });
        assert_eq!(d.card, HudCard::EyeBreak);
    }

    #[test]
    fn the_update_card_ranks_last_and_shows_only_when_offered() {
        // With "Install automatically" on, `update.hud_card` stays false (#424),
        // so nothing shows; off, the offer comes after every other card.
        let mut q = Queue::default();
        let d = run(&mut q, 0, 10 * S, |i| i);
        assert_eq!(d.card, HudCard::None);
        let d = run(&mut q, 11 * S, 11 * S, |i| Input {
            update: true,
            weekly: true,
            ..i
        });
        assert_eq!(d.card, HudCard::Weekly);
        let d = run(&mut q, 12 * S, 12 * S + GRACE_MS, |i| Input {
            update: true,
            ..i
        });
        assert_eq!(d.card, HudCard::Update);
    }

    #[test]
    fn nothing_due_shows_nothing_and_holds_nothing() {
        let mut q = Queue::default();
        let d = run(&mut q, 0, 60 * S, |i| i);
        assert_eq!(d, Decision::default());
    }
}
