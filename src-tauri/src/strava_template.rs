//! Pure text templates for the Strava title and description. No I/O, no clock
//! reads (the local times come in as values), so all of it is unit-tested.
//!
//! Each field in `FIELDS` is a placeholder such as `{distance_km}`. Anything
//! else in braces is left exactly as typed.
//!
//! A field can have no value for a walk (no heart rate sensor, no program, no
//! samples). Such a field renders as nothing, and the text around it collapses:
//!
//! - A line splits into segments at separators: ` · `, ` | `, ` / `, ` // `,
//!   ` - `, ` • ` (a run of those symbols with a space on both sides) and
//!   `, ` or `; `.
//! - A segment whose fields are all empty goes, with its label and unit, and
//!   so does one separator: the one before it, or the one after it when it is
//!   the line's first. `A · PULSE {avg_heart_rate} bpm · B` becomes `A · B`.
//! - A line left with nothing goes too, so no blank line is added.
//! - In a segment that keeps a value, an empty field is blank: brackets
//!   wrapped round it go, and the space it leaves closes up.
//!
//! `cargo test` writes `FIELDS` to `src/ipc/bindings/TEMPLATE_FIELDS.ts` for
//! the template help text.

use crate::util::format_duration;
use chrono::NaiveDateTime;
use serde::Serialize;

const DEFAULT_TITLE: &str = "NEON STRIDER // {distance_km} KM UPLINK";
const DEFAULT_ROLLUP_TITLE: &str =
    "NEON STRIDER // DAILY UPLINK \u{b7} {walks} WALKS \u{b7} {distance_km} KM";
const DEFAULT_DESCRIPTION: &str = "DECK_CYCLES {duration} \u{b7} DISPLACEMENT {distance_km} km \u{b7} ALTITUDE_BIAS {avg_incline}% \u{b7} METABOLIC_BURN {calories} kcal\nLogged by Neon Strider";

/// Strava titles are cut to this many characters.
/// Longer titles are cut in Strava's feed anyway.
pub const MAX_TITLE_CHARS: usize = 100;
/// Longest template text accepted when saving.
/// Room for placeholders that expand to fewer characters, but not a pasted page.
/// TemplateEditor reads it from `SettingLimits`.
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
    /// Local wall-clock end of the last walk.
    pub end_local: NaiveDateTime,
    /// The programs run, joined with " + "; `None` without one.
    pub program: Option<String>,
    /// Beats per minute over the samples with a reading; `None` without any.
    pub avg_heart_rate: Option<u16>,
    pub max_heart_rate: Option<u16>,
    /// Fastest sample in km/h; `None` without samples.
    pub max_speed: Option<f64>,
    /// Steepest sample in %; `None` without samples.
    pub max_incline: Option<f64>,
    /// Metres climbed, the TCX altitude gain; `None` without samples.
    pub climb_m: Option<f64>,
    /// PC time walked on the walk's day, whole %; `None` until that day has
    /// 10 minutes of PC time (`pc_walk_share::share_pct`).
    pub pc_walked_pct: Option<u32>,
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

/// Every template field in the order the help text lists them, with what it
/// shows.
pub const FIELDS: &[(&str, &str)] = &[
    ("distance_km", "Distance in km."),
    ("duration", "Walking time."),
    ("avg_speed", "Average speed in km/h."),
    ("max_speed", "Top speed in km/h."),
    ("avg_incline", "Average incline in %."),
    ("max_incline", "Steepest incline in %."),
    ("climb_m", "Metres climbed."),
    ("calories", "Calories burned in kcal."),
    ("steps", "Step count."),
    ("avg_heart_rate", "Average heart rate in bpm."),
    ("max_heart_rate", "Highest heart rate in bpm."),
    ("program", "Name of the program walked."),
    ("pc_walked", "Share of the day's PC time walked, in %."),
    ("walks", "Walks joined into the activity."),
    ("date", "Start date."),
    ("time", "Start time."),
    ("end_time", "End time."),
];

