//! Calls to the Strava REST API, and running them with a valid token.

use super::auth;
use crate::strava_plan::{self as plan, ActivityPlan};
use crate::strava_status as status;
use serde_json::Value;
use std::future::Future;
use std::time::Duration;
use tauri::AppHandle;

const API: &str = "https://www.strava.com/api/v3";
/// Strava's API answers in a second or two; 30 s covers a slow link.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
/// A TCX file can be large; give the upload itself longer.
/// A long walk's TCX is a few MB.
const UPLOAD_TIMEOUT: Duration = Duration::from_secs(60);
/// How many of the athlete's activities one list call returns.
/// One page covers any day's activities, so the duplicate check is one request.
const LIST_PAGE_SIZE: &str = "50";
/// Long enough for Strava's JSON error body to still parse.
const EXCERPT_CHARS: usize = 600;

pub(super) const NOT_CONNECTED: &str = "Strava is not connected";

pub(super) fn client() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(REQUEST_TIMEOUT)
        .build()
        .unwrap_or_default()
}

/// A single-line excerpt of an error body (never includes credentials: Strava
/// does not echo them). `strava_status` turns it into a sentence, so the raw
/// text is only ever logged.
pub(super) fn excerpt(body: &str) -> String {
    let flat: String = body.split_whitespace().collect::<Vec<_>>().join(" ");
    flat.chars().take(EXCERPT_CHARS).collect()
}

pub(super) enum ApiError {
    /// 401: the token needs a refresh.
    Unauthorized,
    Failed(String),
}

fn failed(e: impl ToString) -> ApiError {
    ApiError::Failed(e.to_string())
}

async fn read_json(res: reqwest::Response) -> Result<Value, ApiError> {
    let code = res.status();
    if code.as_u16() == 401 {
        return Err(ApiError::Unauthorized);
    }
    if !code.is_success() {
        let retry_after = res
            .headers()
            .get("retry-after")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.trim().parse::<u32>().ok());
        let body = res.text().await.unwrap_or_default();
        // Strava may answer a repeated upload with an error that names the
        // activity it duplicates; that is still a success for us.
        if let Some(id) = plan::duplicate_activity_id(&body) {
            return Ok(serde_json::json!({ "activity_id": id }));
        }
        return Err(ApiError::Failed(status::answered(
            code.as_u16(),
            retry_after,
            &excerpt(&body),
        )));
    }
    res.json().await.map_err(failed)
}

async fn send(request: reqwest::RequestBuilder) -> Result<Value, ApiError> {
    read_json(request.send().await.map_err(failed)?).await
}

/// Run `op` with a valid token: refresh first when it is about to expire, and
/// once more (with one retry) if Strava answers 401.
pub(super) async fn with_token<T, F, Fut>(app: &AppHandle, op: F) -> Result<T, String>
where
    F: Fn(String) -> Fut,
    Fut: Future<Output = Result<T, ApiError>>,
{
    let mut auth = auth::load(app).ok_or(NOT_CONNECTED)?;
    if plan::needs_refresh(auth.expires_at, chrono::Utc::now().timestamp()) {
        auth = auth::refresh(app, &auth).await?;
    }
    match op(auth.access_token.clone()).await {
        Ok(v) => return Ok(v),
        Err(ApiError::Failed(e)) => return Err(e),
        Err(ApiError::Unauthorized) => {}
    }
    let auth = auth::refresh(app, &auth).await?;
    op(auth.access_token).await.map_err(|e| match e {
        ApiError::Failed(e) => e,
        ApiError::Unauthorized => {
            "Strava rejected the login. Connect your account again.".to_string()
        }
    })
}

/// `POST /uploads` with the TCX file.
pub(super) async fn post_tcx(
    token: String,
    activity: &ActivityPlan,
    external_id: &str,
    tcx: String,
) -> Result<Value, ApiError> {
    let part = reqwest::multipart::Part::bytes(tcx.into_bytes())
        .file_name("activity.tcx")
        .mime_str("application/vnd.garmin.tcx+xml")
        .map_err(failed)?;
    let form = reqwest::multipart::Form::new()
        .part("file", part)
        .text("data_type", "tcx")
        .text("name", activity.name.clone())
        .text("description", activity.description.clone())
        // A treadmill walk: `sport_type` is the current field, `activity_type`
        // the older one; both say Walk. `trainer=1` marks it indoor.
        .text("sport_type", "Walk")
        .text("activity_type", "walk")
        .text("trainer", "1")
        .text("external_id", external_id.to_string());
    send(
        client()
            .post(format!("{}/uploads", API))
            .bearer_auth(token)
            .multipart(form)
            .timeout(UPLOAD_TIMEOUT),
    )
    .await
}

/// `POST /activities`: a manual activity, for a walk with too few samples for a file.
pub(super) async fn post_manual(token: String, activity: &ActivityPlan) -> Result<Value, ApiError> {
    send(
        client()
            .post(format!("{}/activities", API))
            .bearer_auth(token)
            .form(&[
                ("name", activity.name.clone()),
                ("sport_type", "Walk".to_string()),
                ("trainer", "1".to_string()),
                (
                    "start_date_local",
                    plan::start_date_local(activity.start_ms),
                ),
                ("elapsed_time", activity.elapsed_s.max(1).to_string()),
                ("distance", format!("{:.1}", activity.distance_m)),
                ("description", activity.description.clone()),
            ]),
    )
    .await
}

/// `GET /uploads/{id}`: the processing status of a file upload.
pub(super) async fn get_upload(token: String, upload_id: String) -> Result<Value, ApiError> {
    send(
        client()
            .get(format!("{}/uploads/{}", API, upload_id))
            .bearer_auth(token),
    )
    .await
}

/// `GET /athlete`: with `profile:read_all` it includes the shoes.
pub(super) async fn get_athlete(token: String) -> Result<Value, ApiError> {
    send(client().get(format!("{}/athlete", API)).bearer_auth(token)).await
}

/// One page of the athlete's activities that started between `after` and
/// `before` (epoch seconds).
pub(super) async fn list_activities(
    token: String,
    after: i64,
    before: i64,
) -> Result<Value, ApiError> {
    send(
        client()
            .get(format!("{}/athlete/activities", API))
            .bearer_auth(token)
            .query(&[
                ("after", after.to_string()),
                ("before", before.to_string()),
                ("per_page", LIST_PAGE_SIZE.to_string()),
            ]),
    )
    .await
}

/// `PUT /activities/{id}`: what an upload cannot set itself (shoe, mute).
pub(super) async fn put_activity(
    token: String,
    activity_id: String,
    body: Value,
) -> Result<Value, ApiError> {
    send(
        client()
            .put(format!("{}/activities/{}", API, activity_id))
            .bearer_auth(token)
            .json(&body),
    )
    .await
}
