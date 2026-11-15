//! What Strava is doing, and what went wrong, in words a person can act on.
//! Pure: no I/O, no clock. `strava.rs` feeds it raw error text and the live
//! numbers; the windows only see the results.
//!
//! Raw errors from `strava.rs` look like `Strava answered 403: {json}` (built by
//! [`answered`]). They are parsed here and never shown as they are: Strava's own
//! `{message, errors:[{resource, field, code}]}` body becomes one plain sentence.

use serde::{Deserialize, Serialize};

pub const API_SETTINGS_URL: &str = "https://www.strava.com/settings/api";

/// Which step of an upload is running.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub enum StravaStep {
    Preparing,
    Sending,
    /// Polling Strava's upload status.
    Processing,
    /// The PUT for shoes and mute.
    Finishing,
}

/// What the Strava worker is busy with.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ts_rs::TS)]
#[serde(tag = "state", rename_all = "snake_case")]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub enum StravaActivity {
    Idle,
    /// The browser is open and Rust is waiting for the callback.
    Authorizing {
        expires_in_s: u32,
    },
    Uploading {
        label: String,
        step: StravaStep,
    },
    CheckingDuplicates,
    /// A failed pass waits before the next try.
    BackingOff {
        retry_in_s: u32,
        reason: String,
    },
    /// Auto-sync stopped because retrying cannot help until the user acts.
    Paused {
        reason: String,
    },
    /// More than one upload is waiting.
    Queued {
        count: u32,
    },
}

/// What the person can do about a failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub enum StravaHint {
    #[default]
    None,
    Reconnect,
    CheckInternet,
    /// Open strava.com/settings/api.
    OpenApiSettings,
    Wait,
}

/// One failure, explained.
#[derive(Debug, Clone, PartialEq)]
pub struct StravaIssue {
    pub message: String,
    pub hint: StravaHint,
    /// Retrying will not fix it by itself.
    pub permanent: bool,
    /// Seconds Strava asked us to wait, when it said.
    pub retry_after_s: Option<u32>,
}

impl StravaIssue {
    fn new(message: impl Into<String>, hint: StravaHint) -> Self {
        StravaIssue {
            message: message.into(),
            hint,
            permanent: false,
            retry_after_s: None,
        }
    }
}

/// Collapse whitespace into one line and cut at `max` characters.
pub fn one_line(text: &str, max: usize) -> String {
    let flat = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.chars().count() <= max {
        return flat;
    }
    let cut: String = flat.chars().take(max.saturating_sub(1)).collect();
    format!("{}\u{2026}", cut.trim_end())
}

/// Strava's error body.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct StravaBody {
    pub message: Option<String>,
    /// (resource, field, code)
    pub errors: Vec<(String, String, String)>,
}

fn parse_body(body: &str) -> Option<StravaBody> {
    let v: serde_json::Value = serde_json::from_str(body.trim()).ok()?;
    let obj = v.as_object()?;
    let text = |v: &serde_json::Value, k: &str| {
        v.get(k).and_then(|x| x.as_str()).unwrap_or("").to_string()
    };
    let errors = obj
        .get("errors")
        .and_then(|e| e.as_array())
        .map(|a| {
            a.iter()
                .map(|e| (text(e, "resource"), text(e, "field"), text(e, "code")))
                .collect()
        })
        .unwrap_or_default();
    Some(StravaBody {
        message: obj
            .get("message")
            .and_then(|m| m.as_str())
            .map(str::to_string),
        errors,
    })
}

/// The raw error text for an HTTP failure: `Strava answered 403 (retry in 90s): body`.
/// `describe` parses it back with `split_status`, so the two must agree.
pub fn answered(status: u16, retry_after_s: Option<u32>, body: &str) -> String {
    let wait = retry_after_s
        .map(|s| format!(" (retry in {}s)", s))
        .unwrap_or_default();
    format!("Strava answered {}{}: {}", status, wait, body)
}

/// Split `... Strava answered 403 (retry in 90s): body` into status, retry and body.
fn split_status(raw: &str) -> Option<(u16, Option<u32>, &str)> {
    let at = raw.find("Strava answered ")? + "Strava answered ".len();
    let rest = &raw[at..];
    let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
    let status = digits.parse().ok()?;
    let mut rest = &rest[digits.len()..];
    let mut retry = None;
    if let Some(inner) = rest.strip_prefix(" (retry in ") {
        if let Some(end) = inner.find("s)") {
            retry = inner[..end].parse().ok();
            rest = &inner[end + 2..];
        }
    }
    let body = rest.strip_prefix(':').unwrap_or(rest).trim();
    Some((status, retry, body))
}

