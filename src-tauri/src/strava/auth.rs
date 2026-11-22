//! The Strava login: the stored token set, the browser OAuth flow with its
//! local callback listener, token exchange and refresh, and disconnecting.
//!
//! Tokens, the client secret, the OAuth code and its `state` are never logged and never
//! leave Rust. `Auth`, `Callback` and the token reply deliberately do not
//! implement `Debug`, so they cannot end up in a log line by accident.

use super::api;
use super::live::{self, live};
use super::settings;
use super::upload;
use crate::persistence;
use crate::secrets::{self, Secret};
use crate::strava_plan as plan;
use crate::strava_status as status;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};
use tauri::AppHandle;
use tiny_http::{Response, Server};
use url::Url;

const CALLBACK_ADDR: &str = "127.0.0.1:18392";
/// Same host the listener binds: "localhost" can resolve to ::1 first. Strava
/// accepts both localhost and 127.0.0.1 as callback domains.
const REDIRECT_URI: &str = "http://127.0.0.1:18392/callback";
const AUTHORIZE_URL: &str = "https://www.strava.com/oauth/authorize";
const TOKEN_URL: &str = "https://www.strava.com/oauth/token";
const DEAUTHORIZE_URL: &str = "https://www.strava.com/oauth/deauthorize";
/// Five minutes to log in on Strava's page, type a password and approve.
const AUTH_TIMEOUT: Duration = Duration::from_secs(300);
/// A previous Connect's listener can hold the port for up to a second after it
/// is superseded; retry the bind this many times, this far apart.
const BIND_RETRIES: u32 = 8;
/// Eight tries 300 ms apart wait 2.4 s, past the second the old listener can hold it.
const BIND_RETRY_DELAY: Duration = Duration::from_millis(300);

/// Bumped by every Connect (and Cancel) so an older callback listener gives up its port.
static LISTEN_GENERATION: AtomicU64 = AtomicU64::new(0);

fn current_generation() -> u64 {
    LISTEN_GENERATION.load(Ordering::SeqCst)
}

/// The stored token set.
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Auth {
    pub access_token: String,
    pub refresh_token: String,
    /// Epoch seconds.
    pub expires_at: i64,
    #[serde(default)]
    pub athlete_name: Option<String>,
    /// Permissions Strava granted (comma separated). Absent on logins made
    /// before it was stored; those are treated as holding only the old ones.
    #[serde(default)]
    pub scope: Option<String>,
}

/// The stored login, if there is a usable one.
pub(super) fn load(app: &AppHandle) -> Option<Auth> {
    let raw = secrets::get(app, Secret::Auth)?;
    serde_json::from_str::<Auth>(&raw)
        .ok()
        .filter(|a| !a.access_token.is_empty() && !a.refresh_token.is_empty())
}

/// The scopes of the stored login (`None` for a login that predates storing them).
pub(super) fn stored_scope(app: &AppHandle) -> Option<String> {
    load(app).and_then(|a| a.scope)
}

fn save(app: &AppHandle, auth: &Auth) -> Result<(), String> {
    let json = serde_json::to_string(auth).map_err(|e| e.to_string())?;
    secrets::set(app, Secret::Auth, &json)
}

/// Forget the login locally.
pub(super) fn clear(app: &AppHandle) {
    secrets::delete(app, Secret::Auth);
    live::reload(app);
}

/// What the browser brought back to `/callback`.
struct Callback {
    code: String,
    scope: Option<String>,
}

/// What a request to `/callback` means for the attempt waiting on it.
enum CallbackOutcome {
    /// Strava's redirect for this attempt, with a code.
    Accepted(Callback),
    /// Strava's redirect for this attempt, after the user declined.
    Denied,
    /// Not this attempt's redirect (wrong or missing `state`, or no code): keep waiting.
    Ignored,
}

/// A fresh random `state` for one Connect (122 random bits from the OS).
fn new_state() -> String {
    uuid::Uuid::new_v4().simple().to_string()
}

/// Equal without stopping at the first differing byte.
fn same_secret(a: &str, b: &str) -> bool {
    a.len() == b.len()
        && a.bytes()
            .zip(b.bytes())
            .fold(0u8, |diff, (x, y)| diff | (x ^ y))
            == 0
}

