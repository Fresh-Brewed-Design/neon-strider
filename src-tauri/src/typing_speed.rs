//! Labs, typing by speed (#250): how many keys the walker presses per minute
//! at each walking speed, over recent walks, so a working speed can be picked
//! from data.
//!
//! Off by default (`labs_typing_speed`). While it is on, key presses are
//! counted the same way Neural flow counts them (`hooks::take_key_count`, a
//! count only, never which keys, capped per second like Neural flow), and each
//! full minute of walking saves its count on the sample that closes it
//! (`samples.key_presses`, migration 13). Off, nothing is counted for it and
//! nothing is saved; counts saved before stay in the database.
//!
//! The table is pure (`MinuteCount`, `minutes_of`, `bands`): each saved
//! minute goes in the speed band of the belt's average speed over that minute,
//! and a band shows its minutes and the median count once it has enough of
//! them. It only reads; nothing here moves the belt.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use tauri::AppHandle;

/// The settings key of the Labs switch.
const KEY_ENABLED: &str = "labs_typing_speed";

/// One saved count covers this many seconds of walking. A minute matches the
/// "presses per minute" the table shows, so a saved count needs no scaling,
/// and it is coarse enough that the timing of single key presses is never
/// kept.
const MINUTE_S: u32 = 60;

/// Speed bands are this many tenths of a km/h wide (0.5 km/h). The deck steps
/// in 0.1 km/h, too fine to feel a difference in typing, while 0.5 km/h is a
/// change a walker notices. Desk walking speeds (about 1 to 6 km/h) then make
/// at most ten rows, which fit the Labs panel, and the minutes are not split
/// so thin that few bands reach `MIN_MINUTES`.
const BAND_TENTHS: u32 = 5;

/// A band shows once it has this many minutes with typing. The median of
/// fewer is moved a long way by one odd minute (a quick reply, a pasted
/// block), and five minutes is about one short stretch of work at one speed.
pub const MIN_MINUTES: u32 = 5;

/// The table reads the newest this many walks that have counts. Twenty is two
/// to four weeks of desk walking: recent enough to follow a changed desk,
/// shoes or keyboard, and enough minutes for several bands to show. Walks
/// with the switch off have no counts and are not among them.
pub const RECENT_WALKS: u32 = 20;

// Pure logic.

/// The running count of the minute being walked.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MinuteCount {
    seconds: u32,
    keys: u32,
}

impl MinuteCount {
    /// One second. `walking` is the belt Running with a session open and the
    /// switch on; anything else (a pause, a stop, the switch off) drops the
    /// part minute, since a part minute's count would read low against a
    /// whole one. Returns the count when this second completes a minute.
    pub fn tick(&mut self, walking: bool, keys: u32) -> Option<u16> {
        if !walking {
            *self = Self::default();
            return None;
        }
        self.seconds += 1;
        self.keys += keys.min(crate::neural::MAX_KEYS_PER_TICK);
        if self.seconds < MINUTE_S {
            return None;
        }
        let n = self.keys;
        *self = Self::default();
        Some(u16::try_from(n).unwrap_or(u16::MAX))
    }
}

/// One saved sample, as the table reads it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WalkSample {
    pub at_ms: i64,
    pub kmh: f32,
    pub key_presses: Option<u16>,
}

/// One row of the table.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct TypingBand {
    /// The band's lowest speed, km/h (included).
    pub from_kmh: f32,
    /// The band's top speed, km/h (the next band's lowest).
    pub to_kmh: f32,
    /// Minutes walked in the band with at least one key press.
    pub minutes: u32,
    /// The median key presses per minute over those minutes.
    pub median_kpm: u32,
}

/// Each counted minute of one walk (samples oldest first): the belt's average
/// speed over the minute the count closes, and the count. A minute with no
/// moving sample in it is left out.
pub fn minutes_of(walk: &[WalkSample]) -> Vec<(f32, u16)> {
    let window_ms = i64::from(MINUTE_S) * 1000;
    let mut out = Vec::new();
    let mut start = 0;
    for (i, s) in walk.iter().enumerate() {
        let Some(keys) = s.key_presses else {
            continue;
        };
        while start < i && walk[start].at_ms <= s.at_ms - window_ms {
            start += 1;
        }
        let moving: Vec<f32> = walk[start..=i]
            .iter()
            .map(|w| w.kmh)
            .filter(|k| k.is_finite() && *k > 0.0)
            .collect();
        if moving.is_empty() {
            continue;
        }
        out.push((moving.iter().sum::<f32>() / moving.len() as f32, keys));
    }
    out
}

