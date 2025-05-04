use axum::{
    Router,
    http::Method,
    middleware::{from_extractor, map_response},
    routing::{delete, get, post},
};
use const_format::formatc;
use tower::ServiceBuilder;
use tower_http::{cors::CorsLayer, services::ServeDir};

use crate::{
    IS_DEBUG,
    api::{
        api_auth, api_delete_cookie, api_get_config, api_get_cookies, api_messages,
        api_post_config, api_post_cookie, api_version,
    },
    config::CLEWDR_CONFIG,
    middleware::{
        RequireAdminAuth, RequireClaudeAuth, RequireOaiAuth,
        transform_oai_response, handle_stop_sequences
    },
    context::RequestContext,
};

/// RouterBuilder for the application
pub struct RouterBuilder<S = RequestContext> {
    state: S,
    inner: Router<S>,
}

impl RouterBuilder {
    /// Creates a blank RouterBuilder instance
    /// Initializes the router with the provided application state
    ///
    /// # Arguments
    /// * `state` - The application state containing client information
    pub fn new(state: RequestContext) -> Self {
        RouterBuilder {
            state,
            inner: Router::new(),
        }
    }

    /// Creates a new RouterBuilder instance
    /// Sets up routes for API endpoints and static file serving
    pub fn with_default_setup(self) -> Self {
        self.route_claude_endpoints()
            .route_api_endpoints()
            .route_openai_endpoints()
            .setup_static_serving()
            .with_cors()
    }

    /// Sets up routes for v1 endpoints
    fn route_claude_endpoints(mut self) -> Self {
        let router = Router::new()
            .route("/v1/messages", post(api_messages))
            .layer(
                ServiceBuilder::new()
                    .layer(from_extractor::<RequireClaudeAuth>())
                    .layer(map_response(handle_stop_sequences))
            );
        self.inner = self.inner.merge(router);
        self
    }

    /// Sets up routes for API endpoints
    fn route_api_endpoints(mut self) -> Self {
        let router = Router::new()
            .nest(
                "/api",
                Router::new()
                    .route("/cookies", get(api_get_cookies))
                    .route("/cookie", delete(api_delete_cookie).post(api_post_cookie))
                    .route("/auth", get(api_auth))
                    .route("/config", get(api_get_config).put(api_post_config))
                    .route_layer(from_extractor::<RequireAdminAuth>()),
            )
            .route("/api/version", get(api_version));
        self.inner = self.inner.merge(router);
        self
    }

    /// Optionally sets up routes for OpenAI compatible endpoints
    fn route_openai_endpoints(mut self) -> Self {
        if CLEWDR_CONFIG.load().enable_oai {
            let router = Router::new()
                .route("/v1/chat/completions", post(api_messages))
                .layer(
                    ServiceBuilder::new()
                        .layer(from_extractor::<RequireOaiAuth>())
                        .layer(map_response(handle_stop_sequences))
                        .layer(map_response(transform_oai_response)),
                );
            self.inner = self.inner.merge(router);
        }
        self
    }

    /// Sets up static file serving
    fn setup_static_serving(mut self) -> Self {
        if IS_DEBUG {
            self.inner = self.inner.fallback_service(ServeDir::new(formatc!(
                "{}/static",
                env!("CARGO_MANIFEST_DIR")
            )));
        } else {
            use include_dir::{Dir, include_dir};
            const INCLUDE_STATIC: Dir = include_dir!("$CARGO_MANIFEST_DIR/static");
            self.inner = self
                .inner
                .fallback_service(tower_serve_static::ServeDir::new(&INCLUDE_STATIC));
        }
        self
    }

    /// Adds CORS support to the router
    fn with_cors(mut self) -> Self {
        let cors = CorsLayer::new()
            .allow_origin(tower_http::cors::Any)
            .allow_methods([Method::GET, Method::POST, Method::DELETE])
            .allow_headers([
                axum::http::header::AUTHORIZATION,
                axum::http::header::CONTENT_TYPE,
            ]);

        self.inner = self.inner.layer(cors);
        self
    }

    /// Returns the configured router
    /// Finalizes the router configuration for use with axum
    pub fn build(self) -> Router {
        self.inner.with_state(self.state)
    }
}
