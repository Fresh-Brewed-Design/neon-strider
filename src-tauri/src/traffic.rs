//! Traffic recorder: a diagnostics capture of every packet written to the
//! treadmill and every notification it sends back, one JSON line each, into
//! `<log dir>/traffic/`. The format is in docs/protocol.md ("Traffic
//! recordings"); the simulator's timings come from these files.
//!
//! It only listens: nothing here writes to the treadmill or touches the engine.
//! Off at every start (not a setting). While off, the hooks in `bluetooth.rs`
//! cost one atomic load. While on, a line goes into a `BufWriter` behind a
//! mutex (memory; the file is written at most about once a second), so the
//! BLE tasks never wait on the disk.

use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, PoisonError};
use std::time::{Duration, Instant};

/// Version of the line format below.
pub const FORMAT_VERSION: u32 = 1;
/// The folder inside the app log dir.
pub const FOLDER: &str = "traffic";
/// A recording stops by itself after this long. The status poll runs every
/// 600 ms, so a walk makes about 2 sent and 2 received lines a second, about
/// 2 MB an hour: 4 hours is a long walk plus the deck going to sleep after it.
pub const MAX_DURATION: Duration = Duration::from_secs(4 * 3600);
/// ...or at this size, in case a deck sends far more than expected.
pub const MAX_BYTES: u64 = 20 * 1024 * 1024;
/// Buffered lines reach the file at least this often.
const FLUSH_EVERY: Duration = Duration::from_secs(1);

/// One line of a recording. `t_ms` counts from the start of the recording
/// (monotonic); `at_ms` is the wall clock (epoch ms).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TrafficLine {
    /// The first line.
    Header {
        format: u32,
        app_version: String,
        started_ms: i64,
        /// Local start time, RFC 3339.
        started_local: String,
    },
    /// A packet written to the treadmill (logged once the write succeeded).
    Tx {
        t_ms: u64,
        at_ms: i64,
        /// `PacketKind::label`, or `RAW` for the diagnostics command cycler.
        kind: String,
        hex: String,
    },
    /// A notification from the treadmill, whatever its characteristic.
    Rx {
        t_ms: u64,
        at_ms: i64,
        uuid: String,
        hex: String,
    },
    /// The last line, when the recording stopped (missing if the app quit).
    End {
        t_ms: u64,
        at_ms: i64,
        reason: StopReason,
        packets: u32,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StopReason {
    /// Switched off by the user.
    Stopped,
    DurationLimit,
    SizeLimit,
}

impl StopReason {
    /// Why it stopped, for the dashboard; `None` when the user stopped it.
    fn message(self) -> Option<String> {
        match self {
            StopReason::Stopped => None,
            StopReason::DurationLimit => Some(format!(
                "Stopped by itself at the {}-hour limit.",
                MAX_DURATION.as_secs() / 3600
            )),
            StopReason::SizeLimit => Some(format!(
                "Stopped by itself at the {} MB limit.",
                MAX_BYTES / (1024 * 1024)
            )),
        }
    }
}

/// Lowercase hex, no separators.
pub fn to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

/// The recorder as the windows see it.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct TrafficSnapshot {
    pub recording: bool,
    /// The current or last recording's file (None before the first one).
    pub file_path: Option<String>,
    pub file_name: Option<String>,
    /// Packets sent plus frames received.
    pub packets: u32,
    #[ts(type = "number")]
    pub bytes: u64,
    /// How long it has run (or ran).
    pub elapsed_s: u32,
    /// Why the last recording stopped by itself or failed; None when it is
    /// running or the user stopped it.
    pub stop_reason: Option<String>,
}

struct Active {
    writer: BufWriter<File>,
    path: PathBuf,
    started: Instant,
    last_flush: Instant,
    packets: u32,
    bytes: u64,
}

struct Finished {
    path: PathBuf,
    packets: u32,
    bytes: u64,
    elapsed: Duration,
    reason: Option<String>,
}

