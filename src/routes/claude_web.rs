use axum::{Router, routing::{get, post}, middleware::{from_extractor, map_response}};
use tower::ServiceBuilder;
use tower_http::compression::CompressionLayer;

use crate::{
    api::*,
    claude_web_state::ClaudeWebState,
    middleware::{
        RequireBearerAuth, RequireXApiKeyAuth,
        claude::{add_usage_info, apply_stop_sequences, check_overloaded, to_oai},
    },
};

pub fn build_claude_web_router(state: ClaudeWebState) -> Router {
    Router::new()
        .route("/v1/messages", post(api_claude_web))
        .layer(
            ServiceBuilder::new()
                .layer(from_extractor::<RequireXApiKeyAuth>())
                .layer(CompressionLayer::new())
                .layer(map_response(add_usage_info))
                .layer(map_response(apply_stop_sequences))
                .layer(map_response(check_overloaded)),
        )
        .with_state(state)
}

pub fn build_claude_web_oai_router(state: ClaudeWebState) -> Router {
    Router::new()
        .route("/v1/chat/completions", post(api_claude_web))
        .route("/v1/models", get(api_get_models))
        .layer(
            ServiceBuilder::new()
                .layer(from_extractor::<RequireBearerAuth>())
                .layer(CompressionLayer::new())
                .layer(map_response(to_oai))
                .layer(map_response(apply_stop_sequences))
                .layer(map_response(check_overloaded)),
        )
        .with_state(state)
}

