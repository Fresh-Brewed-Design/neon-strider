//! DECK_PROFILE: which speed bytes the deck runs, learned from its replies.
//!
//! The deck echoes every target it takes with the speed byte it actually runs
//! (it clamps to its floor and ceiling), and nacks one it refuses. Each reply
//! is filed here per byte asked; nothing is ever sent to learn it. A byte
//! never asked stays allowed. The engine uses the profile to send, show and
//! step only to bytes the deck runs, and to bound speeds by the deck's real
//! floor and ceiling once they are seen.
//!
//! One profile per treadmill, by its saved Bluetooth identity: each is one JSON
//! value in the settings table (`key_for`), holding both the speed bytes and
//! the incline levels, since two maps of at most 256 entries are small, read
//! when the connection switches to its treadmill and rewritten whole, so they
//! need no table of their own.
//!
//! Incline levels are learned the same way from the same replies (the echo's
//! incline byte, a nack of a packet that changed only the incline), in a map
//! of their own: level 0 is flat, a real level, and is never skipped.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// The settings key the one app-wide profile was stored under, before there
/// was one per treadmill. The first treadmill to connect adopts it.
pub const LEGACY_KEY: &str = "deckProfile";

/// The settings key the profile of the treadmill with this normalized device
/// id is stored under.
pub fn key_for(device_id: &str) -> String {
    format!("{}:{}", LEGACY_KEY, device_id)
}

/// A floor or ceiling is only read from a change of at least this many bytes:
/// a deck that skips an in-between speed moves it to a neighbour, and taking
/// that for its floor would hide every slower speed for good. Incline levels
/// use the same rule for the top level, for the same reason.
const LIMIT_JUMP: u8 = 2;

/// What the deck did with one asked speed byte.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    /// Echoed the target with this speed byte: the asked byte itself when the
    /// deck runs it, another one when it changed it.
    Took(u8),
    /// Nacked the target.
    Refused,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeckProfile {
    /// Asked speed byte -> what the deck did with it, the latest reply wins.
    bytes: BTreeMap<u8, Outcome>,
    /// Asked incline level -> what the deck did with it, the latest reply wins.
    #[serde(default)]
    levels: BTreeMap<u8, Outcome>,
    /// Changed since the last save.
    #[serde(skip)]
    unsaved: bool,
}

impl DeckProfile {
    /// The stored profile, or an empty one when the value is missing or
    /// unreadable.
    pub fn from_stored(raw: Option<&str>) -> Self {
        let Some(raw) = raw else {
            return Self::default();
        };
        match serde_json::from_str::<Self>(raw) {
            Ok(p) => p,
            Err(e) => {
                log::warn!("could not parse saved deck profile: {}", e);
                Self::default()
            }
        }
    }

    /// Nothing learned, neither speeds nor incline levels.
    pub fn is_empty(&self) -> bool {
        self.bytes.is_empty() && self.levels.is_empty()
    }

    /// At least one speed has a reply on file.
    pub fn has_speeds(&self) -> bool {
        !self.bytes.is_empty()
    }

    /// At least one incline level has a reply on file.
    pub fn has_levels(&self) -> bool {
        !self.levels.is_empty()
    }

    /// How many incline levels have a reply on file.
    pub fn level_count(&self) -> usize {
        self.levels.len()
    }

    /// How many speed bytes have a reply on file.
    pub fn len(&self) -> usize {
        self.bytes.len()
    }

    /// File the deck's reply to a target at `asked`. Byte 0 is the still
    /// belt, never a running speed, so it is not filed.
    pub fn learn(&mut self, asked: u8, outcome: Outcome) {
        if asked == 0 || outcome == Outcome::Took(0) {
            return;
        }
        if self.bytes.insert(asked, outcome) != Some(outcome) {
            self.unsaved = true;
        }
    }

    /// The deck has answered a target at `byte`.
    pub fn knows(&self, byte: u8) -> bool {
        self.bytes.contains_key(&byte)
    }