/// "Resource field code" bits of an error list, for the fallback sentence.
fn error_summary(errors: &[(String, String, String)]) -> String {
    errors
        .iter()
        .map(|(r, f, c)| {
            [r.as_str(), f.as_str(), c.as_str()]
                .iter()
                .filter(|s| !s.is_empty())
                .copied()
                .collect::<Vec<_>>()
                .join(" ")
        })
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(", ")
}

fn http_issue(status: u16, retry: Option<u32>, body: &str) -> StravaIssue {
    let parsed = parse_body(body);
    let errors = parsed.as_ref().map(|b| b.errors.as_slice()).unwrap_or(&[]);
    let message = parsed.as_ref().and_then(|b| b.message.clone());

    match status {
        401 => StravaIssue::new(
            "Strava needs you to reconnect. Press Connect account.",
            StravaHint::Reconnect,
        ),
        403 => {
            let inactive = errors
                .iter()
                .any(|(r, f, c)| r == "Application" && f == "Status" && c == "inactive");
            if inactive {
                return StravaIssue {
                    message: "Strava turned off this API app: the account that owns it needs a Strava subscription. Subscribe, then reactivate it at strava.com/settings/api \u{2014} or use Save for Strava below.".to_string(),
                    hint: StravaHint::OpenApiSettings,
                    permanent: true,
                    retry_after_s: None,
                };
            }
            let missing: Vec<&str> = errors
                .iter()
                .filter(|(_, f, c)| c == "missing" && !f.is_empty())
                .map(|(_, f, _)| f.as_str())
                .collect();
            if !missing.is_empty() {
                return StravaIssue::new(
                    format!(
                        "Strava is missing the {} permission. Reconnect and keep every box ticked.",
                        missing.join(", ")
                    ),
                    StravaHint::Reconnect,
                );
            }
            let detail = message
                .filter(|m| !m.is_empty())
                .map(|m| format!(" ({})", one_line(&m, 60)))
                .unwrap_or_default();
            StravaIssue::new(
                format!(
                    "Strava refused the request{}. Reconnect and keep every box ticked.",
                    detail
                ),
                StravaHint::Reconnect,
            )
        }
        429 => StravaIssue {
            message: match retry {
                Some(s) => format!(
                    "Strava rate limit reached. Strava asks for {} before the next try.",
                    wait_text(s)
                ),
                None => "Strava rate limit reached. It will try again later.".to_string(),
            },
            hint: StravaHint::Wait,
            permanent: false,
            retry_after_s: retry,
        },
        500..=599 => StravaIssue::new(
            format!(
                "Strava is having problems (error {}). It will try again.",
                status
            ),
            StravaHint::Wait,
        ),
        _ => {
            let mut parts = Vec::new();
            if let Some(m) = message.filter(|m| !m.is_empty()) {
                parts.push(m);
            }
            let summary = error_summary(errors);
            if !summary.is_empty() {
                parts.push(summary);
            }
            if parts.is_empty() {
                // Not Strava's JSON shape: never show the raw body.
                let plain = if body.trim_start().starts_with(['{', '[', '<']) {
                    ""
                } else {
                    body
                };
                if plain.is_empty() {
                    return StravaIssue::new(
                        format!("Strava answered with error {}.", status),
                        StravaHint::None,
                    );
                }
                return StravaIssue::new(
                    format!("Strava answered {}: {}", status, one_line(plain, 120)),
                    StravaHint::None,
                );
            }
            StravaIssue::new(
                format!("Strava said: {}", one_line(&parts.join(". "), 140)),
                StravaHint::None,
            )
        }
    }
}

fn wait_text(secs: u32) -> String {
    if secs >= 90 {
        format!("{} min", (secs + 30) / 60)
    } else {
        format!("{} s", secs.max(1))
    }
}

