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

async fn augment_utilization(cookies: Vec<CookieStatus>, handle: CookiePool) -> Vec<Value> {
    let concurrency = 5usize;
    stream::iter(cookies.into_iter().map(move |cookie| {
        let handle = handle.clone();
        async move {
            let base = serde_json::to_value(&cookie).unwrap_or(json!({}));
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