/// One recording at a time, plus what the last one left behind.
pub struct Recorder {
    active: Option<Active>,
    last: Option<Finished>,
    max_duration: Duration,
    max_bytes: u64,
}

impl Default for Recorder {
    fn default() -> Self {
        Self::with_limits(MAX_DURATION, MAX_BYTES)
    }
}

impl Recorder {
    pub fn with_limits(max_duration: Duration, max_bytes: u64) -> Self {
        Self {
            active: None,
            last: None,
            max_duration,
            max_bytes,
        }
    }

    pub fn is_recording(&self) -> bool {
        self.active.is_some()
    }

    /// Start a new file in `dir`, named after the local start time. A running
    /// recording is stopped first.
    pub fn start(
        &mut self,
        dir: &Path,
        app_version: &str,
        now: Instant,
        local: chrono::DateTime<chrono::Local>,
    ) -> Result<PathBuf, String> {
        self.stop(now, local.timestamp_millis(), StopReason::Stopped);
        std::fs::create_dir_all(dir)
            .map_err(|e| format!("Could not create {}: {}", dir.display(), e))?;
        let stem = format!("traffic-{}", local.format("%Y-%m-%d_%H-%M-%S"));
        // Two recordings in the same second never overwrite each other.
        let path = (1..)
            .map(|n| match n {
                1 => dir.join(format!("{}.jsonl", stem)),
                n => dir.join(format!("{}-{}.jsonl", stem, n)),
            })
            .find(|p| !p.exists())
            .unwrap_or_else(|| dir.join(format!("{}.jsonl", stem)));
        let file = File::create(&path)
            .map_err(|e| format!("Could not create {}: {}", path.display(), e))?;
        self.active = Some(Active {
            writer: BufWriter::new(file),
            path: path.clone(),
            started: now,
            last_flush: now,
            packets: 0,
            bytes: 0,
        });
        self.last = None;
        let header = TrafficLine::Header {
            format: FORMAT_VERSION,
            app_version: app_version.to_string(),
            started_ms: local.timestamp_millis(),
            started_local: local.to_rfc3339(),
        };
        self.write(&header, now);
        match &self.active {
            Some(_) => Ok(path),
            None => Err(self
                .last
                .as_ref()
                .and_then(|f| f.reason.clone())
                .unwrap_or_else(|| "Could not write the recording".to_string())),
        }
    }

    fn t_ms(&self, now: Instant) -> u64 {
        self.active
            .as_ref()
            .map_or(0, |a| now.duration_since(a.started).as_millis() as u64)
    }

    /// A packet written to the treadmill.
    pub fn record_tx(&mut self, kind: &str, bytes: &[u8], now: Instant, at_ms: i64) {
        if self.active.is_none() {
            return;
        }
        let line = TrafficLine::Tx {
            t_ms: self.t_ms(now),
            at_ms,
            kind: kind.to_string(),
            hex: to_hex(bytes),
        };
        self.record(&line, now, at_ms);
    }

    /// A notification from the treadmill.
    pub fn record_rx(&mut self, uuid: &str, bytes: &[u8], now: Instant, at_ms: i64) {
        if self.active.is_none() {
            return;
        }
        let line = TrafficLine::Rx {
            t_ms: self.t_ms(now),
            at_ms,
            uuid: uuid.to_string(),
            hex: to_hex(bytes),
        };
        self.record(&line, now, at_ms);
    }

    /// Count and write one packet line, then check the limits.
    fn record(&mut self, line: &TrafficLine, now: Instant, at_ms: i64) {
        if let Some(a) = self.active.as_mut() {
            a.packets = a.packets.saturating_add(1);
        }
        self.write(line, now);
        self.check_limits(now, at_ms);
    }

    /// Stop at the duration limit even when nothing arrives (called each snapshot).
    pub fn check_limits(&mut self, now: Instant, at_ms: i64) {
        let Some(a) = &self.active else { return };
        if now.duration_since(a.started) >= self.max_duration {
            self.stop(now, at_ms, StopReason::DurationLimit);
        } else if a.bytes >= self.max_bytes {
            self.stop(now, at_ms, StopReason::SizeLimit);
        }
    }

