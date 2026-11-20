//! SQLite persistence: the one shared connection, the versioned schema, the
//! sessions and samples, and the key/value `settings` and `automation_state`
//! tables (including the calibration and speed limits in `AppConfig`).
//!
//! Every function here takes the connection lock for the length of one call.
//! Errors leave this module as `String`, which is what the IPC commands return.

use crate::neural::{self, NeuralSensitivity};
use crate::protocol::CalibrationPoint;
use rusqlite::{params, Connection, Transaction};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;
use tauri::{AppHandle, Manager};

/// The one connection every persistence function uses. Opened (and migrated)
/// by `init_db`, or lazily by the first caller if something runs before it.
static DB: Mutex<Option<Connection>> = Mutex::new(None);

/// How long SQLite waits on a locked database before giving up.
const BUSY_TIMEOUT: Duration = Duration::from_millis(5000);

/// Explicit column list, so row mapping never depends on physical column order.
const SESSION_COLS: &str = "id, start_time, end_time, duration_seconds, distance_meters, \
     raw_distance_meters, calories, steps, raw_steps, avg_speed, raw_avg_speed, avg_incline, \
     strava_id, neural_heat, neural_ticks, last_hardware_time, hardware_time_offset";

/// Settings key: JSON array of the widgets pinned to the HUD.
pub const KEY_PINNED_ELEMENTS: &str = "pinned_elements";
/// Settings key: the dashboard stays open when it loses focus.
pub const KEY_IS_PINNED: &str = "is_pinned";

/// Per-connection settings. `journal_mode=WAL` is stored in the file; the rest
/// apply to this connection only.
fn configure(conn: &Connection) -> rusqlite::Result<()> {
    conn.busy_timeout(BUSY_TIMEOUT)?;
    // journal_mode answers with a row, so it has to be read.
    let _mode: String = conn.query_row("PRAGMA journal_mode = WAL", [], |r| r.get(0))?;
    conn.execute_batch("PRAGMA synchronous = NORMAL; PRAGMA foreign_keys = ON;")?;
    Ok(())
}

fn has_column(tx: &Transaction, table: &str, column: &str) -> rusqlite::Result<bool> {
    let mut stmt = tx.prepare(&format!("PRAGMA table_info({})", table))?;
    let names = stmt.query_map([], |row| row.get::<_, String>(1))?;
    for name in names {
        if name? == column {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Migration 1: the schema as the app has always created it. Safe on a fresh
/// file and on databases from any earlier build (which may lack the columns
/// added over time, or `automation_state`).
fn migration_1_baseline(tx: &Transaction) -> rusqlite::Result<()> {
    tx.execute_batch(
        "CREATE TABLE IF NOT EXISTS settings (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS automation_state (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS sessions (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            start_time INTEGER NOT NULL,
            end_time INTEGER NOT NULL,
            duration_seconds INTEGER DEFAULT 0,
            distance_meters INTEGER DEFAULT 0,
            raw_distance_meters INTEGER DEFAULT 0,
            calories REAL DEFAULT 0,
            steps INTEGER DEFAULT 0,
            raw_steps INTEGER DEFAULT 0,
            avg_speed REAL DEFAULT 0,
            raw_avg_speed REAL DEFAULT 0,
            avg_incline REAL DEFAULT 0,
            strava_id TEXT DEFAULT NULL,
            neural_heat REAL DEFAULT 0,
            neural_ticks INTEGER DEFAULT 0,
            last_hardware_time INTEGER DEFAULT 0,
            hardware_time_offset INTEGER DEFAULT 0
        );
        CREATE TABLE IF NOT EXISTS samples (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            session_id INTEGER NOT NULL,
            timestamp INTEGER NOT NULL,
            speed REAL,
            incline REAL,
            distance REAL DEFAULT 0,
            raw_distance REAL DEFAULT 0,
            steps REAL DEFAULT 0,
            raw_steps REAL DEFAULT 0,
            raw_speed REAL DEFAULT 0,
            FOREIGN KEY(session_id) REFERENCES sessions(id)
        );",
    )?;
    // Columns older builds added with ALTER TABLE, in the order they appeared.
    for (column, decl) in [
        ("neural_heat", "REAL DEFAULT 0"),
        ("neural_ticks", "INTEGER DEFAULT 0"),
        ("last_hardware_time", "INTEGER DEFAULT 0"),
        ("hardware_time_offset", "INTEGER DEFAULT 0"),
    ] {
        if !has_column(tx, "sessions", column)? {
            tx.execute_batch(&format!(
                "ALTER TABLE sessions ADD COLUMN {} {}",
                column, decl
            ))?;
        }
    }
    Ok(())
}

/// Migration 2: indexes for the lookups the app does (a session's samples, the
/// session list and the since-midnight sums).
fn migration_2_indexes(tx: &Transaction) -> rusqlite::Result<()> {
    tx.execute_batch(
        "CREATE INDEX IF NOT EXISTS idx_samples_session ON samples(session_id);
         CREATE INDEX IF NOT EXISTS idx_sessions_start_time ON sessions(start_time);",
    )
}

/// Migration 3: the research-based defaults (docs/research.md, roadmap #26).
///
/// - The sit-too-long nudge moves from 60 to 30 minutes if it is still at 60
///   (the old default; the settings page saves it with every change, so a
///   stored 60 is treated as never chosen). Any other value is kept.
/// - Seeded programs still exactly as shipped get the new values and their
///   one-sentence note; one the user edited is left alone.
/// - Micro-break and Brisk are added after the user's programs, unless the
///   list is empty (they deleted every program) or one has that name already.
///
/// Frozen: the values are written out here, not read from `seed_programs`,
/// so later seed changes don't change what this migration did.
fn migration_3_research_programs(tx: &Transaction) -> rusqlite::Result<()> {
    tx.execute(
        "UPDATE settings SET value = '30'
         WHERE key = 'seated_minutes' AND trim(value, ' \"') = '60'",
        [],
    )?;
    let Some(raw) = get_setting_str(tx, "programs") else {
        return Ok(()); // Never seeded: the first start seeds the new values.
    };
    let next_id = get_setting_str(tx, "program_next_id");
    if let Some((programs, next_id)) = research_programs_v3(&raw, next_id.as_deref()) {
        for (key, value) in [
            ("programs", programs),
            ("program_next_id", next_id.to_string()),
        ] {
            tx.execute(
                "INSERT OR REPLACE INTO settings (key, value) VALUES (?1, ?2)",
                params![key, value],
            )?;
        }
    }
    Ok(())
}

/// Migration 5: desk days (roadmap #28). One row per local calendar day the PC
/// had keyboard or mouse input while the app ran, for DESK_SYNC ("walked on 9
/// of your last 10 desk days"). `day` is the local date as `YYYY-MM-DD`.
fn migration_5_desk_days(tx: &Transaction) -> rusqlite::Result<()> {
    tx.execute_batch(
        "CREATE TABLE IF NOT EXISTS desk_days (
            day TEXT PRIMARY KEY NOT NULL,
            first_input_ms INTEGER NOT NULL
        );",
    )
}

/// Migration 3's change to the stored programs JSON. `None` when there is
/// nothing to change or the value can't be read (it is then left as is).
fn research_programs_v3(raw: &str, next_id: Option<&str>) -> Option<(String, u32)> {
    use serde_json::{json, Value};
    let mut list: Vec<Value> = serde_json::from_str(raw).ok()?;
    let num = |v: &Value, x: f64| v.as_f64().is_some_and(|f| (f - x).abs() < 1e-3);
    let opt = |v: &Value, x: Option<f64>| match x {
        None => v.is_null(),
        Some(x) => num(v, x),
    };
    // (name, old step, old repeat) -> (new step, note)
    type Step = (Option<f64>, f64, f64);
    let shipped: [(&str, Step, Option<f64>, Step, &str); 3] = [
        (
            "Post-meal",
            (Some(10.0), 4.0, 10.0),
            None,
            (Some(10.0), 4.0, 5.0),
            "A walk soon after eating blunts the rise in blood sugar.",
        ),
        (
            "Workday",
            (Some(5.0), 3.0, 5.0),
            Some(30.0),
            (Some(5.0), 3.0, 5.0),
            "Five minutes every half hour offsets long sitting.",
        ),
        (
            "Focus",
            (None, 2.5, 5.0),
            None,
            (None, 2.0, 0.0),
            "Slow and flat, so typing and thinking stay steady.",
        ),
    ];
    let mut changed = false;
    for p in list.iter_mut() {
        for (name, (old_min, old_kmh, old_pct), old_repeat, (min, kmh, pct), note) in shipped {
            let steps = p["steps"].as_array();
            let unedited = p["name"] == name
                && p.get("note").is_none_or(Value::is_null)
                && opt(&p["repeat_every_minutes"], old_repeat)
                && steps.is_some_and(|s| {
                    s.len() == 1
                        && opt(&s[0]["duration_minutes"], old_min)
                        && num(&s[0]["speed_kmh"], old_kmh)
                        && num(&s[0]["incline_pct"], old_pct)
                });
            if unedited {
                p["steps"] =
                    json!([{ "duration_minutes": min, "speed_kmh": kmh, "incline_pct": pct }]);
                p["note"] = json!(note);
                changed = true;
            }
        }
    }
    let max_id = list
        .iter()
        .filter_map(|p| p["id"].as_u64())
        .max()
        .unwrap_or(0) as u32;
    let mut next = next_id
        .and_then(|s| s.trim().trim_matches('"').parse::<u32>().ok())
        .unwrap_or(0)
        .max(max_id + 1);
    if !list.is_empty() {
        let added = [
            json!({
                "name": "Micro-break",
                "steps": [{ "duration_minutes": 2.0, "speed_kmh": 3.2, "incline_pct": 0.0 }],
                "repeat_every_minutes": 20.0,
                "note": "Two easy minutes every 20 lower blood sugar after meals.",
            }),
            json!({
                "name": "Brisk",
                "steps": [{ "duration_minutes": 30.0, "speed_kmh": 5.0, "incline_pct": 0.0 }],
                "repeat_every_minutes": null,
                "note": "Moderate walking toward the weekly 150 minutes.",
            }),
        ];
        for mut p in added {
            if list.iter().any(|q| q["name"] == p["name"]) {
                continue;
            }
            p["id"] = json!(next);
            next += 1;
            list.push(p);
            changed = true;
        }
    }
    if !changed {
        return None;
    }
    Some((serde_json::to_string(&list).ok()?, next))
}

type Migration = fn(&Transaction) -> rusqlite::Result<()>;

/// Append new migrations at the end; never reorder or edit a shipped one.
/// `PRAGMA user_version` is the count of migrations already applied.
const MIGRATIONS: &[Migration] = &[
    migration_1_baseline,
    migration_2_indexes,
    migration_3_research_programs,
    migration_5_desk_days,
];

fn schema_version(conn: &Connection) -> rusqlite::Result<u32> {
    conn.query_row("PRAGMA user_version", [], |r| r.get(0))
}

/// Apply every migration the file has not seen, each in its own transaction
/// together with its version bump. Returns the resulting version. A file from
/// a newer build (higher version) is left alone.
fn migrate(conn: &mut Connection) -> rusqlite::Result<u32> {
    let current = schema_version(conn)?;
    if current as usize > MIGRATIONS.len() {
        log::warn!(
            "database schema version {} is newer than this build knows ({}); leaving it as is",
            current,
            MIGRATIONS.len()
        );
        return Ok(current);
    }
    for (index, migration) in MIGRATIONS.iter().enumerate().skip(current as usize) {
        let tx = conn.transaction()?;
        migration(&tx)?;
        tx.execute_batch(&format!("PRAGMA user_version = {}", index + 1))?;
        tx.commit()?;
        log::info!("database migrated to schema version {}", index + 1);
    }
    Ok(MIGRATIONS.len() as u32)
}

/// Open the file, apply the pragmas and bring the schema up to date.
fn open_database(path: &Path) -> Result<Connection, String> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("could not create {:?}: {}", dir, e))?;
    }
    let existed = path.exists();
    let mut conn = Connection::open(path).map_err(|e| format!("could not open database: {}", e))?;
    configure(&conn).map_err(|e| format!("could not configure database: {}", e))?;
    if existed {
        backup_before_migrating(&conn, path);
    }
    migrate(&mut conn).map_err(|e| format!("database migration failed: {}", e))?;
    Ok(conn)
}

