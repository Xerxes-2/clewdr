use axum::Json;
use axum_auth::AuthBearer;
use serde_json::json;
// StatusCode not needed; using ApiError for responses

use crate::{config::CLEWDR_CONFIG, persistence};
use super::error::ApiError;

/// Import configuration and runtime state from file into the database
/// Only available when compiled with `db` feature and DB mode enabled.
pub async fn api_storage_import(
    AuthBearer(t): AuthBearer,
) -> Result<Json<serde_json::Value>, ApiError> {
    if !CLEWDR_CONFIG.load().admin_auth(&t) {
        return Err(ApiError::unauthorized());
    }
    if persistence::storage().is_enabled() {
        match persistence::storage().import_from_file().await {
            Ok(v) => Ok(Json(v)),
            Err(e) => Err(ApiError::internal(e.to_string())),
        }
    } else {
        Err(ApiError::not_implemented("DB feature not enabled"))
    }
}

/// Export configuration and runtime state from database into the file
/// Only available when compiled with `db` feature and DB mode enabled.
pub async fn api_storage_export(
    AuthBearer(t): AuthBearer,
) -> Result<Json<serde_json::Value>, ApiError> {
    if !CLEWDR_CONFIG.load().admin_auth(&t) {
        return Err(ApiError::unauthorized());
    }
    if persistence::storage().is_enabled() {
        match persistence::storage().export_to_file().await {
            Ok(v) => Ok(Json(v)),
            Err(e) => Err(ApiError::internal(e.to_string())),
        }
    } else {
        Err(ApiError::not_implemented("DB feature not enabled"))
    }
}

/// DB status: enabled/mode/healthy/details/metrics
pub async fn api_storage_status() -> Json<serde_json::Value> {
    if persistence::storage().is_enabled() {
        if let Ok(s) = persistence::storage().status().await {
            return Json(s);
        }
    }
    Json(json!({
        "enabled": false,
        "mode": "file",
        "healthy": false,
    }))
}
