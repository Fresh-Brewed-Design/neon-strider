//! Pure Strava logic: grouping sessions into walks, choosing what auto-sync
//! should upload, building the activity (TCX or manual), and parsing Strava's
//! replies. No I/O, no Tauri, no clock reads, so all of it is unit-tested here.
//! `strava.rs` owns the HTTP, the database and the background worker.

use crate::persistence::{Sample, Session};
use crate::snapshot::{MuteMode, StravaGear};
use crate::strava_template::{self as template, Stats, Templates};
use chrono::{DateTime, Local, NaiveDate, NaiveTime, SecondsFormat, TimeZone, Utc};
use std::collections::BTreeMap;

/// Walks shorter than this are junk (a belt start/stop) and never auto-uploaded.
/// 50 m is under a minute at the slowest walk, so any real walk passes, while a
/// belt started and stopped again does not. DESK_SYNC and the weekly summary
/// count a walked day by the same distance.
pub const MIN_WALK_DISTANCE_M: f64 = 50.0;
/// A coffee or a call between two walks is one walk on Strava; a lunch break is two.
const DEFAULT_MERGE_MINUTES: u32 = 10;
/// The "join walks closer than" range: a minute, up to an hour (longer gaps are
/// separate outings). History's setting reads it from `SettingLimits`.
pub const MIN_MERGE_MINUTES: u32 = 1;
pub const MAX_MERGE_MINUTES: u32 = 60;
pub const DEFAULT_SHIFT_END: &str = "18:00";
/// Refresh the access token when it expires within this many seconds.
/// Five minutes covers a slow upload that starts just before the token expires.
const REFRESH_MARGIN_S: i64 = 300;
/// Permissions asked for at connect. `activity:write` uploads and edits;
/// `activity:read` lets us list recent activities (duplicate check);
/// `profile:read_all` is what makes `GET /athlete` include the shoes.
pub const REQUESTED_SCOPE: &str = "read,activity:write,activity:read,profile:read_all";
/// Shorter walks are the idle nudge's breaks: kept off the feed, still uploaded.
pub const DEFAULT_MUTE_MINUTES: u32 = 20;
/// The "mute walks under" range: a minute up to a four-hour walk, past which
/// nothing at a desk is short. UploadOptions reads it from `SettingLimits`.
pub const MIN_MUTE_MINUTES: u32 = 1;
pub const MAX_MUTE_MINUTES: u32 = 240;
/// Another app's activity this close to ours (either side) is the same walk.
/// A watch started a few minutes before or after the belt is the same walk;
/// two real walks are rarely that close.
const DUPLICATE_TOLERANCE_MS: i64 = 5 * MINUTE_MS;
/// The list request looks this far past each end of our walk.
/// Twice the tolerance, so a match at its edge is still in the list.
const DUPLICATE_WINDOW_PAD_S: i64 = 10 * 60;

const MINUTE_MS: i64 = 60_000;

// Settings values.

/// Strict "HH:MM", 24 hour (what the dashboard's time input reports).
pub fn parse_shift_end(raw: &str) -> Option<NaiveTime> {
    let b = raw.as_bytes();
    if b.len() != 5 || b[2] != b':' {
        return None;
    }
    if ![b[0], b[1], b[3], b[4]].iter().all(u8::is_ascii_digit) {
        return None;
    }
    let h = (b[0] - b'0') * 10 + (b[1] - b'0');
    let m = (b[3] - b'0') * 10 + (b[4] - b'0');
    NaiveTime::from_hms_opt(h as u32, m as u32, 0)
}

/// Merge threshold from its stored setting (JSON number, maybe quoted). Out of
/// range or unreadable values fall back to the clamp/default the dashboard uses.
pub fn parse_merge_minutes(raw: Option<&str>) -> u32 {
    raw.and_then(|s| s.trim().trim_matches('"').parse::<f64>().ok())
        .filter(|v| v.is_finite())
        .map(|v| v.round().clamp(MIN_MERGE_MINUTES as f64, MAX_MERGE_MINUTES as f64) as u32)
        .unwrap_or(DEFAULT_MERGE_MINUTES)
}

/// Stored mute mode; anything unreadable is the default, "all".
pub fn parse_mute_mode(raw: Option<&str>) -> MuteMode {
    match raw.map(|s| s.trim().trim_matches('"')) {
        Some("short") => MuteMode::Short,
        Some("none") => MuteMode::None,
        _ => MuteMode::All,
    }
}

/// How a mute mode is stored; `parse_mute_mode` reads it back.
pub fn mute_mode_setting(mode: MuteMode) -> &'static str {
    match mode {
        MuteMode::All => "all",
        MuteMode::Short => "short",
        MuteMode::None => "none",
    }
}

/// Minutes below which a walk is "short": 1..=240, default 20.
pub fn parse_mute_minutes(raw: Option<&str>) -> u32 {
    raw.and_then(|s| s.trim().trim_matches('"').parse::<f64>().ok())
        .filter(|v| v.is_finite())
        .map(|v| v.round().clamp(MIN_MUTE_MINUTES as f64, MAX_MUTE_MINUTES as f64) as u32)
        .unwrap_or(DEFAULT_MUTE_MINUTES)
}

/// Hide this activity from the Strava home feed?
pub fn should_hide(mode: MuteMode, under_minutes: u32, duration_s: i64) -> bool {
    match mode {
        MuteMode::All => true,
        MuteMode::Short => duration_s < under_minutes as i64 * 60,
        MuteMode::None => false,
    }
}

/// A shoe id from the dashboard: what Strava uses ("g" and digits), nothing more.
/// Blank means no shoe.
pub fn clean_gear_id(raw: Option<&str>) -> Result<Option<String>, String> {
    let Some(id) = raw.map(str::trim).filter(|s| !s.is_empty()) else {
        return Ok(None);
    };
    // Strava gear ids are a letter and a number, far under 40 characters.
    if id.len() > 40
        || !id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        return Err("That is not a Strava shoe id".to_string());
    }
    Ok(Some(id.to_string()))
}

// Token and OAuth decisions.

pub fn needs_refresh(expires_at_s: i64, now_s: i64) -> bool {
    expires_at_s - now_s <= REFRESH_MARGIN_S
}

/// A refresh that Strava answers with 400/401 means the grant is gone.
pub fn refresh_means_disconnected(status: u16) -> bool {
    status == 400 || status == 401
}

/// The authorize redirect lists the scopes the user actually granted (they can
/// untick "upload"). A missing `scope` parameter is not held against them.
pub fn scope_allows_upload(scope: Option<&str>) -> bool {
    match scope {
        Some(s) => s.split(',').any(|p| p.trim() == "activity:write"),
        None => true,
    }
}

/// What a stored scope string lets us do beyond uploading.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Scopes {
    /// `GET /athlete` includes the shoes.
    pub gear: bool,
    /// `GET /athlete/activities` is allowed.
    pub list_activities: bool,
}

/// `None` (a login from before the scope was stored) grants nothing extra: it
/// was made with the older, smaller request.
pub fn scopes(scope: Option<&str>) -> Scopes {
    let has =
        |wanted: &[&str]| scope.is_some_and(|s| s.split(',').any(|p| wanted.contains(&p.trim())));
    Scopes {
        gear: has(&["profile:read_all"]),
        list_activities: has(&["activity:read", "activity:read_all"]),
    }
}

/// A linked login that is missing a permission the app now asks for.
pub fn needs_reconnect(connected: bool, scope: Option<&str>) -> bool {
    let s = scopes(scope);
    connected && !(s.gear && s.list_activities)
}

pub fn athlete_display_name(first: Option<&str>, last: Option<&str>) -> Option<String> {
    let name = format!(
        "{} {}",
        first.unwrap_or("").trim(),
        last.unwrap_or("").trim()
    );
    let name = name.trim();
    (!name.is_empty()).then(|| name.to_string())
}

// Merging sessions into walks (same rule as src/services/sessionMerge.ts)

