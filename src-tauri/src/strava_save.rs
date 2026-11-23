//! "Save for Strava": the no-API fallback. Writes a walk's TCX file (the same
//! builder and title/description templates the upload uses) into
//! `Downloads\Neon Strider`, so the user can drop it on Strava's upload page.
//!
//! This module never touches the network: it builds names, picks the walk and
//! writes a file. Opening the browser and revealing the file are done by the
//! caller in `strava.rs` through the opener plugin.

use crate::persistence::Session;
use crate::strava_plan as plan;
use serde::Serialize;
use std::path::{Path, PathBuf};

/// Strava's manual upload page.
pub const UPLOAD_URL: &str = "https://www.strava.com/upload/select";
/// The folder inside Downloads.
pub const FOLDER_NAME: &str = "Neon Strider";

/// Well inside Windows' 255-character file name, leaving room for the folder path.
const MAX_STEM_CHARS: usize = 100;
const RESERVED: [&str; 22] = [
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
    "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

/// What the window shows after a save.
#[derive(Debug, Clone, PartialEq, Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct SavedFile {
    pub file_name: String,
    pub folder: String,
}

/// A name that is safe on Windows: no reserved characters, no control
/// characters, no reserved device names, no trailing dots or spaces, bounded
/// length. Never empty. Pure.
fn sanitize_filename(raw: &str) -> String {
    let mut out = String::new();
    for c in raw.chars() {
        if c.is_whitespace() {
            out.push(' ');
            continue;
        }
        let bad =
            c.is_control() || matches!(c, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*');
        out.push(if bad { '-' } else { c });
    }
    let collapsed = out.split_whitespace().collect::<Vec<_>>().join(" ");
    let cut: String = collapsed.chars().take(MAX_STEM_CHARS).collect();
    let trimmed = cut
        .trim_matches(|c: char| c == '.' || c == ' ' || c == '-')
        .to_string();
    if trimmed.is_empty() {
        return "walk".to_string();
    }
    let stem_upper = trimmed.split('.').next().unwrap_or("").to_uppercase();
    if RESERVED.contains(&stem_upper.as_str()) {
        return format!("_{}", trimmed);
    }
    trimmed
}

/// `<date> <title>.tcx`, date as `YYYY-MM-DD` in local time. Pure.
pub fn file_name(date: &str, title: &str) -> String {
    format!("{}.tcx", sanitize_filename(&format!("{} {}", date, title)))
}

/// The walks `sessions` form once merged, the newest one that has no live
/// session in it and that is not on Strava yet. Ids oldest first. Pure.
pub fn latest_unsent(
    sessions: &[Session],
    active: Option<i64>,
    merge_minutes: u32,
) -> Option<Vec<i64>> {
    let open: Vec<Session> = sessions
        .iter()
        .filter(|s| (active.is_some() && s.id == active) || !plan::is_uploaded(&s.strava_id))
        .cloned()
        .collect();
    plan::merge_sessions(&open, merge_minutes)
        .into_iter()
        .rev()
        .find(|g| !g.ids.iter().any(|id| Some(*id) == active) && g.distance_meters > 0.0)
        .map(|g| g.ids)
}

/// Write `contents` as `name` inside `dir` (created if needed). An existing
/// file is kept: the new one gets " (2)", " (3)" and so on. Returns the path.
pub fn write_tcx(dir: &Path, name: &str, contents: &str) -> Result<PathBuf, String> {
    std::fs::create_dir_all(dir)
        .map_err(|e| format!("Could not create {}: {}", dir.display(), e))?;
    let (stem, ext) = match name.rsplit_once('.') {
        Some((s, e)) => (s.to_string(), format!(".{}", e)),
        None => (name.to_string(), String::new()),
    };
    let mut path = dir.join(name);
    let mut n = 2;
    while path.exists() {
        path = dir.join(format!("{} ({}){}", stem, n, ext));
        n += 1;
        // 999 copies of one walk's file is a loop, not a user.
        if n > 999 {
            return Err("Too many files with that name in the folder".to_string());
        }
    }
    std::fs::write(&path, contents)
        .map_err(|e| format!("Could not write {}: {}", path.display(), e))?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session(id: i64, start: i64, end: i64, km: f64, strava: Option<&str>) -> Session {
        Session {
            id: Some(id),
            start_time: start,
            end_time: end,
            duration_seconds: (end - start) / 1000,
            distance_meters: km * 1000.0,
            raw_distance_meters: 0.0,
            calories: 0.0,
            steps: 0.0,
            raw_steps: 0.0,
            avg_speed: 0.0,
            raw_avg_speed: 0.0,
            avg_incline: 0.0,
            strava_id: strava.map(str::to_string),
            neural_heat: 0.0,
            neural_ticks: 0,
            last_hardware_time: 0,
            hardware_time_offset: 0,
            avg_heart_rate: None,
            max_heart_rate: None,
            program_name: None,
        }
    }

    #[test]
    fn sanitizes_reserved_characters() {
        assert_eq!(
            sanitize_filename("a/b\\c:d*e?f\"g<h>i|j"),
            "a-b-c-d-e-f-g-h-i-j"
        );
        assert_eq!(sanitize_filename("tab\there\nnewline"), "tab here newline");
    }

    #[test]
    fn sanitizes_edges_and_reserved_names() {
        assert_eq!(sanitize_filename("  walk.  "), "walk");
        assert_eq!(sanitize_filename("..."), "walk");
        assert_eq!(sanitize_filename(""), "walk");
        assert_eq!(sanitize_filename("CON"), "_CON");
        assert_eq!(sanitize_filename("nul.txt"), "_nul.txt");
        assert_eq!(sanitize_filename("console"), "console");
    }

    #[test]
    fn sanitizing_bounds_length_on_characters() {
        let long = "\u{1f6b6}".repeat(300);
        assert_eq!(sanitize_filename(&long).chars().count(), MAX_STEM_CHARS);
    }

    #[test]
    fn file_name_is_date_title_tcx() {
        assert_eq!(
            file_name("2026-11-13", "Treadmill walk: 2.4 km / easy"),
            "2026-11-13 Treadmill walk- 2.4 km - easy.tcx"
        );
    }

    #[test]
    fn picks_the_newest_walk_not_on_strava() {
        let min = 60_000;
        let s = [
            session(1, 0, 10 * min, 1.0, Some("123")),
            session(2, 60 * min, 90 * min, 2.0, None),
            session(3, 200 * min, 230 * min, 3.0, None),
            session(4, 300 * min, 310 * min, 1.0, Some("456")),
        ];
        assert_eq!(latest_unsent(&s, None, 20), Some(vec![3]));
        // The live session is never offered, and it stops its group.
        assert_eq!(latest_unsent(&s, Some(3), 20), Some(vec![2]));
        assert_eq!(latest_unsent(&[], None, 20), None);
        assert_eq!(
            latest_unsent(&[session(1, 0, min, 1.0, Some("1"))], None, 20),
            None
        );
    }

    #[test]
    fn writes_into_a_new_folder_and_never_overwrites() {
        let dir =
            std::env::temp_dir().join(format!("neon-strider-save-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let nested = dir.join("Neon Strider");
        let a = write_tcx(&nested, "2026-11-13 Walk.tcx", "one").unwrap();
        let b = write_tcx(&nested, "2026-11-13 Walk.tcx", "two").unwrap();
        assert_eq!(a.file_name().unwrap(), "2026-11-13 Walk.tcx");
        assert_eq!(b.file_name().unwrap(), "2026-11-13 Walk (2).tcx");
        assert_eq!(std::fs::read_to_string(&a).unwrap(), "one");
        assert_eq!(std::fs::read_to_string(&b).unwrap(), "two");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The fallback must work with no account and no connection, so this module
    /// has no way to make a request: it does not use the HTTP client.
    #[test]
    fn this_module_never_touches_the_network() {
        let source = include_str!("strava_save.rs");
        let code = source.split("#[cfg(test)]").next().unwrap();
        for forbidden in ["reqwest", "TcpStream", "tiny_http", "hyper", "UdpSocket"] {
            assert!(
                !code.contains(forbidden),
                "{forbidden} found in strava_save.rs"
            );
        }
    }
}
