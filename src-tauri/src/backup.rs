//! Rolling daily backups of the history database.
//!
//! The database runs in WAL mode, so a plain file copy can miss committed data
//! sitting in the -wal file. Backups use SQLite's online backup API instead,
//! on their own connection so writers are not blocked while it runs.

use chrono::NaiveDate;
use rusqlite::backup::Backup;
use rusqlite::Connection;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tauri::{AppHandle, Manager};

/// How many daily backups to keep (including today's).
const KEEP_BACKUPS: usize = 14;

const PREFIX: &str = "treadmill-";
const SUFFIX: &str = ".db";
/// How long the backup's own connection waits on a locked database.
const BUSY_TIMEOUT: Duration = Duration::from_millis(5000);
/// Pages copied per backup step, and the pause between steps that lets
/// writers in.
const PAGES_PER_STEP: std::os::raw::c_int = 128;
const STEP_PAUSE: Duration = Duration::from_millis(10);
/// How often the running app checks whether today's backup exists.
const CHECK_EVERY: Duration = Duration::from_secs(60 * 60);

/// `treadmill-YYYY-MM-DD.db`
fn backup_file_name(date: NaiveDate) -> String {
    format!("{}{}{}", PREFIX, date.format("%Y-%m-%d"), SUFFIX)
}

/// The date in a backup file name, or None for any other file.
fn parse_backup_date(name: &str) -> Option<NaiveDate> {
    let stem = name.strip_prefix(PREFIX)?.strip_suffix(SUFFIX)?;
    NaiveDate::parse_from_str(stem, "%Y-%m-%d").ok()
}

/// Which of `names` to delete so only the newest `keep` backups remain.
/// Files that are not backups (including in-progress `.tmp` files) are never
/// returned.
fn backups_to_delete(names: &[String], keep: usize) -> Vec<String> {
    let mut dated: Vec<(NaiveDate, &String)> = names
        .iter()
        .filter_map(|n| parse_backup_date(n).map(|d| (d, n)))
        .collect();
    dated.sort_by(|a, b| b.0.cmp(&a.0));
    dated
        .into_iter()
        .skip(keep)
        .map(|(_, n)| n.clone())
        .collect()
}

/// What a backup run did.
#[derive(Debug, PartialEq, Eq)]
pub enum BackupOutcome {
    /// Today's backup already existed.
    Skipped,
    Created {
        path: PathBuf,
        pruned: usize,
    },
}

/// Copy the database at `src_path` to `dest` with the online backup API,
/// through a temp file so a half-written backup never carries a real name.
pub fn backup_database_file(src_path: &Path, dest: &Path) -> Result<(), String> {
    let tmp = dest.with_extension("db.tmp");
    let _ = std::fs::remove_file(&tmp);

    let result = (|| -> Result<(), String> {
        let src = Connection::open(src_path).map_err(|e| format!("open source: {}", e))?;
        src.busy_timeout(BUSY_TIMEOUT).map_err(|e| e.to_string())?;
        let mut dst = Connection::open(&tmp).map_err(|e| format!("open destination: {}", e))?;
        {
            let backup = Backup::new(&src, &mut dst).map_err(|e| format!("start backup: {}", e))?;
            backup
                .run_to_completion(PAGES_PER_STEP, STEP_PAUSE, None)
                .map_err(|e| format!("copy: {}", e))?;
        }
        let check: String = dst
            .query_row("PRAGMA quick_check", [], |r| r.get(0))
            .map_err(|e| format!("verify: {}", e))?;
        if check != "ok" {
            return Err(format!("verify: {}", check));
        }
        Ok(())
    })();

    match result {
        Ok(()) => std::fs::rename(&tmp, dest).map_err(|e| format!("rename: {}", e)),
        Err(e) => {
            let _ = std::fs::remove_file(&tmp);
            Err(e)
        }
    }
}

fn backups_dir(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("backups"))
}

/// Today's backup if missing, then prune to `KEEP_BACKUPS`.
fn run_backup(app: &AppHandle) -> Result<BackupOutcome, String> {
    let dir = backups_dir(app)?;
    let today = chrono::Local::now().date_naive();
    let dest = dir.join(backup_file_name(today));
    if dest.exists() {
        return Ok(BackupOutcome::Skipped);
    }
    std::fs::create_dir_all(&dir).map_err(|e| format!("create backups folder: {}", e))?;
    backup_database_file(&crate::persistence::get_db_path(app)?, &dest)?;

    let names: Vec<String> = std::fs::read_dir(&dir)
        .map_err(|e| format!("list backups: {}", e))?
        .filter_map(|entry| entry.ok())
        .filter_map(|entry| entry.file_name().into_string().ok())
        .collect();
    let mut pruned = 0;
    for name in backups_to_delete(&names, KEEP_BACKUPS) {
        match std::fs::remove_file(dir.join(&name)) {
            Ok(()) => pruned += 1,
            Err(e) => log::warn!("could not delete old backup {}: {}", name, e),
        }
    }
    Ok(BackupOutcome::Created { path: dest, pruned })
}

