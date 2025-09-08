pub mod gemini;
pub mod gemini_cli;
pub mod claude_web;
pub mod claude_code;
pub mod admin;

pub use gemini::build_gemini_router;
pub use gemini_cli::build_gemini_cli_router;
pub use claude_web::{build_claude_web_router, build_claude_web_oai_router};
pub use claude_code::{build_claude_code_router, build_claude_code_oai_router};
pub use admin::build_admin_router;