/// A run of adjacent sessions folded into one. `ids` are DB ids, oldest first.
#[derive(Debug, Clone, PartialEq)]
pub struct MergedSession {
    pub ids: Vec<i64>,
    pub start_time: i64,
    pub end_time: i64,
    pub duration_seconds: i64,
    pub distance_meters: f64,
    pub raw_distance_meters: f64,
    pub calories: f64,
    pub steps: f64,
    pub raw_steps: f64,
    pub avg_speed: f64,
    pub raw_avg_speed: f64,
    pub avg_incline: f64,
    pub neural_heat: f64,
    pub neural_ticks: i64,
    pub strava_id: Option<String>,
}

impl MergedSession {
    fn from_session(s: &Session) -> Self {
        Self {
            ids: s.id.into_iter().collect(),
            start_time: s.start_time,
            end_time: s.end_time,
            duration_seconds: s.duration_seconds,
            distance_meters: s.distance_meters,
            raw_distance_meters: s.raw_distance_meters,
            calories: s.calories as f64,
            steps: s.steps,
            raw_steps: s.raw_steps,
            avg_speed: s.avg_speed as f64,
            raw_avg_speed: s.raw_avg_speed as f64,
            avg_incline: s.avg_incline as f64,
            neural_heat: s.neural_heat as f64,
            neural_ticks: s.neural_ticks,
            strava_id: s.strava_id.clone(),
        }
    }

    fn absorb(&mut self, s: &Session) {
        let total = self.duration_seconds + s.duration_seconds;
        let hours = total as f64 / 3600.0;
        self.avg_incline = if total > 0 {
            (self.avg_incline * self.duration_seconds as f64
                + s.avg_incline as f64 * s.duration_seconds as f64)
                / total as f64
        } else {
            0.0
        };
        self.end_time = s.end_time;
        self.duration_seconds = total;
        self.distance_meters += s.distance_meters;
        self.raw_distance_meters += s.raw_distance_meters;
        self.calories += s.calories as f64;
        self.steps += s.steps;
        self.raw_steps += s.raw_steps;
        self.neural_heat += s.neural_heat as f64;
        self.neural_ticks += s.neural_ticks;
        if !is_uploaded(&self.strava_id) {
            self.strava_id = s.strava_id.clone();
        }
        self.ids.extend(s.id);
        self.avg_speed = if hours > 0.0 {
            self.distance_meters / 1000.0 / hours
        } else {
            0.0
        };
        self.raw_avg_speed = if hours > 0.0 {
            self.raw_distance_meters / 1000.0 / hours
        } else {
            0.0
        };
    }
}

pub fn is_uploaded(strava_id: &Option<String>) -> bool {
    strava_id.as_deref().is_some_and(|v| !v.is_empty())
}

/// Fold sessions that start within `threshold_minutes` of the previous
/// session's end into one. Output is oldest first.
pub fn merge_sessions(sessions: &[Session], threshold_minutes: u32) -> Vec<MergedSession> {
    let threshold_ms = threshold_minutes as i64 * MINUTE_MS;
    let mut ordered: Vec<&Session> = sessions.iter().collect();
    ordered.sort_by_key(|s| s.start_time);

    let mut merged: Vec<MergedSession> = Vec::new();
    for s in ordered {
        match merged.last_mut() {
            Some(last) if s.start_time - last.end_time < threshold_ms => last.absorb(s),
            _ => merged.push(MergedSession::from_session(s)),
        }
    }
    merged
}

// What should auto-sync upload?

/// Sessions auto-sync may consider: ended after the cutoff and not on Strava
/// yet. The live session is always kept so the group it belongs to can be
/// recognised (and skipped) rather than split around it.
fn candidates(sessions: &[Session], active: Option<i64>, since_ms: i64) -> Vec<Session> {
    sessions
        .iter()
        .filter(|s| {
            (active.is_some() && s.id == active)
                || (s.end_time > since_ms && !is_uploaded(&s.strava_id))
        })
        .cloned()
        .collect()
}

/// Walks to upload one by one (daily rollup off), oldest first: merged groups
/// that ended after `since_ms`, are not on Strava, do not contain the live
/// session, have been quiet for the merge threshold (so a short break-and-resume
/// merges first) and cover at least `MIN_WALK_DISTANCE_M`.
pub fn select_walks(
    sessions: &[Session],
    active: Option<i64>,
    since_ms: i64,
    threshold_minutes: u32,
    now_ms: i64,
) -> Vec<MergedSession> {
    let threshold_ms = threshold_minutes as i64 * MINUTE_MS;
    merge_sessions(&candidates(sessions, active, since_ms), threshold_minutes)
        .into_iter()
        .filter(|g| {
            !g.ids.iter().any(|id| Some(*id) == active)
                && now_ms - g.end_time >= threshold_ms
                && g.distance_meters >= MIN_WALK_DISTANCE_M
        })
        .collect()
}

/// One local calendar day to upload as a single activity.
#[derive(Debug, Clone, PartialEq)]
pub struct RollupDay {
    pub day: NaiveDate,
    pub ids: Vec<i64>,
    /// Walks (merged groups) in the day, for the activity name.
    pub walks: usize,
}

fn local_day(ms: i64) -> Option<NaiveDate> {
    Local
        .timestamp_millis_opt(ms)
        .earliest()
        .map(|d| d.date_naive())
}

/// Days to roll up, oldest first (daily rollup on). A day qualifies once its
/// local shift end has passed, so a day missed while the app was closed is
/// picked up at the next pass. A day waits while the live session started on it
/// or while its last session is inside the merge threshold, and is skipped when
/// the day's total distance is junk. Sessions are assigned to the local day they
/// started on.
pub fn select_rollup_days(
    sessions: &[Session],
    active: Option<i64>,
    since_ms: i64,
    shift_end: NaiveTime,
    threshold_minutes: u32,
    now: DateTime<Local>,
) -> Vec<RollupDay> {
    let threshold_ms = threshold_minutes as i64 * MINUTE_MS;
    let now_ms = now.timestamp_millis();

    let mut by_day: BTreeMap<NaiveDate, Vec<Session>> = BTreeMap::new();
    for s in candidates(sessions, active, since_ms) {
        if let Some(day) = local_day(s.start_time) {
            by_day.entry(day).or_default().push(s);
        }
    }

    let mut out = Vec::new();
    for (day, mut list) in by_day {
        // Compared as wall-clock time so DST gaps cannot make a day vanish.
        if now.naive_local() < day.and_time(shift_end) {
            continue;
        }
        if list.iter().any(|s| active.is_some() && s.id == active) {
            continue;
        }
        list.sort_by_key(|s| s.start_time);
        let last_end = list.iter().map(|s| s.end_time).max().unwrap_or(0);
        let distance: f64 = list.iter().map(|s| s.distance_meters).sum();
        if now_ms - last_end < threshold_ms || distance < MIN_WALK_DISTANCE_M {
            continue;
        }
        out.push(RollupDay {
            day,
            walks: merge_sessions(&list, threshold_minutes).len(),
            ids: list.iter().filter_map(|s| s.id).collect(),
        });
    }
    out
}

// Building the activity.

#[derive(Debug, Clone, PartialEq)]
pub struct Point {
    pub timestamp_ms: i64,
    pub speed_kmh: f32,
    pub incline_percent: f32,
    /// Metres since the start of its own session.
    pub distance_m: f32,
    /// Beats per minute, when HEART_LINK had a reading.
    pub heart_rate: Option<u16>,
}

