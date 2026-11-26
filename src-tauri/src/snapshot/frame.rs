//! What one `app-state` event carries to one window (#274): the hot parts of
//! the snapshot (the belt, the walk, the HUD view) on every send, the cold
//! parts (programs, Strava, HUD config, weekly, update, ...) only when they
//! changed or the window has not had them yet, and nothing to a hidden
//! window, which gets everything again as soon as it shows.
//!
//! Which part is hot or cold only decides how much is sent, never whether a
//! window ends up right: a change to a cold part sends a full frame.

use super::AppSnapshot;
use serde::ser::{Serialize, SerializeMap, Serializer};
use std::collections::HashSet;

/// One window's `app-state` frame. Serialised flat, as the snapshot's own
/// fields plus `full`; without `full` the cold fields are left out and the
/// window keeps the ones it has (`src/state/mergeAppState.ts`).
#[derive(Clone, Copy)]
pub struct Frame<'a> {
    pub snapshot: &'a AppSnapshot,
    pub full: bool,
}

macro_rules! snapshot_parts {
    (hot: [$($hot:ident),* $(,)?], cold: [$($cold:ident),* $(,)?] $(,)?) => {
        /// The snapshot fields every frame carries.
        #[cfg(test)]
        pub const HOT_KEYS: &[&str] = &[$(stringify!($hot)),*];
        /// The snapshot fields only a full frame carries.
        #[cfg_attr(not(test), allow(dead_code))]
        pub const COLD_KEYS: &[&str] = &[$(stringify!($cold)),*];

        /// Every `AppSnapshot` field is hot or cold: a new field in neither
        /// list does not compile here.
        #[allow(dead_code)]
        fn every_field_is_listed(s: AppSnapshot) {
            let AppSnapshot { $($hot: _,)* $($cold: _,)* } = s;
        }

        /// The cold parts of `a` and `b` are the same.
        pub fn cold_equal(a: &AppSnapshot, b: &AppSnapshot) -> bool {
            true $(&& a.$cold == b.$cold)*
        }

        impl Serialize for Frame<'_> {
            fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
                let mut m = s.serialize_map(None)?;
                m.serialize_entry("full", &self.full)?;
                $(m.serialize_entry(stringify!($hot), &self.snapshot.$hot)?;)*
                if self.full {
                    $(m.serialize_entry(stringify!($cold), &self.snapshot.$cold)?;)*
                }
                m.end()
            }
        }
    };
}

snapshot_parts! {
    hot: [
        connection, deck, session, active_program, neural, speed_drift, hud_view, hud_go_hold, hud_card,
        maintenance, odometer, seated, eye_break, calls, walkaway, easy_start,
        easy_stop, traffic, calibration, ghost_pod, heart_rate, guided_task,
        pc_walk_share, presenting, program_tones,
    ],
    cold: [
        programs, fit_to_deck, fit_to_time, program_last_run_id, hud, hud_monitors, strava, weekly, desk_sync,
        wall_clock, lights_out, update, typing_speed, today_timeline, platform, shortcuts,
    ],
}

/// What one window gets this tick.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Delivery {
    Nothing,
    /// The hot parts only.
    Hot,
    /// Everything.
    Full,
}

/// Why this tick may send.
#[derive(Debug, Clone, Copy, Default)]
pub struct Due {
    /// The snapshot differs from the one sent last.
    pub changed: bool,
    /// Its cold parts differ too.
    pub cold_changed: bool,
    /// The heartbeat is due (hot parts at least).
    pub heartbeat: bool,
    /// The full refresh is due (everything to every shown window).
    pub refresh: bool,
}

/// The windows that hold the last frames sent: they had a full frame since
/// they last showed, and every frame after it.
#[derive(Debug, Default)]
pub struct Windows {
    in_sync: HashSet<String>,
}

impl Windows {
    /// What each window (label, shown) gets. A hidden window gets nothing and
    /// is forgotten, so it gets a full frame once it shows; so does a new one.
    pub fn plan(&mut self, windows: &[(String, bool)], due: Due) -> Vec<(String, Delivery)> {
        self.in_sync
            .retain(|l| windows.iter().any(|(label, shown)| label == l && *shown));
        windows
            .iter()
            .map(|(label, shown)| {
                let send = if !shown {
                    Delivery::Nothing
                } else if !self.in_sync.contains(label) || due.cold_changed || due.refresh {
                    Delivery::Full
                } else if due.changed || due.heartbeat {
                    Delivery::Hot
                } else {
                    Delivery::Nothing
                };
                if send == Delivery::Full {
                    self.in_sync.insert(label.clone());
                }
                (label.clone(), send)
            })
            .collect()
    }

    /// A send to `label` failed: it gets a full frame next time.
    pub fn lost(&mut self, label: &str) {
        self.in_sync.remove(label);
    }
}

