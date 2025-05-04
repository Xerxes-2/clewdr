/// Authentication, request processing, and response transformation middleware
///
/// This module contains middleware components that handle various aspects of
/// request processing and response transformation in the Clewdr proxy service:
///
/// - Authentication: Verify API keys for different authentication methods (admin, OpenAI, Claude)
/// - Request preprocessing: Normalize requests from different API formats
/// - Response transformation: Convert between different response formats and handle streaming
/// - Stop sequence handling: Process stop sequences in streaming responses
mod auth;
mod request;
mod response;
mod stop_sequence;

pub use auth::{RequireAdminAuth, RequireClaudeAuth, RequireOaiAuth};
pub use request::{FormatInfo, Preprocess};
pub use response::transform_oai_response;
pub use stop_sequence::handle_stop_sequences;