/// Before an existing database is upgraded, keep a copy of it as it was:
/// `backups/treadmill-pre-v<N>-<timestamp>.db`. The daily-backup pruning only
/// matches dated names, so these are kept. A failed copy is logged and the
/// (transactional, additive) migration still runs.
fn backup_before_migrating(conn: &Connection, path: &Path) {
    let version = schema_version(conn).unwrap_or(0);
    if version as usize >= MIGRATIONS.len() {
        return;
    }
    let Some(dir) = path.parent().map(|d| d.join("backups")) else {
        return;
    };
    let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
    let dest = dir.join(format!("treadmill-pre-v{}-{}.db", version, stamp));
    let result = std::fs::create_dir_all(&dir)
        .map_err(|e| e.to_string())
        .and_then(|_| crate::backup::backup_database_file(path, &dest));
    match result {
        Ok(()) => log::info!("pre-migration backup written to {:?}", dest),
        Err(e) => log::error!("pre-migration backup failed (migrating anyway): {}", e),
    }
}

/// Run `f` on the shared connection, opening it first if needed. The lock is
/// held for the call, so never call back into persistence (or anything that
/// does) from inside `f`.
fn with_conn<T>(
    app: &AppHandle,
    f: impl FnOnce(&mut Connection) -> rusqlite::Result<T>,
) -> Result<T, String> {
    // A panic elsewhere while holding the lock must not take the database down.
    let mut guard = DB.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let conn = match guard.take() {
        Some(conn) => conn,
        None => open_database(&get_db_path(app)?)?,
    };
    f(guard.insert(conn)).map_err(|e| e.to_string())
}

/// `treadmill.db` in the app data folder.
pub fn get_db_path(app: &AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_data_dir()
        .map(|dir| dir.join("treadmill.db"))
        .map_err(|e| format!("could not find the app data folder: {}", e))
}

/// Open and migrate the database at startup. A failure is logged; later calls
/// retry the open and report their own errors.
pub fn init_db(app: &AppHandle) {
    let mut guard = DB.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let opened = get_db_path(app).and_then(|path| Ok((open_database(&path)?, path)));
    match opened {
        Ok((conn, path)) => {
            *guard = Some(conn);
            log::info!("database ready at {:?}", path);
        }
        Err(e) => log::error!("database initialisation failed: {}", e),
    }
}

fn get_setting_str(conn: &Connection, key: &str) -> Option<String> {
    conn.query_row(
        "SELECT value FROM settings WHERE key = ?1",
        params![key],
        |row| row.get::<_, String>(0),
    )
    .ok()
}

pub const DEFAULT_BODY_MASS_KG: f32 = 75.0;
pub const MIN_BODY_MASS_KG: f32 = 30.0;
pub const MAX_BODY_MASS_KG: f32 = 250.0;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AppConfig {
    pub last_device_id: Option<String>,
    pub calibration_points: Vec<CalibrationPoint>,
    pub belt_length: f32,
    pub min_speed: f32,
    pub max_speed: f32,
    pub max_incline: u8,
    pub tilt_multiplier: f32,
    pub incline_offset: f32,
    pub focus_stability_enabled: bool,
    /// Body weight for the calorie estimate; kept within MIN/MAX_BODY_MASS_KG.
    pub body_mass_kg: f32,
    /// Neural flow's "Slowest speed", % of the base speed (50 to 95).
    #[serde(default = "default_neural_floor_pct")]
    pub neural_floor_pct: f32,
    #[serde(default)]
    pub neural_sensitivity: NeuralSensitivity,
}

fn default_neural_floor_pct() -> f32 {
    neural::DEFAULT_FLOOR_PCT
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            last_device_id: None,
            calibration_points: Vec::new(),
            belt_length: 120.0,
            min_speed: 0.6,
            max_speed: 12.0,
            max_incline: 15,
            tilt_multiplier: 1.0,
            incline_offset: 0.0,
            focus_stability_enabled: false,
            body_mass_kg: DEFAULT_BODY_MASS_KG,
            neural_floor_pct: neural::DEFAULT_FLOOR_PCT,
            neural_sensitivity: NeuralSensitivity::Medium,
        }
    }
}

/// A plain value in the settings table. Stored text may be JSON-quoted (the
/// dashboard writes `JSON.stringify` output), so quotes are ignored on load.
trait ScalarSetting {
    /// Replace the value with the stored text. Text that does not parse keeps
    /// the current value (the default).
    fn load(&mut self, stored: &str);
    fn store(&self) -> String;
}

impl ScalarSetting for f32 {
    fn load(&mut self, stored: &str) {
        if let Ok(v) = stored.trim_matches('"').parse() {
            *self = v;
        }
    }
    fn store(&self) -> String {
        self.to_string()
    }
}

impl ScalarSetting for u8 {
    // Read as u32 and narrowed with `as`, which wraps above 255. That is how
    // the value has always been read; the dashboard never writes more than 255.
    fn load(&mut self, stored: &str) {
        if let Ok(v) = stored.trim_matches('"').parse::<u32>() {
            *self = v as u8;
        }
    }
    fn store(&self) -> String {
        self.to_string()
    }
}

impl ScalarSetting for bool {
    /// Anything other than `true` is false.
    fn load(&mut self, stored: &str) {
        *self = stored.trim_matches('"') == "true";
    }
    fn store(&self) -> String {
        self.to_string()
    }
}

type ScalarField = fn(&mut AppConfig) -> &mut dyn ScalarSetting;

