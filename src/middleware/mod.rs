/// Authentication, request processing, and response transformation middleware
///
/// This module contains middleware components that handle various aspects of
/// request processing and response transformation in the Clewdr proxy service:
///
/// - Authentication: Verify API keys for different authentication methods (admin, OpenAI, Claude)
/// - Request preprocessing: Normalize requests from different API formats
/// - Response transformation: Convert between different response formats and handle streaming
mod auth;
pub mod claude;
pub mod gemini;

pub use auth::{
    RequireAdminAuth,
    RequireBearerAuth,
    RequireGeminiCliAuth,
    RequireGeminiFlexibleAuth,
    RequireQueryKeyAuth,
    RequireXApiKeyAuth,
};