/// The median, rounded to a whole count; 0 for none.
fn median(sorted: &[u16]) -> u32 {
    match sorted.len() {
        0 => 0,
        n if n % 2 == 1 => u32::from(sorted[n / 2]),
        n => (u32::from(sorted[n / 2 - 1]) + u32::from(sorted[n / 2])).div_ceil(2),
    }
}

/// What the table shows for these walks.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Table {
    pub bands: Vec<TypingBand>,
    /// Walks read (each has counts).
    pub walks: u32,
    /// Minutes with at least one key press, in any band.
    pub typing_minutes: u32,
}

/// The table: each minute with typing goes in its speed's band, and a band
/// with `MIN_MINUTES` of them shows, slowest first. A minute with no key
/// press is time away from the keyboard (a call, reading, a walk away from
/// the desk), which says nothing about typing at that speed, so it is left
/// out of both the minutes and the median.
pub fn table(walks: &[Vec<WalkSample>]) -> Table {
    let mut by_band: BTreeMap<u32, Vec<u16>> = BTreeMap::new();
    let mut typing_minutes = 0;
    for walk in walks {
        for (kmh, keys) in minutes_of(walk) {
            if keys == 0 {
                continue;
            }
            typing_minutes += 1;
            // To tenths first, so 3.5 from a float like 3.4999 lands in 3.5's band.
            let tenths = (kmh * 10.0).round().max(0.0) as u32;
            by_band.entry(tenths / BAND_TENTHS).or_default().push(keys);
        }
    }
    let bands = by_band
        .into_iter()
        .filter(|(_, counts)| counts.len() as u32 >= MIN_MINUTES)
        .map(|(band, mut counts)| {
            counts.sort_unstable();
            TypingBand {
                from_kmh: (band * BAND_TENTHS) as f32 / 10.0,
                to_kmh: ((band + 1) * BAND_TENTHS) as f32 / 10.0,
                minutes: counts.len() as u32,
                median_kpm: median(&counts),
            }
        })
        .collect();
    Table {
        bands,
        walks: walks.len() as u32,
        typing_minutes,
    }
}

// Snapshot.

/// Labs, typing by speed, as the windows see it.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct TypingSpeedSnapshot {
    /// The Labs switch (off by default).
    pub enabled: bool,
    /// Speed bands with enough typing minutes, slowest first.
    pub bands: Vec<TypingBand>,
    /// Walks with counts the table read (at most `recent_walks`).
    pub walks: u32,
    /// Minutes with at least one key press over those walks, in any band.
    pub typing_minutes: u32,
    /// A band shows once it has this many typing minutes.
    pub min_minutes: u32,
    /// The table reads at most this many recent walks with counts.
    pub recent_walks: u32,
}

// Runtime state.

static ENABLED: AtomicBool = AtomicBool::new(false);
static TABLE: Mutex<Option<Table>> = Mutex::new(None);

fn table_lock() -> std::sync::MutexGuard<'static, Option<Table>> {
    TABLE.lock().unwrap_or_else(|e| e.into_inner())
}

/// The Labs switch is on (read each second by the engine).
pub fn enabled() -> bool {
    ENABLED.load(Ordering::Relaxed)
}

/// Load the switch and, when on, the table. Blocking; call once after the DB
/// is initialised.
pub fn init(app: &AppHandle) {
    let on = crate::persistence::parse_setting(
        crate::persistence::read_setting(app, KEY_ENABLED).as_deref(),
    )
    .unwrap_or(false);
    ENABLED.store(on, Ordering::Relaxed);
    if on {
        refresh(app);
    }
}

/// The Labs switch, saved. On reads the table at once.
pub fn set_enabled(app: &AppHandle, on: bool) -> Result<(), String> {
    crate::persistence::write_setting(app, KEY_ENABLED, if on { "true" } else { "false" })?;
    ENABLED.store(on, Ordering::Relaxed);
    log::info!("Typing by speed {}", if on { "on" } else { "off" });
    if on {
        refresh(app);
    }
    Ok(())
}