/// Read a `/callback` request against the `state` this attempt sent. Only a
/// matching `state` can end the attempt; anything else is ignored.
fn classify_callback(url: &Url, expected_state: &str) -> CallbackOutcome {
    let param = |name: &str| {
        url.query_pairs()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.into_owned())
    };
    match param("state") {
        Some(state) if same_secret(&state, expected_state) => {}
        _ => return CallbackOutcome::Ignored,
    }
    if let Some(code) = param("code").filter(|c| !c.is_empty()) {
        return CallbackOutcome::Accepted(Callback {
            code,
            scope: param("scope"),
        });
    }
    if param("error").as_deref() == Some("access_denied") {
        return CallbackOutcome::Denied;
    }
    CallbackOutcome::Ignored
}

/// Wait for Strava's redirect to `/callback` carrying this attempt's `state`,
/// answering anything else with a short page and waiting on. Gives up after
/// `AUTH_TIMEOUT` or when a newer Connect replaces this one.
fn wait_for_callback(server: Server, generation: u64, state: &str) -> Result<Callback, String> {
    let deadline = Instant::now() + AUTH_TIMEOUT;
    loop {
        if current_generation() != generation {
            return Err("Superseded by a newer connect attempt".to_string());
        }
        if Instant::now() >= deadline {
            return Err("Timed out waiting for Strava. Try again.".to_string());
        }
        // Wakes once a second to check the deadline and whether a newer Connect superseded it.
        let request = match server.recv_timeout(Duration::from_secs(1)) {
            Ok(Some(r)) => r,
            Ok(None) => continue,
            Err(e) => return Err(format!("Callback listener failed: {}", e)),
        };
        let Ok(url) = Url::parse(&format!("http://{}{}", CALLBACK_ADDR, request.url())) else {
            let _ = request.respond(Response::from_string("Bad request").with_status_code(400));
            continue;
        };
        if url.path() != "/callback" {
            let _ = request.respond(Response::from_string("Not found").with_status_code(404));
            continue;
        }
        match classify_callback(&url, state) {
            CallbackOutcome::Accepted(cb) => {
                let _ = request.respond(Response::from_string(
                    "Strava is linked. Close the browser tab and go back to Neon Strider.",
                ));
                return Ok(cb);
            }
            CallbackOutcome::Denied => {
                let _ = request.respond(Response::from_string(
                    "Strava was not authorized. Close the browser tab and go back to Neon Strider.",
                ));
                return Err("Strava access was denied".to_string());
            }
            CallbackOutcome::Ignored => {
                log::info!("ignored a Strava callback that was not this connect attempt's");
                let _ = request.respond(
                    Response::from_string(
                        "Neon Strider is not waiting for this sign-in. Use Connect in Neon Strider to try again.",
                    )
                    .with_status_code(400),
                );
            }
        }
    }
}

async fn bind_listener() -> Result<Server, String> {
    let mut bound = Server::http(CALLBACK_ADDR);
    for _ in 0..BIND_RETRIES {
        if bound.is_ok() {
            break;
        }
        tokio::time::sleep(BIND_RETRY_DELAY).await;
        bound = Server::http(CALLBACK_ADDR);
    }
    bound.map_err(|e| format!("Could not listen on port 18392: {}", e))
}

/// Open the browser to authorize. Returns once the callback listener is up;
/// the exchange and its outcome happen in the background.
pub async fn connect(app: &AppHandle) -> Result<(), String> {
    let (client_id, client_secret) =
        settings::load_credentials(app).ok_or("Save your Strava client ID and secret first")?;

    let generation = LISTEN_GENERATION.fetch_add(1, Ordering::SeqCst) + 1;
    let server = bind_listener().await?;
    let state = new_state();

    let auth_url = Url::parse_with_params(
        AUTHORIZE_URL,
        &[
            ("client_id", client_id.as_str()),
            ("redirect_uri", REDIRECT_URI),
            ("response_type", "code"),
            ("approval_prompt", "auto"),
            ("scope", plan::REQUESTED_SCOPE),
            ("state", state.as_str()),
        ],
    )
    .map_err(|e| e.to_string())?;
    {
        use tauri_plugin_opener::OpenerExt;
        app.opener()
            .open_url(auth_url.as_str(), None::<&str>)
            .map_err(|e| format!("Could not open the browser: {}", e))?;
    }

    live().authorizing_until = Some(Instant::now() + AUTH_TIMEOUT);
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let waited = super::blocking(move || wait_for_callback(server, generation, &state)).await;
        // The browser step is over, whichever way it went (unless a newer
        // Connect owns it now).
        if current_generation() == generation {
            live().authorizing_until = None;
        }
        let outcome = match waited {
            Err(e) => Err(e),
            Ok(cb) => login_with(&client_id, &client_secret, cb).await,
        };
        match outcome {
            Ok(auth) => finish_login(&app, auth).await,
            // A newer attempt owns the outcome now.
            Err(e) if current_generation() != generation => {
                log::info!("connect attempt ended: {}", e);
            }
            Err(e) => {
                log::warn!("connect failed: {}", e);
                live::record_failure("Could not connect.", &e);
            }
        }
    });
    Ok(())
}

