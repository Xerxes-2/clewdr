/// API module for handling all external HTTP endpoints and request/response transformations
///
/// This module serves as the main entry point for all API requests, providing endpoints
/// for configuration management, message handling, authentication, and OpenAI-compatible
/// interfaces. It also implements response transformation between different API formats.
mod claude_code;
mod claude_web;
mod config;
mod error;
mod gemini;
mod misc;
mod storage;
pub use claude_code::{api_claude_code, api_claude_code_count_tokens};
/// Message handling endpoints for creating and managing chat conversations
pub use claude_web::api_claude_web;
/// Configuration related endpoints for retrieving and updating Clewdr settings
pub use config::{api_get_config, api_post_config};
pub use error::ApiError;
pub use gemini::{api_post_gemini, api_post_gemini_oai};
/// Miscellaneous endpoints for authentication, cookies, and version information
pub use misc::{
    api_auth, api_delete_cookie, api_delete_key, api_delete_vertex_credential, api_get_cookies,
    api_get_keys, api_get_models, api_get_vertex_credentials, api_post_cookie, api_post_key,
    api_post_vertex_credential, api_version,
};
pub use storage::{api_storage_export, api_storage_import, api_storage_status};
// merged above