    fn write(&mut self, line: &TrafficLine, now: Instant) {
        let Some(a) = self.active.as_mut() else {
            return;
        };
        let mut text = serde_json::to_string(line).unwrap_or_default();
        text.push('\n');
        let mut result = a.writer.write_all(text.as_bytes());
        if result.is_ok() && now.duration_since(a.last_flush) >= FLUSH_EVERY {
            a.last_flush = now;
            result = a.writer.flush();
        }
        match result {
            Ok(()) => a.bytes += text.len() as u64,
            Err(e) => {
                log::warn!("Traffic recording failed: {}", e);
                let reason = format!("Stopped: could not write the file ({}).", e);
                self.finish(now, Some(reason));
            }
        }
    }

    /// End the recording (if one runs) with an `end` line.
    pub fn stop(&mut self, now: Instant, at_ms: i64, reason: StopReason) {
        let Some(a) = &self.active else { return };
        let end = TrafficLine::End {
            t_ms: now.duration_since(a.started).as_millis() as u64,
            at_ms,
            reason,
            packets: a.packets,
        };
        self.write(&end, now);
        if self.active.is_some() {
            if reason != StopReason::Stopped {
                log::info!("Traffic recording: {:?}", reason);
            }
            self.finish(now, reason.message());
        }
    }

    fn finish(&mut self, now: Instant, reason: Option<String>) {
        let Some(mut a) = self.active.take() else {
            return;
        };
        let reason = match a.writer.flush() {
            Ok(()) => reason,
            Err(e) => reason.or(Some(format!("Stopped: could not write the file ({}).", e))),
        };
        self.last = Some(Finished {
            path: a.path,
            packets: a.packets,
            bytes: a.bytes,
            elapsed: now.duration_since(a.started),
            reason,
        });
    }

    pub fn snapshot(&self, now: Instant) -> TrafficSnapshot {
        let (path, packets, bytes, elapsed, reason) = match (&self.active, &self.last) {
            (Some(a), _) => (
                Some(&a.path),
                a.packets,
                a.bytes,
                now.duration_since(a.started),
                None,
            ),
            (None, Some(f)) => (
                Some(&f.path),
                f.packets,
                f.bytes,
                f.elapsed,
                f.reason.clone(),
            ),
            (None, None) => (None, 0, 0, Duration::ZERO, None),
        };
        TrafficSnapshot {
            recording: self.active.is_some(),
            file_path: path.map(|p| p.to_string_lossy().into_owned()),
            file_name: path
                .and_then(|p| p.file_name())
                .map(|n| n.to_string_lossy().into_owned()),
            packets,
            bytes,
            elapsed_s: elapsed.as_secs() as u32,
            stop_reason: reason,
        }
    }
}

// ---------------------------------------------------------------------------
// The app's one recorder
// ---------------------------------------------------------------------------

/// Mirrors `RECORDER.is_recording()`, so the BLE hooks skip the lock while off.
static ACTIVE: AtomicBool = AtomicBool::new(false);
static RECORDER: Mutex<Option<Recorder>> = Mutex::new(None);

fn with_recorder<T>(f: impl FnOnce(&mut Recorder) -> T) -> T {
    let mut guard = RECORDER.lock().unwrap_or_else(PoisonError::into_inner);
    let recorder = guard.get_or_insert_with(Recorder::default);
    let out = f(recorder);
    ACTIVE.store(recorder.is_recording(), Ordering::Relaxed);
    out
}

/// Hook: a packet was written to the treadmill.
pub fn record_tx(kind: &str, bytes: &[u8]) {
    if ACTIVE.load(Ordering::Relaxed) {
        with_recorder(|r| r.record_tx(kind, bytes, Instant::now(), crate::util::now_ms()));
    }
}

/// Hook: a notification arrived from the treadmill.
pub fn record_rx(uuid: &uuid::Uuid, bytes: &[u8]) {
    if ACTIVE.load(Ordering::Relaxed) {
        let uuid = uuid.to_string();
        with_recorder(|r| r.record_rx(&uuid, bytes, Instant::now(), crate::util::now_ms()));
    }
}