/// One session's telemetry. Samples are deduplicated by timestamp (the last one
/// wins) and ordered. When the stored distance never moved although the session
/// covered ground (old data) the distance is rebuilt by integrating the real
/// speed readings. Nothing is invented: sparse data stays sparse.
fn build_points(session: &Session, samples: &[Sample]) -> Vec<Point> {
    let mut unique: BTreeMap<i64, &Sample> = BTreeMap::new();
    for s in samples {
        if let Some(ts) = s.timestamp.filter(|t| *t > 0) {
            unique.insert(ts, s);
        }
    }
    let ordered: Vec<(i64, &Sample)> = unique.into_iter().collect();

    let integrate = ordered
        .last()
        .is_some_and(|(_, s)| s.distance == 0.0 && session.distance_meters > MIN_WALK_DISTANCE_M);

    let mut points = Vec::with_capacity(ordered.len());
    let mut integrated = 0.0_f64;
    for (i, (ts, s)) in ordered.iter().enumerate() {
        if integrate && i > 0 {
            let (prev_ts, prev) = ordered[i - 1];
            let dt = (*ts - prev_ts) as f64 / 1000.0;
            if prev.speed != 0.0 {
                integrated += prev.speed as f64 / 3.6 * dt;
            }
        }
        points.push(Point {
            timestamp_ms: *ts,
            speed_kmh: if s.speed.is_finite() { s.speed } else { 0.0 },
            incline_percent: if s.incline.is_finite() {
                s.incline
            } else {
                0.0
            },
            distance_m: if integrate {
                integrated as f32
            } else if s.distance.is_finite() {
                s.distance
            } else {
                0.0
            },
            heart_rate: s.heart_rate.filter(|b| *b > 0),
        });
    }
    points
}

/// Average and maximum heart rate over these points, `None` without any.
fn heart_rate_summary(points: &[Point]) -> Option<(u16, u16)> {
    let rates: Vec<u16> = points.iter().filter_map(|p| p.heart_rate).collect();
    let max = *rates.iter().max()?;
    let sum: u32 = rates.iter().map(|&b| u32::from(b)).sum();
    let avg = (sum as f64 / rates.len() as f64).round() as u16;
    Some((avg, max))
}

#[derive(Debug, Clone, PartialEq)]
pub struct Lap {
    pub start_ms: i64,
    pub total_time_s: i64,
    pub distance_m: f64,
    pub calories: f64,
    pub points: Vec<Point>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Naming {
    Walk,
    Rollup { walks: usize },
}

#[derive(Debug, Clone, PartialEq)]
pub enum Payload {
    /// A TCX file with real telemetry.
    Tcx(String),
    /// Fewer than two usable samples: a manual activity, no file.
    Manual,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ActivityPlan {
    pub name: String,
    pub description: String,
    pub start_ms: i64,
    /// End of the last session.
    pub end_ms: i64,
    pub elapsed_s: i64,
    pub distance_m: f64,
    pub payload: Payload,
}

fn local_naive(ms: i64) -> chrono::NaiveDateTime {
    Local
        .timestamp_millis_opt(ms)
        .earliest()
        .map(|d| d.naive_local())
        .unwrap_or_default()
}

/// Build the activity for these sessions (any order) with their samples. The
/// title and description come from the user's templates.
pub fn plan_activity(
    data: &[(Session, Vec<Sample>)],
    naming: Naming,
    templates: &Templates,
) -> ActivityPlan {
    let mut ordered: Vec<&(Session, Vec<Sample>)> = data.iter().collect();
    ordered.sort_by_key(|(s, _)| s.start_time);

    let start_ms = ordered.first().map_or(0, |(s, _)| s.start_time);
    let end_ms = ordered
        .iter()
        .map(|(s, _)| s.end_time)
        .max()
        .unwrap_or(start_ms)
        .max(start_ms);
    let distance_m: f64 = ordered.iter().map(|(s, _)| s.distance_meters).sum();
    let duration_s: i64 = ordered.iter().map(|(s, _)| s.duration_seconds).sum();
    let calories: f64 = ordered.iter().map(|(s, _)| s.calories as f64).sum();
    let steps: f64 = ordered.iter().map(|(s, _)| s.steps).sum();
    // Duration-weighted, the same way merged walks average their incline.
    let avg_incline = if duration_s > 0 {
        ordered
            .iter()
            .map(|(s, _)| s.avg_incline as f64 * s.duration_seconds as f64)
            .sum::<f64>()
            / duration_s as f64
    } else {
        0.0
    };

    let laps: Vec<Lap> = ordered
        .iter()
        .map(|(s, samples)| Lap {
            start_ms: s.start_time,
            total_time_s: s.duration_seconds,
            distance_m: s.distance_meters,
            calories: s.calories as f64,
            points: build_points(s, samples),
        })
        .filter(|l| !l.points.is_empty())
        .collect();
    let sample_count: usize = laps.iter().map(|l| l.points.len()).sum();

    let (title_template, walks) = match naming {
        Naming::Walk => (&templates.title, 1),
        Naming::Rollup { walks } => (&templates.rollup_title, walks),
    };
    let stats = Stats {
        distance_m,
        duration_s,
        avg_incline,
        calories,
        steps,
        walks,
        start_local: local_naive(start_ms),
    };
    let name = template::render_title(title_template, &stats);
    let description = template::render_description(&templates.description, &stats);

    let payload = if sample_count >= 2 {
        Payload::Tcx(generate_tcx(&laps, start_ms, &description))
    } else {
        Payload::Manual
    };

    ActivityPlan {
        name,
        description,
        start_ms,
        end_ms,
        elapsed_s: duration_s,
        distance_m,
        payload,
    }
}

fn xml_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            c => out.push(c),
        }
    }
    out
}

fn iso(ms: i64) -> String {
    Utc.timestamp_millis_opt(ms)
        .earliest()
        .unwrap_or_default()
        .to_rfc3339_opts(SecondsFormat::Millis, true)
}

/// One `<Lap>` per session, with distance continuing across laps.
fn generate_tcx(laps: &[Lap], start_ms: i64, description: &str) -> String {
    let id = iso(start_ms);
    let mut tcx = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<TrainingCenterDatabase xmlns="http://www.garmin.com/xmlschemas/TrainingCenterDatabase/v2">
  <Activities>
    <Activity Sport="Other">
      <Id>{}</Id>"#,
        id
    );

    let mut altitude = 0.0_f64;
    let mut offset = 0.0_f64;
    for lap in laps {
        tcx.push_str(&format!(
            r#"
      <Lap StartTime="{}">
        <TotalTimeSeconds>{}</TotalTimeSeconds>
        <DistanceMeters>{:.1}</DistanceMeters>
        <Calories>{}</Calories>{}
        <Intensity>Active</Intensity>
        <TriggerMethod>Manual</TriggerMethod>
        <Track>"#,
            iso(lap.start_ms),
            lap.total_time_s.max(0),
            lap.distance_m,
            lap.calories.max(0.0) as i64,
            heart_rate_summary(&lap.points).map_or(String::new(), |(avg, max)| format!(
                "
        <AverageHeartRateBpm><Value>{}</Value></AverageHeartRateBpm>
        <MaximumHeartRateBpm><Value>{}</Value></MaximumHeartRateBpm>",
                avg, max
            ))
        ));

        let mut last_distance = 0.0_f64;
        for p in &lap.points {
            let d = p.distance_m as f64;
            let delta = d - last_distance;
            if delta > 0.0 {
                altitude += delta * (p.incline_percent as f64 / 100.0);
            }
            last_distance = d;
            tcx.push_str(&format!(
                r#"
          <Trackpoint>
            <Time>{}</Time>
            <AltitudeMeters>{:.2}</AltitudeMeters>
            <DistanceMeters>{:.2}</DistanceMeters>{}
            <Extensions>
              <TPX xmlns="http://www.garmin.com/xmlschemas/ActivityExtension/v2">
                <Speed>{:.2}</Speed>
              </TPX>
            </Extensions>
          </Trackpoint>"#,
                iso(p.timestamp_ms),
                altitude,
                offset + d,
                p.heart_rate.map_or(String::new(), |b| format!(
                    "
            <HeartRateBpm><Value>{}</Value></HeartRateBpm>",
                    b
                )),
                p.speed_kmh / 3.6
            ));
        }
        offset += last_distance;

        tcx.push_str(
            r#"
        </Track>
      </Lap>"#,
        );
    }

    tcx.push_str(&format!(
        "\n      <Notes>{}</Notes>\n    </Activity>\n  </Activities>\n</TrainingCenterDatabase>",
        xml_escape(description)
    ));
    tcx
}