/// `COLD_KEYS` as TypeScript, for `src/ipc/bindings/SNAPSHOT_PARTS.ts`.
#[cfg(test)]
fn typescript() -> String {
    let keys: Vec<String> = COLD_KEYS.iter().map(|k| format!("  \"{k}\",\n")).collect();
    format!(
        "// Generated by `cargo test` from src-tauri/src/snapshot/frame.rs. Do not edit this file manually.\n\
         import type {{ AppSnapshot }} from \"./AppSnapshot\";\n\
         \n\
         /** The snapshot fields an `app-state` frame carries only when `full` (they changed, or the window just showed). */\n\
         export const COLD_KEYS = [\n{}] as const satisfies readonly (keyof AppSnapshot)[];\n",
        keys.concat()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shown(labels: &[(&str, bool)]) -> Vec<(String, bool)> {
        labels.iter().map(|(l, s)| (l.to_string(), *s)).collect()
    }

    fn sends(plan: &[(String, Delivery)]) -> Vec<Delivery> {
        plan.iter().map(|(_, s)| *s).collect()
    }

    #[test]
    fn export_snapshot_parts() {
        crate::limits::write_binding("SNAPSHOT_PARTS.ts", &typescript());
    }

    #[test]
    fn hot_and_cold_cover_every_field_once() {
        let snapshot = super::super::tests::sample_snapshot();
        let json = serde_json::to_value(&snapshot).unwrap();
        let mut fields: Vec<&str> = json
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        let mut listed: Vec<&str> = HOT_KEYS.iter().chain(COLD_KEYS).copied().collect();
        fields.sort_unstable();
        listed.sort_unstable();
        assert_eq!(fields, listed);
    }

    #[test]
    fn a_hot_frame_leaves_the_cold_parts_out() {
        let snapshot = super::super::tests::sample_snapshot();
        let hot = serde_json::to_value(Frame {
            snapshot: &snapshot,
            full: false,
        })
        .unwrap();
        let full = serde_json::to_value(Frame {
            snapshot: &snapshot,
            full: true,
        })
        .unwrap();
        assert_eq!(hot["full"], false);
        assert_eq!(full["full"], true);
        for key in COLD_KEYS {
            assert!(hot.get(key).is_none(), "{key} in a hot frame");
            assert!(full.get(key).is_some(), "{key} missing from a full frame");
        }
        for key in HOT_KEYS {
            assert!(hot.get(key).is_some(), "{key} missing from a hot frame");
        }
        // A full frame is the snapshot itself, plus `full`.
        let mut plain = serde_json::to_value(&snapshot).unwrap();
        plain["full"] = true.into();
        assert_eq!(full, plain);
    }

    #[test]
    fn cold_parts_are_not_sent_again_while_unchanged() {
        let mut w = Windows::default();
        let windows = shown(&[("main", true), ("hud", true)]);
        let first = w.plan(
            &windows,
            Due {
                changed: true,
                ..Due::default()
            },
        );
        assert_eq!(
            sends(&first),
            [Delivery::Full, Delivery::Full],
            "new windows get everything"
        );
        let walking = w.plan(
            &windows,
            Due {
                changed: true,
                ..Due::default()
            },
        );
        assert_eq!(sends(&walking), [Delivery::Hot, Delivery::Hot]);
        let still = w.plan(&windows, Due::default());
        assert_eq!(sends(&still), [Delivery::Nothing, Delivery::Nothing]);
        let beat = w.plan(
            &windows,
            Due {
                heartbeat: true,
                ..Due::default()
            },
        );
        assert_eq!(sends(&beat), [Delivery::Hot, Delivery::Hot]);
        let saved = Due {
            changed: true,
            cold_changed: true,
            ..Due::default()
        };
        assert_eq!(
            sends(&w.plan(&windows, saved)),
            [Delivery::Full, Delivery::Full]
        );
        let refresh = w.plan(
            &windows,
            Due {
                refresh: true,
                ..Due::default()
            },
        );
        assert_eq!(sends(&refresh), [Delivery::Full, Delivery::Full]);
    }

    #[test]
    fn a_hidden_window_gets_nothing_then_everything_once_shown() {
        let mut w = Windows::default();
        let walk = Due {
            changed: true,
            ..Due::default()
        };
        w.plan(&shown(&[("main", true), ("hud", true)]), walk);
        let hidden = w.plan(&shown(&[("main", false), ("hud", true)]), walk);
        assert_eq!(sends(&hidden), [Delivery::Nothing, Delivery::Hot]);
        // Even a cold change waits until it shows.
        let cold = Due {
            changed: true,
            cold_changed: true,
            ..Due::default()
        };
        let still_hidden = w.plan(&shown(&[("main", false), ("hud", true)]), cold);
        assert_eq!(sends(&still_hidden), [Delivery::Nothing, Delivery::Full]);
        let back = w.plan(&shown(&[("main", true), ("hud", true)]), walk);
        assert_eq!(sends(&back), [Delivery::Full, Delivery::Hot]);
        // Shown with nothing changed: it still gets everything at once.
        w.plan(&shown(&[("main", false), ("hud", true)]), walk);
        let quiet = w.plan(&shown(&[("main", true), ("hud", true)]), Due::default());
        assert_eq!(sends(&quiet), [Delivery::Full, Delivery::Nothing]);
    }

    #[test]
    fn a_closed_window_or_a_failed_send_starts_over_with_everything() {
        let mut w = Windows::default();
        let walk = Due {
            changed: true,
            ..Due::default()
        };
        w.plan(&shown(&[("main", true)]), walk);
        w.plan(&shown(&[]), walk);
        assert_eq!(
            sends(&w.plan(&shown(&[("main", true)]), walk)),
            [Delivery::Full]
        );
        w.lost("main");
        assert_eq!(
            sends(&w.plan(&shown(&[("main", true)]), walk)),
            [Delivery::Full]
        );
        assert_eq!(
            sends(&w.plan(&shown(&[("main", true)]), walk)),
            [Delivery::Hot]
        );
    }
}