/// Turn a raw error from `strava.rs` into one plain sentence.
pub fn describe(raw: &str) -> StravaIssue {
    if let Some((status, retry, body)) = split_status(raw) {
        return http_issue(status, retry, body);
    }
    let r = raw.to_lowercase();
    let has = |needles: &[&str]| needles.iter().any(|n| r.contains(n));
    if has(&[
        "connect your account again",
        "revoked",
        "rejected the login",
        "is not connected",
    ]) {
        return StravaIssue::new(
            "Strava needs you to reconnect. Press Connect account.",
            StravaHint::Reconnect,
        );
    }
    if has(&[
        "error sending request",
        "dns",
        "tcp connect",
        "connection refused",
        "connection reset",
        "timed out",
        "timeout",
        "network",
        "could not refresh the strava token",
    ]) {
        return StravaIssue::new(
            "Can't reach Strava. Check your internet connection.",
            StravaHint::CheckInternet,
        );
    }
    if let Some(at) = raw.find("Strava rejected the file:") {
        let why = raw[at + "Strava rejected the file:".len()..].trim();
        let why = one_line(why, 120);
        return StravaIssue::new(
            if why.is_empty() {
                "Strava rejected the walk.".to_string()
            } else {
                format!("Strava rejected the walk: {}", why)
            },
            StravaHint::None,
        );
    }
    if has(&["still processing"]) {
        return StravaIssue::new(
            "Strava is slow to process this walk. It will try again.",
            StravaHint::Wait,
        );
    }
    let line = one_line(raw, 140);
    if line.starts_with(['{', '[']) {
        return StravaIssue::new(
            "Strava returned a reply this app does not understand.",
            StravaHint::None,
        );
    }
    StravaIssue::new(line, StravaHint::None)
}

/// The inputs to [`activity`], read from the live state.
#[derive(Debug, Clone, Default)]
pub struct ActivityInputs {
    pub authorizing_left_s: Option<u32>,
    pub pending: u32,
    pub checking_duplicates: bool,
    /// (label, step) of the upload that is running.
    pub uploading: Option<(String, StravaStep)>,
    pub backoff: Option<(u32, String)>,
    pub paused: Option<String>,
}

pub fn activity(i: &ActivityInputs) -> StravaActivity {
    if let Some(expires_in_s) = i.authorizing_left_s {
        return StravaActivity::Authorizing { expires_in_s };
    }
    if i.pending > 0 {
        if i.checking_duplicates {
            return StravaActivity::CheckingDuplicates;
        }
        if let Some((label, step)) = &i.uploading {
            return StravaActivity::Uploading {
                label: label.clone(),
                step: *step,
            };
        }
        if i.pending > 1 {
            return StravaActivity::Queued { count: i.pending };
        }
        return StravaActivity::Uploading {
            label: String::new(),
            step: StravaStep::Preparing,
        };
    }
    if let Some(reason) = &i.paused {
        return StravaActivity::Paused {
            reason: reason.clone(),
        };
    }
    if let Some((retry_in_s, reason)) = &i.backoff {
        return StravaActivity::BackingOff {
            retry_in_s: *retry_in_s,
            reason: reason.clone(),
        };
    }
    StravaActivity::Idle
}

#[cfg(test)]
mod tests {
    use super::*;

    const INACTIVE: &str = r#"Strava answered 403: {"message":"Forbidden","errors":[{"resource":"Application","field":"Status","code":"inactive"}]}"#;

    #[test]
    fn answered_text_parses_back() {
        assert_eq!(answered(404, None, "nope"), "Strava answered 404: nope");
        assert_eq!(
            answered(429, Some(90), "{}"),
            "Strava answered 429 (retry in 90s): {}"
        );
        let raw = format!("Upload failed. {}", answered(429, Some(90), "{}"));
        assert_eq!(split_status(&raw), Some((429, Some(90), "{}")));
        assert_eq!(
            split_status(&answered(503, None, "")),
            Some((503, None, ""))
        );
    }

    #[test]
    fn parses_the_strava_error_shape() {
        let body = r#"{"message":"Forbidden","errors":[{"resource":"Application","field":"Status","code":"inactive"}]}"#;
        let b = parse_body(body).unwrap();
        assert_eq!(b.message.as_deref(), Some("Forbidden"));
        assert_eq!(
            b.errors,
            vec![("Application".into(), "Status".into(), "inactive".into())]
        );
        assert!(parse_body("not json").is_none());
    }

    #[test]
    fn inactive_application_is_explained_and_permanent() {
        let i = describe(INACTIVE);
        assert!(i.message.starts_with("Strava turned off this API app"));
        assert!(i.message.contains("subscription"));
        assert!(i.message.contains("Save for Strava"));
        assert!(i.message.contains("strava.com/settings/api"));
        assert_eq!(i.hint, StravaHint::OpenApiSettings);
        assert!(i.permanent);
        assert!(!i.message.contains('{'));
    }

