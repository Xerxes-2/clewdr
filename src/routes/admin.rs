use axum::{Router, routing::{get, post, delete}, middleware::from_extractor};

use crate::{
    api::*,
    middleware::RequireAdminAuth,
    services::{cookie_actor::CookieActorHandle, key_actor::KeyActorHandle},
};

pub fn build_admin_router(cookie_handle: CookieActorHandle, key_handle: KeyActorHandle) -> Router {
    let cookie_router = Router::new()
        .route("/cookies", get(api_get_cookies))
        .route("/cookie", delete(api_delete_cookie).post(api_post_cookie))
        .with_state(cookie_handle);
    let key_router = Router::new()
        .route("/key", post(api_post_key).delete(api_delete_key))
        .route("/keys", get(api_get_keys))
        .with_state(key_handle);
    let admin_router = Router::new()
        .route("/auth", get(api_auth))
        .route("/config", get(api_get_config).put(api_post_config))
        .route("/storage/import", post(api_storage_import))
        .route("/storage/export", post(api_storage_export))
        .route("/storage/status", get(api_storage_status));
    Router::new()
        .nest(
            "/api",
            cookie_router
                .merge(key_router)
                .merge(admin_router)
                .layer(from_extractor::<RequireAdminAuth>()),
        )
        .route("/api/version", get(api_version))
}