/// `run_backup` that only logs: a failed backup must never disturb the app.
fn run_backup_logged(app: &AppHandle) {
    match run_backup(app) {
        Ok(BackupOutcome::Skipped) => log::debug!("today's database backup already exists"),
        Ok(BackupOutcome::Created { path, pruned }) => {
            log::info!(
                "database backed up to {:?} ({} old backups removed)",
                path,
                pruned
            )
        }
        Err(e) => log::error!("database backup failed: {}", e),
    }
}

/// Back up shortly after startup, then check hourly so a long-running app
/// still gets one backup per day.
pub fn spawn_daily(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        loop {
            let app_c = app.clone();
            // Panics inside the blocking task surface as a JoinError, not a crash.
            if let Err(e) =
                tauri::async_runtime::spawn_blocking(move || run_backup_logged(&app_c)).await
            {
                log::error!("database backup task failed: {}", e);
            }
            tokio::time::sleep(CHECK_EVERY).await;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    fn files(dates: &[NaiveDate]) -> Vec<String> {
        dates.iter().map(|d| backup_file_name(*d)).collect()
    }

    #[test]
    fn file_names_round_trip() {
        assert_eq!(backup_file_name(d(2026, 11, 3)), "treadmill-2026-11-03.db");
        assert_eq!(
            parse_backup_date("treadmill-2026-11-03.db"),
            Some(d(2026, 11, 3))
        );
        assert_eq!(parse_backup_date("treadmill.db"), None);
        assert_eq!(parse_backup_date("treadmill-2026-11-03.db.tmp"), None);
        assert_eq!(parse_backup_date("treadmill-2026-13-40.db"), None);
    }

    #[test]
    fn keeps_the_newest_and_deletes_the_rest() {
        let dates: Vec<NaiveDate> = (1..=20).map(|day| d(2026, 10, day)).collect();
        let mut names = files(&dates);
        names.reverse(); // directory order is arbitrary
        let mut doomed = backups_to_delete(&names, 14);
        doomed.sort();
        assert_eq!(doomed, files(&dates[..6]));
    }

    #[test]
    fn nothing_is_deleted_at_or_under_the_limit() {
        let names = files(&[d(2026, 1, 1), d(2026, 1, 2)]);
        assert!(backups_to_delete(&names, 14).is_empty());
        let fourteen: Vec<NaiveDate> = (1..=14).map(|day| d(2026, 2, day)).collect();
        assert!(backups_to_delete(&files(&fourteen), 14).is_empty());
    }

    #[test]
    fn sorts_by_date_not_by_name_across_months_and_years() {
        let names = files(&[d(2025, 12, 31), d(2026, 1, 1), d(2026, 1, 2)]);
        assert_eq!(backups_to_delete(&names, 2), files(&[d(2025, 12, 31)]));
    }

    #[test]
    fn other_files_are_never_deleted() {
        let mut names = files(&[d(2026, 1, 1), d(2026, 1, 2)]);
        names.push("notes.txt".into());
        names.push("treadmill-2026-01-03.db.tmp".into());
        names.push("treadmill.db".into());
        assert_eq!(backups_to_delete(&names, 1), files(&[d(2026, 1, 1)]));
    }

    #[test]
    fn online_backup_copies_committed_wal_data() {
        let dir =
            std::env::temp_dir().join(format!("neon-strider-backup-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let src_path = dir.join("treadmill.db");
        let dest = dir.join(backup_file_name(d(2026, 11, 13)));

        let src = Connection::open(&src_path).unwrap();
        let _mode: String = src
            .query_row("PRAGMA journal_mode = WAL", [], |r| r.get(0))
            .unwrap();
        src.execute_batch(
            "CREATE TABLE settings (key TEXT PRIMARY KEY, value TEXT NOT NULL);
             INSERT INTO settings VALUES ('a', '1');",
        )
        .unwrap();

        backup_database_file(&src_path, &dest).unwrap();
        assert!(dest.exists());
        assert!(!dest.with_extension("db.tmp").exists());

        let copy = Connection::open(&dest).unwrap();
        let v: String = copy
            .query_row("SELECT value FROM settings WHERE key = 'a'", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(v, "1");
        drop(copy);
        drop(src);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