    #[test]
    fn missing_scope_names_the_permission() {
        let raw = r#"Strava answered 403: {"message":"Authorization Error","errors":[{"resource":"AccessToken","field":"activity:write","code":"missing"}]}"#;
        let i = describe(raw);
        assert!(i.message.contains("activity:write"));
        assert_eq!(i.hint, StravaHint::Reconnect);
        assert!(!i.permanent);
    }

    #[test]
    fn other_403_offers_reconnect() {
        let i = describe(r#"Strava answered 403: {"message":"Forbidden"}"#);
        assert_eq!(i.hint, StravaHint::Reconnect);
        assert!(i.message.contains("Forbidden"));
        assert!(!i.message.contains('{'));
    }

    #[test]
    fn unauthorized_and_revoked_mean_reconnect() {
        assert_eq!(
            describe("Strava answered 401: {}").hint,
            StravaHint::Reconnect
        );
        assert_eq!(
            describe("Strava access was revoked. Connect your account again.").hint,
            StravaHint::Reconnect
        );
    }

    #[test]
    fn rate_limit_reads_the_retry_time() {
        let i =
            describe(r#"Strava answered 429 (retry in 120s): {"message":"Rate Limit Exceeded"}"#);
        assert_eq!(i.hint, StravaHint::Wait);
        assert_eq!(i.retry_after_s, Some(120));
        assert!(i.message.contains("2 min"));
        let i = describe("Strava answered 429: {}");
        assert_eq!(i.retry_after_s, None);
    }

    #[test]
    fn server_errors_and_network() {
        assert!(describe("Strava answered 503: <html>down</html>")
            .message
            .contains("503"));
        let n = describe(
            "error sending request for url (https://www.strava.com/api/v3/uploads): dns error",
        );
        assert_eq!(n.hint, StravaHint::CheckInternet);
    }

    #[test]
    fn upload_rejection_keeps_strava_text_on_one_line() {
        let i = describe("Strava rejected the file: There was an error\nprocessing your activity.");
        assert_eq!(
            i.message,
            "Strava rejected the walk: There was an error processing your activity."
        );
    }

    #[test]
    fn other_4xx_use_message_and_errors() {
        let i = describe(
            r#"Strava answered 400: {"message":"Bad Request","errors":[{"resource":"Activity","field":"distance","code":"invalid"}]}"#,
        );
        assert_eq!(
            i.message,
            "Strava said: Bad Request. Activity distance invalid"
        );
    }

    #[test]
    fn raw_json_is_never_shown() {
        for raw in [
            r#"{"a":1}"#,
            r#"Strava answered 418: {"weird":true}"#,
            "Strava answered 400: [1,2]",
        ] {
            let m = describe(raw).message;
            assert!(!m.contains('{') && !m.contains('['), "{m}");
        }
    }

    #[test]
    fn one_line_trims() {
        assert_eq!(one_line("a\n  b", 10), "a b");
        assert!(one_line(&"x".repeat(300), 50).chars().count() <= 50);
    }

    #[test]
    fn activity_priorities() {
        let base = ActivityInputs::default();
        assert_eq!(activity(&base), StravaActivity::Idle);
        let a = ActivityInputs {
            authorizing_left_s: Some(250),
            pending: 1,
            ..base.clone()
        };
        assert_eq!(
            activity(&a),
            StravaActivity::Authorizing { expires_in_s: 250 }
        );
        let u = ActivityInputs {
            pending: 2,
            uploading: Some(("2.40 km".into(), StravaStep::Sending)),
            ..base.clone()
        };
        assert_eq!(
            activity(&u),
            StravaActivity::Uploading {
                label: "2.40 km".into(),
                step: StravaStep::Sending
            }
        );
        let q = ActivityInputs {
            pending: 3,
            ..base.clone()
        };
        assert_eq!(activity(&q), StravaActivity::Queued { count: 3 });
        let c = ActivityInputs {
            pending: 1,
            checking_duplicates: true,
            ..base.clone()
        };
        assert_eq!(activity(&c), StravaActivity::CheckingDuplicates);
        let b = ActivityInputs {
            backoff: Some((600, "x".into())),
            ..base.clone()
        };
        assert_eq!(
            activity(&b),
            StravaActivity::BackingOff {
                retry_in_s: 600,
                reason: "x".into()
            }
        );
        let p = ActivityInputs {
            paused: Some("y".into()),
            backoff: Some((5, "x".into())),
            ..base
        };
        assert_eq!(activity(&p), StravaActivity::Paused { reason: "y".into() });
    }
}
