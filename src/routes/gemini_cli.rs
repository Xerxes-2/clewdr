use axum::{Router, routing::{get, post}, middleware::from_extractor};
use tower_http::compression::CompressionLayer;

use crate::{
    api::*,
    gemini_state::GeminiState,
    middleware::{RequireBearerAuth, RequireGeminiCliAuth},
};

pub fn build_gemini_cli_router(state: GeminiState) -> Router {
    // Native Gemini format under /gemini/cli prefix
    let native = Router::new()
        .route("/gemini/cli/v1/v1beta/{*path}", post(api_post_gemini_cli))
        .route("/gemini/cli/vertex/v1beta/{*path}", post(api_post_gemini_cli))
        .route("/gemini/cli/v1/models", get(api_gemini_cli_models))
        .route("/gemini/cli/v1beta/models", get(api_gemini_cli_models))
        .route("/gemini/cli/v1/models/{*path}", get(api_gemini_cli_model_info))
        .layer(from_extractor::<RequireGeminiCliAuth>())
        .layer(CompressionLayer::new())
        .with_state(state.to_owned());

    // OpenAI compatible format under /gemini/cli prefix
    let oai = Router::new()
        .route("/gemini/cli/chat/completions", post(api_post_gemini_cli_oai))
        .route(
            "/gemini/cli/vertex/chat/completions",
            post(api_post_gemini_cli_oai),
        )
        .layer(from_extractor::<RequireBearerAuth>())
        .layer(CompressionLayer::new())
        .with_state(state);

    native.merge(oai)
}