/// The scalar `AppConfig` fields and their settings keys. `last_device_id`,
/// `calibration_points` and the Neural flow settings have their own formats
/// and are handled separately.
const SCALAR_FIELDS: [(&str, ScalarField); 8] = [
    ("beltLength", |c| &mut c.belt_length),
    ("maxSpeed", |c| &mut c.max_speed),
    ("minSpeed", |c| &mut c.min_speed),
    ("maxIncline", |c| &mut c.max_incline),
    ("tiltMultiplier", |c| &mut c.tilt_multiplier),
    ("inclineOffset", |c| &mut c.incline_offset),
    ("focusStabilityEnabled", |c| &mut c.focus_stability_enabled),
    ("bodyMassKg", |c| &mut c.body_mass_kg),
];

const KEY_LAST_DEVICE_ID: &str = "lastDeviceId";
const KEY_CALIBRATION_POINTS: &str = "calibrationPoints";

/// `AppConfig` from stored settings; `read` returns the raw value of a key.
fn config_from_settings(read: impl Fn(&str) -> Option<String>) -> AppConfig {
    let mut config = AppConfig {
        last_device_id: read(KEY_LAST_DEVICE_ID).map(|s| s.trim_matches('"').to_string()),
        ..AppConfig::default()
    };
    if let Some(raw) = read(KEY_CALIBRATION_POINTS) {
        match serde_json::from_str(&raw) {
            Ok(points) => config.calibration_points = points,
            Err(e) => log::warn!("could not parse saved calibrationPoints: {}", e),
        }
    }
    for (key, field) in SCALAR_FIELDS {
        if let Some(raw) = read(key) {
            field(&mut config).load(&raw);
        }
    }
    config.body_mass_kg = config
        .body_mass_kg
        .clamp(MIN_BODY_MASS_KG, MAX_BODY_MASS_KG);
    (config.neural_floor_pct, config.neural_sensitivity) = neural_from_settings(&read);
    config
}

/// Neural flow's floor and sensitivity. A database that lacks either key is
/// migrated from the legacy glide settings, which `save_config` then deletes.
fn neural_from_settings(read: &impl Fn(&str) -> Option<String>) -> (f32, NeuralSensitivity) {
    let parse_f32 = |key: &str| read(key).and_then(|s| s.trim_matches('"').parse::<f32>().ok());
    let floor = parse_f32(neural::KEY_FLOOR_PCT);
    let sensitivity = read(neural::KEY_SENSITIVITY).and_then(|s| NeuralSensitivity::parse(&s));
    let (legacy_floor, legacy_sensitivity) = if floor.is_none() || sensitivity.is_none() {
        neural::migrate_legacy(
            parse_f32("glideBiasPercentage"),
            parse_f32("glideThreshold"),
        )
    } else {
        (neural::DEFAULT_FLOOR_PCT, NeuralSensitivity::Medium)
    };
    (
        neural::clamp_floor_pct(floor.unwrap_or(legacy_floor)),
        sensitivity.unwrap_or(legacy_sensitivity),
    )
}

/// The `(key, value)` rows `save_config` writes for `config`.
fn config_to_settings(config: &AppConfig) -> Result<Vec<(&'static str, String)>, String> {
    let mut rows = Vec::with_capacity(SCALAR_FIELDS.len() + 4);
    if let Some(id) = &config.last_device_id {
        rows.push((KEY_LAST_DEVICE_ID, id.clone()));
    }
    let calibration =
        serde_json::to_string(&config.calibration_points).map_err(|e| e.to_string())?;
    rows.push((KEY_CALIBRATION_POINTS, calibration));
    // The accessors hand out `&mut`, so read the values from a copy.
    let mut copy = config.clone();
    for (key, field) in SCALAR_FIELDS {
        rows.push((key, field(&mut copy).store()));
    }
    rows.push((
        neural::KEY_FLOOR_PCT,
        neural::clamp_floor_pct(config.neural_floor_pct).to_string(),
    ));
    rows.push((
        neural::KEY_SENSITIVITY,
        config.neural_sensitivity.as_str().to_string(),
    ));
    Ok(rows)
}

/// The saved config, with defaults for anything missing or unreadable.
pub fn load_config(app: &AppHandle) -> AppConfig {
    let result = with_conn(app, |conn| {
        Ok(config_from_settings(|key| get_setting_str(conn, key)))
    });
    match result {
        Ok(config) => {
            // Called every 15 s by the auto-connect watchdog, so not info.
            log::debug!(
                "config loaded: device={:?}, max_speed={:.1}, calibration_points={}",
                config.last_device_id,
                config.max_speed,
                config.calibration_points.len()
            );
            config
        }
        Err(e) => {
            log::error!("could not load config, using defaults: {}", e);
            AppConfig::default()
        }
    }
}

/// Write the whole config in one transaction.
pub fn save_config(app: &AppHandle, config: &AppConfig) -> Result<(), String> {
    let rows = config_to_settings(config)?;
    with_conn(app, |conn| {
        let tx = conn.transaction()?;
        {
            let mut stmt =
                tx.prepare("INSERT OR REPLACE INTO settings (key, value) VALUES (?1, ?2)")?;
            for (key, value) in &rows {
                stmt.execute(params![key, value])?;
            }
        }
        // The Neural flow keys replace the legacy glide settings.
        for key in neural::LEGACY_KEYS {
            tx.execute("DELETE FROM settings WHERE key = ?1", params![key])?;
        }
        tx.commit()
    })
}

#[tauri::command]
pub async fn get_rust_config(app: tauri::AppHandle) -> Result<AppConfig, String> {
    Ok(load_config(&app))
}

/// One walk on the treadmill, as stored in `sessions`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub id: Option<i64>,
    pub start_time: i64,
    pub end_time: i64,
    pub duration_seconds: i64,
    pub distance_meters: f64,
    pub raw_distance_meters: f64,
    pub calories: f32,
    pub steps: f64,
    pub raw_steps: f64,
    pub avg_speed: f32,
    pub raw_avg_speed: f32,
    pub avg_incline: f32,
    pub strava_id: Option<String>,
    pub neural_heat: f32,
    pub neural_ticks: i64,
    pub last_hardware_time: i64,
    pub hardware_time_offset: i64,
}

/// One telemetry reading during a session, as stored in `samples`.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Sample {
    pub timestamp: Option<i64>,
    pub speed: f32,
    pub incline: f32,
    pub distance: f32,
    pub raw_distance: f32,
    pub steps: f32,
    pub raw_steps: f32,
    pub raw_speed: f32,
}

/// Insert an empty session starting now and return its id.
pub fn db_create_session(app: &tauri::AppHandle) -> Result<i64, String> {
    let now = crate::util::now_ms();
    with_conn(app, |conn| {
        conn.execute(
            "INSERT INTO sessions (start_time, end_time) VALUES (?1, ?1)",
            params![now],
        )?;
        Ok(conn.last_insert_rowid())
    })
}

/// Write a session's running totals; `end_time` becomes now.
pub fn db_update_session(app: &tauri::AppHandle, id: i64, data: Session) -> Result<(), String> {
    let now = crate::util::now_ms();
    with_conn(app, |conn| {
        conn.execute(
            "UPDATE sessions SET
                end_time = ?1,
                duration_seconds = ?2,
                distance_meters = ?3,
                raw_distance_meters = ?4,
                calories = ?5,
                steps = ?6,
                raw_steps = ?7,
                avg_speed = ?8,
                raw_avg_speed = ?9,
                avg_incline = ?10,
                neural_heat = ?11,
                neural_ticks = ?12,
                last_hardware_time = ?13,
                hardware_time_offset = ?14
             WHERE id = ?15",
            params![
                now,
                data.duration_seconds,
                data.distance_meters,
                data.raw_distance_meters,
                data.calories,
                data.steps,
                data.raw_steps,
                data.avg_speed,
                data.raw_avg_speed,
                data.avg_incline,
                data.neural_heat,
                data.neural_ticks,
                data.last_hardware_time,
                data.hardware_time_offset,
                id
            ],
        )
        .map(|_| ())
    })
}

/// Append a sample to a session, stamped now.
pub fn db_add_sample(
    app: &tauri::AppHandle,
    session_id: i64,
    sample: Sample,
) -> Result<(), String> {
    let now = crate::util::now_ms();
    with_conn(app, |conn| {
        conn.execute(
            "INSERT INTO samples (session_id, timestamp, speed, raw_speed, incline, distance, raw_distance, steps, raw_steps)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                session_id,
                now,
                sample.speed,
                sample.raw_speed,
                sample.incline,
                sample.distance,
                sample.raw_distance,
                sample.steps,
                sample.raw_steps
            ],
        )
        .map(|_| ())
    })
}

fn session_from_row(row: &rusqlite::Row) -> rusqlite::Result<Session> {
    Ok(Session {
        id: Some(row.get(0)?),
        start_time: row.get(1)?,
        end_time: row.get(2)?,
        duration_seconds: row.get(3)?,
        distance_meters: row.get(4)?,
        raw_distance_meters: row.get(5)?,
        calories: row.get(6)?,
        steps: row.get(7)?,
        raw_steps: row.get(8)?,
        avg_speed: row.get(9)?,
        raw_avg_speed: row.get(10)?,
        avg_incline: row.get(11)?,
        strava_id: row.get(12)?,
        neural_heat: row.get(13)?,
        neural_ticks: row.get(14)?,
        last_hardware_time: row.get::<_, Option<i64>>(15)?.unwrap_or(0),
        hardware_time_offset: row.get::<_, Option<i64>>(16)?.unwrap_or(0),
    })
}