/// Check the granted scopes and exchange the code for a token set.
async fn login_with(client_id: &str, client_secret: &str, cb: Callback) -> Result<Auth, String> {
    if !plan::scope_allows_upload(cb.scope.as_deref()) {
        return Err(
            "Strava needs permission to upload activities. Connect again and keep it ticked."
                .to_string(),
        );
    }
    // No scope on the redirect means nothing was unticked.
    let granted = cb
        .scope
        .unwrap_or_else(|| plan::REQUESTED_SCOPE.to_string());
    let auth = exchange_code(client_id, client_secret, &cb.code).await?;
    Ok(Auth {
        scope: Some(granted),
        ..auth
    })
}

/// Store a fresh login and report it.
async fn finish_login(app: &AppHandle, auth: Auth) {
    let name = auth.athlete_name.clone();
    if let Err(e) = save(app, &auth) {
        log::warn!("could not save the Strava login: {}", e);
        live::record(false, secrets::NOT_SAVED_SECURELY);
        return;
    }
    live::reload(app);
    live::record(
        true,
        match name {
            Some(n) => format!("Connected as {}", n),
            None => "Connected to Strava".to_string(),
        },
    );
    upload::refresh_gear(app, false).await;
}

/// Stop waiting for the browser login.
pub fn cancel_connect() -> Result<(), String> {
    // The listener leaves within a second of being superseded.
    LISTEN_GENERATION.fetch_add(1, Ordering::SeqCst);
    live().authorizing_until = None;
    Ok(())
}

/// Forget the login, and revoke it at Strava so the app leaves the account's
/// authorized apps. The revoke is best effort: an expired token just means it
/// is forgotten locally.
pub fn disconnect(app: &AppHandle) -> Result<(), String> {
    if let Some(auth) = load(app) {
        let token = auth.access_token;
        tauri::async_runtime::spawn(async move {
            let _ = api::client()
                .post(DEAUTHORIZE_URL)
                .form(&[("access_token", token)])
                .send()
                .await;
        });
    }
    if let Err(e) = persistence::delete_setting(app, settings::KEY_GEAR_CACHE) {
        log::warn!("could not clear the shoe list: {}", e);
    }
    clear(app);
    {
        let mut l = live();
        l.clear_backoff();
        l.upload_errors.clear();
    }
    live::record(true, "Disconnected from Strava");
    Ok(())
}

#[derive(Deserialize)]
struct TokenReply {
    access_token: String,
    refresh_token: String,
    expires_at: i64,
    #[serde(default)]
    athlete: Option<AthleteReply>,
}

#[derive(Deserialize)]
struct AthleteReply {
    firstname: Option<String>,
    lastname: Option<String>,
}

/// Why a token request failed.
enum TokenFailure {
    /// Strava answered with this status; the body excerpt is for the error text.
    Status(u16, String),
    /// The request never got an answer, or the reply was unreadable.
    Other(String),
}

/// `POST /oauth/token`. Strava does not echo the form back, so error text
/// never contains the secret, the code or a token.
async fn post_token(form: &[(&str, &str)]) -> Result<TokenReply, TokenFailure> {
    let res = api::client()
        .post(TOKEN_URL)
        .form(form)
        .send()
        .await
        .map_err(|e| TokenFailure::Other(e.to_string()))?;
    let code = res.status();
    if !code.is_success() {
        let body = res.text().await.unwrap_or_default();
        return Err(TokenFailure::Status(code.as_u16(), api::excerpt(&body)));
    }
    res.json()
        .await
        .map_err(|e| TokenFailure::Other(e.to_string()))
}

