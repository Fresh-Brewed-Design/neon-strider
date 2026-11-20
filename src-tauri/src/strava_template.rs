//! Pure text templates for the Strava title and description. No I/O, no clock
//! reads (the local start time comes in as a value), so all of it is unit-tested.
//!
//! Placeholders: `{distance_km}` `{duration}` `{avg_speed}` `{avg_incline}`
//! `{calories}` `{steps}` `{walks}` `{date}` `{time}`. Anything else in braces
//! is left exactly as typed.

use crate::util::format_duration;
use chrono::NaiveDateTime;

const DEFAULT_TITLE: &str = "NEON STRIDER // {distance_km} KM UPLINK";
const DEFAULT_ROLLUP_TITLE: &str =
    "NEON STRIDER // DAILY UPLINK \u{b7} {walks} WALKS \u{b7} {distance_km} KM";
const DEFAULT_DESCRIPTION: &str = "DECK_CYCLES {duration} \u{b7} DISPLACEMENT {distance_km} km \u{b7} ALTITUDE_BIAS {avg_incline}% \u{b7} METABOLIC_BURN {calories} kcal\nLogged by Neon Strider";

/// Strava titles are cut to this many characters.
/// Longer titles are cut in Strava's feed anyway.
const MAX_TITLE_CHARS: usize = 100;
/// Longest template text accepted when saving.
/// Room for placeholders that expand to fewer characters, but not a pasted page.
/// TemplateEditor's `maxLength` is the same.
pub const MAX_TITLE_TEMPLATE_CHARS: usize = 300;
/// A long description with every placeholder, well under Strava's own limit.
pub const MAX_DESCRIPTION_TEMPLATE_CHARS: usize = 2000;

/// What a template can show about one activity (a walk, or a day's rollup).
#[derive(Debug, Clone, PartialEq)]
pub struct Stats {
    pub distance_m: f64,
    pub duration_s: i64,
    pub avg_incline: f64,
    pub calories: f64,
    pub steps: f64,
    /// Merged walks in the activity (1 for a single walk).
    pub walks: usize,
    /// Local wall-clock start.
    pub start_local: NaiveDateTime,
}

/// The three templates in effect (never empty: blanks fall back to defaults).
#[derive(Debug, Clone, PartialEq)]
pub struct Templates {
    pub title: String,
    pub rollup_title: String,
    pub description: String,
}

impl Default for Templates {
    fn default() -> Self {
        Self {
            title: DEFAULT_TITLE.to_string(),
            rollup_title: DEFAULT_ROLLUP_TITLE.to_string(),
            description: DEFAULT_DESCRIPTION.to_string(),
        }
    }
}

impl Templates {
    /// Build from stored values; a missing or blank one means "use the default".
    pub fn from_stored(
        title: Option<&str>,
        rollup_title: Option<&str>,
        description: Option<&str>,
    ) -> Self {
        let pick = |v: Option<&str>, default: &str| match v {
            Some(s) if !s.trim().is_empty() => s.to_string(),
            _ => default.to_string(),
        };
        Self {
            title: pick(title, DEFAULT_TITLE),
            rollup_title: pick(rollup_title, DEFAULT_ROLLUP_TITLE),
            description: pick(description, DEFAULT_DESCRIPTION),
        }
    }
}

fn avg_speed_kmh(stats: &Stats) -> f64 {
    if stats.duration_s > 0 {
        stats.distance_m / 1000.0 / (stats.duration_s as f64 / 3600.0)
    } else {
        0.0
    }
}

fn value(name: &str, stats: &Stats) -> Option<String> {
    Some(match name {
        "distance_km" => format!("{:.2}", stats.distance_m / 1000.0),
        "duration" => format_duration(stats.duration_s),
        "avg_speed" => format!("{:.1}", avg_speed_kmh(stats)),
        "avg_incline" => format!("{:.1}", stats.avg_incline),
        "calories" => format!("{:.0}", stats.calories.max(0.0)),
        "steps" => format!("{:.0}", stats.steps.max(0.0)),
        "walks" => stats.walks.to_string(),
        "date" => stats.start_local.format("%Y-%m-%d").to_string(),
        "time" => stats.start_local.format("%H:%M").to_string(),
        _ => return None,
    })
}