/// Re-read the table from the database (a walk started or ended, one was
/// deleted, the switch came on). Blocking. Nothing while the switch is off.
pub fn refresh(app: &AppHandle) {
    if !enabled() {
        return;
    }
    match crate::persistence::typing_walks(app, RECENT_WALKS) {
        Ok(walks) => *table_lock() = Some(table(&walks)),
        Err(e) => log::warn!("typing by speed query failed: {}", e),
    }
}

pub fn snapshot() -> TypingSpeedSnapshot {
    let on = enabled();
    let t = if on {
        table_lock().clone().unwrap_or_default()
    } else {
        Table::default()
    };
    TypingSpeedSnapshot {
        enabled: on,
        bands: t.bands,
        walks: t.walks,
        typing_minutes: t.typing_minutes,
        min_minutes: MIN_MINUTES,
        recent_walks: RECENT_WALKS,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One sample a second at `kmh`, with a count every 60th second.
    fn walk(kmh: f32, minutes: &[u16]) -> Vec<WalkSample> {
        let mut out = Vec::new();
        for (m, &keys) in minutes.iter().enumerate() {
            for s in 1..=60 {
                let at_ms = (m as i64 * 60 + s) * 1000;
                out.push(WalkSample {
                    at_ms,
                    kmh,
                    key_presses: (s == 60).then_some(keys),
                });
            }
        }
        out
    }

    #[test]
    fn a_minute_saves_its_count_only_when_it_is_whole() {
        let mut m = MinuteCount::default();
        for _ in 0..59 {
            assert_eq!(m.tick(true, 2), None);
        }
        assert_eq!(m.tick(true, 2), Some(120));
        // A pause in the middle drops the part minute.
        for _ in 0..30 {
            m.tick(true, 3);
        }
        assert_eq!(m.tick(false, 3), None);
        for _ in 0..59 {
            assert_eq!(m.tick(true, 1), None);
        }
        assert_eq!(m.tick(true, 1), Some(60));
    }

    #[test]
    fn a_held_key_counts_no_more_than_neural_flow_does() {
        let mut m = MinuteCount::default();
        let mut out = None;
        for _ in 0..60 {
            out = m.tick(true, 30);
        }
        assert_eq!(out, Some((crate::neural::MAX_KEYS_PER_TICK * 60) as u16));
    }

    #[test]
    fn a_minute_takes_the_average_speed_it_was_walked_at() {
        let mut w = walk(3.0, &[40]);
        for s in w.iter_mut().skip(30) {
            s.kmh = 4.0;
        }
        let m = minutes_of(&w);
        assert_eq!(m.len(), 1);
        assert!((m[0].0 - 3.5).abs() < 1e-4);
        assert_eq!(m[0].1, 40);
    }

    #[test]
    fn bands_need_five_typing_minutes_and_show_the_median() {
        let walks = vec![
            walk(3.0, &[10, 20, 30, 40, 50, 0, 0]),
            walk(3.4, &[100, 200]),
            walk(4.0, &[5, 6, 7, 8]),
        ];
        let t = table(&walks);
        // 3.0 and 3.4 share the 3.0 to 3.5 band: seven typing minutes, and the
        // two minutes with no key press are left out. 4.0 has only four.
        assert_eq!(
            t.bands,
            vec![TypingBand {
                from_kmh: 3.0,
                to_kmh: 3.5,
                minutes: 7,
                median_kpm: 40,
            }]
        );
        assert_eq!(t.walks, 3);
        assert_eq!(t.typing_minutes, 11);
    }

    #[test]
    fn an_even_number_of_minutes_rounds_the_median_up() {
        assert_eq!(median(&[10, 20, 30, 41, 50, 60]), 36);
        assert_eq!(median(&[]), 0);
    }

    #[test]
    fn a_band_edge_speed_lands_in_the_band_it_starts() {
        let t = table(&[walk(3.4999, &[9; 5])]);
        assert_eq!(t.bands[0].from_kmh, 3.5);
    }

    #[test]
    fn no_walks_make_an_empty_table() {
        assert_eq!(table(&[]), Table::default());
    }
}
