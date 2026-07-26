use std::{
    sync::LazyLock,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use axum::{
    Json,
    extract::{Query, State},
    http::HeaderMap,
};
use axum_auth::AuthBearer;
use moka::sync::Cache;
use serde::Deserialize;
use serde_json::{Value, json};
use tracing::{error, info, warn};
use wreq::StatusCode;

use super::error::ApiError;
use crate::{
    VERSION_INFO,
    claude_code_state::ClaudeCodeState,
    claude_web_state::ClaudeWebState,
    config::{CLEWDR_CONFIG, CookieStatus},
    services::cookie_pool::CookiePool,
};

/// Cache entry for cookie status responses
#[derive(Clone)]
struct CookieStatusCache {
    data: Value,
    timestamp: u64,
}

/// Query parameters for cookie status endpoint
#[derive(Deserialize)]
pub struct CookieStatusQuery {
    #[serde(default)]
    refresh: bool,
}

/// Global cache for cookie status responses (TTL: 5 minutes)
static COOKIES_CACHE: LazyLock<Cache<String, CookieStatusCache>> = LazyLock::new(|| {
    Cache::builder()
        .max_capacity(1)
        .time_to_live(Duration::from_mins(5))
        .build()
});

/// Cache key for cookie status
const COOKIE_STATUS_CACHE_KEY: &str = "all_cookies";

/// API endpoint to submit a new cookie
/// Validates and adds the cookie to the cookie manager
///
/// # Arguments
/// * `s` - Application state containing event sender
/// * `t` - Auth bearer token for admin authentication
/// * `c` - Cookie status to be submitted
///
/// # Returns
/// * `StatusCode` - HTTP status code indicating success or failure
///
/// # Errors
/// [`ApiError::unauthorized`] if the bearer token is not the admin password.
/// Submitting a cookie the pool already knows is a no-op, not an error.
pub async fn api_post_cookie(
    State(s): State<CookiePool>,
    AuthBearer(t): AuthBearer,
    Json(mut c): Json<CookieStatus>,
) -> Result<StatusCode, ApiError> {
    if !CLEWDR_CONFIG.load().admin_auth(&t) {
        return Err(ApiError::unauthorized());
    }
    c.reset_time = None;
    info!("Cookie accepted: {}", c.cookie);
    s.submit(c);
    // Clear cache to ensure fresh data on next request
    COOKIES_CACHE.invalidate(COOKIE_STATUS_CACHE_KEY);
    info!("Cookie status cache invalidated after adding new cookie");
    Ok(StatusCode::OK)
}

/// API endpoint to retrieve all cookies and their status
/// Gets information about valid, exhausted, and invalid cookies
///
/// # Arguments
/// * `s` - Application state containing event sender
/// * `t` - Auth bearer token for admin authentication
/// * `query` - Query parameters including optional refresh flag
///
/// # Returns
/// * `Result<(HeaderMap, Json<Value>), ApiError>` - Response with cache headers and cookie status
///
/// # Errors
/// [`ApiError::unauthorized`] if the bearer token is not the admin password.
pub async fn api_get_cookies(
    State(s): State<CookiePool>,
    AuthBearer(t): AuthBearer,
    Query(query): Query<CookieStatusQuery>,
) -> Result<(HeaderMap, Json<Value>), ApiError> {
    if !CLEWDR_CONFIG.load().admin_auth(&t) {
        return Err(ApiError::unauthorized());
    }

    let mut headers = HeaderMap::new();

    // Check cache if not force refreshing
    if !query.refresh
        && let Some(cached) = COOKIES_CACHE.get(COOKIE_STATUS_CACHE_KEY)
    {
        headers.insert("X-Cache-Status", HeaderValue::from_static("HIT"));
        headers.insert(
            "X-Cache-Timestamp",
            HeaderValue::from_str(&cached.timestamp.to_string())
                .unwrap_or_else(|_| HeaderValue::from_static("0")),
        );
        info!("Cookie status served from cache");
        return Ok((headers, Json(cached.data)));
    }

    // Cache miss or force refresh - fetch fresh data
    let status = s.status();
    let valid = augment_utilization(status.valid, s.clone()).await;
    let exhausted = augment_utilization(status.exhausted, s.clone()).await;
    let invalid = status
        .invalid
        .into_iter()
        .map(|u| serde_json::to_value(u).unwrap_or(json!({})))
        .collect::<Vec<_>>();

    let response_data = json!({
        "valid": valid,
        "exhausted": exhausted,
        "invalid": invalid,
    });

    // Store in cache
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_else(|e| {
            warn!("System time error: {}, using fallback timestamp", e);
            Duration::from_secs(0)
        })
        .as_secs();

    COOKIES_CACHE.insert(
        COOKIE_STATUS_CACHE_KEY.to_string(),
        CookieStatusCache {
            data: response_data.clone(),
            timestamp,
        },
    );

    headers.insert("X-Cache-Status", HeaderValue::from_static("MISS"));
    headers.insert(
        "X-Cache-Timestamp",
        HeaderValue::from_str(&timestamp.to_string())
            .unwrap_or_else(|_| HeaderValue::from_static("0")),
    );

    if query.refresh {
        info!("Cookie status force refreshed");
    } else {
        info!("Cookie status fetched and cached");
    }

    Ok((headers, Json(response_data)))
}

/// API endpoint to delete a specific cookie
/// Removes the cookie from all collections in the cookie manager
///
/// # Arguments
/// * `s` - Application state containing event sender
/// * `t` - Auth bearer token for admin authentication
/// * `c` - Cookie status to be deleted
///
/// # Returns
/// * `Result<StatusCode, (StatusCode, Json<serde_json::Value>)>` - Success status or error
///
/// # Errors
/// [`ApiError::unauthorized`] if the bearer token is not the admin password,
/// or [`ApiError::internal`] if the cookie is not present or cannot be removed.
pub async fn api_delete_cookie(
    State(s): State<CookiePool>,
    AuthBearer(t): AuthBearer,
    Json(c): Json<CookieStatus>,
) -> Result<StatusCode, ApiError> {
    if !CLEWDR_CONFIG.load().admin_auth(&t) {
        return Err(ApiError::unauthorized());
    }

    match s.delete(&c) {
        Ok(()) => {
            info!("Cookie deleted successfully: {}", c.cookie);
            // Clear cache to ensure fresh data on next request
            COOKIES_CACHE.invalidate(COOKIE_STATUS_CACHE_KEY);
            info!("Cookie status cache invalidated");
            Ok(StatusCode::NO_CONTENT)
        }
        Err(e) => {
            error!("Failed to delete cookie: {}", e);
            Err(ApiError::internal(format!("Failed to delete cookie: {e}")))
        }
    }
}

/// API endpoint to get the application version information
///
/// # Returns
/// * `String` - Version information string
pub async fn api_version() -> String {
    VERSION_INFO.to_string()
}

/// API endpoint to verify authentication
/// Checks if the provided token is valid for admin access
///
/// # Arguments
/// * `t` - Auth bearer token to verify
///
/// # Returns
/// * `StatusCode` - OK if authorized, UNAUTHORIZED otherwise
pub async fn api_auth(AuthBearer(t): AuthBearer) -> StatusCode {
    if !CLEWDR_CONFIG.load().admin_auth(&t) {
        return StatusCode::UNAUTHORIZED;
    }
    info!("Auth token accepted,");
    StatusCode::OK
}

/// API endpoint to get the list of available models
/// Retrieves the list of models from the configuration
pub async fn api_get_models() -> Json<Value> {
    let data: Vec<Value> = crate::types::model::advertised_models()
        .map(|model| {
            json!({
                "id": model,
                "object": "model",
                "created": 0,
                "owned_by": "clewdr",
            })
        })
        .collect::<Vec<_>>();
    Json(json!({
        "object": "list",
        "data": data,
    }))
}

// ------------------------------
// Ephemeral org usage enrichment
// ------------------------------
use futures::{StreamExt, TryFutureExt, stream};
use http::HeaderValue;

/// Render a stored boundary the way the usage endpoint sends it.
///
/// `CookieStatus` keeps these as epoch seconds, parsed out of the endpoint's
/// RFC 3339 strings by `fetch_usage_resets`. This is that step inverted, so a
/// boundary we already know reaches the UI in the shape it expects whether or
/// not the live probe answered.
fn render_boundary(epoch: Option<i64>) -> Value {
    epoch
        .and_then(|ts| chrono::DateTime::from_timestamp(ts, 0))
        .map_or(Value::Null, |dt| {
            json!(dt.to_rfc3339_opts(chrono::SecondsFormat::Secs, true))
        })
}

/// Project a cookie into the JSON the frontend deserializes.
///
/// The serialized `CookieStatus` is not that type: it carries the boundaries as
/// integers under names the API does not use. Both have to be translated here,
/// because the frontend reads the listing as one value and a single field of
/// the wrong type fails all of it.
fn cookie_api_value(cookie: &CookieStatus) -> Value {
    let mut value = serde_json::to_value(cookie).unwrap_or_else(|_| json!({}));
    value["session_resets_at"] = render_boundary(cookie.session_resets_at);
    value["seven_day_resets_at"] = render_boundary(cookie.weekly_resets_at);
    value["seven_day_sonnet_resets_at"] = render_boundary(cookie.weekly_sonnet_resets_at);
    value
}

async fn augment_utilization(cookies: Vec<CookieStatus>, handle: CookiePool) -> Vec<Value> {
    let concurrency = 5usize;
    stream::iter(cookies.into_iter().map(move |cookie| {
        let handle = handle.clone();
        async move {
            let base = cookie_api_value(&cookie);
            match fetch_usage_percent(cookie, handle).await {
                Some((
                    five_hour,
                    five_reset,
                    seven_day,
                    seven_reset,
                    seven_day_sonnet,
                    sonnet_reset,
                )) => {
                    let mut obj = base;
                    obj["session_utilization"] = json!(five_hour);
                    obj["session_resets_at"] = json!(five_reset);
                    obj["seven_day_utilization"] = json!(seven_day);
                    obj["seven_day_resets_at"] = json!(seven_reset);
                    obj["seven_day_sonnet_utilization"] = json!(seven_day_sonnet);
                    obj["seven_day_sonnet_resets_at"] = json!(sonnet_reset);
                    obj
                }
                None => base,
            }
        }
    }))
    .buffer_unordered(concurrency)
    .collect::<Vec<_>>()
    .await
}

async fn fetch_usage_percent(
    cookie: CookieStatus,
    handle: CookiePool,
) -> Option<(
    u32,
    Option<String>,
    u32,
    Option<String>,
    u32,
    Option<String>,
)> {
    let oauth_handle = handle.clone();
    let fallback_cookie = cookie.clone();
    let fallback_cookie_name = fallback_cookie.cookie.to_string();
    let usage = try_oauth_usage(&cookie, &oauth_handle)
        .or_else(|()| async move {
            info!(
                "OAuth usage unavailable for {}, trying web fallback",
                fallback_cookie_name
            );
            ClaudeWebState::fetch_web_usage(handle, fallback_cookie)
                .await
                .ok_or_else(|| {
                    warn!(
                        "Web usage fallback also failed for {}",
                        fallback_cookie_name
                    );
                })
        })
        .await
        .ok()?;

    Some(extract_usage_fields(&usage))
}

/// Try the OAuth endpoint (`api.anthropic.com/api/oauth/usage`)
async fn try_oauth_usage(
    cookie: &CookieStatus,
    handle: &CookiePool,
) -> Result<serde_json::Value, ()> {
    let Ok(mut state) = ClaudeCodeState::from_cookie(handle.clone(), cookie.clone()) else {
        warn!("try_oauth_usage: from_cookie failed for {}", cookie.cookie);
        return Err(());
    };
    let result = state.fetch_usage_metrics().await;
    state.return_cookie(None);
    result
        .inspect_err(|e| {
            warn!("try_oauth_usage: fetch failed for {}: {}", cookie.cookie, e);
        })
        .map_err(|_| ())
}

/// The six usage fields, each defaulting to zero / absent when the endpoint
/// omits it.
type Usage = (
    u32,
    Option<String>,
    u32,
    Option<String>,
    u32,
    Option<String>,
);

/// Extract the six usage fields from the usage JSON returned by either endpoint
///
/// Utilization percentages are clamped into `u32` range: the cast is only safe
/// because the endpoint reports a 0-100 percentage, and clamping keeps a
/// malformed value from wrapping.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "values are clamped into range immediately before the cast"
)]
fn extract_usage_fields(usage: &serde_json::Value) -> Usage {
    let five = usage
        .get("five_hour")
        .and_then(|o| o.get("utilization"))
        .and_then(serde_json::Value::as_f64)
        .map_or(0, |v| v.round().clamp(0.0, f64::from(u32::MAX)) as u32);
    let five_reset = usage
        .get("five_hour")
        .and_then(|o| o.get("resets_at"))
        .and_then(|v| v.as_str())
        .map(std::string::ToString::to_string);
    let seven = usage
        .get("seven_day")
        .and_then(|o| o.get("utilization"))
        .and_then(serde_json::Value::as_f64)
        .map_or(0, |v| v.round().clamp(0.0, f64::from(u32::MAX)) as u32);
    let seven_reset = usage
        .get("seven_day")
        .and_then(|o| o.get("resets_at"))
        .and_then(|v| v.as_str())
        .map(std::string::ToString::to_string);
    let seven_sonnet = usage
        .get("seven_day_sonnet")
        .and_then(|o| o.get("utilization"))
        .and_then(serde_json::Value::as_f64)
        .map_or(0, |v| v.round().clamp(0.0, f64::from(u32::MAX)) as u32);
    let sonnet_reset = usage
        .get("seven_day_sonnet")
        .and_then(|o| o.get("resets_at"))
        .and_then(|v| v.as_str())
        .map(std::string::ToString::to_string);
    (
        five,
        five_reset,
        seven,
        seven_reset,
        seven_sonnet,
        sonnet_reset,
    )
}