/// The traffic folder inside the app log dir.
pub fn folder(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    use tauri::Manager;
    app.path()
        .app_log_dir()
        .map(|d| d.join(FOLDER))
        .map_err(|e| format!("Could not find the log folder: {}", e))
}

/// Switch recording on (a new file) or off. Blocking (creates the file).
pub fn set_recording(app: &tauri::AppHandle, on: bool) -> Result<(), String> {
    let now = Instant::now();
    if !on {
        with_recorder(|r| r.stop(now, crate::util::now_ms(), StopReason::Stopped));
        return Ok(());
    }
    let dir = folder(app)?;
    let version = app.package_info().version.to_string();
    let path = with_recorder(|r| r.start(&dir, &version, now, chrono::Local::now()))?;
    log::info!("Traffic recording to {}", path.display());
    Ok(())
}

/// Show the traffic folder in the file manager.
pub fn open_folder(app: &tauri::AppHandle) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;
    let dir = folder(app)?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    app.opener()
        .open_path(dir.to_string_lossy().into_owned(), None::<&str>)
        .map_err(|e| format!("Could not open {}: {}", dir.display(), e))
}

/// For the snapshot; also stops a recording that reached its time limit.
pub fn snapshot() -> TrafficSnapshot {
    if !ACTIVE.load(Ordering::Relaxed) {
        let guard = RECORDER.lock().unwrap_or_else(PoisonError::into_inner);
        return guard
            .as_ref()
            .map(|r| r.snapshot(Instant::now()))
            .unwrap_or_default();
    }
    with_recorder(|r| {
        let now = Instant::now();
        r.check_limits(now, crate::util::now_ms());
        r.snapshot(now)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "neon-strider-traffic-test-{}-{}",
            tag,
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    fn lines(path: &Path) -> Vec<TrafficLine> {
        std::fs::read_to_string(path)
            .unwrap()
            .lines()
            .map(|l| serde_json::from_str(l).unwrap())
            .collect()
    }

    fn local() -> chrono::DateTime<chrono::Local> {
        chrono::Local::now()
    }

    #[test]
    fn lines_round_trip_and_hex_is_lowercase() {
        let line = TrafficLine::Tx {
            t_ms: 600,
            at_ms: 1_763_200_000_000,
            kind: "POLL_STATUS".into(),
            hex: to_hex(&[0x02, 0x51, 0x51, 0x03]),
        };
        let text = serde_json::to_string(&line).unwrap();
        assert_eq!(
            text,
            r#"{"type":"tx","t_ms":600,"at_ms":1763200000000,"kind":"POLL_STATUS","hex":"02515103"}"#
        );
        assert_eq!(serde_json::from_str::<TrafficLine>(&text).unwrap(), line);
        let end: TrafficLine = serde_json::from_str(
            r#"{"type":"end","t_ms":5,"at_ms":6,"reason":"size_limit","packets":2}"#,
        )
        .unwrap();
        assert!(matches!(
            end,
            TrafficLine::End {
                reason: StopReason::SizeLimit,
                ..
            }
        ));
    }

    #[test]
    fn records_sent_and_received_including_polls() {
        let dir = temp_dir("both");
        let mut r = Recorder::default();
        let t0 = Instant::now();
        let path = r.start(&dir, "9.9.9", t0, local()).unwrap();
        assert!(path.starts_with(&dir));
        let poll = crate::protocol::status_request();
        r.record_tx("POLL_STATUS", &poll, t0 + Duration::from_millis(600), 10);
        r.record_rx(
            "0000fff1",
            &[0x02, 0x51, 0x00, 0x03],
            t0 + Duration::from_millis(650),
            11,
        );
        r.record_tx(
            "CONTROL_SET",
            &[0x02, 0x53, 0x02],
            t0 + Duration::from_millis(700),
            12,
        );
        assert_eq!(r.snapshot(t0 + Duration::from_secs(2)).packets, 3);
        r.stop(t0 + Duration::from_secs(3), 13, StopReason::Stopped);

        let got = lines(&path);
        assert_eq!(got.len(), 5);
        assert!(
            matches!(&got[0], TrafficLine::Header { app_version, format: 1, .. } if app_version == "9.9.9")
        );
        assert_eq!(
            got[1],
            TrafficLine::Tx {
                t_ms: 600,
                at_ms: 10,
                kind: "POLL_STATUS".into(),
                hex: to_hex(&poll)
            }
        );
        assert_eq!(
            got[2],
            TrafficLine::Rx {
                t_ms: 650,
                at_ms: 11,
                uuid: "0000fff1".into(),
                hex: "02510003".into()
            }
        );
        assert!(matches!(&got[3], TrafficLine::Tx { kind, .. } if kind == "CONTROL_SET"));
        assert!(matches!(
            got[4],
            TrafficLine::End {
                reason: StopReason::Stopped,
                packets: 3,
                t_ms: 3000,
                ..
            }
        ));

        let snap = r.snapshot(t0 + Duration::from_secs(60));
        assert!(!snap.recording);
        assert_eq!(snap.elapsed_s, 3);
        assert_eq!(snap.stop_reason, None);
        assert_eq!(snap.file_path, Some(path.to_string_lossy().into_owned()));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn nothing_is_written_while_off() {
        let dir = temp_dir("off");
        let mut r = Recorder::default();
        let now = Instant::now();
        r.record_tx("POLL_STATUS", &[1, 2], now, 0);
        r.record_rx("0000fff1", &[3], now, 0);
        assert!(!dir.exists());
        assert_eq!(r.snapshot(now), TrafficSnapshot::default());

        // After a stop, packets go nowhere either.
        let path = r.start(&dir, "1", now, local()).unwrap();
        r.stop(now, 0, StopReason::Stopped);
        let before = std::fs::read_to_string(&path).unwrap();
        r.record_tx("POLL_STATUS", &[1, 2], now, 0);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), before);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_duration_limit_stops_it_even_with_no_traffic() {
        let dir = temp_dir("duration");
        let mut r = Recorder::with_limits(Duration::from_secs(10), MAX_BYTES);
        let t0 = Instant::now();
        let path = r.start(&dir, "1", t0, local()).unwrap();
        r.record_rx("0000fff1", &[1], t0 + Duration::from_secs(5), 0);
        r.check_limits(t0 + Duration::from_secs(9), 0);
        assert!(r.is_recording());
        r.check_limits(t0 + Duration::from_secs(10), 0);
        assert!(!r.is_recording());

        let snap = r.snapshot(t0 + Duration::from_secs(99));
        assert_eq!(snap.elapsed_s, 10);
        assert!(snap.stop_reason.unwrap().contains("limit"));
        let got = lines(&path);
        assert!(matches!(
            got.last(),
            Some(TrafficLine::End {
                reason: StopReason::DurationLimit,
                packets: 1,
                ..
            })
        ));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_size_limit_stops_it() {
        let dir = temp_dir("size");
        let mut r = Recorder::with_limits(MAX_DURATION, 500);
        let t0 = Instant::now();
        let path = r.start(&dir, "1", t0, local()).unwrap();
        let mut sent = 0;
        while r.is_recording() && sent < 100 {
            r.record_tx("POLL_STATUS", &[0x02, 0x51, 0x51, 0x03], t0, 0);
            sent += 1;
        }
        assert!(!r.is_recording());
        assert!(sent < 100);
        // The packet that crossed the limit is kept, then nothing more.
        r.record_tx("POLL_STATUS", &[0], t0, 0);
        let got = lines(&path);
        assert_eq!(got.len(), sent + 2);
        assert!(matches!(
            got.last(),
            Some(TrafficLine::End {
                reason: StopReason::SizeLimit,
                ..
            })
        ));
        assert!(r.snapshot(t0).stop_reason.unwrap().contains("MB"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
