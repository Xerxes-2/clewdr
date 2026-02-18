mod claude2oai;
mod request;
mod response;
mod stop_sequences;

pub(crate) use claude2oai::*;
pub use request::*;
pub use response::*;
pub use stop_sequences::*;
use strum::Display;

use crate::types::claude::Usage;

/// Represents the format of the API response
///
/// This enum defines the available API response formats that Clewdr can use
/// when communicating with clients. It supports both Claude's native format
/// and an OpenAI-compatible format for broader compatibility with existing tools.
#[derive(Display, Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClaudeApiFormat {
    /// Claude native format
    Claude,
    /// OpenAI compatible format
    OpenAI,
}

#[derive(Debug, Clone)]
pub enum ClaudeContext {
    Web(ClaudeWebContext),
    Code(ClaudeCodeContext),
}

impl ClaudeContext {
    pub fn is_stream(&self) -> bool {
        match self {
            ClaudeContext::Web(ctx) => ctx.stream,
            ClaudeContext::Code(ctx) => ctx.stream,
        }
    }

    pub fn api_format(&self) -> ClaudeApiFormat {
        match self {
            ClaudeContext::Web(ctx) => ctx.api_format,
            ClaudeContext::Code(ctx) => ctx.api_format,
        }
    }

    pub fn is_web(&self) -> bool {
        matches!(self, ClaudeContext::Web(_))
    }

    pub fn is_code(&self) -> bool {
        matches!(self, ClaudeContext::Code(_))
    }

    pub fn stop_sequences(&self) -> &[String] {
        match self {
            ClaudeContext::Web(ctx) => &ctx.stop_sequences,
            ClaudeContext::Code(_) => &[],
        }
    }

    pub fn system_prompt_hash(&self) -> Option<u64> {
        match self {
            ClaudeContext::Web(_) => None,
            ClaudeContext::Code(ctx) => ctx.system_prompt_hash,
        }
    }

    pub fn usage(&self) -> &Usage {
        match self {
            ClaudeContext::Web(ctx) => &ctx.usage,
            ClaudeContext::Code(ctx) => &ctx.usage,
        }
    }

    pub fn anthropic_beta(&self) -> Option<&str> {
        match self {
            ClaudeContext::Web(_) => None,
            ClaudeContext::Code(ctx) => ctx.anthropic_beta.as_deref(),
        }
    }
}
