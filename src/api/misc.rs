use super::error::ApiError;
use axum::{Json, extract::State};
use axum_auth::AuthBearer;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tracing::{error, info, warn};
use wreq::StatusCode;
use yup_oauth2::ServiceAccountKey;

use crate::{
    VERSION_INFO,
    config::{CLEWDR_CONFIG, ClewdrConfig, CookieStatus, KeyStatus},
    persistence,
    services::{
        cookie_actor::{CookieActorHandle, CookieStatusInfo},
        key_actor::{KeyActorHandle, KeyStatusInfo},
    },
};

const DB_UNAVAILABLE_MESSAGE: &str = "Database storage is unavailable";

#[derive(Deserialize)]
pub struct VertexCredentialPayload {
    pub credential: ServiceAccountKey,
}

#[derive(Deserialize)]
pub struct VertexCredentialDeletePayload {
    pub client_email: String,
}

#[derive(Serialize)]
pub struct VertexCredentialInfo {
    pub client_email: String,
    pub project_id: Option<String>,
    pub private_key_id: Option<String>,
}

async fn ensure_db_writable() -> Result<(), ApiError> {
    let storage = persistence::storage();
    if !storage.is_enabled() {
        return Ok(());
    }

    match storage.status().await {
        Ok(status) => {
            let is_healthy = status
                .get("healthy")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            if is_healthy {
                return Ok(());
            }

            if let Some(detail) = status
                .get("error")
                .and_then(|v| v.as_str())
                .or_else(|| status.get("last_error").and_then(|v| v.as_str()))
            {
                warn!("Database health check failed: {detail}");
            }
        }
        Err(e) => {
            warn!("Database status fetch failed: {}", e);
        }
    }

    Err(ApiError::service_unavailable(DB_UNAVAILABLE_MESSAGE))
}

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
    ensure_db_writable().await?;
    c.reset_time = None;
    info!("Cookie accepted: {}", c.cookie);
    match s.submit(c).await {
        Ok(_) => {
            info!("Cookie submitted successfully");
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

pub async fn api_post_key(
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
    ensure_db_writable().await?;
    info!("Key accepted: {}", c.key);
    match s.submit(c).await {
        Ok(_) => {
            info!("Key submitted successfully");
            Ok(StatusCode::OK)
        }
        Err(e) => {
            error!("Failed to submit key: {}", e);
            Err(ApiError::internal(format!("Failed to submit key: {}", e)))
        }
    }
}

pub async fn api_get_vertex_credentials(
    AuthBearer(t): AuthBearer,
) -> Result<Json<Vec<VertexCredentialInfo>>, ApiError> {
    if !CLEWDR_CONFIG.load().admin_auth(&t) {
        return Err(ApiError::unauthorized());
    }

    let infos = CLEWDR_CONFIG
        .load()
        .vertex
        .credential_list()
        .into_iter()
        .map(|cred| VertexCredentialInfo {
            client_email: cred.client_email.clone(),
            project_id: cred.project_id.clone(),
            private_key_id: cred.private_key_id.clone(),
        })
        .collect();

    Ok(Json(infos))
}

pub async fn api_post_vertex_credential(
    AuthBearer(t): AuthBearer,
    Json(payload): Json<VertexCredentialPayload>,
) -> Result<StatusCode, ApiError> {
    if !CLEWDR_CONFIG.load().admin_auth(&t) {
        return Err(ApiError::unauthorized());
    }
    ensure_db_writable().await?;
    let client_email = payload.credential.client_email.clone();
    if client_email.trim().is_empty() {
        return Err(ApiError::bad_request("client_email is required"));
    }

    CLEWDR_CONFIG.rcu(|config| {
        let mut new_config = ClewdrConfig::clone(config);
        new_config
            .vertex
            .credentials
            .retain(|cred| !cred.client_email.eq_ignore_ascii_case(&client_email));
        new_config
            .vertex
            .credentials
            .push(payload.credential.clone());
        new_config = new_config.validate();
        new_config
    });

    if let Err(e) = CLEWDR_CONFIG.load().save().await {
        error!("Failed to persist vertex credential: {}", e);
        return Err(ApiError::internal(format!(
            "Failed to persist vertex credential: {}",
            e
        )));
    }

    info!("Vertex credential accepted: {}", client_email);
    Ok(StatusCode::OK)
}

pub async fn api_delete_vertex_credential(
    AuthBearer(t): AuthBearer,
    Json(payload): Json<VertexCredentialDeletePayload>,
) -> Result<StatusCode, ApiError> {
    if !CLEWDR_CONFIG.load().admin_auth(&t) {
        return Err(ApiError::unauthorized());
    }
    ensure_db_writable().await?;

    let exists = CLEWDR_CONFIG
        .load()
        .vertex
        .credential_list()
        .iter()
        .any(|cred| {
            cred.client_email
                .eq_ignore_ascii_case(&payload.client_email)
        });

    if !exists {
        return Err(ApiError::bad_request("Credential not found"));
    }

    CLEWDR_CONFIG.rcu(|config| {
        let mut new_config = ClewdrConfig::clone(config);
        new_config.vertex.credentials.retain(|cred| {
            !cred
                .client_email
                .eq_ignore_ascii_case(&payload.client_email)
        });
        new_config = new_config.validate();
        new_config
    });

    if let Err(e) = CLEWDR_CONFIG.load().save().await {
        error!("Failed to delete vertex credential: {}", e);
        return Err(ApiError::internal(format!(
            "Failed to delete vertex credential: {}",
            e
        )));
    }

    info!("Vertex credential deleted: {}", payload.client_email);
    Ok(StatusCode::NO_CONTENT)
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
        Err(e) => Err(ApiError::internal(format!(
            "Failed to get cookie status: {}",
            e
        ))),
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
        Err(e) => Err(ApiError::internal(format!(
            "Failed to get keys status: {}",
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

    ensure_db_writable().await?;

    match s.delete_cookie(c.to_owned()).await {
        Ok(_) => {
            info!("Cookie deleted successfully: {}", c.cookie);
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

    ensure_db_writable().await?;

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
