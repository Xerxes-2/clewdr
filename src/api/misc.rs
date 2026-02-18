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
    config::{CLEWDR_CONFIG, CookieStatus},
    services::cookie_actor::CookieActorHandle,
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
        .time_to_live(Duration::from_secs(300)) // 5 minutes
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
pub async fn api_post_cookie(
    State(s): State<CookieActorHandle>,
    AuthBearer(t): AuthBearer,
    Json(mut c): Json<CookieStatus>,
) -> Result<StatusCode, ApiError> {
    if !CLEWDR_CONFIG.load().admin_auth(&t) {
        return Err(ApiError::unauthorized());
    }
    c.reset_time = None;
    if c.supports_claude_1m_sonnet.is_none() {
        c.supports_claude_1m_sonnet = Some(true);
    }
    if c.supports_claude_1m_opus.is_none() {
        c.supports_claude_1m_opus = Some(true);
    }
    info!("Cookie accepted: {}", c.cookie);
    match s.submit(c).await {
        Ok(_) => {
            info!("Cookie submitted successfully");
            // Clear cache to ensure fresh data on next request
            COOKIES_CACHE.invalidate(COOKIE_STATUS_CACHE_KEY);
            info!("Cookie status cache invalidated after adding new cookie");
            Ok(StatusCode::OK)
        }
        Err(e) => {
            error!("Failed to submit cookie: {}", e);
            Err(ApiError::internal(format!(
                "Failed to submit cookie: {}",
                e
            )))
        }
    }
}

