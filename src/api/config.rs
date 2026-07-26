use std::sync::Arc;

use axum::{Json, extract::State};
use axum_auth::AuthBearer;
use clewdr_types::ConfigApi;
use serde_json::json;

use super::error::ApiError;
use crate::{
    config::{CLEWDR_CONFIG, ClewdrConfig},
    services::cookie_pool::CookiePool,
};

/// Return the current configuration, with secrets already elided by the
/// [`ConfigApi`] conversion.
///
/// # Errors
/// [`ApiError::unauthorized`] if the bearer token is not the admin password.
pub async fn api_get_config(AuthBearer(t): AuthBearer) -> Result<Json<ConfigApi>, ApiError> {
    if !CLEWDR_CONFIG.load().admin_auth(&t) {
        return Err(ApiError::unauthorized());
    }

    let api: ConfigApi = CLEWDR_CONFIG.load().as_ref().into();
    Ok(Json(api))
}

/// Replace the configuration and persist it.
///
/// Cookies are unaffected: they belong to the pool, and are only recombined
/// with the settings when the file is written.
///
/// # Errors
/// [`ApiError::unauthorized`] if the bearer token is not the admin password,
/// or [`ApiError::internal`] if the new config cannot be written to disk.
pub async fn api_post_config(
    State(pool): State<CookiePool>,
    AuthBearer(t): AuthBearer,
    Json(c): Json<ConfigApi>,
) -> Result<Json<serde_json::Value>, ApiError> {
    if !CLEWDR_CONFIG.load().admin_auth(&t) {
        return Err(ApiError::unauthorized());
    }
    let c: ClewdrConfig = ClewdrConfig::from(c).validate();
    CLEWDR_CONFIG.store(Arc::new(c.clone()));
    if let Err(e) = CLEWDR_CONFIG.load().save(&pool.snapshot()).await {
        return Err(ApiError::internal(format!("Failed to save config: {e}")));
    }

    Ok(Json(json!({
        "message": "Config updated successfully",
        "config": ConfigApi::from(&c)
    })))
}
