use std::sync::Arc;

use axum::{Extension, extract::State, response::Response};

use crate::{
    error::ClewdrError,
    middleware::claude::{ClaudeCodePreprocess, ClaudeContext},
    providers::LLMProvider,
    providers::claude::{ClaudeCodeProvider, ClaudeInvocation, ClaudeProviderResponse},
};

pub async fn api_claude_code(
    State(provider): State<Arc<ClaudeCodeProvider>>,
    ClaudeCodePreprocess(params, context): ClaudeCodePreprocess,
) -> Result<(Extension<ClaudeContext>, Response), ClewdrError> {
    let ClaudeProviderResponse { context, response } = provider
        .invoke(ClaudeInvocation {
            params,
            context: context.clone(),
        })
        .await?;
    Ok((Extension(context), response))
}
