use axum::{Router, routing::{get, post}, middleware::{from_extractor, map_response}};
use tower::ServiceBuilder;
use tower_http::compression::CompressionLayer;

use crate::{
    api::*,
    claude_code_state::ClaudeCodeState,
    middleware::{RequireBearerAuth, RequireXApiKeyAuth, claude::to_oai},
};

pub fn build_claude_code_router(state: ClaudeCodeState) -> Router {
    Router::new()
        .route("/code/v1/messages", post(api_claude_code))
        .layer(
            ServiceBuilder::new()
                .layer(from_extractor::<RequireXApiKeyAuth>())
                .layer(CompressionLayer::new()),
        )
        .with_state(state)
}

pub fn build_claude_code_oai_router(state: ClaudeCodeState) -> Router {
    Router::new()
        .route("/code/v1/chat/completions", post(api_claude_code))
        .route("/code/v1/models", get(api_get_models))
        .layer(
            ServiceBuilder::new()
                .layer(from_extractor::<RequireBearerAuth>())
                .layer(CompressionLayer::new())
                .layer(map_response(to_oai)),
        )
        .with_state(state)
}

