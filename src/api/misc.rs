use axum::{Json, extract::State};
use axum_auth::AuthBearer;
use serde_json::{Value, json};
use tracing::{error, info, warn};
use wreq::StatusCode;
use super::error::ApiError;

use crate::{
    VERSION_INFO,
    config::{CLEWDR_CONFIG, CookieStatus, KeyStatus},
    services::{
        cookie_actor::{CookieActorHandle, CookieStatusInfo},
        key_actor::{KeyActorHandle, KeyStatusInfo},
        cli_token_actor::{CliTokenActorHandle, CliTokenStatusInfo},
    },
};

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
) -> StatusCode {
    if !CLEWDR_CONFIG.load().admin_auth(&t) {
        return StatusCode::UNAUTHORIZED;
    }
    c.reset_time = None;
    info!("Cookie accepted: {}", c.cookie);
    match s.submit(c).await {
        Ok(_) => {
            info!("Cookie submitted successfully");
            StatusCode::OK
        }
        Err(e) => {
            error!("Failed to submit cookie: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        }
    }
}

pub async fn api_post_key(
    State(s): State<KeyActorHandle>,
    AuthBearer(t): AuthBearer,
    Json(c): Json<KeyStatus>,
) -> StatusCode {
    if !CLEWDR_CONFIG.load().admin_auth(&t) {
        return StatusCode::UNAUTHORIZED;
    }
    if !c.key.validate() {
        warn!("Invalid key: {}", c.key);
        return StatusCode::BAD_REQUEST;
    }
    info!("Key accepted: {}", c.key);
    match s.submit(c).await {
        Ok(_) => {
            info!("Key submitted successfully");
            StatusCode::OK
        }
        Err(e) => {
            error!("Failed to submit key: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        }
    }
}

pub async fn api_post_cli_token(
    State(s): State<CliTokenActorHandle>,
    AuthBearer(t): AuthBearer,
    Json(body): Json<Value>,
) -> StatusCode {
    if !CLEWDR_CONFIG.load().admin_auth(&t) {
        return StatusCode::UNAUTHORIZED;
    }
    // Accept either raw token JSON {"token":"ya29..."} or full OAuth JSON
    let token = body
        .get("token")
        .and_then(|v| v.as_str())
        .or_else(|| body.get("access_token").and_then(|v| v.as_str()))
        .map(|s| s.to_string());
    let Some(token) = token else {
        warn!("CLI token submission missing token/access_token field");
        return StatusCode::BAD_REQUEST;
    };

    // Optional metadata for refresh
    use chrono::{DateTime, Utc};
    let expiry = body
        .get("expiry")
        .and_then(|v| v.as_str())
        .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
        .map(|dt| dt.with_timezone(&Utc));
    let meta = crate::config::CliOAuthMeta {
        client_id: body.get("client_id").and_then(|v| v.as_str()).map(|s| s.to_string()),
        client_secret: body.get("client_secret").and_then(|v| v.as_str()).map(|s| s.to_string()),
        refresh_token: body.get("refresh_token").and_then(|v| v.as_str()).map(|s| s.to_string()),
        token_uri: body.get("token_uri").and_then(|v| v.as_str()).map(|s| s.to_string()),
        scopes: body
            .get("scopes")
            .and_then(|v| v.as_array())
            .map(|arr| arr.iter().filter_map(|x| x.as_str().map(|s| s.to_string())).collect()),
        project_id: body.get("project_id").and_then(|v| v.as_str()).map(|s| s.to_string()),
    };
    let status = crate::config::CliTokenStatus { token: token.into(), count_403: 0, expiry, meta: Some(meta) };
    info!("CLI token accepted: {}", status.token.ellipse());
    match s.submit(status).await {
        Ok(_) => StatusCode::OK,
        Err(e) => {
            error!("Failed to submit CLI token: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        }
    }
}

/// API endpoint to retrieve all cookies and their status
/// Gets information about valid, exhausted, and invalid cookies
///
/// # Arguments
/// * `s` - Application state containing event sender
/// * `t` - Auth bearer token for admin authentication
///
/// # Returns
/// * `Result<Json<CookieStatusInfo>, (StatusCode, Json<serde_json::Value>)>` - Cookie status info or error
pub async fn api_get_cookies(
    State(s): State<CookieActorHandle>,
    AuthBearer(t): AuthBearer,
) -> Result<Json<CookieStatusInfo>, ApiError> {
    if !CLEWDR_CONFIG.load().admin_auth(&t) {
        return Err(ApiError::unauthorized());
    }

    match s.get_status().await {
        Ok(status) => Ok(Json(status)),
        Err(e) => Err(ApiError::internal(format!("Failed to get cookie status: {}", e))),
    }
}

pub async fn api_get_keys(
    State(s): State<KeyActorHandle>,
    AuthBearer(t): AuthBearer,
) -> Result<Json<KeyStatusInfo>, ApiError> {
    if !CLEWDR_CONFIG.load().admin_auth(&t) {
        return Err(ApiError::unauthorized());
    }

    match s.get_status().await {
        Ok(status) => Ok(Json(status)),
        Err(e) => Err(ApiError::internal(format!("Failed to get keys status: {}", e))),
    }
}

pub async fn api_get_cli_tokens(
    State(s): State<CliTokenActorHandle>,
    AuthBearer(t): AuthBearer,
) -> Result<Json<CliTokenStatusInfo>, ApiError> {
    if !CLEWDR_CONFIG.load().admin_auth(&t) {
        return Err(ApiError::unauthorized());
    }
    match s.get_status().await {
        Ok(status) => Ok(Json(status)),
        Err(e) => Err(ApiError::internal(format!("Failed to get CLI tokens status: {}", e))),
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
            Ok(StatusCode::NO_CONTENT)
        }
        Err(e) => {
            error!("Failed to delete cookie: {}", e);
            Err(ApiError::internal(format!("Failed to delete cookie: {}", e)))
        }
    }
}

pub async fn api_delete_key(
    State(s): State<KeyActorHandle>,
    AuthBearer(t): AuthBearer,
    Json(c): Json<KeyStatus>,
) -> Result<StatusCode, ApiError> {
    if !CLEWDR_CONFIG.load().admin_auth(&t) {
        return Err(ApiError::unauthorized());
    }
    if !c.key.validate() {
        warn!("Invalid key: {}", c.key);
        return Err(ApiError::bad_request("Invalid key"));
    }

    match s.delete_key(c.to_owned()).await {
        Ok(_) => {
            info!("Key deleted successfully: {}", c.key);
            Ok(StatusCode::NO_CONTENT)
        }
        Err(e) => {
            error!("Failed to delete key: {}", e);
            Err(ApiError::internal(format!("Failed to delete key: {}", e)))
        }
    }
}

pub async fn api_delete_cli_token(
    State(s): State<CliTokenActorHandle>,
    AuthBearer(t): AuthBearer,
    Json(c): Json<crate::config::CliTokenStatus>,
) -> Result<StatusCode, ApiError> {
    if !CLEWDR_CONFIG.load().admin_auth(&t) {
        return Err(ApiError::unauthorized());
    }
    match s.delete(c.to_owned()).await {
        Ok(_) => Ok(StatusCode::NO_CONTENT),
        Err(e) => Err(ApiError::internal(format!("Failed to delete CLI token: {}", e))),
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

const MODEL_LIST: [&str; 10] = [
    "claude-3-7-sonnet-20250219",
    "claude-3-7-sonnet-20250219-thinking",
    "claude-sonnet-4-20250514",
    "claude-sonnet-4-20250514-thinking",
    "claude-sonnet-4-20250514-1M",
    "claude-sonnet-4-20250514-1M-thinking",
    "claude-opus-4-20250514",
    "claude-opus-4-20250514-thinking",
    "claude-opus-4-1-20250805",
    "claude-opus-4-1-20250805-thinking",
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
