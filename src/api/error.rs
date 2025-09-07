use axum::{Json, response::{IntoResponse, Response}};
use wreq::StatusCode;

#[derive(Debug, Clone)]
pub struct ApiError {
    pub code: StatusCode,
    pub body: serde_json::Value,
}

impl ApiError {
    pub fn unauthorized() -> Self {
        Self { code: StatusCode::UNAUTHORIZED, body: serde_json::json!({"error": "Unauthorized"}) }
    }
    pub fn bad_request(msg: impl Into<String>) -> Self {
        Self { code: StatusCode::BAD_REQUEST, body: serde_json::json!({"error": msg.into()}) }
    }
    pub fn internal(msg: impl Into<String>) -> Self {
        Self { code: StatusCode::INTERNAL_SERVER_ERROR, body: serde_json::json!({"error": msg.into()}) }
    }
    pub fn not_implemented(msg: impl Into<String>) -> Self {
        Self { code: StatusCode::NOT_IMPLEMENTED, body: serde_json::json!({"error": msg.into()}) }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.code, Json(self.body)).into_response()
    }
}