/// A field's text: `Some(None)` for a field with no value for this walk,
/// `None` for a name that is not a field.
fn value(name: &str, stats: &Stats) -> Option<Option<String>> {
    Some(match name {
        "distance_km" => Some(format!("{:.2}", stats.distance_m / 1000.0)),
        "duration" => Some(format_duration(stats.duration_s)),
        "avg_speed" => Some(format!("{:.1}", avg_speed_kmh(stats))),
        "max_speed" => stats.max_speed.map(|v| format!("{:.1}", v.max(0.0))),
        "avg_incline" => Some(format!("{:.1}", stats.avg_incline)),
        "max_incline" => stats.max_incline.map(|v| format!("{:.1}", v)),
        "climb_m" => stats.climb_m.map(|v| format!("{:.0}", v.max(0.0))),
        "calories" => Some(format!("{:.0}", stats.calories.max(0.0))),
        "steps" => Some(format!("{:.0}", stats.steps.max(0.0))),
        "avg_heart_rate" => stats.avg_heart_rate.map(|b| b.to_string()),
        "max_heart_rate" => stats.max_heart_rate.map(|b| b.to_string()),
        "program" => stats
            .program
            .as_deref()
            .map(str::trim)
            .filter(|p| !p.is_empty())
            .map(str::to_string),
        "pc_walked" => stats.pc_walked_pct.map(|p| p.to_string()),
        "walks" => Some(stats.walks.to_string()),
        "date" => Some(stats.start_local.format("%Y-%m-%d").to_string()),
        "time" => Some(stats.start_local.format("%H:%M").to_string()),
        "end_time" => Some(stats.end_local.format("%H:%M").to_string()),
        _ => return None,
    })
}

/// One piece of a template line.
#[derive(Debug, Clone, PartialEq)]
enum Piece<'a> {
    Text(&'a str),
    /// A field's value; `None` when the walk has none.
    Field(Option<String>),
    Sep(&'a str),
}

/// Symbols that separate segments with a space on both sides: the middle dot
/// the default templates use, and the ones people type between stats.
const SEP_SYMBOLS: &[char] = &['\u{b7}', '|', '/', '-', '\u{2022}'];

fn spaces(s: &str) -> usize {
    s.chars().take_while(|c| *c == ' ' || *c == '\t').count()
}

/// Length in bytes of the separator starting at `text[i..]`, if one does.
fn separator_len(text: &str, i: usize) -> Option<usize> {
    let rest = &text[i..];
    let lead = spaces(rest);
    let after_lead = &rest[lead..];
    // `, ` and `; `, with or without a space before.
    if after_lead.starts_with([',', ';']) {
        let tail = spaces(&after_lead[1..]);
        return (tail > 0).then_some(lead + 1 + tail);
    }
    if lead == 0 {
        return None;
    }
    let symbols: usize = after_lead
        .chars()
        .take_while(|c| SEP_SYMBOLS.contains(c))
        .map(char::len_utf8)
        .sum();
    if symbols == 0 {
        return None;
    }
    let tail = spaces(&after_lead[symbols..]);
    (tail > 0).then_some(lead + symbols + tail)
}

/// Literal text split at its separators.
fn split_text<'a>(text: &'a str, out: &mut Vec<Piece<'a>>) {
    let mut start = 0;
    let mut i = 0;
    while i < text.len() {
        if let Some(len) = separator_len(text, i) {
            if i > start {
                out.push(Piece::Text(&text[start..i]));
            }
            out.push(Piece::Sep(&text[i..i + len]));
            i += len;
            start = i;
        } else {
            i += text[i..].chars().next().map_or(1, char::len_utf8);
        }
    }
    if start < text.len() {
        out.push(Piece::Text(&text[start..]));
    }
}

/// One template line as pieces. Unknown or unclosed placeholders stay text.
fn parse_line<'a>(line: &'a str, stats: &Stats) -> Vec<Piece<'a>> {
    let mut out = Vec::new();
    let mut rest = line;
    let mut from = 0;
    while let Some(open) = rest[from..].find('{').map(|o| o + from) {
        let after = &rest[open + 1..];
        let known = after
            .find('}')
            .and_then(|close| value(&after[..close], stats).map(|v| (v, close)));
        match known {
            Some((v, close)) => {
                split_text(&rest[..open], &mut out);
                out.push(Piece::Field(v));
                rest = &after[close + 1..];
                from = 0;
            }
            None => from = open + 1,
        }
    }
    split_text(rest, &mut out);
    out
}

