//! The ranges and caps of the settings the dashboard edits, gathered from the
//! modules that enforce them. `cargo test` writes them to
//! `src/ipc/bindings/SETTING_LIMITS.ts` as `SETTING_LIMITS`, so the dashboard's
//! fields use Rust's numbers instead of copies, and CI fails if the file falls
//! behind a change here.

use serde::{Deserialize, Serialize};

/// An inclusive range of a numeric setting.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct SettingRange {
    pub min: f32,
    pub max: f32,
}

const fn range(min: f32, max: f32) -> SettingRange {
    SettingRange { min, max }
}

/// Every range a dashboard field offers, as Rust enforces it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct SettingLimits {
    // ---- Treadmill (persistence.rs) ----
    pub min_speed_kmh: SettingRange,
    pub max_speed_kmh: SettingRange,
    pub max_incline_level: SettingRange,
    pub tilt_multiplier: SettingRange,
    pub incline_offset_pct: SettingRange,
    pub body_mass_kg: SettingRange,
    // ---- Neural flow (neural.rs) ----
    pub neural_floor_pct: SettingRange,
    // ---- Speed drift (speed_drift.rs) ----
    /// Either end of the band, calibrated km/h.
    pub speed_drift_kmh: SettingRange,
    /// How far the fastest speed may sit above the slowest.
    pub speed_drift_width_kmh: SettingRange,
    pub speed_drift_minutes: SettingRange,
    // ---- HEART_LINK (heart_rate/worker.rs) ----
    pub max_heart_rate_bpm: SettingRange,
    /// The max heart rate zones use until one is set (`DEFAULT_MAX_BPM`).
    pub default_max_heart_rate_bpm: u32,
    // ---- Desk (snapshot/seated.rs, eyebreak.rs, walkaway.rs, calls.rs) ----
    pub seated_minutes: SettingRange,
    pub seated_program_buttons: u32,
    pub eye_break_minutes: SettingRange,
    pub eye_break_seconds: SettingRange,
    pub walkaway_idle_minutes: SettingRange,
    pub calls_quiet_kmh: SettingRange,
    // ---- Belt reminder (snapshot/maintenance.rs) ----
    pub maintenance_km: SettingRange,
    pub maintenance_days: SettingRange,
    pub maintenance_hours: SettingRange,
    // ---- Strava and History (strava_plan.rs, strava_template.rs) ----
    pub strava_mute_minutes: SettingRange,
    pub session_merge_minutes: SettingRange,
    pub title_template_chars: u32,
    pub description_template_chars: u32,
    /// Where Strava titles are cut, after the placeholders are filled in.
    pub title_chars: u32,
    // ---- Programs (programs.rs) ----
    pub program_name_chars: u32,
    pub program_steps: u32,
    /// A cool-down step's heart rate end, in % of maximum heart rate (#233).
    pub cool_down_pulse_pct: SettingRange,
    /// The heart rate end a cool-down step starts with in the editor.
    pub default_cool_down_pulse_pct: f32,
    // ---- Diagnostics (protocol.rs) ----
    /// The longest body the command cycler may send (`protocol::RAW_BODY_MAX`).
    pub raw_body_bytes: u32,
}