async fn exchange_code(client_id: &str, client_secret: &str, code: &str) -> Result<Auth, String> {
    let reply = post_token(&[
        ("client_id", client_id),
        ("client_secret", client_secret),
        ("code", code),
        ("grant_type", "authorization_code"),
    ])
    .await
    .map_err(|failure| match failure {
        TokenFailure::Status(code, body) => status::answered(code, None, &body),
        TokenFailure::Other(e) => e,
    })?;
    let athlete_name = reply
        .athlete
        .as_ref()
        .and_then(|a| plan::athlete_display_name(a.firstname.as_deref(), a.lastname.as_deref()));
    Ok(Auth {
        access_token: reply.access_token,
        refresh_token: reply.refresh_token,
        expires_at: reply.expires_at,
        athlete_name,
        scope: None,
    })
}

enum RefreshError {
    /// Strava refused the refresh token: the grant is gone.
    Rejected,
    Failed(String),
}

async fn request_refresh(
    client_id: &str,
    client_secret: &str,
    old: &Auth,
) -> Result<Auth, RefreshError> {
    let reply = post_token(&[
        ("client_id", client_id),
        ("client_secret", client_secret),
        ("grant_type", "refresh_token"),
        ("refresh_token", old.refresh_token.as_str()),
    ])
    .await
    .map_err(|failure| match failure {
        TokenFailure::Status(code, _) if plan::refresh_means_disconnected(code) => {
            RefreshError::Rejected
        }
        TokenFailure::Status(code, body) => {
            RefreshError::Failed(status::answered(code, None, &body))
        }
        TokenFailure::Other(e) => RefreshError::Failed(e),
    })?;
    Ok(Auth {
        access_token: reply.access_token,
        refresh_token: reply.refresh_token,
        expires_at: reply.expires_at,
        athlete_name: old.athlete_name.clone(),
        scope: old.scope.clone(),
    })
}

/// Refresh and persist. A rejected refresh disconnects the account.
pub(super) async fn refresh(app: &AppHandle, old: &Auth) -> Result<Auth, String> {
    let (id, secret) = settings::load_credentials(app).ok_or("Strava credentials are missing")?;
    match request_refresh(&id, &secret, old).await {
        Ok(auth) => {
            save(app, &auth)?;
            Ok(auth)
        }
        Err(RefreshError::Rejected) => {
            clear(app);
            Err("Strava access was revoked. Connect your account again.".to_string())
        }
        Err(RefreshError::Failed(e)) => Err(format!("Could not refresh the Strava token: {}", e)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const STATE: &str = "0123456789abcdef0123456789abcdef";

    fn classify(query: &str) -> CallbackOutcome {
        let url = Url::parse(&format!("http://{}/callback?{}", CALLBACK_ADDR, query)).unwrap();
        classify_callback(&url, STATE)
    }

    #[test]
    fn a_matching_state_with_a_code_is_accepted() {
        let CallbackOutcome::Accepted(cb) =
            classify(&format!("state={STATE}&code=abc&scope=read,activity:write"))
        else {
            panic!("expected the callback to be accepted");
        };
        assert_eq!(cb.code, "abc");
        assert_eq!(cb.scope.as_deref(), Some("read,activity:write"));
    }

    #[test]
    fn a_code_without_the_right_state_is_ignored() {
        for query in [
            "code=abc".to_string(),
            "state=&code=abc".to_string(),
            "state=wrong&code=abc".to_string(),
            format!("state={}&code=abc", &STATE[..STATE.len() - 1]),
            format!("state={STATE}0&code=abc"),
        ] {
            assert!(
                matches!(classify(&query), CallbackOutcome::Ignored),
                "{query}"
            );
        }
    }

    #[test]
    fn a_matching_state_with_no_code_keeps_waiting() {
        for query in [
            format!("state={STATE}"),
            format!("state={STATE}&code="),
            format!("state={STATE}&error=server_error"),
        ] {
            assert!(
                matches!(classify(&query), CallbackOutcome::Ignored),
                "{query}"
            );
        }
    }

    #[test]
    fn only_a_matching_state_can_report_a_denial() {
        assert!(matches!(
            classify(&format!("state={STATE}&error=access_denied")),
            CallbackOutcome::Denied
        ));
        assert!(matches!(
            classify("state=wrong&error=access_denied"),
            CallbackOutcome::Ignored
        ));
        assert!(matches!(
            classify("error=access_denied"),
            CallbackOutcome::Ignored
        ));
    }

    #[test]
    fn each_connect_gets_a_fresh_state() {
        let (a, b) = (new_state(), new_state());
        assert_eq!(a.len(), 32);
        assert_ne!(a, b);
    }
}