/// Fill in the placeholders. Unknown or unclosed ones stay as typed.
fn render(template: &str, stats: &Stats) -> String {
    let mut out = String::with_capacity(template.len() + 16);
    let mut rest = template;
    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        let known = after
            .find('}')
            .and_then(|close| value(&after[..close], stats).map(|v| (v, close)));
        match known {
            Some((v, close)) => {
                out.push_str(&v);
                rest = &after[close + 1..];
            }
            None => {
                out.push('{');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

/// A rendered title: one line, at most `MAX_TITLE_CHARS` characters.
pub fn render_title(template: &str, stats: &Stats) -> String {
    let one_line = render(template, stats).replace(['\r', '\n'], " ");
    let trimmed = one_line.trim();
    trimmed
        .chars()
        .take(MAX_TITLE_CHARS)
        .collect::<String>()
        .trim_end()
        .to_string()
}

pub fn render_description(template: &str, stats: &Stats) -> String {
    render(template, stats).trim().to_string()
}

/// A walk for the settings preview.
pub fn sample_stats() -> Stats {
    Stats {
        distance_m: 3420.0,
        duration_s: 3_725,
        avg_incline: 4.5,
        calories: 214.0,
        steps: 4_980.0,
        walks: 3,
        start_local: chrono::NaiveDate::from_ymd_opt(2026, 11, 13)
            .and_then(|d| d.and_hms_opt(7, 45, 0))
            .unwrap_or_default(),
    }
}

/// Validate a template before saving (empty is allowed: it means "default").
pub fn check_length(template: &str, max_chars: usize) -> Result<(), String> {
    if template.chars().count() > max_chars {
        return Err(format!(
            "That template is too long (limit {} characters)",
            max_chars
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn stats() -> Stats {
        Stats {
            distance_m: 1500.0,
            duration_s: 1800,
            avg_incline: 4.04,
            calories: 98.6,
            steps: 2100.4,
            walks: 2,
            start_local: NaiveDate::from_ymd_opt(2026, 11, 3)
                .unwrap()
                .and_hms_opt(7, 5, 59)
                .unwrap(),
        }
    }

    #[test]
    fn duration_is_h_mm_ss_or_m_ss() {
        assert_eq!(format_duration(0), "0:00");
        assert_eq!(format_duration(59), "0:59");
        assert_eq!(format_duration(1934), "32:14");
        assert_eq!(format_duration(3599), "59:59");
        assert_eq!(format_duration(3600), "1:00:00");
        assert_eq!(format_duration(3725), "1:02:05");
        assert_eq!(format_duration(-5), "0:00");
    }

    #[test]
    fn every_placeholder_renders() {
        let t = "{distance_km}|{duration}|{avg_speed}|{avg_incline}|{calories}|{steps}|{walks}|{date}|{time}";
        assert_eq!(
            render(t, &stats()),
            "1.50|30:00|3.0|4.0|99|2100|2|2026-11-03|07:05"
        );
    }

    #[test]
    fn unknown_and_broken_placeholders_stay_as_typed() {
        let s = stats();
        assert_eq!(render("{nope} {distance_km", &s), "{nope} {distance_km");
        assert_eq!(render("{{distance_km}}", &s), "{1.50}");
        assert_eq!(render("} { {} {Distance_km}", &s), "} { {} {Distance_km}");
        assert_eq!(render("no braces", &s), "no braces");
        assert_eq!(render("", &s), "");
    }

    #[test]
    fn non_ascii_text_survives() {
        assert_eq!(
            render("\u{b7} {steps} \u{2192} {nope}\u{e9}", &stats()),
            "\u{b7} 2100 \u{2192} {nope}\u{e9}"
        );
    }

    #[test]
    fn zero_duration_has_zero_speed() {
        let mut s = stats();
        s.duration_s = 0;
        assert_eq!(render("{avg_speed}", &s), "0.0");
    }

    #[test]
    fn title_is_one_line_and_capped() {
        let s = stats();
        assert_eq!(render_title("a\nb {walks}", &s), "a b 2");
        let long = "x".repeat(250);
        assert_eq!(render_title(&long, &s).chars().count(), MAX_TITLE_CHARS);
        // The cap counts characters, not bytes.
        let wide = "\u{e9}".repeat(150);
        assert_eq!(render_title(&wide, &s).chars().count(), MAX_TITLE_CHARS);
    }

    #[test]
    fn default_templates_render_in_the_house_voice() {
        let s = stats();
        assert_eq!(
            render_title(DEFAULT_TITLE, &s),
            "NEON STRIDER // 1.50 KM UPLINK"
        );
        assert_eq!(
            render_title(DEFAULT_ROLLUP_TITLE, &s),
            "NEON STRIDER // DAILY UPLINK \u{b7} 2 WALKS \u{b7} 1.50 KM"
        );
        assert_eq!(
            render_description(DEFAULT_DESCRIPTION, &s),
            "DECK_CYCLES 30:00 \u{b7} DISPLACEMENT 1.50 km \u{b7} ALTITUDE_BIAS 4.0% \u{b7} METABOLIC_BURN 99 kcal\nLogged by Neon Strider"
        );
    }

    #[test]
    fn blank_stored_templates_mean_defaults() {
        let t = Templates::from_stored(Some("  "), None, Some("custom {steps}"));
        assert_eq!(t.title, DEFAULT_TITLE);
        assert_eq!(t.rollup_title, DEFAULT_ROLLUP_TITLE);
        assert_eq!(t.description, "custom {steps}");
    }

    #[test]
    fn long_templates_are_refused() {
        assert!(check_length(&"a".repeat(300), MAX_TITLE_TEMPLATE_CHARS).is_ok());
        assert!(check_length(&"a".repeat(301), MAX_TITLE_TEMPLATE_CHARS).is_err());
    }
}