/// A kept segment: an empty field renders as nothing, brackets wrapped round
/// it go, and the space it leaves closes up.
fn render_segment(pieces: &[Piece]) -> String {
    let mut out = String::new();
    let mut had_empty = false;
    let mut skip_space = false;
    let mut skip_closer: Option<char> = None;
    for (i, piece) in pieces.iter().enumerate() {
        match piece {
            Piece::Text(text) => {
                let mut t: &str = text;
                if let Some(rest) = skip_closer.take().and_then(|c| t.strip_prefix(c)) {
                    t = rest;
                }
                if skip_space {
                    t = t.trim_start();
                }
                skip_space = false;
                out.push_str(t);
            }
            Piece::Field(Some(v)) => {
                skip_space = false;
                out.push_str(v);
            }
            Piece::Field(None) => {
                had_empty = true;
                let closer = match out.trim_end().chars().last() {
                    Some('(') => Some(')'),
                    Some('[') => Some(']'),
                    _ => None,
                };
                if let (Some(c), Some(Piece::Text(next))) = (closer, pieces.get(i + 1)) {
                    if next.starts_with(c) {
                        let keep = out.trim_end().len() - 1;
                        out.truncate(keep);
                        skip_closer = Some(c);
                    }
                }
                skip_space = out.is_empty() || out.ends_with(char::is_whitespace);
            }
            Piece::Sep(s) => out.push_str(s),
        }
    }
    if had_empty {
        out.truncate(out.trim_end().len());
    }
    out
}

/// One line with its empty segments dropped; `None` when a line that had
/// fields has nothing left.
fn render_line(pieces: &[Piece]) -> Option<String> {
    // Each segment with the separator before it.
    let mut segments: Vec<(&str, Vec<Piece>)> = vec![("", Vec::new())];
    for piece in pieces {
        match piece {
            Piece::Sep(s) => segments.push((*s, Vec::new())),
            other => {
                if let Some((_, seg)) = segments.last_mut() {
                    seg.push(other.clone());
                }
            }
        }
    }
    let total = segments.len();
    let kept: Vec<(&str, Vec<Piece>)> = segments
        .into_iter()
        .filter(|(_, seg)| {
            let empty = seg.iter().any(|p| matches!(p, Piece::Field(None)));
            let filled = seg.iter().any(|p| matches!(p, Piece::Field(Some(_))));
            !empty || filled
        })
        .collect();
    let mut out = String::new();
    for (n, (sep, seg)) in kept.iter().enumerate() {
        if n > 0 {
            out.push_str(sep);
        }
        out.push_str(&render_segment(seg));
    }
    if kept.len() < total && out.trim().is_empty() {
        return None;
    }
    Some(out)
}

/// Fill in the fields, collapsing the text round empty ones (see the module
/// notes).
fn render(template: &str, stats: &Stats) -> String {
    template
        .split('\n')
        .filter_map(|line| render_line(&parse_line(line, stats)))
        .collect::<Vec<_>>()
        .join("\n")
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

/// A walk for the settings preview, with a value for every field.
pub fn sample_stats() -> Stats {
    let day = chrono::NaiveDate::from_ymd_opt(2026, 11, 13).unwrap_or_default();
    let at = |h, m| day.and_hms_opt(h, m, 0).unwrap_or_default();
    Stats {
        distance_m: 3420.0,
        duration_s: 3_725,
        avg_incline: 4.5,
        calories: 214.0,
        steps: 4_980.0,
        walks: 3,
        start_local: at(7, 45),
        end_local: at(9, 5),
        program: Some("Brisk".to_string()),
        avg_heart_rate: Some(118),
        max_heart_rate: Some(134),
        max_speed: Some(4.2),
        max_incline: Some(6.0),
        climb_m: Some(154.0),
        pc_walked_pct: Some(38),
    }
}

/// One field for the template help text.
#[derive(Debug, Clone, PartialEq, Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct TemplateField {
    /// The placeholder without braces, such as `distance_km`.
    pub name: String,
    /// What it shows, one short sentence.
    pub about: String,
    /// Its text for the sample walk.
    pub sample: String,
}

/// `FIELDS` with each one's sample text.
pub fn template_fields() -> Vec<TemplateField> {
    let stats = sample_stats();
    FIELDS
        .iter()
        .map(|(name, about)| TemplateField {
            name: name.to_string(),
            about: about.to_string(),
            sample: value(name, &stats).flatten().unwrap_or_default(),
        })
        .collect()
}

/// The generated `TEMPLATE_FIELDS.ts`.
pub fn typescript() -> String {
    let json = serde_json::to_string_pretty(&template_fields()).expect("the fields serialize");
    format!(
        "// Generated by `cargo test` from src-tauri/src/strava_template.rs. Do not edit this file manually.\n\
         import type {{ TemplateField }} from \"./TemplateField\";\n\
         \n\
         export const TEMPLATE_FIELDS: TemplateField[] = {json};\n"
    )
}