    /// File the deck's reply to a target at incline `asked`.
    pub fn learn_level(&mut self, asked: u8, outcome: Outcome) {
        if self.levels.insert(asked, outcome) != Some(outcome) {
            self.unsaved = true;
        }
    }

    /// Forget everything learned.
    pub fn clear(&mut self) {
        if !self.is_empty() {
            self.bytes.clear();
            self.levels.clear();
            self.unsaved = true;
        }
    }

    /// The JSON to save when the profile changed since the last call, None
    /// when it did not. An empty profile saves as None too: `Some(None)`
    /// means "delete the stored value".
    pub fn take_unsaved(&mut self) -> Option<Option<String>> {
        if !std::mem::take(&mut self.unsaved) {
            return None;
        }
        if self.is_empty() {
            return Some(None);
        }
        Some(serde_json::to_string(self).ok())
    }

    /// Bytes the deck is known to run: every byte it echoed.
    fn runs(&self) -> impl Iterator<Item = u8> + '_ {
        self.bytes.values().filter_map(|o| match o {
            Outcome::Took(b) => Some(*b),
            Outcome::Refused => None,
        })
    }

    /// The deck's floor: its slowest known running byte, once a byte at
    /// least `LIMIT_JUMP` slower was asked and the deck took that one instead.
    pub fn floor(&self) -> Option<u8> {
        let lowest = self.runs().min()?;
        self.bytes
            .iter()
            .any(|(asked, o)| {
                asked.saturating_add(LIMIT_JUMP) <= lowest && *o == Outcome::Took(lowest)
            })
            .then_some(lowest)
    }

    /// The deck's ceiling: its fastest known running byte, once a byte at
    /// least `LIMIT_JUMP` faster was asked and the deck took that one instead.
    pub fn ceiling(&self) -> Option<u8> {
        let highest = self.runs().max()?;
        self.bytes
            .iter()
            .any(|(asked, o)| {
                *asked >= highest.saturating_add(LIMIT_JUMP) && *o == Outcome::Took(highest)
            })
            .then_some(highest)
    }

    /// The deck is known not to run `byte` as asked: it changed or refused
    /// it, or the byte lies outside the floor or ceiling. A byte never asked
    /// inside them is allowed.
    pub fn skips(&self, byte: u8) -> bool {
        if byte == 0 {
            return false;
        }
        if self.floor().is_some_and(|f| byte < f) || self.ceiling().is_some_and(|c| byte > c) {
            return true;
        }
        match self.bytes.get(&byte) {
            Some(Outcome::Took(b)) => *b != byte,
            Some(Outcome::Refused) => true,
            // Echoed as the byte taken for another one: it runs.
            None => false,
        }
    }

    /// The byte to send for `byte`: itself unless the deck is known to skip
    /// it. Below the floor or above the ceiling it is that limit; a byte the
    /// deck changed is what it changed it to; a refused byte is the nearest
    /// byte not skipped (the slower one on a tie). Byte 0 (still) stays 0,
    /// and a running byte never becomes 0.
    pub fn snap(&self, byte: u8) -> u8 {
        if !self.skips(byte) {
            return byte;
        }
        if let Some(f) = self.floor().filter(|f| byte < *f) {
            return f;
        }
        if let Some(c) = self.ceiling().filter(|c| byte > *c) {
            return c;
        }
        if let Some(Outcome::Took(b)) = self.bytes.get(&byte) {
            if !self.skips(*b) {
                return *b;
            }
        }
        (1..=u8::MAX)
            .flat_map(|d| [byte.checked_sub(d), byte.checked_add(d)])
            .flatten()
            .find(|b| *b != 0 && !self.skips(*b))
            .unwrap_or(byte)
    }

    /// The deck's top incline level: its highest known running level, once a
    /// level at least `LIMIT_JUMP` higher was asked and the deck took that one
    /// instead. Level 0 is the floor and needs no learning.
    pub fn incline_ceiling(&self) -> Option<u8> {
        let highest = self.levels.values().filter_map(took).max()?;
        self.levels
            .iter()
            .any(|(asked, o)| {
                *asked >= highest.saturating_add(LIMIT_JUMP) && *o == Outcome::Took(highest)
            })
            .then_some(highest)
    }

    /// The deck is known not to run incline `level` as asked: it changed or
    /// refused it, or the level lies above the ceiling. Level 0 never is.
    pub fn skips_level(&self, level: u8) -> bool {
        if level == 0 {
            return false;
        }
        if self.incline_ceiling().is_some_and(|c| level > c) {
            return true;
        }
        match self.levels.get(&level) {
            Some(Outcome::Took(l)) => *l != level,
            Some(Outcome::Refused) => true,
            None => false,
        }
    }

    /// The level to send for `level`: itself unless the deck is known to skip
    /// it. Above the ceiling it is the ceiling; a level the deck changed is
    /// what it changed it to; a refused one is the nearest level not skipped
    /// (the lower one on a tie).
    pub fn snap_level(&self, level: u8) -> u8 {
        if !self.skips_level(level) {
            return level;
        }
        if let Some(c) = self.incline_ceiling().filter(|c| level > *c) {
            return c;
        }
        if let Some(Outcome::Took(l)) = self.levels.get(&level) {
            if !self.skips_level(*l) {
                return *l;
            }
        }
        (1..=u8::MAX)
            .flat_map(|d| [level.checked_sub(d), level.checked_add(d)])
            .flatten()
            .find(|l| !self.skips_level(*l))
            .unwrap_or(level)
    }

    /// The incline levels the deck changed or refused, for Diagnostics.
    pub fn skipped_levels(&self) -> Vec<u8> {
        self.levels
            .iter()
            .filter(|(asked, o)| **o != Outcome::Took(**asked))
            .map(|(asked, _)| *asked)
            .collect()
    }

    /// The bytes the deck changed or refused, for Diagnostics.
    pub fn skipped(&self) -> Vec<u8> {
        self.bytes
            .iter()
            .filter(|(asked, o)| **o != Outcome::Took(**asked))
            .map(|(asked, _)| *asked)
            .collect()
    }
}

