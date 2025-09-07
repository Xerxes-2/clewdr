use axum::{Router, routing::post, middleware::from_extractor};
use tower_http::compression::CompressionLayer;

use crate::{
    api::*,
    gemini_state::GeminiState,
    middleware::{RequireBearerAuth, RequireQueryKeyAuth},
};

pub fn build_gemini_router(state: GeminiState) -> Router {
    let router_gemini = Router::new()
        .route("/v1/v1beta/{*path}", post(api_post_gemini))
        .route("/v1/vertex/v1beta/{*path}", post(api_post_gemini))
        .layer(from_extractor::<RequireQueryKeyAuth>())
        .layer(CompressionLayer::new())
        .with_state(state.to_owned());
    let router_oai = Router::new()
        .route("/gemini/chat/completions", post(api_post_gemini_oai))
        .route("/gemini/vertex/chat/completions", post(api_post_gemini_oai))
        .layer(from_extractor::<RequireBearerAuth>())
        .layer(CompressionLayer::new())
        .with_state(state);
    router_gemini.merge(router_oai)
}