/// Validate a template before saving (empty is allowed: it means "default").
pub fn check_length(template: &str, max_chars: usize) -> Result<(), String> {
    if template.chars().count() > max_chars {
        return Err(format!(
            "Template is too long (limit {} characters)",
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
        let day = NaiveDate::from_ymd_opt(2026, 11, 3).unwrap();
        Stats {
            distance_m: 1500.0,
            duration_s: 1800,
            avg_incline: 4.04,
            calories: 98.6,
            steps: 2100.4,
            walks: 2,
            start_local: day.and_hms_opt(7, 5, 59).unwrap(),
            end_local: day.and_hms_opt(7, 41, 2).unwrap(),
            program: Some("VO2 max".to_string()),
            avg_heart_rate: Some(121),
            max_heart_rate: Some(148),
            max_speed: Some(5.04),
            max_incline: Some(8.0),
            climb_m: Some(60.4),
            pc_walked_pct: Some(42),
        }
    }

    /// A walk with nothing optional: no program, no heart rate, no samples,
    /// too little PC time for a share.
    fn bare() -> Stats {
        Stats {
            program: None,
            avg_heart_rate: None,
            max_heart_rate: None,
            max_speed: None,
            max_incline: None,
            climb_m: None,
            pc_walked_pct: None,
            ..stats()
        }
    }

    fn every_field() -> String {
        FIELDS
            .iter()
            .map(|(name, _)| format!("{{{name}}}"))
            .collect::<Vec<_>>()
            .join("|")
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
    fn every_field_renders() {
        // "|" without spaces round it is not a separator.
        assert_eq!(
            render(&every_field(), &stats()),
            "1.50|30:00|3.0|5.0|4.0|8.0|60|99|2100|121|148|VO2 max|42|2|2026-11-03|07:05|07:41"
        );
    }

    #[test]
    fn each_field_alone() {
        let s = stats();
        for (name, want) in [
            ("distance_km", "1.50"),
            ("duration", "30:00"),
            ("avg_speed", "3.0"),
            ("max_speed", "5.0"),
            ("avg_incline", "4.0"),
            ("max_incline", "8.0"),
            ("climb_m", "60"),
            ("calories", "99"),
            ("steps", "2100"),
            ("avg_heart_rate", "121"),
            ("max_heart_rate", "148"),
            ("program", "VO2 max"),
            ("pc_walked", "42"),
            ("walks", "2"),
            ("date", "2026-11-03"),
            ("time", "07:05"),
            ("end_time", "07:41"),
        ] {
            assert_eq!(
                render(&format!("x {{{name}}} y"), &s),
                format!("x {want} y")
            );
        }
    }

    #[test]
    fn the_fields_without_a_value_for_a_bare_walk() {
        let s = bare();
        let empty: Vec<&str> = FIELDS
            .iter()
            .map(|(name, _)| *name)
            .filter(|name| value(name, &s) == Some(None))
            .collect();
        assert_eq!(
            empty,
            [
                "max_speed",
                "max_incline",
                "climb_m",
                "avg_heart_rate",
                "max_heart_rate",
                "program",
                "pc_walked"
            ]
        );
        // Each one alone leaves nothing.
        for name in empty {
            assert_eq!(render(&format!("{{{name}}}"), &s), "", "{name}");
            assert_eq!(render_title(&format!("LABEL {{{name}}} unit"), &s), "");
        }
        // A blank program name counts as none.
        let mut blank = bare();
        blank.program = Some("  ".to_string());
        assert_eq!(render("{program}", &blank), "");
    }

    #[test]
    fn zero_values_still_show() {
        let mut s = stats();
        s.climb_m = Some(0.0);
        s.pc_walked_pct = Some(0);
        s.max_incline = Some(0.0);
        assert_eq!(
            render("{climb_m} m, {pc_walked} %, {max_incline} %", &s),
            "0 m, 0 %, 0.0 %"
        );
    }

    #[test]
    fn an_empty_segment_goes_with_one_separator() {
        let s = bare();
        let t = "DECK_CYCLES {duration} \u{b7} PULSE {avg_heart_rate} bpm \u{b7} METABOLIC_BURN {calories} kcal";
        assert_eq!(
            render(t, &s),
            "DECK_CYCLES 30:00 \u{b7} METABOLIC_BURN 99 kcal"
        );
        assert_eq!(
            render(t, &stats()),
            "DECK_CYCLES 30:00 \u{b7} PULSE 121 bpm \u{b7} METABOLIC_BURN 99 kcal"
        );
        // First, last, and two in a row.
        assert_eq!(render("{program} | {distance_km} km", &s), "1.50 km");
        assert_eq!(render("{distance_km} km | {program}", &s), "1.50 km");
        assert_eq!(
            render(
                "{steps} / {avg_heart_rate} / {max_heart_rate} bpm / {walks}",
                &s
            ),
            "2100 / 2"
        );
        // Every separator kind.
        for sep in [
            " \u{b7} ",
            " | ",
            " / ",
            " // ",
            " - ",
            " \u{2022} ",
            ", ",
            "; ",
            " , ",
        ] {
            assert_eq!(
                render(&format!("A{sep}HR {{avg_heart_rate}}{sep}B"), &s),
                format!("A{sep}B"),
                "{sep:?}"
            );
        }
        // The default title's " // " is a separator too.
        assert_eq!(
            render_title("NEON STRIDER // {program} // {distance_km} KM", &s),
            "NEON STRIDER // 1.50 KM"
        );
    }

    #[test]
    fn symbols_without_spaces_do_not_separate() {
        let s = bare();
        // "km/h" and a hyphenated word stay inside their segment, which
        // keeps a value, so only the field itself is blank.
        assert_eq!(render("{avg_speed} km/h {max_speed}", &s), "3.0 km/h");
        assert_eq!(
            render("{steps} low-key {program} walk", &s),
            "2100 low-key walk"
        );
        assert_eq!(render("{steps} a,{program} b", &s), "2100 a, b");
    }

    #[test]
    fn a_segment_with_a_value_keeps_its_text() {
        let s = bare();
        assert_eq!(render("{distance_km} km ({program})", &s), "1.50 km");
        assert_eq!(
            render("{distance_km} km [{program}] done", &s),
            "1.50 km done"
        );
        assert_eq!(
            render("{program} walk of {distance_km} km", &s),
            "walk of 1.50 km"
        );
        assert_eq!(render("{distance_km}  {program}  km", &s), "1.50  km");
        // Literal brackets that wrap no empty field stay.
        assert_eq!(render("({distance_km}) () [x]", &s), "(1.50) () [x]");
    }

    #[test]
    fn a_line_left_empty_goes() {
        let s = bare();
        let t = "DECK_CYCLES {duration}\nPULSE {avg_heart_rate} / {max_heart_rate} bpm\n{program}\nLogged by Neon Strider";
        assert_eq!(
            render_description(t, &s),
            "DECK_CYCLES 30:00\nLogged by Neon Strider"
        );
        // A blank line typed on purpose stays.
        assert_eq!(
            render_description("A {steps}\n\nB\r\n{program}\r\nC", &s),
            "A 2100\n\nB\r\nC"
        );
        // The title of nothing but empty fields is empty.
        assert_eq!(render_title("{program}\n{avg_heart_rate} bpm", &s), "");
    }

    #[test]
    fn values_never_split_segments() {
        let mut s = bare();
        s.program = Some("Walk - easy, then brisk".to_string());
        assert_eq!(
            render("{program} \u{b7} {avg_heart_rate} bpm", &s),
            "Walk - easy, then brisk"
        );
    }

    #[test]
    fn unknown_and_broken_placeholders_stay_as_typed() {
        let s = bare();
        assert_eq!(render("{nope} {distance_km", &s), "{nope} {distance_km");
        assert_eq!(render("{{distance_km}}", &s), "{1.50}");
        assert_eq!(render("} { {} {Distance_km}", &s), "} { {} {Distance_km}");
        assert_eq!(render("no braces", &s), "no braces");
        assert_eq!(render("", &s), "");
        // Unknown text counts as a label, so its segment goes with the field.
        assert_eq!(render("A \u{b7} {nope} {program}", &s), "A");
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
        for s in [stats(), bare()] {
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

    #[test]
    fn the_help_list_names_every_field_with_a_sample() {
        let fields = template_fields();
        assert_eq!(fields.len(), FIELDS.len());
        let mut names: Vec<&str> = fields.iter().map(|f| f.name.as_str()).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), FIELDS.len(), "a field is listed twice");
        for f in &fields {
            assert!(!f.sample.is_empty(), "{} has no sample", f.name);
            assert!(f.about.ends_with('.'), "{}", f.name);
            assert!(!f.about.contains(['\u{2013}', '\u{2014}']), "{}", f.name);
        }
        // The names the old help text listed are all still fields.
        for old in [
            "distance_km",
            "duration",
            "avg_speed",
            "avg_incline",
            "calories",
            "steps",
            "walks",
            "date",
            "time",
        ] {
            assert!(names.contains(&old), "{old}");
        }
    }

    /// Writes `TEMPLATE_FIELDS.ts` beside the ts-rs bindings; CI clears the
    /// folder, runs the tests and fails on a diff.
    #[test]
    fn export_template_fields() {
        crate::limits::write_binding("TEMPLATE_FIELDS.ts", &typescript());
    }
}