/// Sessions matching `filter` (an SQL tail such as `WHERE ... ORDER BY ...`).
fn query_sessions(
    app: &AppHandle,
    filter: &str,
    params: impl rusqlite::Params,
) -> Result<Vec<Session>, String> {
    with_conn(app, |conn| {
        let mut stmt =
            conn.prepare(&format!("SELECT {} FROM sessions {}", SESSION_COLS, filter))?;
        let rows = stmt.query_map(params, session_from_row)?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
    })
}

/// The newest `limit` sessions, newest first (the History list).
#[tauri::command]
pub async fn get_sessions(app: tauri::AppHandle, limit: u32) -> Result<Vec<Session>, String> {
    query_sessions(&app, "ORDER BY start_time DESC LIMIT ?1", params![limit])
}

/// Every saved session, oldest first (for the CSV export).
pub fn db_all_sessions(app: &tauri::AppHandle) -> Result<Vec<Session>, String> {
    query_sessions(app, "ORDER BY start_time ASC, id ASC", [])
}

/// Sessions whose end_time is after `since_ms` (epoch ms), oldest first.
pub fn db_sessions_ended_after(
    app: &tauri::AppHandle,
    since_ms: i64,
) -> Result<Vec<Session>, String> {
    query_sessions(
        app,
        "WHERE end_time > ?1 ORDER BY start_time ASC",
        params![since_ms],
    )
}

/// Most recent session; the caller decides whether it is recent enough.
pub fn get_latest_session(app: &tauri::AppHandle) -> Option<Session> {
    query_sessions(app, "ORDER BY start_time DESC LIMIT 1", [])
        .ok()
        .and_then(|rows| rows.into_iter().next())
}

/// The sessions with these ids, oldest first. Unknown ids are simply absent.
pub fn db_sessions_by_ids(app: &tauri::AppHandle, ids: &[i64]) -> Result<Vec<Session>, String> {
    with_conn(app, |conn| {
        let mut stmt = conn.prepare(&format!(
            "SELECT {} FROM sessions WHERE id = ?1",
            SESSION_COLS
        ))?;
        let mut out = Vec::with_capacity(ids.len());
        for id in ids {
            if let Ok(s) = stmt.query_row(params![id], session_from_row) {
                out.push(s);
            }
        }
        out.sort_by_key(|s| s.start_time);
        Ok(out)
    })
}

#[tauri::command]
pub async fn get_samples(app: tauri::AppHandle, session_id: i64) -> Result<Vec<Sample>, String> {
    db_get_samples(&app, session_id)
}

