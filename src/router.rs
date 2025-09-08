use axum::{Router, http::Method};
use tower_http::cors::CorsLayer;

use crate::{
    claude_code_state::ClaudeCodeState,
    claude_web_state::ClaudeWebState,
    gemini_state::GeminiState,
    services::{cookie_actor::CookieActorHandle, key_actor::KeyActorHandle, cli_token_actor::CliTokenActorHandle},
};
use crate::routes::{
    build_admin_router,
    build_claude_code_oai_router,
    build_claude_code_router,
    build_claude_web_oai_router,
    build_claude_web_router,
    build_gemini_router,
    build_gemini_cli_router,
};

/// RouterBuilder for the application
pub struct RouterBuilder {
    claude_web_state: ClaudeWebState,
    claude_code_state: ClaudeCodeState,
    cookie_actor_handle: CookieActorHandle,
    key_actor_handle: KeyActorHandle,
    cli_token_handle: CliTokenActorHandle,
    gemini_state: GeminiState,
    inner: Router,
}

impl RouterBuilder {
    /// Creates a blank RouterBuilder instance
    /// Initializes the router with the provided application state
    ///
    /// # Arguments
    /// * `state` - The application state containing client information
    pub async fn new() -> Self {
        let cookie_handle = CookieActorHandle::start()
            .await
            .expect("Failed to start CookieActor");
        let claude_web_state = ClaudeWebState::new(cookie_handle.to_owned());
        let claude_code_state = ClaudeCodeState::new(cookie_handle.to_owned());
        let key_tx = KeyActorHandle::start()
            .await
            .expect("Failed to start KeyActorHandle");
        let cli_token_tx = CliTokenActorHandle::start()
            .await
            .expect("Failed to start CliTokenActorHandle");
        let gemini_state = GeminiState::new(key_tx.to_owned(), cli_token_tx.to_owned());
        // Background DB sync (keys/cookies) for multi-instance eventual consistency
        let _bg = crate::services::sync::spawn(cookie_handle.clone(), key_tx.clone());
        RouterBuilder {
            claude_web_state,
            claude_code_state,
            cookie_actor_handle: cookie_handle,
            key_actor_handle: key_tx,
            cli_token_handle: cli_token_tx,
            gemini_state,
            inner: Router::new(),
        }
    }

    /// Creates a new RouterBuilder instance
    /// Sets up routes for API endpoints and static file serving
    pub fn with_default_setup(mut self) -> Self {
        // compose domain routers then apply common layers
        let composed = Router::new()
            .merge(build_gemini_router(self.gemini_state.to_owned()))
            // CLI-dedicated Gemini routes with separate prefix to avoid confusion
            .merge(build_gemini_cli_router(self.gemini_state.to_owned()))
            .merge(build_claude_web_router(self.claude_web_state.to_owned().with_claude_format()))
            .merge(build_claude_code_router(self.claude_code_state.to_owned()))
            .merge(build_claude_web_oai_router(self.claude_web_state.to_owned().with_openai_format()))
            .merge(build_claude_code_oai_router(self.claude_code_state.to_owned()))
            .merge(build_admin_router(
                self.cookie_actor_handle.to_owned(),
                self.key_actor_handle.to_owned(),
                self.cli_token_handle.to_owned(),
            ));
        self.inner = self.inner.merge(composed);
        self.setup_static_serving().with_tower_trace().with_cors()
    }

    // legacy builder methods replaced by helpers; kept for clarity

    /// Sets up routes for v1 endpoints

    /// Sets up routes for v1 endpoints

    /// Sets up routes for API endpoints

    /// Sets up routes for OpenAI compatible endpoints

    /// Sets up routes for OpenAI compatible endpoints
    // builder methods removed in favor of helper composition

    /// Sets up static file serving
    fn setup_static_serving(mut self) -> Self {
        #[cfg(feature = "embed-resource")]
        {
            use include_dir::{Dir, include_dir};
            const INCLUDE_STATIC: Dir = include_dir!("$CARGO_MANIFEST_DIR/static");
            self.inner = self
                .inner
                .fallback_service(tower_serve_static::ServeDir::new(&INCLUDE_STATIC));
        }
        #[cfg(feature = "external-resource")]
        {
            use const_format::formatc;
            use tower_http::services::ServeDir;
            self.inner = self.inner.fallback_service(ServeDir::new(formatc!(
                "{}/static",
                env!("CARGO_MANIFEST_DIR")
            )));
        }
        self
    }

    /// Adds CORS support to the router
    fn with_cors(mut self) -> Self {
        use axum::http::header::{AUTHORIZATION, CONTENT_TYPE};
        use http::header::HeaderName;

        let cors = CorsLayer::new()
            .allow_origin(tower_http::cors::Any)
            .allow_methods([Method::GET, Method::POST, Method::DELETE])
            .allow_headers([
                AUTHORIZATION,
                CONTENT_TYPE,
                HeaderName::from_static("x-api-key"),
            ]);

        self.inner = self.inner.layer(cors);
        self
    }

    fn with_tower_trace(mut self) -> Self {
        use tower_http::trace::TraceLayer;

        let layer = TraceLayer::new_for_http();

        self.inner = self.inner.layer(layer);
        self
    }

    /// Returns the configured router
    /// Finalizes the router configuration for use with axum
    pub fn build(self) -> Router {
        self.inner
    }
}

// helper builders moved to crate::routes