/// Strava's `start_date_local` for a manual activity: wall-clock time in the
/// local zone with a trailing Z, which is how Strava documents the field.
pub fn start_date_local(ms: i64) -> String {
    Local
        .timestamp_millis_opt(ms)
        .earliest()
        .map(|d| d.format("%Y-%m-%dT%H:%M:%SZ").to_string())
        .unwrap_or_default()
}

// Strava's upload status.

#[derive(Debug, Clone, PartialEq)]
pub enum UploadStatus {
    Done(String),
    Failed(String),
    Pending,
}

/// "... duplicate of activity 123 ..." -> "123".
pub fn duplicate_activity_id(error: &str) -> Option<String> {
    const MARK: &str = "duplicate of activity";
    let rest = &error[error.find(MARK)? + MARK.len()..];
    let digits: String = rest
        .trim_start()
        .chars()
        .take_while(char::is_ascii_digit)
        .collect();
    (!digits.is_empty()).then_some(digits)
}

/// A JSON id that may arrive as a number or a string.
pub fn json_id(v: &serde_json::Value) -> Option<String> {
    match v {
        serde_json::Value::Number(n) => Some(n.to_string()),
        serde_json::Value::String(s) if !s.is_empty() => Some(s.clone()),
        _ => None,
    }
}

/// Reply of `POST /uploads` or `GET /uploads/{id}`. A duplicate is a success:
/// the walk is already on Strava, so mark it with that activity.
pub fn parse_upload_status(v: &serde_json::Value) -> UploadStatus {
    if let Some(id) = json_id(&v["activity_id"]) {
        return UploadStatus::Done(id);
    }
    if let Some(err) = v["error"].as_str().filter(|e| !e.is_empty()) {
        return match duplicate_activity_id(err) {
            Some(id) => UploadStatus::Done(id),
            None => UploadStatus::Failed(err.to_string()),
        };
    }
    UploadStatus::Pending
}

// Shoes, the activity update, and the duplicate check.

/// The shoes in a `GET /athlete` reply. Without `profile:read_all` Strava sends
/// the summary athlete, which has no `shoes` key at all: that is an error, not
/// "no shoes". `distance` is metres.
pub fn parse_shoes(athlete: &serde_json::Value) -> Result<Vec<StravaGear>, String> {
    let Some(list) = athlete["shoes"].as_array() else {
        return Err(
            "Strava did not share your shoes. Reconnect and allow profile access.".to_string(),
        );
    };
    Ok(list
        .iter()
        .filter_map(|g| {
            let id = json_id(&g["id"])?;
            let name = ["name", "nickname"]
                .iter()
                .filter_map(|k| g[*k].as_str())
                .map(str::trim)
                .find(|n| !n.is_empty())
                .unwrap_or(&id)
                .to_string();
            Some(StravaGear {
                name,
                distance_km: g["distance"].as_f64().unwrap_or(0.0) / 1000.0,
                primary: g["primary"].as_bool().unwrap_or(false),
                retired: g["retired"].as_bool().unwrap_or(false),
                id,
            })
        })
        .collect())
}

/// The one `PUT /activities/{id}` body for what an upload cannot set itself:
/// the shoe and the mute. `None` when there is nothing to change.
pub fn activity_update(gear_id: Option<&str>, hide_from_home: bool) -> Option<serde_json::Value> {
    if gear_id.is_none() && !hide_from_home {
        return None;
    }
    let mut body = serde_json::json!({ "trainer": true });
    if let Some(g) = gear_id {
        body["gear_id"] = g.into();
    }
    if hide_from_home {
        body["hide_from_home"] = true.into();
    }
    Some(body)
}

/// `after`/`before` (epoch seconds) for the list call that looks for the same
/// walk uploaded by another app.
pub fn duplicate_window(start_ms: i64, end_ms: i64) -> (i64, i64) {
    (
        start_ms.div_euclid(1000) - DUPLICATE_WINDOW_PAD_S,
        end_ms.max(start_ms).div_euclid(1000) + DUPLICATE_WINDOW_PAD_S,
    )
}

/// An activity already on Strava, reduced to what the duplicate check reads.
#[derive(Debug, Clone, PartialEq)]
pub struct Existing {
    pub id: String,
    pub start_ms: i64,
    /// `sport_type`, else `type`.
    pub kind: String,
    pub trainer: bool,
}