/// A session's samples, oldest first.
pub fn db_get_samples(app: &tauri::AppHandle, session_id: i64) -> Result<Vec<Sample>, String> {
    with_conn(app, |conn| {
        let mut stmt = conn.prepare(
            "SELECT timestamp, speed, incline, distance, raw_distance, steps, raw_steps, raw_speed
             FROM samples WHERE session_id = ?1 ORDER BY timestamp ASC",
        )?;
        let rows = stmt.query_map(params![session_id], |row| {
            Ok(Sample {
                timestamp: Some(row.get(0)?),
                speed: row.get(1)?,
                incline: row.get(2)?,
                distance: row.get(3)?,
                raw_distance: row.get(4)?,
                steps: row.get(5)?,
                raw_steps: row.get(6)?,
                raw_speed: row.get(7)?,
            })
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
    })
}

#[tauri::command]
pub async fn delete_session(app: tauri::AppHandle, id: i64) -> Result<(), String> {
    with_conn(&app, |conn| {
        // One transaction: never leave samples without their session (or the reverse).
        let tx = conn.transaction()?;
        tx.execute("DELETE FROM samples WHERE session_id = ?1", params![id])?;
        tx.execute("DELETE FROM sessions WHERE id = ?1", params![id])?;
        tx.commit()
    })?;
    // The weekly totals and the odometer are cached; a deleted session changes them.
    crate::snapshot::weekly::refresh(&app, None);
    crate::snapshot::desk_sync::refresh(&app, None);
    crate::snapshot::odometer::refresh_current(&app);
    Ok(())
}

/// Record the Strava activity id on a session (the "uploaded" marker).
pub fn db_mark_uploaded(app: &tauri::AppHandle, id: i64, strava_id: &str) -> Result<(), String> {
    with_conn(app, |conn| {
        conn.execute(
            "UPDATE sessions SET strava_id = ?1 WHERE id = ?2",
            params![strava_id, id],
        )
        .map(|_| ())
    })
}

/// The key/value tables the generic settings commands may touch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum KvTable {
    Settings,
    AutomationState,
}

impl KvTable {
    fn parse(table: &str) -> Result<Self, String> {
        match table {
            "settings" => Ok(Self::Settings),
            "automation_state" => Ok(Self::AutomationState),
            _ => Err("Invalid table".to_string()),
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::Settings => "settings",
            Self::AutomationState => "automation_state",
        }
    }
}

/// The table a window asked for, refusing the secrets kept in `settings`
/// (they live in Credential Manager and never reach JS).
fn kv_table_for(table: &str, key: &str) -> Result<KvTable, String> {
    if table == KvTable::Settings.name() && crate::secrets::is_protected_key(key) {
        return Err("Protected setting".to_string());
    }
    KvTable::parse(table)
}

#[tauri::command]
pub async fn get_db_setting(
    app: tauri::AppHandle,
    table: String,
    key: String,
) -> Result<Option<String>, String> {
    let table = kv_table_for(&table, &key)?;
    let query = format!("SELECT value FROM {} WHERE key = ?1", table.name());
    with_conn(&app, |conn| {
        Ok(conn
            .query_row(&query, params![key], |row| row.get::<_, String>(0))
            .ok())
    })
}

#[tauri::command]
pub async fn set_db_setting(
    app: tauri::AppHandle,
    table: String,
    key: String,
    value: String,
) -> Result<(), String> {
    let table = kv_table_for(&table, &key)?;
    let query = format!(
        "INSERT OR REPLACE INTO {} (key, value) VALUES (?1, ?2)",
        table.name()
    );
    with_conn(&app, |conn| {
        conn.execute(&query, params![key, value]).map(|_| ())
    })?;
    // The connection lock is released: the hook below reads settings back.
    if table == KvTable::Settings {
        crate::snapshot::on_setting_written(&app, &key);
    }
    Ok(())
}

/// Some widget is pinned to the HUD.
pub fn has_pinned_elements(app: &tauri::AppHandle) -> bool {
    match read_setting(app, KEY_PINNED_ELEMENTS) {
        Some(val) => {
            log::debug!("pinned elements raw value: {}", val);
            // Any JSON array longer than `[]` has an entry.
            val.len() > 2 && val != "[]"
        }
        None => {
            log::debug!("no pinned elements entry");
            false
        }
    }
}

/// The dashboard is pinned open. The frontend stores the bare string `true`.
pub fn is_window_pinned(app: &tauri::AppHandle) -> bool {
    read_setting(app, KEY_IS_PINNED).is_some_and(|val| val == "true")
}

/// Read one raw value from the `settings` table. None when the DB or key is missing.
pub fn read_setting(app: &tauri::AppHandle, key: &str) -> Option<String> {
    with_conn(app, |conn| Ok(get_setting_str(conn, key)))
        .ok()
        .flatten()
}

/// Remove one key from the `settings` table.
pub fn delete_setting(app: &tauri::AppHandle, key: &str) -> Result<(), String> {
    with_conn(app, |conn| {
        conn.execute("DELETE FROM settings WHERE key = ?1", params![key])
            .map(|_| ())
    })
}

/// Upsert one raw value in the `settings` table (same shape set_db_setting writes).
pub fn write_setting(app: &tauri::AppHandle, key: &str, value: &str) -> Result<(), String> {
    with_conn(app, |conn| {
        conn.execute(
            "INSERT OR REPLACE INTO settings (key, value) VALUES (?1, ?2)",
            params![key, value],
        )
        .map(|_| ())
    })
}

/// A raw setting parsed as `T`, ignoring surrounding whitespace and JSON quotes.
pub fn parse_setting<T: std::str::FromStr>(raw: Option<&str>) -> Option<T> {
    raw?.trim().trim_matches('"').parse().ok()
}

/// A raw setting parsed as `T` if it lies in `range`, otherwise `default`.
pub fn setting_in<T: std::str::FromStr + PartialOrd>(
    raw: Option<&str>,
    range: std::ops::RangeInclusive<T>,
    default: T,
) -> T {
    parse_setting(raw)
        .filter(|v| range.contains(v))
        .unwrap_or(default)
}

/// Local midnight of the day containing `now`, as epoch MILLISECONDS
/// (sessions.start_time is written with timestamp_millis in db_create_session).
pub fn local_midnight_ms(now: chrono::DateTime<chrono::Local>) -> i64 {
    use chrono::TimeZone;
    now.date_naive()
        .and_hms_opt(0, 0, 0)
        .and_then(|naive| chrono::Local.from_local_datetime(&naive).earliest())
        .map(|dt| dt.timestamp_millis())
        // DST gap at midnight or an invalid date: fall back to a rolling 24h window.
        .unwrap_or_else(|| now.timestamp_millis() - 24 * 60 * 60 * 1000)
}

/// Sum of distance_meters for sessions started at or after `since` (local
/// midnight, from `local_midnight_ms`), optionally skipping one session (the
/// live one, whose distance the snapshot adds itself).
pub fn daily_distance_m(app: &tauri::AppHandle, since: i64, exclude_id: Option<i64>) -> f64 {
    let sum = with_conn(app, |conn| {
        conn.query_row(
            "SELECT COALESCE(SUM(distance_meters), 0.0) FROM sessions
             WHERE start_time >= ?1 AND (?2 IS NULL OR id != ?2)",
            params![since, exclude_id],
            |row| row.get::<_, f64>(0),
        )
    });
    sum.unwrap_or_else(|e| {
        log::warn!("daily distance query failed: {}", e);
        0.0
    })
}

/// (metres, seconds) summed over sessions started at or after `since_ms` (epoch
/// ms), optionally skipping one session (the live one). The seconds are the
/// deck's own timer, which runs while the belt does. For the maintenance reminder.
pub fn usage_since(app: &tauri::AppHandle, since_ms: i64, exclude_id: Option<i64>) -> (f64, i64) {
    with_conn(app, |conn| {
        conn.query_row(
            "SELECT COALESCE(SUM(distance_meters), 0.0),
                    CAST(COALESCE(SUM(duration_seconds), 0) AS INTEGER)
             FROM sessions
             WHERE start_time >= ?1 AND (?2 IS NULL OR id != ?2)",
            params![since_ms, exclude_id],
            |row| Ok((row.get::<_, f64>(0)?, row.get::<_, i64>(1)?)),
        )
    })
    .unwrap_or_else(|e| {
        log::warn!("usage-since query failed: {}", e);
        (0.0, 0)
    })
}

/// Everything ever saved: (metres, seconds, start of the earliest session in
/// epoch ms), optionally skipping one session (the live one). For the odometer.
pub fn lifetime_totals(
    app: &tauri::AppHandle,
    exclude_id: Option<i64>,
) -> Result<(f64, i64, Option<i64>), String> {
    with_conn(app, |conn| {
        conn.query_row(
            "SELECT COALESCE(SUM(distance_meters), 0.0),
                    CAST(COALESCE(SUM(duration_seconds), 0) AS INTEGER),
                    MIN(start_time)
             FROM sessions
             WHERE ?1 IS NULL OR id != ?1",
            params![exclude_id],
            |row| {
                Ok((
                    row.get::<_, f64>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, Option<i64>>(2)?,
                ))
            },
        )
    })
}

/// (start_time ms, distance_meters, duration_seconds, calories) of every session
/// started at or after `since_ms`, optionally skipping one (the live session,
/// whose row is still partial). For the weekly summary.
pub fn session_rows_since(
    app: &tauri::AppHandle,
    since_ms: i64,
    exclude_id: Option<i64>,
) -> Result<Vec<crate::snapshot::weekly::SessionRow>, String> {
    with_conn(app, |conn| {
        let mut stmt = conn.prepare(
            "SELECT start_time, COALESCE(distance_meters, 0), COALESCE(duration_seconds, 0),
                    COALESCE(calories, 0)
             FROM sessions
             WHERE start_time >= ?1 AND (?2 IS NULL OR id != ?2)",
        )?;
        let rows = stmt.query_map(params![since_ms, exclude_id], |row| {
            Ok(crate::snapshot::weekly::SessionRow {
                start_ms: row.get(0)?,
                distance_m: row.get(1)?,
                duration_s: row.get(2)?,
                calories: row.get(3)?,
            })
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
    })
}

/// Record `day` (local `YYYY-MM-DD`) as a desk day. A day already recorded
/// keeps its first input time.
pub fn record_desk_day(app: &tauri::AppHandle, day: &str, input_ms: i64) -> Result<(), String> {
    with_conn(app, |conn| {
        conn.execute(
            "INSERT OR IGNORE INTO desk_days (day, first_input_ms) VALUES (?1, ?2)",
            params![day, input_ms],
        )
        .map(|_| ())
    })
}

/// Desk days (local `YYYY-MM-DD`) on or after `since_day`.
pub fn desk_days_since(app: &tauri::AppHandle, since_day: &str) -> Result<Vec<String>, String> {
    with_conn(app, |conn| {
        let mut stmt = conn.prepare("SELECT day FROM desk_days WHERE day >= ?1 ORDER BY day")?;
        let rows = stmt.query_map(params![since_day], |row| row.get::<_, String>(0))?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
    })
}

/// `start_time` (epoch ms) of one session, if it exists.
pub fn session_start_ms(app: &tauri::AppHandle, id: i64) -> Option<i64> {
    with_conn(app, |conn| {
        conn.query_row(
            "SELECT start_time FROM sessions WHERE id = ?1",
            params![id],
            |row| row.get::<_, i64>(0),
        )
    })
    .ok()
}

/// Marks that the one-time averages repair below has run.
const AVERAGES_BACKFILL_KEY: &str = "session_averages_backfill_v1";

/// Samples recorded before this instant stored incline as a hardware level, not
/// a percent (the engine switched on 2026-11-13, around 16:05 Pacific).
const INCLINE_PCT_SINCE_MS: i64 = 1_794_614_700_000;

/// Averages for one stored session: (avg speed, raw avg speed, avg incline %).
/// Speeds are distance over session time (the engine's rule); incline is the
/// mean of the samples, converting pre-switch levels with the current
/// calibration. Without samples, a pre-switch stored incline is converted too.
fn backfill_averages(
    session: &Session,
    samples: &[Sample],
    tilt_multiplier: f32,
    incline_offset: f32,
) -> (f32, f32, f32) {
    let to_pct = |value: f32, ts: i64| {
        if ts < INCLINE_PCT_SINCE_MS {
            value * tilt_multiplier + incline_offset
        } else {
            value
        }
    };
    let secs = session.duration_seconds.max(0) as f64;
    let per_hour = |m: f64| {
        if secs > 0.0 {
            (m / secs * 3.6) as f32
        } else {
            0.0
        }
    };
    let incline = if samples.is_empty() {
        to_pct(session.avg_incline, session.start_time)
    } else {
        let sum: f32 = samples
            .iter()
            .map(|s| to_pct(s.incline, s.timestamp.unwrap_or(0)))
            .sum();
        sum / samples.len() as f32
    };
    (
        per_hour(session.distance_meters),
        per_hour(session.raw_distance_meters),
        incline,
    )
}

/// Rewrite avg_speed / raw_avg_speed / avg_incline for every saved session,
/// once: earlier builds saved 0 or a single instantaneous reading there.
pub fn backfill_session_averages(app: &AppHandle, tilt_multiplier: f32, incline_offset: f32) {
    if read_setting(app, AVERAGES_BACKFILL_KEY).is_some() {
        return;
    }
    let result = with_conn(app, |conn| {
        let tx = conn.transaction()?;
        let sessions: Vec<Session> = {
            let mut stmt = tx.prepare(&format!("SELECT {} FROM sessions", SESSION_COLS))?;
            let rows = stmt.query_map([], session_from_row)?;
            rows.collect::<rusqlite::Result<_>>()?
        };
        {
            let mut sample_stmt =
                tx.prepare("SELECT timestamp, incline FROM samples WHERE session_id = ?1")?;
            let mut update = tx.prepare(
                "UPDATE sessions SET avg_speed = ?1, raw_avg_speed = ?2, avg_incline = ?3 WHERE id = ?4",
            )?;
            for s in &sessions {
                let Some(id) = s.id else { continue };
                // Only the timestamp and incline matter to the averages.
                let samples: Vec<Sample> = sample_stmt
                    .query_map(params![id], |row| {
                        Ok(Sample {
                            timestamp: Some(row.get(0)?),
                            incline: row.get(1)?,
                            speed: 0.0,
                            distance: 0.0,
                            raw_distance: 0.0,
                            steps: 0.0,
                            raw_steps: 0.0,
                            raw_speed: 0.0,
                        })
                    })?
                    .collect::<rusqlite::Result<_>>()?;
                let (a, r, i) = backfill_averages(s, &samples, tilt_multiplier, incline_offset);
                update.execute(params![a, r, i, id])?;
            }
        }
        tx.commit()?;
        Ok(sessions.len())
    });
    match result {
        Ok(n) => {
            log::info!("repaired saved averages for {} sessions", n);
            if let Err(e) = write_setting(app, AVERAGES_BACKFILL_KEY, "done") {
                log::warn!("could not record averages repair: {}", e);
            }
        }
        // Left unmarked so the next start tries again.
        Err(e) => log::error!("averages repair failed: {}", e),
    }
}

#[cfg(test)]
mod db_tests {
    use super::*;

    /// The schema exactly as the previous `init_db` created it (no automation_state,
    /// no indexes), which is what a database from the last build looks like.
    const PREVIOUS_INIT_DB_SCHEMA: &str = "
        CREATE TABLE settings (key TEXT PRIMARY KEY, value TEXT NOT NULL);
        CREATE TABLE sessions (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            start_time INTEGER NOT NULL,
            end_time INTEGER NOT NULL,
            duration_seconds INTEGER DEFAULT 0,
            distance_meters INTEGER DEFAULT 0,
            raw_distance_meters INTEGER DEFAULT 0,
            calories REAL DEFAULT 0,
            steps INTEGER DEFAULT 0,
            raw_steps INTEGER DEFAULT 0,
            avg_speed REAL DEFAULT 0,
            raw_avg_speed REAL DEFAULT 0,
            avg_incline REAL DEFAULT 0,
            strava_id TEXT DEFAULT NULL,
            neural_heat REAL DEFAULT 0,
            neural_ticks INTEGER DEFAULT 0,
            last_hardware_time INTEGER DEFAULT 0,
            hardware_time_offset INTEGER DEFAULT 0
        );
        CREATE TABLE samples (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            session_id INTEGER NOT NULL,
            timestamp INTEGER NOT NULL,
            speed REAL,
            incline REAL,
            distance REAL DEFAULT 0,
            raw_distance REAL DEFAULT 0,
            steps REAL DEFAULT 0,
            raw_steps REAL DEFAULT 0,
            raw_speed REAL DEFAULT 0,
            FOREIGN KEY(session_id) REFERENCES sessions(id)
        );";

    /// A database as an installed older build left it: no hardware_time_offset
    /// column, but automation_state and an index on samples.
    const OLDER_BUILD_SCHEMA: &str = "
        CREATE TABLE settings (key TEXT PRIMARY KEY, value TEXT NOT NULL);
        CREATE TABLE automation_state (key TEXT PRIMARY KEY, value TEXT NOT NULL);
        CREATE TABLE sessions (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            start_time INTEGER NOT NULL,
            end_time INTEGER NOT NULL,
            duration_seconds INTEGER DEFAULT 0,
            distance_meters INTEGER DEFAULT 0,
            raw_distance_meters INTEGER DEFAULT 0,
            calories REAL DEFAULT 0,
            steps INTEGER DEFAULT 0,
            raw_steps INTEGER DEFAULT 0,
            avg_speed REAL DEFAULT 0,
            raw_avg_speed REAL DEFAULT 0,
            avg_incline REAL DEFAULT 0,
            strava_id TEXT DEFAULT NULL
        , neural_heat REAL DEFAULT 0, neural_ticks INTEGER DEFAULT 0, last_hardware_time INTEGER DEFAULT 0);
        CREATE TABLE samples (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            session_id INTEGER NOT NULL,
            timestamp INTEGER NOT NULL,
            speed REAL,
            incline REAL,
            distance REAL DEFAULT 0,
            raw_distance REAL DEFAULT 0,
            steps REAL DEFAULT 0,
            raw_steps REAL DEFAULT 0,
            raw_speed REAL DEFAULT 0,
            FOREIGN KEY(session_id) REFERENCES sessions(id)
        );
        CREATE INDEX idx_samples_session ON samples(session_id);";

    fn latest() -> u32 {
        MIGRATIONS.len() as u32
    }

    fn version(conn: &Connection) -> u32 {
        conn.query_row("PRAGMA user_version", [], |r| r.get(0))
            .unwrap()
    }

    fn names(conn: &Connection, kind: &str) -> Vec<String> {
        let mut stmt = conn
            .prepare("SELECT name FROM sqlite_master WHERE type = ?1 ORDER BY name")
            .unwrap();
        let rows = stmt.query_map([kind], |r| r.get::<_, String>(0)).unwrap();
        rows.map(|r| r.unwrap()).collect()
    }

    fn temp_db(tag: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("neon-strider-test-{}-{}", tag, std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir.join("treadmill.db")
    }

    #[test]
    fn existing_database_is_backed_up_before_migrating_and_not_after() {
        let path = temp_db("premigrate");
        {
            let conn = Connection::open(&path).unwrap();
            conn.execute_batch(
                "CREATE TABLE sessions (id INTEGER PRIMARY KEY, start_time INTEGER, end_time INTEGER,
                 duration_seconds INTEGER, distance_meters REAL, calories REAL, strava_id TEXT);
                 INSERT INTO sessions (start_time, end_time, duration_seconds, distance_meters, calories)
                 VALUES (1, 2, 3, 4.0, 5.0);",
            )
            .unwrap();
        }
        let backups = path.parent().unwrap().join("backups");
        let count = || {
            std::fs::read_dir(&backups)
                .map(|d| {
                    d.filter_map(|e| e.ok())
                        .filter(|e| {
                            e.file_name()
                                .to_string_lossy()
                                .starts_with("treadmill-pre-v0-")
                        })
                        .count()
                })
                .unwrap_or(0)
        };

        drop(open_database(&path).unwrap());
        assert_eq!(count(), 1, "old database copied before migrating");

        // Already migrated: opening again makes no further copy.
        drop(open_database(&path).unwrap());
        assert_eq!(count(), 1);
    }

    #[test]
    fn fresh_database_needs_no_pre_migration_backup() {
        let path = temp_db("fresh-nobackup");
        drop(open_database(&path).unwrap());
        assert!(!path.parent().unwrap().join("backups").exists());
    }

    fn seed_old_rows(conn: &Connection) {
        conn.execute(
            "INSERT INTO sessions (start_time, end_time, duration_seconds, distance_meters, calories, strava_id)
             VALUES (1000, 2000, 600, 1500, 80.5, '987')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO samples (session_id, timestamp, speed, incline) VALUES (1, 1500, 3.5, 2.0)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO settings (key, value) VALUES ('beltLength', '120')",
            [],
        )
        .unwrap();
    }

    fn assert_rows_intact(conn: &Connection) {
        let mut stmt = conn
            .prepare(&format!("SELECT {} FROM sessions", SESSION_COLS))
            .unwrap();
        let sessions: Vec<Session> = stmt
            .query_map([], session_from_row)
            .unwrap()
            .map(|s| s.unwrap())
            .collect();
        assert_eq!(sessions.len(), 1);
        let s = &sessions[0];
        assert_eq!(
            (s.start_time, s.end_time, s.duration_seconds),
            (1000, 2000, 600)
        );
        assert_eq!(s.distance_meters, 1500.0);
        assert!((s.calories - 80.5).abs() < 1e-4);
        assert_eq!(s.strava_id.as_deref(), Some("987"));
        assert_eq!(s.hardware_time_offset, 0);
        let n: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM samples WHERE session_id = 1",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(n, 1);
        assert_eq!(get_setting_str(conn, "beltLength").as_deref(), Some("120"));
    }

    fn assert_latest_schema(conn: &Connection) {
        assert_eq!(version(conn), latest());
        let tables = names(conn, "table");
        for t in [
            "settings",
            "automation_state",
            "sessions",
            "samples",
            "desk_days",
        ] {
            assert!(tables.iter().any(|n| n == t), "missing table {}", t);
        }
        let indexes = names(conn, "index");
        for i in ["idx_samples_session", "idx_sessions_start_time"] {
            assert!(indexes.iter().any(|n| n == i), "missing index {}", i);
        }
    }

    /// The programs JSON v1.6 seeded, as `persist` wrote it.
    const V1_SEEDS: &str = r#"[{"id":1,"name":"Post-meal","steps":[{"duration_minutes":10.0,"speed_kmh":4.0,"incline_pct":10.0}],"repeat_every_minutes":null},{"id":2,"name":"Workday","steps":[{"duration_minutes":5.0,"speed_kmh":3.0,"incline_pct":5.0}],"repeat_every_minutes":30.0},{"id":3,"name":"Focus","steps":[{"duration_minutes":null,"speed_kmh":2.5,"incline_pct":5.0}],"repeat_every_minutes":null}]"#;

    fn migrated_to_v2() -> Connection {
        let mut conn = Connection::open_in_memory().unwrap();
        for (i, m) in MIGRATIONS[..2].iter().enumerate() {
            let tx = conn.transaction().unwrap();
            m(&tx).unwrap();
            tx.execute_batch(&format!("PRAGMA user_version = {}", i + 1))
                .unwrap();
            tx.commit().unwrap();
        }
        conn
    }

    fn set(conn: &Connection, key: &str, value: &str) {
        conn.execute(
            "INSERT OR REPLACE INTO settings (key, value) VALUES (?1, ?2)",
            params![key, value],
        )
        .unwrap();
    }

    fn stored_programs(conn: &Connection) -> Vec<crate::programs::Program> {
        serde_json::from_str(&get_setting_str(conn, "programs").unwrap()).unwrap()
    }

    #[test]
    fn migration_3_on_a_fresh_database_writes_nothing() {
        let mut conn = Connection::open_in_memory().unwrap();
        migrate(&mut conn).unwrap();
        for key in ["programs", "program_next_id", "seated_minutes"] {
            assert_eq!(get_setting_str(&conn, key), None, "{}", key);
        }
    }

    #[test]
    fn migration_3_moves_unedited_seeds_and_the_default_to_the_research_values() {
        let mut conn = migrated_to_v2();
        set(&conn, "programs", V1_SEEDS);
        set(&conn, "program_next_id", "4");
        set(&conn, "seated_minutes", "60");
        migrate(&mut conn).unwrap();
        assert_eq!(version(&conn), latest());
        // Exactly what a new install now starts with.
        assert_eq!(stored_programs(&conn), crate::programs::seed_programs());
        assert_eq!(
            get_setting_str(&conn, "program_next_id").as_deref(),
            Some("6")
        );
        assert_eq!(
            get_setting_str(&conn, "seated_minutes").as_deref(),
            Some("30")
        );
    }

    #[test]
    fn migration_3_keeps_what_the_user_changed() {
        let mut conn = migrated_to_v2();
        // Post-meal edited to 15 min, Focus deleted, a program of their own
        // with a higher id, and a sitting time they picked.
        let edited = V1_SEEDS
            .replacen("\"duration_minutes\":10.0", "\"duration_minutes\":15.0", 1)
            .replace(
                r#",{"id":3,"name":"Focus","steps":[{"duration_minutes":null,"speed_kmh":2.5,"incline_pct":5.0}],"repeat_every_minutes":null}"#,
                r#",{"id":7,"name":"Brisk","steps":[{"duration_minutes":20.0,"speed_kmh":4.5,"incline_pct":0.0}],"repeat_every_minutes":null}"#,
            );
        set(&conn, "programs", &edited);
        set(&conn, "program_next_id", "9");
        set(&conn, "seated_minutes", "45");
        migrate(&mut conn).unwrap();
        let list = stored_programs(&conn);
        let names: Vec<&str> = list.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, ["Post-meal", "Workday", "Brisk", "Micro-break"]);
        // The edited Post-meal is untouched; the unedited Workday gains its note.
        assert_eq!(list[0].steps[0].duration_minutes, Some(15.0));
        assert_eq!(list[0].steps[0].incline_pct, 10.0);
        assert_eq!(list[0].note, None);
        assert!(list[1].note.is_some());
        // Their own "Brisk" is kept and not duplicated; Micro-break takes the next id.
        assert_eq!(list[2].id, 7);
        assert_eq!(list[2].steps[0].speed_kmh, 4.5);
        assert_eq!(list[3].id, 9);
        assert_eq!(
            get_setting_str(&conn, "program_next_id").as_deref(),
            Some("10")
        );
        assert_eq!(
            get_setting_str(&conn, "seated_minutes").as_deref(),
            Some("45")
        );
        // Forced to run again, nothing changes.
        let before = get_setting_str(&conn, "programs");
        conn.execute_batch("PRAGMA user_version = 2").unwrap();
        migrate(&mut conn).unwrap();
        assert_eq!(get_setting_str(&conn, "programs"), before);
    }

    #[test]
    fn migration_3_leaves_an_empty_or_unreadable_list_alone() {
        for raw in ["[]", "not json"] {
            let mut conn = migrated_to_v2();
            set(&conn, "programs", raw);
            migrate(&mut conn).unwrap();
            assert_eq!(get_setting_str(&conn, "programs").as_deref(), Some(raw));
            assert_eq!(get_setting_str(&conn, "program_next_id"), None);
        }
    }

    #[test]
    fn migration_3_moves_a_quoted_sixty_too() {
        let mut conn = migrated_to_v2();
        set(&conn, "seated_minutes", "\"60\"");
        migrate(&mut conn).unwrap();
        assert_eq!(
            get_setting_str(&conn, "seated_minutes").as_deref(),
            Some("30")
        );
    }

    #[test]
    fn fresh_database_migrates_to_latest() {
        let mut conn = Connection::open_in_memory().unwrap();
        assert_eq!(version(&conn), 0);
        assert_eq!(migrate(&mut conn).unwrap(), latest());
        assert_latest_schema(&conn);
        // The full column list reads back from a fresh table.
        conn.execute(
            "INSERT INTO sessions (start_time, end_time) VALUES (1, 1)",
            [],
        )
        .unwrap();
        let s = conn
            .query_row(
                &format!("SELECT {} FROM sessions", SESSION_COLS),
                [],
                session_from_row,
            )
            .unwrap();
        assert_eq!(s.id, Some(1));
    }

    #[test]
    fn database_from_previous_init_db_keeps_its_rows() {
        let mut conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(PREVIOUS_INIT_DB_SCHEMA).unwrap();
        seed_old_rows(&conn);
        assert_eq!(version(&conn), 0);
        migrate(&mut conn).unwrap();
        assert_latest_schema(&conn);
        assert_rows_intact(&conn);
    }

    #[test]
    fn database_from_an_older_build_gains_missing_columns_and_tables() {
        let mut conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(OLDER_BUILD_SCHEMA).unwrap();
        seed_old_rows(&conn);
        migrate(&mut conn).unwrap();
        assert_latest_schema(&conn);
        assert_rows_intact(&conn);
        // The pre-existing index was reused, not duplicated.
        let n = names(&conn, "index")
            .iter()
            .filter(|n| n.as_str() == "idx_samples_session")
            .count();
        assert_eq!(n, 1);
    }

    #[test]
    fn migrations_are_idempotent() {
        let mut conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(PREVIOUS_INIT_DB_SCHEMA).unwrap();
        seed_old_rows(&conn);
        migrate(&mut conn).unwrap();
        migrate(&mut conn).unwrap();
        assert_latest_schema(&conn);
        assert_rows_intact(&conn);
        // Even forced to run again from scratch, nothing is lost or duplicated.
        conn.execute_batch("PRAGMA user_version = 0").unwrap();
        migrate(&mut conn).unwrap();
        assert_latest_schema(&conn);
        assert_rows_intact(&conn);
    }

    #[test]
    fn a_newer_schema_version_is_left_alone() {
        let mut conn = Connection::open_in_memory().unwrap();
        migrate(&mut conn).unwrap();
        conn.execute_batch("PRAGMA user_version = 99").unwrap();
        assert_eq!(migrate(&mut conn).unwrap(), 99);
        assert_eq!(version(&conn), 99);
    }

    #[test]
    fn opening_a_file_applies_pragmas_and_migrations() {
        let path = temp_db("open");
        {
            let conn = open_database(&path).unwrap();
            let mode: String = conn
                .query_row("PRAGMA journal_mode", [], |r| r.get(0))
                .unwrap();
            assert_eq!(mode, "wal");
            let fk: i64 = conn
                .query_row("PRAGMA foreign_keys", [], |r| r.get(0))
                .unwrap();
            assert_eq!(fk, 1);
            let sync: i64 = conn
                .query_row("PRAGMA synchronous", [], |r| r.get(0))
                .unwrap();
            assert_eq!(sync, 1); // NORMAL
            let busy: i64 = conn
                .query_row("PRAGMA busy_timeout", [], |r| r.get(0))
                .unwrap();
            assert_eq!(busy, 5000);
            assert_eq!(version(&conn), latest());
            conn.execute(
                "INSERT INTO sessions (start_time, end_time) VALUES (5, 5)",
                [],
            )
            .unwrap();
        }
        // Reopening keeps the data and does not migrate again.
        let conn = open_database(&path).unwrap();
        let n: i64 = conn
            .query_row("SELECT COUNT(*) FROM sessions", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 1);
        assert_eq!(version(&conn), latest());
        drop(conn);
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn foreign_keys_reject_samples_for_missing_sessions() {
        let path = temp_db("fk");
        let conn = open_database(&path).unwrap();
        let err = conn.execute(
            "INSERT INTO samples (session_id, timestamp) VALUES (42, 1)",
            [],
        );
        assert!(err.is_err());
        drop(conn);
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }
}

#[cfg(test)]
mod averages_tests {
    use super::*;

    fn session(duration: i64, distance: f64, raw: f64, avg_incline: f32, start: i64) -> Session {
        Session {
            id: Some(1),
            start_time: start,
            end_time: start,
            duration_seconds: duration,
            distance_meters: distance,
            raw_distance_meters: raw,
            calories: 0.0,
            steps: 0.0,
            raw_steps: 0.0,
            avg_speed: 0.0,
            raw_avg_speed: 0.0,
            avg_incline,
            strava_id: None,
            neural_heat: 0.0,
            neural_ticks: 0,
            last_hardware_time: 0,
            hardware_time_offset: 0,
        }
    }

    fn sample(ts: i64, incline: f32) -> Sample {
        Sample {
            timestamp: Some(ts),
            speed: 0.0,
            incline,
            distance: 0.0,
            raw_distance: 0.0,
            steps: 0.0,
            raw_steps: 0.0,
            raw_speed: 0.0,
        }
    }

    #[test]
    fn speeds_are_distance_over_session_time() {
        let s = session(1800, 1500.0, 1650.0, 0.0, 0);
        let (a, r, _) = backfill_averages(&s, &[], 1.0, 0.0);
        assert!((a - 3.0).abs() < 1e-4);
        assert!((r - 3.3).abs() < 1e-4);
        let (a, r, _) = backfill_averages(&session(0, 100.0, 100.0, 0.0, 0), &[], 1.0, 0.0);
        assert_eq!((a, r), (0.0, 0.0));
    }

    #[test]
    fn old_sample_levels_convert_and_new_percents_do_not() {
        let s = session(60, 0.0, 0.0, 0.0, 0);
        let old = INCLINE_PCT_SINCE_MS - 1;
        let new = INCLINE_PCT_SINCE_MS + 1;
        // Levels 2 and 4 at 1.5%/level + 0.5 offset -> 3.5% and 6.5%.
        let (_, _, i) = backfill_averages(&s, &[sample(old, 2.0), sample(old, 4.0)], 1.5, 0.5);
        assert!((i - 5.0).abs() < 1e-4);
        let (_, _, i) = backfill_averages(&s, &[sample(new, 5.0), sample(new, 7.0)], 1.5, 0.5);
        assert!((i - 6.0).abs() < 1e-4);
    }

    #[test]
    fn without_samples_an_old_stored_incline_is_converted() {
        let old = session(60, 0.0, 0.0, 4.0, INCLINE_PCT_SINCE_MS - 1);
        assert!((backfill_averages(&old, &[], 2.0, 1.0).2 - 9.0).abs() < 1e-4);
        let new = session(60, 0.0, 0.0, 4.0, INCLINE_PCT_SINCE_MS + 1);
        assert!((backfill_averages(&new, &[], 2.0, 1.0).2 - 4.0).abs() < 1e-4);
    }
}

#[cfg(test)]
mod settings_tests {
    use super::*;
    use std::collections::HashMap;

    fn stored(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    fn load(map: &HashMap<String, String>) -> AppConfig {
        config_from_settings(|key| map.get(key).cloned())
    }

    #[test]
    fn nothing_stored_gives_the_defaults() {
        let c = load(&HashMap::new());
        let d = AppConfig::default();
        assert_eq!(c.last_device_id, None);
        assert!(c.calibration_points.is_empty());
        assert_eq!(
            (c.belt_length, c.min_speed, c.max_speed, c.max_incline),
            (d.belt_length, d.min_speed, d.max_speed, d.max_incline)
        );
        assert!(!c.focus_stability_enabled);
        assert_eq!(c.body_mass_kg, DEFAULT_BODY_MASS_KG);
        assert_eq!(c.neural_floor_pct, d.neural_floor_pct);
        assert_eq!(c.neural_sensitivity, d.neural_sensitivity);
    }

    #[test]
    fn stored_values_load_quoted_or_bare() {
        let c = load(&stored(&[
            ("lastDeviceId", "\"AA:BB\""),
            ("beltLength", "\"130.5\""),
            ("maxSpeed", "8"),
            ("maxIncline", "\"12\""),
            ("focusStabilityEnabled", "\"true\""),
            ("neuralFloorPct", "\"70\""),
            ("neuralSensitivity", "\"low\""),
            (
                "calibrationPoints",
                r#"[{"speed":2.0,"stride_length_cm":65.0,"distance_scale":1.1}]"#,
            ),
        ]));
        assert_eq!(c.last_device_id.as_deref(), Some("AA:BB"));
        assert_eq!(c.belt_length, 130.5);
        assert_eq!(c.max_speed, 8.0);
        assert_eq!(c.max_incline, 12);
        assert!(c.focus_stability_enabled);
        assert_eq!(c.neural_floor_pct, 70.0);
        assert_eq!(c.neural_sensitivity, NeuralSensitivity::Low);
        assert_eq!(c.calibration_points.len(), 1);
    }

    #[test]
    fn unreadable_values_keep_the_defaults() {
        let c = load(&stored(&[
            ("beltLength", "long"),
            ("maxIncline", "-3"),
            ("focusStabilityEnabled", "yes"),
            ("calibrationPoints", "not json"),
        ]));
        let d = AppConfig::default();
        assert_eq!(c.belt_length, d.belt_length);
        assert_eq!(c.max_incline, d.max_incline);
        assert!(!c.focus_stability_enabled);
        assert!(c.calibration_points.is_empty());
    }

    #[test]
    fn body_mass_is_clamped() {
        assert_eq!(
            load(&stored(&[("bodyMassKg", "5")])).body_mass_kg,
            MIN_BODY_MASS_KG
        );
        assert_eq!(
            load(&stored(&[("bodyMassKg", "900")])).body_mass_kg,
            MAX_BODY_MASS_KG
        );
        assert_eq!(load(&stored(&[("bodyMassKg", "82.5")])).body_mass_kg, 82.5);
    }

    #[test]
    fn incline_above_255_wraps_as_it_always_has() {
        assert_eq!(load(&stored(&[("maxIncline", "260")])).max_incline, 4);
    }

    #[test]
    fn saved_rows_load_back_to_the_same_config() {
        let config = AppConfig {
            last_device_id: Some("dev-1".to_string()),
            calibration_points: vec![CalibrationPoint {
                speed: 3.0,
                stride_length_cm: 70.0,
                distance_scale: 0.9,
            }],
            belt_length: 140.0,
            min_speed: 1.0,
            max_speed: 10.0,
            max_incline: 9,
            tilt_multiplier: 1.5,
            incline_offset: -0.5,
            focus_stability_enabled: true,
            body_mass_kg: 90.0,
            neural_floor_pct: 80.0,
            neural_sensitivity: NeuralSensitivity::Low,
        };
        let rows = config_to_settings(&config).unwrap();
        let keys: Vec<&str> = rows.iter().map(|(k, _)| *k).collect();
        assert_eq!(
            keys,
            [
                "lastDeviceId",
                "calibrationPoints",
                "beltLength",
                "maxSpeed",
                "minSpeed",
                "maxIncline",
                "tiltMultiplier",
                "inclineOffset",
                "focusStabilityEnabled",
                "bodyMassKg",
                "neuralFloorPct",
                "neuralSensitivity",
            ]
        );
        let map: HashMap<String, String> =
            rows.into_iter().map(|(k, v)| (k.to_string(), v)).collect();
        assert_eq!(map["maxIncline"], "9");
        assert_eq!(map["focusStabilityEnabled"], "true");
        let back = load(&map);
        assert_eq!(
            serde_json::to_value(&back).unwrap(),
            serde_json::to_value(&config).unwrap()
        );
    }

    #[test]
    fn no_device_id_writes_no_device_row() {
        let rows = config_to_settings(&AppConfig::default()).unwrap();
        assert!(rows.iter().all(|(k, _)| *k != "lastDeviceId"));
        assert_eq!(rows.len(), SCALAR_FIELDS.len() + 3);
    }

    #[test]
    fn neural_settings_migrate_from_the_legacy_keys_only_when_missing() {
        let legacy = stored(&[("glideBiasPercentage", "20"), ("glideThreshold", "0.25")]);
        let c = load(&legacy);
        let expected = neural::migrate_legacy(Some(20.0), Some(0.25));
        assert_eq!(
            (c.neural_floor_pct, c.neural_sensitivity),
            (neural::clamp_floor_pct(expected.0), expected.1)
        );

        // Both new keys present: the legacy ones are ignored.
        let c = load(&stored(&[
            ("glideBiasPercentage", "20"),
            ("glideThreshold", "0.25"),
            ("neuralFloorPct", "60"),
            ("neuralSensitivity", "high"),
        ]));
        assert_eq!(c.neural_floor_pct, 60.0);
        assert_eq!(Some(c.neural_sensitivity), NeuralSensitivity::parse("high"));

        // The floor is clamped on load and on save.
        assert_eq!(
            load(&stored(&[
                ("neuralFloorPct", "10"),
                ("neuralSensitivity", "low")
            ]))
            .neural_floor_pct,
            neural::MIN_FLOOR_PCT
        );
        let rows = config_to_settings(&AppConfig {
            neural_floor_pct: 500.0,
            ..AppConfig::default()
        })
        .unwrap();
        assert!(rows.contains(&("neuralFloorPct", neural::MAX_FLOOR_PCT.to_string())));
    }

    #[test]
    fn kv_tables_are_whitelisted_and_secrets_refused() {
        assert_eq!(
            kv_table_for("settings", "beltLength"),
            Ok(KvTable::Settings)
        );
        assert_eq!(
            kv_table_for("automation_state", "x"),
            Ok(KvTable::AutomationState)
        );
        assert_eq!(
            kv_table_for("sessions", "x"),
            Err("Invalid table".to_string())
        );
        assert_eq!(
            kv_table_for("settings", "stravaAuth"),
            Err("Protected setting".to_string())
        );
        // Only the settings table holds secrets.
        assert_eq!(
            kv_table_for("automation_state", "stravaAuth"),
            Ok(KvTable::AutomationState)
        );
        for t in [KvTable::Settings, KvTable::AutomationState] {
            assert_eq!(KvTable::parse(t.name()), Ok(t));
        }
    }

    #[test]
    fn local_midnight_is_within_the_last_day() {
        let now = chrono::Local::now();
        let midnight = local_midnight_ms(now);
        let now_ms = now.timestamp_millis();
        assert!(midnight <= now_ms);
        // 25h tolerates a DST day.
        assert!(now_ms - midnight < 25 * 60 * 60 * 1000);
    }
}
