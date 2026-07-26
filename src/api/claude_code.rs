use std::sync::Arc;

use axum::{Extension, extract::State, response::Response};

use crate::{
    error::ClewdrError,
    middleware::claude::{ClaudeCodePreprocess, ClaudeContext},
    providers::{
        LLMProvider,
        claude::{ClaudeCodeProvider, ClaudeInvocation, ClaudeProviderResponse},
    },
};

/// Axum handler for `/v1/messages` on the Claude Code backend.
///
/// # Errors
/// Propagates any [`ClewdrError`] from the provider: no cookie available,
/// an upstream HTTP failure, or exhausted retries.
pub async fn api_claude_code(
    State(provider): State<Arc<ClaudeCodeProvider>>,
    ClaudeCodePreprocess(params, context): ClaudeCodePreprocess,
) -> Result<(Extension<ClaudeContext>, Response), ClewdrError> {
    let ClaudeProviderResponse { context, response } = provider
        .invoke(ClaudeInvocation::messages(params, context.clone()))
        .await?;
    Ok((Extension(context), response))
}

/// Axum handler for `/v1/messages/count_tokens` on the Claude Code backend.
///
/// Forces `stream: false`, since token counting has no streaming form.
///
/// # Errors
/// Propagates any [`ClewdrError`] from the provider: no cookie available,
/// an upstream HTTP failure, or exhausted retries.
pub async fn api_claude_code_count_tokens(
    State(provider): State<Arc<ClaudeCodeProvider>>,
    ClaudeCodePreprocess(mut params, context): ClaudeCodePreprocess,
) -> Result<Response, ClewdrError> {
    params.stream = Some(false);
    let ClaudeProviderResponse { response, .. } = provider
        .invoke(ClaudeInvocation::count_tokens(params, context))
        .await?;
    Ok(response)
}
