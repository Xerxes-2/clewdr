use axum::Json;
use axum_auth::AuthBearer;
use clewdr_types::ConfigApi;
use serde_json::json;

use super::error::ApiError;
use crate::config::{CLEWDR_CONFIG, ClewdrConfig};

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
/// The cookie collections are carried over from the old config rather than
/// taken from the request, so saving settings cannot clobber them.
///
/// # Errors
/// [`ApiError::unauthorized`] if the bearer token is not the admin password,
/// or [`ApiError::internal`] if the new config cannot be written to disk.
pub async fn api_post_config(
    AuthBearer(t): AuthBearer,
    Json(c): Json<ConfigApi>,
) -> Result<Json<serde_json::Value>, ApiError> {
    if !CLEWDR_CONFIG.load().admin_auth(&t) {
        return Err(ApiError::unauthorized());
    }
    let c: ClewdrConfig = ClewdrConfig::from(c).validate();
    CLEWDR_CONFIG.rcu(|old_c| {
        let mut new_c = ClewdrConfig::clone(&c);
        new_c.cookie_array.clone_from(&old_c.cookie_array);
        new_c.wasted_cookie.clone_from(&old_c.wasted_cookie);
        new_c
    });
    if let Err(e) = CLEWDR_CONFIG.load().save().await {
        return Err(ApiError::internal(format!("Failed to save config: {e}")));
    }

    Ok(Json(json!({
        "message": "Config updated successfully",
        "config": ConfigApi::from(&c)
    })))
}