fn took(o: &Outcome) -> Option<u8> {
    match o {
        Outcome::Took(b) => Some(*b),
        Outcome::Refused => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn profile(entries: &[(u8, Outcome)]) -> DeckProfile {
        let mut p = DeckProfile::default();
        for (b, o) in entries {
            p.learn(*b, *o);
        }
        p
    }

    #[test]
    fn an_empty_profile_allows_every_byte() {
        let p = DeckProfile::default();
        assert!(p.is_empty());
        assert_eq!((p.floor(), p.ceiling()), (None, None));
        for b in 0..=u8::MAX {
            assert!(!p.skips(b));
            assert_eq!(p.snap(b), b);
        }
    }

    /// The recordings: 0.3 km/h asked, 0.6 taken; 1.1-1.3 and 2.0 taken as asked.
    #[test]
    fn the_recorded_floor_is_learned_from_the_echo() {
        let p = profile(&[
            (3, Outcome::Took(6)),
            (11, Outcome::Took(11)),
            (12, Outcome::Took(12)),
            (13, Outcome::Took(13)),
            (20, Outcome::Took(20)),
        ]);
        assert_eq!(p.floor(), Some(6));
        assert_eq!(p.ceiling(), None, "nothing above 2.0 asked yet");
        assert_eq!(p.snap(3), 6);
        assert_eq!(p.snap(5), 6, "below the floor");
        assert_eq!(p.snap(8), 8, "never asked: allowed");
        assert_eq!(p.snap(60), 60);
        assert_eq!(p.skipped(), vec![3]);
    }

    #[test]
    fn the_ceiling_and_skipped_speeds_snap_to_what_the_deck_runs() {
        let p = profile(&[
            (50, Outcome::Took(40)),
            (15, Outcome::Took(16)),
            (17, Outcome::Refused),
            (16, Outcome::Took(16)),
        ]);
        assert_eq!(p.ceiling(), Some(40));
        assert_eq!(
            p.floor(),
            None,
            "1.5 -> 1.6 is a skipped speed, not a floor"
        );
        assert_eq!(p.snap(45), 40);
        assert_eq!(p.snap(15), 16);
        assert_eq!(
            p.snap(17),
            16,
            "refused: 1.6 and 1.8 are as near, the slower wins"
        );
        assert_eq!(p.snap(10), 10, "no floor: slower speeds stay allowed");
        assert!(p.skips(15) && p.skips(17) && !p.skips(16) && !p.skips(18));
    }

    #[test]
    fn a_refused_byte_snaps_to_the_nearest_allowed_one_slower_first() {
        let p = profile(&[(17, Outcome::Refused), (18, Outcome::Refused)]);
        assert_eq!(p.snap(17), 16);
        assert_eq!(p.snap(18), 19);
        let low = profile(&[(1, Outcome::Refused)]);
        assert_eq!(low.snap(1), 2, "a running byte never becomes 0");
    }

    #[test]
    fn byte_zero_is_never_filed_or_snapped() {
        let mut p = profile(&[(0, Outcome::Refused), (4, Outcome::Took(0))]);
        assert!(p.is_empty());
        p.learn(3, Outcome::Took(6));
        assert_eq!(p.snap(0), 0);
    }

    #[test]
    fn it_saves_only_after_a_change_and_round_trips() {
        let mut p = DeckProfile::default();
        assert_eq!(p.take_unsaved(), None);
        p.learn(3, Outcome::Took(6));
        p.learn(17, Outcome::Refused);
        let json = p.take_unsaved().flatten().expect("json");
        assert_eq!(p.take_unsaved(), None);
        p.learn(3, Outcome::Took(6));
        assert_eq!(p.take_unsaved(), None, "the same reply again");
        let back = DeckProfile::from_stored(Some(&json));
        assert_eq!(back, p);
        assert_eq!(back.snap(4), 6);
        p.clear();
        assert_eq!(
            p.take_unsaved(),
            Some(None),
            "forgotten: delete the stored value"
        );
        assert!(DeckProfile::from_stored(Some("not json")).is_empty());
        assert!(DeckProfile::from_stored(None).is_empty());
    }

    #[test]
    fn incline_levels_are_learned_from_echoes_and_nacks() {
        let mut p = DeckProfile::default();
        p.learn_level(15, Outcome::Took(12));
        p.learn_level(7, Outcome::Took(8));
        p.learn_level(9, Outcome::Refused);
        p.learn_level(8, Outcome::Took(8));
        p.learn_level(0, Outcome::Took(0));
        assert!(p.has_levels() && !p.has_speeds() && !p.is_empty());
        assert_eq!(p.incline_ceiling(), Some(12));
        assert_eq!(p.snap_level(15), 12, "above the ceiling");
        assert_eq!(p.snap_level(7), 8, "changed by the deck");
        assert_eq!(
            p.snap_level(9),
            8,
            "refused: 8 and 10 are as near, lower wins"
        );
        assert_eq!(p.snap_level(3), 3, "never asked: allowed");
        assert_eq!(p.snap_level(0), 0, "flat is never skipped");
        assert_eq!(p.skipped_levels(), vec![7, 9, 15]);
        assert_eq!(p.snap(9), 9, "speeds are not touched");
    }

    #[test]
    fn a_refused_level_never_snaps_to_flat_wrongly_and_survives_a_round_trip() {
        let mut p = DeckProfile::default();
        p.learn_level(1, Outcome::Refused);
        assert_eq!(p.snap_level(1), 0, "flat is the nearest level");
        let json = p.take_unsaved().flatten().expect("json");
        assert_eq!(DeckProfile::from_stored(Some(&json)), p);
        // A profile saved before incline levels were learned still loads.
        let old = DeckProfile::from_stored(Some(r#"{"bytes":{"3":{"took":6}}}"#));
        assert_eq!(old.snap(4), 6);
        assert!(!old.has_levels());
        p.clear();
        assert_eq!(p.take_unsaved(), Some(None));
    }

    #[test]
    fn each_treadmill_has_its_own_key() {
        assert_ne!(key_for("AABB"), key_for("CCDD"));
        assert_ne!(key_for("AABB"), LEGACY_KEY);
    }
}