/// API endpoint to update per-cookie 1M support settings
/// Only updates supports_claude_1m_sonnet / supports_claude_1m_opus on existing cookies
pub async fn api_put_cookie(
    State(s): State<CookieActorHandle>,
    AuthBearer(t): AuthBearer,
    Json(mut c): Json<CookieStatus>,
) -> Result<StatusCode, ApiError> {
    if !CLEWDR_CONFIG.load().admin_auth(&t) {
        return Err(ApiError::unauthorized());
    }
    if c.supports_claude_1m_sonnet.is_none() {
        c.supports_claude_1m_sonnet = Some(true);
    }
    if c.supports_claude_1m_opus.is_none() {
        c.supports_claude_1m_opus = Some(true);
    }

    match s.update_cookie_1m_support(c.clone()).await {
        Ok(_) => {
            info!("Cookie 1M flags updated: {}", c.cookie);
            COOKIES_CACHE.invalidate(COOKIE_STATUS_CACHE_KEY);
            info!("Cookie status cache invalidated after cookie update");
            Ok(StatusCode::OK)
        }
        Err(e) => {
            error!("Failed to update cookie 1M flags: {}", e);
            Err(ApiError::bad_request(format!(
                "Failed to update cookie 1M flags: {}",
                e
            )))
        }
    }
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
pub async fn api_get_cookies(
    State(s): State<CookieActorHandle>,
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
    match s.get_status().await {
        Ok(status) => {
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
        Err(e) => Err(ApiError::internal(format!(
            "Failed to get cookie status: {}",
            e
        ))),
    }
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
pub async fn api_delete_cookie(
    State(s): State<CookieActorHandle>,
    AuthBearer(t): AuthBearer,
    Json(c): Json<CookieStatus>,
) -> Result<StatusCode, ApiError> {
    if !CLEWDR_CONFIG.load().admin_auth(&t) {
        return Err(ApiError::unauthorized());
    }

    match s.delete_cookie(c.to_owned()).await {
        Ok(_) => {
            info!("Cookie deleted successfully: {}", c.cookie);
            // Clear cache to ensure fresh data on next request
            COOKIES_CACHE.invalidate(COOKIE_STATUS_CACHE_KEY);
            info!("Cookie status cache invalidated");
            Ok(StatusCode::NO_CONTENT)
        }
        Err(e) => {
            error!("Failed to delete cookie: {}", e);
            Err(ApiError::internal(format!(
                "Failed to delete cookie: {}",
                e
            )))
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

const MODEL_LIST: [&str; 26] = [
    "claude-3-7-sonnet-20250219",
    "claude-3-7-sonnet-20250219-thinking",
    "claude-sonnet-4-20250514",
    "claude-sonnet-4-20250514-thinking",
    "claude-sonnet-4-20250514-1M",
    "claude-sonnet-4-20250514-1M-thinking",
    "claude-sonnet-4-5-20250929",
    "claude-sonnet-4-5-20250929-thinking",
    "claude-sonnet-4-5-20250929-1M",
    "claude-sonnet-4-5-20250929-1M-thinking",
    "claude-sonnet-4-6",
    "claude-sonnet-4-6-thinking",
    "claude-sonnet-4-6-1M",
    "claude-sonnet-4-6-1M-thinking",
    "claude-opus-4-20250514",
    "claude-opus-4-20250514-thinking",
    "claude-opus-4-1-20250805",
    "claude-opus-4-1-20250805-thinking",
    "claude-opus-4-5-20251101",
    "claude-opus-4-5-20251101-thinking",
    "claude-opus-4-5",
    "claude-opus-4-5-thinking",
    "claude-opus-4-6",
    "claude-opus-4-6-thinking",
    "claude-opus-4-6-1M",
    "claude-opus-4-6-1M-thinking",
];

/// API endpoint to get the list of available models
/// Retrieves the list of models from the configuration
pub async fn api_get_models() -> Json<Value> {
    let data: Vec<Value> = MODEL_LIST
        .iter()
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
use futures::{StreamExt, stream};
use http::HeaderValue;

async fn augment_utilization(cookies: Vec<CookieStatus>, handle: CookieActorHandle) -> Vec<Value> {
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
                    seven_day_opus,
                    opus_reset,
                    seven_day_sonnet,
                    sonnet_reset,
                )) => {
                    let mut obj = base;
                    obj["session_utilization"] = json!(five_hour);
                    obj["session_resets_at"] = json!(five_reset);
                    obj["seven_day_utilization"] = json!(seven_day);
                    obj["seven_day_resets_at"] = json!(seven_reset);
                    obj["seven_day_opus_utilization"] = json!(seven_day_opus);
                    obj["seven_day_opus_resets_at"] = json!(opus_reset);
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
    handle: CookieActorHandle,
) -> Option<(
    u32,
    Option<String>,
    u32,
    Option<String>,
    u32,
    Option<String>,
    u32,
    Option<String>,
)> {
    let mut state = ClaudeCodeState::from_cookie(handle, cookie).ok()?;
    let usage = state.fetch_usage_metrics().await.ok()?;
    state.return_cookie(None).await;
    let five = usage
        .get("five_hour")
        .and_then(|o| o.get("utilization"))
        .and_then(|v| v.as_f64())
        .map(|v| v.round() as u32)
        .unwrap_or(0);
    let five_reset = usage
        .get("five_hour")
        .and_then(|o| o.get("resets_at"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let seven = usage
        .get("seven_day")
        .and_then(|o| o.get("utilization"))
        .and_then(|v| v.as_f64())
        .map(|v| v.round() as u32)
        .unwrap_or(0);
    let seven_reset = usage
        .get("seven_day")
        .and_then(|o| o.get("resets_at"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let seven_opus = usage
        .get("seven_day_opus")
        .and_then(|o| o.get("utilization"))
        .and_then(|v| v.as_f64())
        .map(|v| v.round() as u32)
        .unwrap_or(0);
    let opus_reset = usage
        .get("seven_day_opus")
        .and_then(|o| o.get("resets_at"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let seven_sonnet = usage
        .get("seven_day_sonnet")
        .and_then(|o| o.get("utilization"))
        .and_then(|v| v.as_f64())
        .map(|v| v.round() as u32)
        .unwrap_or(0);
    let sonnet_reset = usage
        .get("seven_day_sonnet")
        .and_then(|o| o.get("resets_at"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    Some((
        five,
        five_reset,
        seven,
        seven_reset,
        seven_opus,
        opus_reset,
        seven_sonnet,
        sonnet_reset,
    ))
}
