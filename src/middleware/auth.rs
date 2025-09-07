use axum::extract::FromRequestParts;
use axum_auth::AuthBearer;
use tracing::warn;

use super::gemini::GeminiArgs;
use crate::{config::CLEWDR_CONFIG, error::ClewdrError};

/// Extractor for the X-API-Key header used in Claude API compatibility
///
/// This struct extracts the API key from the "x-api-key" header and makes it
/// available to handlers that need to verify Claude-style authentication.
struct XApiKey(pub String);

impl<S> FromRequestParts<S> for XApiKey
where
    S: Sync,
{
    type Rejection = ClewdrError;
    async fn from_request_parts(
        parts: &mut axum::http::request::Parts,
        _: &S,
    ) -> Result<Self, Self::Rejection> {
        let key = parts
            .headers
            .get("x-api-key")
            .and_then(|v| v.to_str().ok())
            .ok_or(ClewdrError::InvalidAuth)?;
        Ok(Self(key.to_string()))
    }
}

pub struct RequireQueryKeyAuth;
impl<S> FromRequestParts<S> for RequireQueryKeyAuth
where
    S: Sync,
{
    type Rejection = ClewdrError;
    async fn from_request_parts(
        parts: &mut axum::http::request::Parts,
        _: &S,
    ) -> Result<Self, Self::Rejection> {
        let query = GeminiArgs::from_request_parts(parts, &()).await?;
        if !CLEWDR_CONFIG.load().user_auth(&query.key) {
            warn!("Invalid query key: {}", query.key);
            return Err(ClewdrError::InvalidAuth);
        }
        Ok(Self)
    }
}

/// Middleware guard that ensures requests have valid admin authentication
///
/// This extractor checks for a valid admin authorization token in the Bearer Auth header.
/// It can be used on routes that should only be accessible to administrators.
///
/// # Example
///
/// ```no_run
/// use axum::response::IntoResponse;
/// use clewdr::middleware::RequireAdminAuth;
///
/// async fn admin_only_handler(_: RequireAdminAuth) -> impl IntoResponse {
///     // This handler only executes if admin authentication succeeds
///     axum::http::StatusCode::OK
/// }
/// ```
pub struct RequireAdminAuth;
impl<S> FromRequestParts<S> for RequireAdminAuth
where
    S: Sync,
{
    type Rejection = ClewdrError;
    async fn from_request_parts(
        parts: &mut axum::http::request::Parts,
        _: &S,
    ) -> Result<Self, Self::Rejection> {
        let AuthBearer(key) = AuthBearer::from_request_parts(parts, &())
            .await
            .map_err(|_| ClewdrError::InvalidAuth)?;
        if !CLEWDR_CONFIG.load().admin_auth(&key) {
            warn!("Invalid admin key");
            return Err(ClewdrError::InvalidAuth);
        }
        Ok(Self)
    }
}

/// Middleware guard that ensures requests have valid OpenAI API authentication
///
/// This extractor validates the Bearer token against the configured OpenAI API keys.
/// It's used to protect OpenAI-compatible API endpoints.
///
/// # Example
///
/// ```no_run
/// use axum::response::IntoResponse;
/// use clewdr::middleware::RequireBearerAuth;
///
/// async fn openai_handler(_: RequireBearerAuth) -> impl IntoResponse {
///     // This handler only executes if OpenAI authentication succeeds
///     axum::http::StatusCode::OK
/// }
/// ```
pub struct RequireBearerAuth;
impl<S> FromRequestParts<S> for RequireBearerAuth
where
    S: Sync,
{
    type Rejection = ClewdrError;
    async fn from_request_parts(
        parts: &mut axum::http::request::Parts,
        _: &S,
    ) -> Result<Self, Self::Rejection> {
        let AuthBearer(key) = AuthBearer::from_request_parts(parts, &())
            .await
            .map_err(|_| ClewdrError::InvalidAuth)?;
        if !CLEWDR_CONFIG.load().user_auth(&key) {
            warn!("Invalid Bearer key: {}", key);
            return Err(ClewdrError::InvalidAuth);
        }
        Ok(Self)
    }
}

/// Middleware guard that ensures requests have valid Claude API authentication
///
/// This extractor validates the X-API-Key header against the configured API keys.
/// It's used to protect Claude-compatible API endpoints.
pub struct RequireXApiKeyAuth;
impl<S> FromRequestParts<S> for RequireXApiKeyAuth
where
    S: Sync,
{
    type Rejection = ClewdrError;
    async fn from_request_parts(
        parts: &mut axum::http::request::Parts,
        _: &S,
    ) -> Result<Self, Self::Rejection> {
        let XApiKey(key) = XApiKey::from_request_parts(parts, &()).await?;
        if !CLEWDR_CONFIG.load().user_auth(&key) {
            warn!("Invalid x-api-key: {}", key);
            return Err(ClewdrError::InvalidAuth);
        }
        Ok(Self)
    }
}