impl Default for SettingLimits {
    fn default() -> Self {
        use crate::persistence as p;
        use crate::snapshot::{calls, eyebreak, maintenance as m, seated, walkaway};
        Self {
            min_speed_kmh: range(p::MIN_SPEED_SETTING_KMH, p::MAX_SPEED_SETTING_KMH),
            max_speed_kmh: range(p::MIN_TOP_SPEED_KMH, p::MAX_SPEED_SETTING_KMH),
            max_incline_level: range(0.0, f32::from(crate::engine::INCLINE_LEVEL_LIMIT)),
            tilt_multiplier: range(p::MIN_TILT_MULTIPLIER, p::MAX_TILT_MULTIPLIER),
            incline_offset_pct: range(p::MIN_INCLINE_OFFSET_PCT, p::MAX_INCLINE_OFFSET_PCT),
            body_mass_kg: range(p::MIN_BODY_MASS_KG, p::MAX_BODY_MASS_KG),
            neural_floor_pct: range(crate::neural::MIN_FLOOR_PCT, crate::neural::MAX_FLOOR_PCT),
            speed_drift_kmh: range(
                crate::speed_drift::MIN_BAND_KMH,
                crate::speed_drift::MAX_BAND_KMH,
            ),
            speed_drift_width_kmh: range(
                crate::speed_drift::MIN_BAND_WIDTH_KMH,
                crate::speed_drift::MAX_BAND_WIDTH_KMH,
            ),
            speed_drift_minutes: range(
                crate::speed_drift::MIN_EVERY_MINUTES as f32,
                crate::speed_drift::MAX_EVERY_MINUTES as f32,
            ),
            max_heart_rate_bpm: range(
                f32::from(crate::heart_rate::MIN_MAX_BPM),
                f32::from(crate::heart_rate::MAX_MAX_BPM),
            ),
            default_max_heart_rate_bpm: u32::from(crate::heart_rate::DEFAULT_MAX_BPM),
            seated_minutes: range(seated::MIN_MINUTES as f32, seated::MAX_MINUTES as f32),
            seated_program_buttons: seated::MAX_PROGRAM_BUTTONS as u32,
            eye_break_minutes: range(eyebreak::MIN_MINUTES as f32, eyebreak::MAX_MINUTES as f32),
            eye_break_seconds: range(eyebreak::MIN_SECONDS as f32, eyebreak::MAX_SECONDS as f32),
            walkaway_idle_minutes: range(
                walkaway::MIN_IDLE_MINUTES as f32,
                walkaway::MAX_IDLE_MINUTES as f32,
            ),
            calls_quiet_kmh: range(calls::MIN_QUIET_KMH, calls::MAX_QUIET_KMH),
            maintenance_km: range(m::MIN_INTERVAL_KM, m::MAX_INTERVAL_KM),
            maintenance_days: range(0.0, m::MAX_INTERVAL_DAYS as f32),
            maintenance_hours: range(m::MIN_INTERVAL_HOURS, m::MAX_INTERVAL_HOURS),
            strava_mute_minutes: range(
                crate::strava_plan::MIN_MUTE_MINUTES as f32,
                crate::strava_plan::MAX_MUTE_MINUTES as f32,
            ),
            session_merge_minutes: range(
                crate::strava_plan::MIN_MERGE_MINUTES as f32,
                crate::strava_plan::MAX_MERGE_MINUTES as f32,
            ),
            title_template_chars: crate::strava_template::MAX_TITLE_TEMPLATE_CHARS as u32,
            description_template_chars: crate::strava_template::MAX_DESCRIPTION_TEMPLATE_CHARS
                as u32,
            title_chars: crate::strava_template::MAX_TITLE_CHARS as u32,
            program_name_chars: crate::programs::MAX_NAME_CHARS as u32,
            program_steps: crate::programs::MAX_STEPS as u32,
            cool_down_pulse_pct: range(
                crate::programs::PULSE_END_MIN_PCT,
                crate::programs::PULSE_END_MAX_PCT,
            ),
            default_cool_down_pulse_pct: crate::programs::PULSE_END_DEFAULT_PCT,
            raw_body_bytes: crate::protocol::RAW_BODY_MAX as u32,
        }
    }
}

/// The generated file's text: `SETTING_LIMITS` typed by the ts-rs binding.
pub fn typescript() -> String {
    constant_module(
        "limits.rs",
        "SettingLimits",
        "SETTING_LIMITS",
        &SettingLimits::default(),
    )
}

/// A TypeScript module that exports `value` as `name`, typed by the ts-rs
/// binding `ty` beside it. `source` names the Rust file it comes from.
pub(crate) fn constant_module(
    source: &str,
    ty: &str,
    name: &str,
    value: &impl Serialize,
) -> String {
    let json = serde_json::to_string_pretty(value).expect("the constant serializes");
    format!(
        "// Generated by `cargo test` from src-tauri/src/{source}. Do not edit this file manually.\n\
         import type {{ {ty} }} from \"./{ty}\";\n\
         \n\
         export const {name}: {ty} = {json};\n"
    )
}

/// Writes a generated module into `src/ipc/bindings/`, as the ts-rs exports do.
#[cfg(test)]
pub(crate) fn write_binding(file: &str, text: &str) {
    let dir = std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../src/ipc/bindings"));
    std::fs::create_dir_all(dir).expect("bindings folder");
    std::fs::write(dir.join(file), text).unwrap_or_else(|e| panic!("write {file}: {e}"));
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Writes the TypeScript constants beside the ts-rs bindings, as their
    /// export does: CI clears the folder, runs the tests and fails on a diff.
    #[test]
    fn export_setting_limits_values() {
        write_binding("SETTING_LIMITS.ts", &typescript());
    }

    #[test]
    fn every_range_runs_low_to_high() {
        let json = serde_json::to_value(SettingLimits::default()).unwrap();
        for (name, v) in json.as_object().unwrap() {
            if let (Some(min), Some(max)) = (v.get("min"), v.get("max")) {
                assert!(min.as_f64() < max.as_f64(), "{name}: {v}");
            }
        }
    }
}