/// Rows of `GET /athlete/activities`; rows without an id or a readable
/// `start_date` (UTC) are dropped.
pub fn parse_activities(list: &serde_json::Value) -> Vec<Existing> {
    list.as_array()
        .map(|rows| {
            rows.iter()
                .filter_map(|a| {
                    let id = json_id(&a["id"])?;
                    let start_ms = DateTime::parse_from_rfc3339(a["start_date"].as_str()?)
                        .ok()?
                        .timestamp_millis();
                    let kind = a["sport_type"]
                        .as_str()
                        .or_else(|| a["type"].as_str())
                        .unwrap_or("")
                        .to_string();
                    Some(Existing {
                        id,
                        start_ms,
                        kind,
                        trainer: a["trainer"].as_bool().unwrap_or(false),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

fn is_on_foot(kind: &str) -> bool {
    ["walk", "run", "trailrun", "virtualrun", "hike"]
        .iter()
        .any(|k| kind.eq_ignore_ascii_case(k))
}

/// The existing activity that is the same walk: it starts within
/// `DUPLICATE_TOLERANCE_MS` of ours and is a walk, run or hike, or is marked as
/// a trainer activity. The closest start wins.
pub fn find_duplicate(existing: &[Existing], start_ms: i64) -> Option<&Existing> {
    existing
        .iter()
        .filter(|e| {
            (e.start_ms - start_ms).abs() <= DUPLICATE_TOLERANCE_MS
                && (is_on_foot(&e.kind) || e.trainer)
        })
        .min_by_key(|e| (e.start_ms - start_ms).abs())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const MIN: i64 = 60_000;

    /// Same shape as the helper in src/services/sessionMerge.test.ts.
    fn session(id: i64, start_min: i64, duration_min: i64) -> Session {
        Session {
            id: Some(id),
            start_time: start_min * MIN,
            end_time: (start_min + duration_min) * MIN,
            duration_seconds: duration_min * 60,
            distance_meters: (duration_min * 100) as f64,
            raw_distance_meters: (duration_min * 110) as f64,
            calories: (duration_min * 4) as f32,
            steps: (duration_min * 90) as f64,
            raw_steps: (duration_min * 95) as f64,
            avg_speed: 6.0,
            raw_avg_speed: 6.6,
            avg_incline: 0.0,
            strava_id: None,
            neural_heat: 0.0,
            neural_ticks: 0,
            last_hardware_time: 0,
            hardware_time_offset: 0,
            avg_heart_rate: None,
            max_heart_rate: None,
        }
    }

    fn ids(m: &[MergedSession]) -> Vec<Vec<i64>> {
        m.iter().map(|g| g.ids.clone()).collect()
    }

    // ---- merge parity with sessionMerge.test.ts -------------------------

    #[test]
    fn merge_returns_nothing_for_no_sessions() {
        assert!(merge_sessions(&[], 10).is_empty());
    }

    #[test]
    fn merge_keeps_a_lone_session_and_records_its_id() {
        let m = merge_sessions(&[session(1, 0, 10)], 10);
        assert_eq!(m[0].ids, vec![1]);
        assert_eq!(m[0].distance_meters, 1000.0);
    }

    #[test]
    fn merge_sums_counters_of_sessions_closer_than_the_threshold() {
        let m = &merge_sessions(&[session(1, 0, 10), session(2, 15, 20)], 10)[0];
        assert_eq!(m.ids, vec![1, 2]);
        assert_eq!(m.duration_seconds, 30 * 60);
        assert_eq!(m.distance_meters, 3000.0);
        assert_eq!(m.raw_distance_meters, 3300.0);
        assert_eq!(m.calories, 120.0);
        assert_eq!(m.steps, 2700.0);
        assert_eq!(m.raw_steps, 2850.0);
        assert_eq!(m.start_time, 0);
        assert_eq!(m.end_time, 35 * MIN);
    }

    #[test]
    fn merge_measures_the_gap_from_the_previous_end_time() {
        // Session 1 ends at minute 60; session 2 starts at 65: a 5 minute gap.
        assert_eq!(
            merge_sessions(&[session(1, 0, 60), session(2, 65, 5)], 10).len(),
            1
        );
    }

    #[test]
    fn merge_splits_when_the_gap_is_at_or_beyond_the_threshold() {
        assert_eq!(
            merge_sessions(&[session(1, 0, 10), session(2, 20, 10)], 10).len(),
            2
        );
        assert_eq!(
            merge_sessions(&[session(1, 0, 10), session(2, 25, 10)], 10).len(),
            2
        );
    }

    #[test]
    fn merge_chains_a_run_measuring_each_gap_from_the_growing_group_end() {
        let m = merge_sessions(
            &[session(1, 0, 10), session(2, 15, 10), session(3, 30, 10)],
            10,
        );
        assert_eq!(ids(&m), vec![vec![1, 2, 3]]);
    }

    #[test]
    fn merge_sorts_unsorted_input_oldest_first() {
        let m = merge_sessions(
            &[session(3, 200, 10), session(1, 0, 10), session(2, 12, 10)],
            10,
        );
        assert_eq!(ids(&m), vec![vec![1, 2], vec![3]]);
    }

    #[test]
    fn merge_recomputes_average_speed_from_summed_totals() {
        let m = &merge_sessions(&[session(1, 0, 30), session(2, 31, 30)], 10)[0];
        assert!((m.avg_speed - 6.0).abs() < 1e-6);
        assert!((m.raw_avg_speed - 6.6).abs() < 1e-6);
    }

    #[test]
    fn merge_weights_average_incline_by_duration() {
        let mut a = session(1, 0, 10);
        a.avg_incline = 2.0;
        let mut b = session(2, 11, 30);
        b.avg_incline = 6.0;
        let m = &merge_sessions(&[a, b], 10)[0];
        assert!((m.avg_incline - 5.0).abs() < 1e-6);
    }

    #[test]
    fn merge_sums_neural_counters_and_keeps_any_strava_id() {
        let mut a = session(1, 0, 10);
        a.neural_heat = 5.0;
        a.neural_ticks = 2;
        let mut b = session(2, 11, 10);
        b.neural_heat = 7.0;
        b.neural_ticks = 3;
        b.strava_id = Some("abc".into());
        let m = &merge_sessions(&[a, b], 10)[0];
        assert_eq!(m.neural_heat, 12.0);
        assert_eq!(m.neural_ticks, 5);
        assert_eq!(m.strava_id.as_deref(), Some("abc"));
    }

    #[test]
    fn merge_handles_zero_duration_groups() {
        let m = &merge_sessions(&[session(1, 0, 0), session(2, 1, 0)], 10)[0];
        assert_eq!(m.avg_speed, 0.0);
        assert_eq!(m.avg_incline, 0.0);
    }

    #[test]
    fn merge_does_not_mutate_its_input() {
        let input = [session(1, 0, 10), session(2, 11, 10)];
        merge_sessions(&input, 10);
        assert_eq!(input[0].duration_seconds, 600);
        assert_eq!(input[0].end_time, 10 * MIN);
    }

    // ---- walk selection --------------------------------------------------

    #[test]
    fn walks_skip_sessions_that_ended_before_the_cutoff() {
        let s = [session(1, 0, 10), session(2, 100, 10)];
        // Cutoff at minute 50: session 1 (ended at 10) is backlog.
        let w = select_walks(&s, None, 50 * MIN, 10, 200 * MIN);
        assert_eq!(ids(&w), vec![vec![2]]);
        // A session that ended exactly at the cutoff is still before it.
        assert!(select_walks(&s, None, 110 * MIN, 10, 200 * MIN).is_empty());
    }

    #[test]
    fn walks_never_include_uploaded_sessions() {
        let mut a = session(1, 0, 10);
        a.strava_id = Some("99".into());
        let w = select_walks(&[a, session(2, 100, 10)], None, -1, 10, 200 * MIN);
        assert_eq!(ids(&w), vec![vec![2]]);
    }

    #[test]
    fn walks_exclude_the_group_holding_the_active_session() {
        // 1 and 2 merge (5 minute gap) and 2 is live: nothing from that group.
        let s = [session(1, 0, 10), session(2, 15, 10), session(3, 100, 10)];
        let w = select_walks(&s, Some(2), -1, 10, 500 * MIN);
        assert_eq!(ids(&w), vec![vec![3]]);
    }

    #[test]
    fn walks_wait_for_the_merge_window_to_elapse() {
        let s = [session(1, 0, 10)]; // ends at minute 10
        assert!(select_walks(&s, None, -1, 10, 19 * MIN).is_empty());
        assert_eq!(select_walks(&s, None, -1, 10, 20 * MIN).len(), 1);
    }

    #[test]
    fn walks_skip_junk_under_fifty_metres() {
        let mut tiny = session(1, 0, 1);
        tiny.distance_meters = 49.9;
        let mut ok = session(2, 100, 1);
        ok.distance_meters = 50.0;
        let w = select_walks(&[tiny, ok], None, -1, 10, 500 * MIN);
        assert_eq!(ids(&w), vec![vec![2]]);
    }

    #[test]
    fn walks_merge_a_resumed_walk_into_one_upload_oldest_first() {
        let s = [session(3, 300, 10), session(1, 0, 10), session(2, 15, 10)];
        let w = select_walks(&s, None, -1, 10, 900 * MIN);
        assert_eq!(ids(&w), vec![vec![1, 2], vec![3]]);
    }

    // ---- rollup day selection -------------------------------------------

    fn local_ms(d: u32, h: u32, m: u32) -> i64 {
        Local
            .with_ymd_and_hms(2026, 11, d, h, m, 0)
            .unwrap()
            .timestamp_millis()
    }

    fn at(d: u32, h: u32, m: u32) -> DateTime<Local> {
        Local.with_ymd_and_hms(2026, 11, d, h, m, 0).unwrap()
    }

    fn walk_at(id: i64, d: u32, h: u32, m: u32, minutes: i64) -> Session {
        let mut s = session(id, 0, minutes);
        s.start_time = local_ms(d, h, m);
        s.end_time = s.start_time + minutes * MIN;
        s
    }

    fn six() -> NaiveTime {
        NaiveTime::from_hms_opt(18, 0, 0).unwrap()
    }

    #[test]
    fn rollup_waits_until_the_shift_end_has_passed() {
        let s = [walk_at(1, 12, 9, 0, 20)];
        assert!(select_rollup_days(&s, None, -1, six(), 10, at(12, 17, 59)).is_empty());
        let days = select_rollup_days(&s, None, -1, six(), 10, at(12, 18, 0));
        assert_eq!(days.len(), 1);
        assert_eq!(days[0].ids, vec![1]);
    }

    #[test]
    fn rollup_picks_up_a_missed_previous_day_but_holds_today() {
        // App was closed over Nov 10 and 11 shift ends; it is now Nov 12 15:00.
        let s = [
            walk_at(1, 10, 9, 0, 20),
            walk_at(2, 10, 14, 0, 20),
            walk_at(3, 11, 10, 0, 20),
            walk_at(4, 12, 10, 0, 20),
        ];
        let days = select_rollup_days(&s, None, -1, six(), 10, at(12, 15, 0));
        assert_eq!(days.len(), 2);
        assert_eq!(days[0].ids, vec![1, 2]);
        assert_eq!(days[0].day, NaiveDate::from_ymd_opt(2026, 11, 10).unwrap());
        assert_eq!(days[1].ids, vec![3]);
        // After today's shift end, today follows.
        let days = select_rollup_days(&s, None, -1, six(), 10, at(12, 18, 30));
        assert_eq!(days.len(), 3);
        assert_eq!(days[2].ids, vec![4]);
    }

    #[test]
    fn rollup_counts_walks_not_rows_for_the_name() {
        // 1 and 2 are a resumed walk, 3 is a separate walk.
        let s = [
            walk_at(1, 10, 9, 0, 20),
            walk_at(2, 10, 9, 25, 20),
            walk_at(3, 10, 15, 0, 20),
        ];
        let days = select_rollup_days(&s, None, -1, six(), 10, at(10, 19, 0));
        assert_eq!(days[0].walks, 2);
        assert_eq!(days[0].ids, vec![1, 2, 3]);
    }

    #[test]
    fn rollup_respects_cutoff_uploaded_active_and_junk() {
        let mut uploaded = walk_at(1, 10, 8, 0, 20);
        uploaded.strava_id = Some("5".into());
        let before_cutoff = walk_at(2, 10, 9, 0, 20);
        let kept = walk_at(3, 10, 12, 0, 20);
        let cutoff = local_ms(10, 11, 0);
        let days = select_rollup_days(
            &[uploaded, before_cutoff, kept],
            None,
            cutoff,
            six(),
            10,
            at(10, 19, 0),
        );
        assert_eq!(days[0].ids, vec![3]);

        // The live session defers its whole day.
        let s = [walk_at(1, 10, 9, 0, 20), walk_at(2, 10, 18, 30, 5)];
        assert!(select_rollup_days(&s, Some(2), -1, six(), 10, at(10, 18, 40)).is_empty());

        // Junk day.
        let mut junk = walk_at(1, 10, 9, 0, 1);
        junk.distance_meters = 20.0;
        assert!(select_rollup_days(&[junk], None, -1, six(), 10, at(10, 19, 0)).is_empty());
    }

    #[test]
    fn rollup_waits_out_the_merge_window_after_the_last_walk() {
        let s = [walk_at(1, 10, 18, 0, 20)]; // ends 18:20
        assert!(select_rollup_days(&s, None, -1, six(), 10, at(10, 18, 25)).is_empty());
        assert_eq!(
            select_rollup_days(&s, None, -1, six(), 10, at(10, 18, 30)).len(),
            1
        );
    }

    // ---- settings values -------------------------------------------------

    #[test]
    fn shift_end_must_be_strict_hh_mm() {
        assert_eq!(parse_shift_end("18:00"), NaiveTime::from_hms_opt(18, 0, 0));
        assert_eq!(parse_shift_end("00:00"), NaiveTime::from_hms_opt(0, 0, 0));
        assert_eq!(parse_shift_end("23:59"), NaiveTime::from_hms_opt(23, 59, 0));
        for bad in [
            "", "24:00", "7:30", "12:60", "12-30", "12:3", "12:300", "ab:cd", " 12:30", "12:30 ",
        ] {
            assert_eq!(parse_shift_end(bad), None, "{bad:?}");
        }
    }

    #[test]
    fn merge_threshold_setting_matches_the_dashboard() {
        assert_eq!(parse_merge_minutes(None), 10);
        assert_eq!(parse_merge_minutes(Some("15")), 15);
        assert_eq!(parse_merge_minutes(Some("\"20\"")), 20);
        assert_eq!(parse_merge_minutes(Some("0")), 1);
        assert_eq!(parse_merge_minutes(Some("500")), 60);
        assert_eq!(parse_merge_minutes(Some("soon")), 10);
    }

    // ---- token and oauth -------------------------------------------------

    #[test]
    fn token_refresh_decision() {
        let now = 1_000_000;
        assert!(!needs_refresh(now + 301, now));
        assert!(needs_refresh(now + 300, now));
        assert!(needs_refresh(now + 10, now));
        assert!(needs_refresh(now - 5, now));
    }

    #[test]
    fn only_400_and_401_refresh_failures_disconnect() {
        assert!(refresh_means_disconnected(400));
        assert!(refresh_means_disconnected(401));
        assert!(!refresh_means_disconnected(500));
        assert!(!refresh_means_disconnected(429));
    }

    #[test]
    fn upload_scope_is_checked_when_present() {
        assert!(scope_allows_upload(Some("read,activity:write")));
        assert!(scope_allows_upload(None));
        assert!(!scope_allows_upload(Some("read")));
        assert!(!scope_allows_upload(Some("read,activity:read")));
    }

    #[test]
    fn athlete_name_joins_what_is_available() {
        assert_eq!(
            athlete_display_name(Some("Ada"), Some("L")).as_deref(),
            Some("Ada L")
        );
        assert_eq!(
            athlete_display_name(Some("Ada"), None).as_deref(),
            Some("Ada")
        );
        assert_eq!(athlete_display_name(None, None), None);
    }

    // ---- upload status ---------------------------------------------------

    #[test]
    fn duplicate_error_yields_the_existing_activity() {
        assert_eq!(
            duplicate_activity_id("activity.tcx duplicate of activity 1234567890").as_deref(),
            Some("1234567890")
        );
        assert_eq!(
            duplicate_activity_id("duplicate of activity  42."),
            Some("42".into())
        );
        assert_eq!(duplicate_activity_id("duplicate of activity"), None);
        assert_eq!(duplicate_activity_id("malformed file"), None);
    }

    #[test]
    fn upload_status_parsing() {
        assert_eq!(
            parse_upload_status(&json!({"id": 1, "activity_id": 77, "error": null})),
            UploadStatus::Done("77".into())
        );
        assert_eq!(
            parse_upload_status(
                &json!({"id": 1, "activity_id": null, "error": null, "status": "Your activity is still being processed."})
            ),
            UploadStatus::Pending
        );
        assert_eq!(
            parse_upload_status(
                &json!({"id": 1, "activity_id": null, "error": "a.tcx duplicate of activity 5"})
            ),
            UploadStatus::Done("5".into())
        );
        assert_eq!(
            parse_upload_status(
                &json!({"id": 1, "activity_id": null, "error": "The file is malformed"})
            ),
            UploadStatus::Failed("The file is malformed".into())
        );
        assert_eq!(parse_upload_status(&json!({})), UploadStatus::Pending);
    }

    // ---- payload building ------------------------------------------------

    fn sample(ts: i64, speed: f32, incline: f32, distance: f32) -> Sample {
        Sample {
            timestamp: Some(ts),
            speed,
            incline,
            distance,
            raw_distance: distance,
            steps: 0.0,
            raw_steps: 0.0,
            raw_speed: speed,
            heart_rate: None,
        }
    }

    #[test]
    fn points_are_deduplicated_by_timestamp_and_ordered() {
        let s = session(1, 0, 10);
        let samples = [
            sample(3000, 3.0, 1.0, 30.0),
            sample(1000, 3.0, 1.0, 10.0),
            sample(1000, 4.0, 2.0, 11.0), // duplicate: the later one wins
            sample(2000, 3.0, 1.0, 20.0),
        ];
        let p = build_points(&s, &samples);
        assert_eq!(
            p.iter().map(|p| p.timestamp_ms).collect::<Vec<_>>(),
            vec![1000, 2000, 3000]
        );
        assert_eq!(p[0].distance_m, 11.0);
        assert_eq!(p[0].speed_kmh, 4.0);
    }

    #[test]
    fn samples_without_a_real_timestamp_are_unusable() {
        let s = session(1, 0, 10);
        let mut none = sample(0, 3.0, 0.0, 1.0);
        none.timestamp = None;
        assert!(build_points(&s, &[none, sample(0, 3.0, 0.0, 1.0)]).is_empty());
    }

    #[test]
    fn legacy_samples_integrate_real_speed_and_never_invent_any() {
        // Stored distance never moved although the session walked 500 m.
        let mut s = session(1, 0, 10);
        s.distance_meters = 500.0;
        let samples = [
            sample(1000, 3.6, 0.0, 0.0),
            sample(2000, 3.6, 0.0, 0.0),
            sample(4000, 0.0, 0.0, 0.0),
            sample(6000, 7.2, 0.0, 0.0),
        ];
        let p = build_points(&s, &samples);
        // 3.6 km/h = 1 m/s: 1 s -> 1 m, then 2 s -> 3 m, the stop adds nothing.
        let d: Vec<f32> = p.iter().map(|p| p.distance_m).collect();
        assert_eq!(d, vec![0.0, 1.0, 3.0, 3.0]);
        // Speeds are the recorded ones, not an average pasted in.
        assert_eq!(p[2].speed_kmh, 0.0);
    }

    #[test]
    fn healthy_samples_keep_their_stored_distance() {
        let s = session(1, 0, 10);
        let p = build_points(
            &s,
            &[sample(1000, 3.0, 2.0, 0.0), sample(2000, 3.0, 2.0, 5.0)],
        );
        assert_eq!(p[1].distance_m, 5.0);
    }

    #[test]
    fn fewer_than_two_usable_samples_make_a_manual_activity() {
        let s = session(1, 0, 10);
        let one = plan_activity(
            &[(s.clone(), vec![sample(1000, 3.0, 0.0, 5.0)])],
            Naming::Walk,
            &Templates::default(),
        );
        assert_eq!(one.payload, Payload::Manual);
        let none = plan_activity(&[(s, vec![])], Naming::Walk, &Templates::default());
        assert_eq!(none.payload, Payload::Manual);
        assert_eq!(none.distance_m, 1000.0);
        assert_eq!(none.elapsed_s, 600);
    }

    #[test]
    fn two_samples_make_a_tcx() {
        let s = session(1, 0, 10);
        let plan = plan_activity(
            &[(
                s,
                vec![sample(1000, 3.0, 2.0, 0.0), sample(2000, 3.6, 2.0, 1.0)],
            )],
            Naming::Walk,
            &Templates::default(),
        );
        let Payload::Tcx(tcx) = plan.payload else {
            panic!("expected a TCX")
        };
        assert_eq!(tcx.matches("<Trackpoint>").count(), 2);
        assert_eq!(tcx.matches("<Lap ").count(), 1);
        assert!(tcx.contains("<Speed>1.00</Speed>"));
        assert_eq!(plan.name, "NEON STRIDER // 1.00 KM UPLINK");
        assert!(tcx.contains("<Notes>DECK_CYCLES 10:00"));
        assert_eq!(plan.end_ms, 10 * MIN);
    }

    #[test]
    fn heart_rate_goes_in_the_trackpoints_and_the_lap() {
        let s = session(1, 0, 10);
        let mut a = sample(1000, 3.0, 2.0, 0.0);
        a.heart_rate = Some(100);
        let b = sample(2000, 3.6, 2.0, 1.0); // no reading
        let mut c = sample(3000, 3.6, 2.0, 2.0);
        c.heart_rate = Some(121);
        let plan = plan_activity(&[(s, vec![a, b, c])], Naming::Walk, &Templates::default());
        let Payload::Tcx(tcx) = plan.payload else {
            panic!("expected a TCX")
        };
        assert_eq!(tcx.matches("<HeartRateBpm>").count(), 2);
        assert!(tcx.contains(
            "<DistanceMeters>0.00</DistanceMeters>
            <HeartRateBpm><Value>100</Value></HeartRateBpm>
            <Extensions>"
        ));
        // TCX order: Calories, then the lap's average and maximum, then Intensity.
        assert!(tcx.contains(
            "</Calories>
        <AverageHeartRateBpm><Value>111</Value></AverageHeartRateBpm>
        <MaximumHeartRateBpm><Value>121</Value></MaximumHeartRateBpm>
        <Intensity>"
        ));
    }

    #[test]
    fn without_heart_rate_the_tcx_has_none() {
        let s = session(1, 0, 10);
        let plan = plan_activity(
            &[(
                s,
                vec![sample(1000, 3.0, 2.0, 0.0), sample(2000, 3.6, 2.0, 1.0)],
            )],
            Naming::Walk,
            &Templates::default(),
        );
        let Payload::Tcx(tcx) = plan.payload else {
            panic!("expected a TCX")
        };
        assert!(!tcx.contains("HeartRate"));
    }

    #[test]
    fn rollup_makes_one_lap_per_session_with_continuing_distance() {
        let a = session(1, 0, 10);
        let b = session(2, 60, 10);
        let plan = plan_activity(
            &[
                (
                    b,
                    vec![sample(61_000, 3.0, 0.0, 0.0), sample(62_000, 3.0, 0.0, 4.0)],
                ),
                (
                    a,
                    vec![sample(1000, 3.0, 0.0, 0.0), sample(2000, 3.0, 0.0, 10.0)],
                ),
            ],
            Naming::Rollup { walks: 2 },
            &Templates::default(),
        );
        let Payload::Tcx(tcx) = plan.payload else {
            panic!("expected a TCX")
        };
        assert_eq!(tcx.matches("<Lap ").count(), 2);
        // Second lap continues from the first lap's 10 m.
        assert!(tcx.contains("<DistanceMeters>14.00</DistanceMeters>"));
        assert_eq!(
            plan.name,
            "NEON STRIDER // DAILY UPLINK \u{b7} 2 WALKS \u{b7} 2.00 KM"
        );
        assert_eq!(plan.start_ms, 0);
        assert_eq!(plan.distance_m, 2000.0);
    }

    #[test]
    fn tcx_escapes_the_description() {
        let laps = [Lap {
            start_ms: 0,
            total_time_s: 1,
            distance_m: 1.0,
            calories: 0.0,
            points: vec![],
        }];
        let tcx = generate_tcx(&laps, 0, "a & b <c> \"d\"");
        assert!(tcx.contains("<Notes>a &amp; b &lt;c&gt; &quot;d&quot;</Notes>"));
        assert!(tcx.contains("<Id>1970-01-01T00:00:00.000Z</Id>"));
    }

    #[test]
    fn templates_drive_the_name_and_description() {
        let mut a = session(1, 0, 10);
        a.avg_incline = 2.0;
        let mut b = session(2, 60, 30);
        b.avg_incline = 6.0;
        let t = Templates {
            title: "{walks} walk {distance_km}".to_string(),
            rollup_title: "{walks} walks {steps} steps".to_string(),
            description: "{duration} @ {avg_speed} km/h, {avg_incline}% {calories} kcal"
                .to_string(),
        };
        let walk = plan_activity(&[(a.clone(), vec![])], Naming::Walk, &t);
        assert_eq!(walk.name, "1 walk 1.00");
        assert_eq!(walk.description, "10:00 @ 6.0 km/h, 2.0% 40 kcal");
        let day = plan_activity(&[(a, vec![]), (b, vec![])], Naming::Rollup { walks: 2 }, &t);
        assert_eq!(day.name, "2 walks 3600 steps");
        // 10 min at 2% and 30 min at 6%: 5% by duration.
        assert_eq!(day.description, "40:00 @ 6.0 km/h, 5.0% 160 kcal");
    }

    #[test]
    fn the_description_note_in_the_tcx_is_the_rendered_template() {
        let t = Templates {
            description: "a & b".to_string(),
            ..Templates::default()
        };
        let plan = plan_activity(
            &[(
                session(1, 0, 10),
                vec![sample(1000, 3.0, 0.0, 0.0), sample(2000, 3.0, 0.0, 1.0)],
            )],
            Naming::Walk,
            &t,
        );
        let Payload::Tcx(tcx) = plan.payload else {
            panic!("expected a TCX")
        };
        assert!(tcx.contains("<Notes>a &amp; b</Notes>"));
    }

    // ---- scopes, mute, gear id ---------------------------------------------

    #[test]
    fn scopes_read_the_stored_grant() {
        let full = "read,activity:write,activity:read,profile:read_all";
        assert_eq!(
            scopes(Some(full)),
            Scopes {
                gear: true,
                list_activities: true
            }
        );
        assert!(!needs_reconnect(true, Some(full)));
        assert!(scopes(Some("activity:read_all,profile:read_all")).list_activities);
        // The old request, and a login that predates storing the scope.
        assert!(needs_reconnect(true, Some("activity:write,read")));
        assert!(needs_reconnect(true, None));
        assert!(needs_reconnect(
            true,
            Some("activity:write,profile:read_all")
        ));
        assert!(!needs_reconnect(false, None));
        assert!(scope_allows_upload(Some(REQUESTED_SCOPE)));
    }

    #[test]
    fn mute_mode_setting_round_trips() {
        for mode in [MuteMode::All, MuteMode::Short, MuteMode::None] {
            assert_eq!(parse_mute_mode(Some(mute_mode_setting(mode))), mode);
        }
        assert_eq!(mute_mode_setting(MuteMode::Short), "short");
    }

    #[test]
    fn mute_settings_parse_with_defaults() {
        assert_eq!(parse_mute_mode(None), MuteMode::All);
        assert_eq!(parse_mute_mode(Some("short")), MuteMode::Short);
        assert_eq!(parse_mute_mode(Some("\"none\"")), MuteMode::None);
        assert_eq!(parse_mute_mode(Some("loud")), MuteMode::All);
        assert_eq!(parse_mute_minutes(None), 20);
        assert_eq!(parse_mute_minutes(Some("35")), 35);
        assert_eq!(parse_mute_minutes(Some("0")), 1);
        assert_eq!(parse_mute_minutes(Some("9999")), 240);
        assert_eq!(parse_mute_minutes(Some("x")), 20);
    }

    #[test]
    fn mute_decision() {
        assert!(should_hide(MuteMode::All, 20, 5 * 3600));
        assert!(!should_hide(MuteMode::None, 20, 10));
        assert!(should_hide(MuteMode::Short, 20, 19 * 60 + 59));
        assert!(!should_hide(MuteMode::Short, 20, 20 * 60));
        assert!(!should_hide(MuteMode::Short, 20, 3600));
    }

    #[test]
    fn gear_ids_are_checked() {
        assert_eq!(clean_gear_id(None), Ok(None));
        assert_eq!(clean_gear_id(Some("  ")), Ok(None));
        assert_eq!(clean_gear_id(Some(" g123 ")), Ok(Some("g123".to_string())));
        assert!(clean_gear_id(Some("g1/../x")).is_err());
        assert!(clean_gear_id(Some(&"g".repeat(41))).is_err());
    }

    // ---- shoes and the activity update --------------------------------------

    #[test]
    fn shoes_come_from_the_athlete_reply() {
        let reply = json!({
            "id": 1,
            "shoes": [
                {"id": "g100", "primary": true, "name": "Cloudmonster", "distance": 123_456.0, "resource_state": 2},
                {"id": "g200", "primary": false, "name": " ", "nickname": "Old pair", "distance": 0, "retired": true},
                {"id": 300, "name": "Numeric id", "distance": 5_000},
                {"name": "no id"}
            ]
        });
        let shoes = parse_shoes(&reply).unwrap();
        assert_eq!(shoes.len(), 3);
        assert_eq!(shoes[0].id, "g100");
        assert!(shoes[0].primary && !shoes[0].retired);
        assert!((shoes[0].distance_km - 123.456).abs() < 1e-9);
        assert_eq!(shoes[1].name, "Old pair");
        assert!(shoes[1].retired);
        assert_eq!(shoes[2].id, "300");
        assert_eq!(parse_shoes(&json!({"shoes": []})).unwrap(), vec![]);
    }

    #[test]
    fn a_summary_athlete_without_shoes_is_an_error_not_an_empty_list() {
        assert!(parse_shoes(&json!({"id": 1, "firstname": "A"})).is_err());
        assert!(parse_shoes(&json!({"shoes": null})).is_err());
    }

    #[test]
    fn the_activity_update_only_exists_when_there_is_something_to_set() {
        assert_eq!(activity_update(None, false), None);
        assert_eq!(
            activity_update(Some("g1"), false),
            Some(json!({"trainer": true, "gear_id": "g1"}))
        );
        assert_eq!(
            activity_update(None, true),
            Some(json!({"trainer": true, "hide_from_home": true}))
        );
        assert_eq!(
            activity_update(Some("g1"), true),
            Some(json!({"trainer": true, "gear_id": "g1", "hide_from_home": true}))
        );
    }

    // ---- duplicates ---------------------------------------------------------

    fn existing(id: &str, start_min: i64, kind: &str, trainer: bool) -> Existing {
        Existing {
            id: id.to_string(),
            start_ms: start_min * MIN,
            kind: kind.to_string(),
            trainer,
        }
    }

    #[test]
    fn duplicate_window_is_ten_minutes_each_side_in_seconds() {
        assert_eq!(
            duplicate_window(100 * MIN, 130 * MIN),
            (100 * 60 - 600, 130 * 60 + 600)
        );
        // An end before the start (bad data) still gives a sane window.
        assert_eq!(
            duplicate_window(100 * MIN, 0),
            (100 * 60 - 600, 100 * 60 + 600)
        );
    }

    #[test]
    fn a_walk_starting_within_five_minutes_is_a_duplicate() {
        let list = [existing("1", 100, "Walk", false)];
        assert_eq!(
            find_duplicate(&list, 100 * MIN).map(|e| e.id.as_str()),
            Some("1")
        );
        assert!(find_duplicate(&list, 105 * MIN).is_some());
        assert!(find_duplicate(&list, 95 * MIN).is_some());
        assert!(find_duplicate(&list, 105 * MIN + 1).is_none());
        assert!(find_duplicate(&list, 94 * MIN).is_none());
    }

    #[test]
    fn only_on_foot_or_trainer_activities_count() {
        let start = 100 * MIN;
        for kind in ["Walk", "walk", "Run", "TrailRun", "VirtualRun", "Hike"] {
            assert!(
                find_duplicate(&[existing("1", 100, kind, false)], start).is_some(),
                "{kind}"
            );
        }
        assert!(find_duplicate(&[existing("1", 100, "Ride", false)], start).is_none());
        assert!(find_duplicate(&[existing("1", 100, "WeightTraining", false)], start).is_none());
        // Any sport marked trainer counts, whatever its type.
        assert!(find_duplicate(&[existing("1", 100, "Workout", true)], start).is_some());
        assert!(find_duplicate(&[], start).is_none());
    }

    #[test]
    fn the_closest_start_is_the_duplicate() {
        let list = [
            existing("far", 103, "Walk", false),
            existing("near", 101, "Run", false),
        ];
        assert_eq!(find_duplicate(&list, 100 * MIN).unwrap().id, "near");
    }

    #[test]
    fn activity_rows_parse_and_bad_rows_are_dropped() {
        let rows = json!([
            {"id": 11, "type": "Walk", "sport_type": "Walk", "trainer": true, "start_date": "2026-11-13T12:00:00Z", "start_date_local": "2026-11-13T07:00:00Z"},
            {"id": "12", "type": "Run", "start_date": "2026-11-13T12:30:00+00:00"},
            {"id": 13, "type": "Walk", "start_date": "yesterday"},
            {"type": "Walk", "start_date": "2026-11-13T12:00:00Z"}
        ]);
        let parsed = parse_activities(&rows);
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].id, "11");
        assert!(parsed[0].trainer);
        assert_eq!(parsed[0].start_ms, 1_794_571_200_000);
        assert_eq!(parsed[1].kind, "Run");
        assert_eq!(parsed[1].start_ms - parsed[0].start_ms, 30 * MIN);
        assert!(parse_activities(&json!({"message": "Authorization Error"})).is_empty());
    }
}