#[cfg(test)]
mod tests {
    use clewdr_types::CookieStatusApi;

    use super::*;

    /// A well-formed cookie whose windows all carry a known boundary.
    fn cookie_with_boundaries() -> CookieStatus {
        let raw = format!("sk-ant-sid01-{}-{}AA", "a".repeat(86), "b".repeat(6));
        let mut cookie = CookieStatus::new(&raw, None).expect("valid test cookie");
        cookie.session_resets_at = Some(1_785_087_657);
        cookie.weekly_resets_at = Some(1_785_600_000);
        cookie.weekly_sonnet_resets_at = Some(1_785_700_000);
        cookie
    }

    /// The frontend deserializes the whole listing in one go, so a single
    /// cookie the live probe could not reach must not make the response
    /// unreadable.
    #[test]
    fn a_cookie_without_live_usage_still_parses_as_the_api_type() {
        let value = cookie_api_value(&cookie_with_boundaries());

        let parsed: CookieStatusApi =
            serde_json::from_value(value).expect("the API type must accept an unprobed cookie");

        assert_eq!(
            parsed.session_resets_at.as_deref(),
            Some("2026-07-26T17:40:57Z"),
            "the stored epoch should reach the UI as the timestamp it was parsed from"
        );
    }

    /// The stored epoch came from parsing the endpoint's RFC 3339 string, so
    /// rendering it back has to produce the same shape the live path sends.
    #[test]
    fn a_rendered_boundary_round_trips_through_the_parser_the_live_path_uses() {
        let value = cookie_api_value(&cookie_with_boundaries());
        let rendered = value["seven_day_resets_at"].as_str().expect("a string");

        let reparsed = chrono::DateTime::parse_from_rfc3339(rendered)
            .expect("the live path parses boundaries with rfc3339")
            .timestamp();

        assert_eq!(reparsed, 1_785_600_000);
    }

    /// An unset boundary is absent, not the epoch.
    #[test]
    fn an_unknown_boundary_stays_absent() {
        let raw = format!("sk-ant-sid01-{}-{}AA", "c".repeat(86), "d".repeat(6));
        let cookie = CookieStatus::new(&raw, None).expect("valid test cookie");

        let parsed: CookieStatusApi =
            serde_json::from_value(cookie_api_value(&cookie)).expect("still parses");

        assert_eq!(parsed.session_resets_at, None);
        assert_eq!(parsed.seven_day_resets_at, None);
        assert_eq!(parsed.seven_day_sonnet_resets_at, None);
    }
}
