//! CSV export of saved sessions.

use crate::persistence::Session;
use tauri::{AppHandle, Manager};
use tauri_plugin_opener::OpenerExt;

const CSV_HEADER: &str =
    "id,start_local,end_local,duration_s,distance_km,avg_speed_kmh,avg_incline_pct,calories,steps,strava_id,avg_heart_rate_bpm,max_heart_rate_bpm";

/// Quote a field when it holds a comma, quote, or line break (RFC 4180).
fn csv_field(value: &str) -> String {
    if value.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_string()
    }
}

/// Epoch milliseconds as local `YYYY-MM-DDTHH:MM:SS`.
fn local_iso(ms: i64) -> String {
    use chrono::TimeZone;
    match chrono::Local.timestamp_millis_opt(ms).single() {
        Some(dt) => dt.format("%Y-%m-%dT%H:%M:%S").to_string(),
        None => String::new(),
    }
}

/// One CSV line (no trailing newline). Distances are km (3 decimals), speed
/// km/h (2), incline % (1), calories 1 decimal, steps whole numbers, heart rate whole bpm (empty without one).
fn csv_row(s: &Session) -> String {
    let fields = [
        s.id.map(|id| id.to_string()).unwrap_or_default(),
        local_iso(s.start_time),
        local_iso(s.end_time),
        s.duration_seconds.to_string(),
        format!("{:.3}", s.distance_meters / 1000.0),
        format!("{:.2}", s.avg_speed),
        format!("{:.1}", s.avg_incline),
        format!("{:.1}", s.calories),
        format!("{:.0}", s.steps),
        s.strava_id.clone().unwrap_or_default(),
        s.avg_heart_rate.map(|b| b.to_string()).unwrap_or_default(),
        s.max_heart_rate.map(|b| b.to_string()).unwrap_or_default(),
    ];
    fields
        .iter()
        .map(|f| csv_field(f))
        .collect::<Vec<_>>()
        .join(",")
}

/// The whole file: header plus one line per session, CRLF line endings.
fn csv_document(sessions: &[Session]) -> String {
    let mut out = String::from(CSV_HEADER);
    out.push_str("\r\n");
    for s in sessions {
        out.push_str(&csv_row(s));
        out.push_str("\r\n");
    }
    out
}

/// Write every session to `<Downloads>/neon-strider-sessions-YYYY-MM-DD.csv`,
/// show it in the file manager, and return its path.
#[tauri::command]
pub async fn export_sessions_csv(app: AppHandle) -> Result<String, String> {
    let app_c = app.clone();
    let path = crate::util::blocking(move || {
        let sessions = crate::persistence::db_all_sessions(&app_c)?;
        let dir = app_c
            .path()
            .download_dir()
            .map_err(|e| format!("Could not find the Downloads folder: {}", e))?;
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let name = format!(
            "neon-strider-sessions-{}.csv",
            chrono::Local::now().format("%Y-%m-%d")
        );
        let path = dir.join(name);
        std::fs::write(&path, csv_document(&sessions))
            .map_err(|e| format!("Could not write {}: {}", path.display(), e))?;
        log::info!("exported {} sessions to {:?}", sessions.len(), path);
        Ok(path)
    })
    .await?;

    if let Err(e) = app.opener().reveal_item_in_dir(&path) {
        // The file is written; failing to show it is not a failed export.
        log::warn!("could not reveal {:?}: {}", path, e);
    }
    Ok(path.to_string_lossy().into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session() -> Session {
        Session {
            id: Some(7),
            start_time: 0,
            end_time: 0,
            duration_seconds: 1805,
            distance_meters: 2345.678,
            raw_distance_meters: 0.0,
            calories: 123.456,
            steps: 3456.6,
            raw_steps: 0.0,
            avg_speed: 4.6789,
            raw_avg_speed: 0.0,
            avg_incline: 1.25,
            strava_id: Some("12345".into()),
            neural_heat: 0.0,
            neural_ticks: 0,
            last_hardware_time: 0,
            hardware_time_offset: 0,
            avg_heart_rate: None,
            max_heart_rate: None,
        }
    }

    #[test]
    fn fields_are_quoted_only_when_needed() {
        assert_eq!(csv_field("plain"), "plain");
        assert_eq!(csv_field("a,b"), "\"a,b\"");
        assert_eq!(csv_field("say \"hi\""), "\"say \"\"hi\"\"\"");
        assert_eq!(csv_field("two\nlines"), "\"two\nlines\"");
        assert_eq!(csv_field(""), "");
    }

    #[test]
    fn row_uses_the_documented_precision() {
        let row = csv_row(&session());
        let cols: Vec<&str> = row.split(',').collect();
        assert_eq!(cols.len(), 12);
        assert_eq!(cols[0], "7");
        assert_eq!(cols[3], "1805");
        assert_eq!(cols[4], "2.346"); // km, 3 decimals
        assert_eq!(cols[5], "4.68"); // km/h, 2 decimals
        assert_eq!(cols[6], "1.2"); // %, 1 decimal (round-half-even of 1.25)
        assert_eq!(cols[7], "123.5");
        assert_eq!(cols[8], "3457");
        assert_eq!(cols[9], "12345");
        assert_eq!(cols[10], ""); // no heart rate saved
        assert_eq!(cols[11], "");
        // Local ISO time shape, whatever the machine's zone.
        assert_eq!(cols[1].len(), 19);
        assert_eq!(&cols[1][10..11], "T");
    }

    #[test]
    fn heart_rate_columns_come_last_as_whole_bpm() {
        assert!(CSV_HEADER.ends_with(",strava_id,avg_heart_rate_bpm,max_heart_rate_bpm"));
        let mut s = session();
        s.avg_heart_rate = Some(112);
        s.max_heart_rate = Some(131);
        let row = csv_row(&s);
        assert!(row.ends_with(",12345,112,131"));
        s.strava_id = None;
        assert!(csv_row(&s).ends_with(",,112,131"));
    }

    #[test]
    fn missing_values_become_empty_fields_and_odd_ids_are_quoted() {
        let mut s = session();
        s.id = None;
        s.strava_id = Some("a,\"b\"".into());
        let row = csv_row(&s);
        assert!(row.starts_with(','));
        assert!(row.ends_with(",\"a,\"\"b\"\"\",,"));
        s.strava_id = None;
        assert!(csv_row(&s).ends_with(",,,"));
    }

    #[test]
    fn document_has_header_and_crlf_lines() {
        let doc = csv_document(&[session(), session()]);
        let lines: Vec<&str> = doc.split("\r\n").collect();
        assert_eq!(lines[0], CSV_HEADER);
        assert_eq!(lines.len(), 4); // header, two rows, trailing empty
        assert_eq!(lines[3], "");
        assert_eq!(csv_document(&[]), format!("{}\r\n", CSV_HEADER));
    }
}
