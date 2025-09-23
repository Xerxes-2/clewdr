use colored::Colorize;
use snafu::ResultExt;
use tracing::{Instrument, error, info};

use crate::{
    claude_code_state::{ClaudeCodeState, TokenStatus},
    config::CLEWDR_CONFIG,
    error::{CheckClaudeErr, ClewdrError, WreqSnafu},
    types::claude::CreateMessageParams,
    utils::forward_response,
};

impl ClaudeCodeState {
    /// Attempts to send a chat message to Claude API with retry mechanism
    ///
    /// This method handles the complete chat flow including:
    /// - Request preparation and logging
    /// - Cookie management for authentication
    /// - Executing the chat request with automatic retries on failure
    /// - Response transformation according to the specified API format
    /// - Error handling and cleanup
    ///
    /// The method implements a sophisticated retry mechanism to handle transient failures,
    /// and manages conversation cleanup to prevent resource leaks. It also includes
    /// performance tracking to measure response times.
    ///
    /// # Arguments
    /// * `p` - The client request body containing messages and configuration
    ///
    /// # Returns
    /// * `Result<axum::response::Response, ClewdrError>` - Formatted response or error
    pub async fn try_chat(
        &mut self,
        p: CreateMessageParams,
    ) -> Result<axum::response::Response, ClewdrError> {
        for i in 0..CLEWDR_CONFIG.load().max_retries + 1 {
            if i > 0 {
                info!("[RETRY] attempt: {}", i.to_string().green());
            }
            let mut state = self.to_owned();
            let p = p.to_owned();

            let cookie = state.request_cookie().await?;
            let retry = async {
                match state.check_token() {
                    TokenStatus::None => {
                        info!("No token found, requesting new token");
                        let org = state.get_organization().await?;
                        let code_res = state.exchange_code(&org).await?;
                        state.exchange_token(code_res).await?;
                        state.return_cookie(None).await;
                    }
                    TokenStatus::Expired => {
                        info!("Token expired, refreshing token");
                        state.refresh_token().await?;
                        state.return_cookie(None).await;
                    }
                    TokenStatus::Valid => {
                        info!("Token is valid, proceeding with request");
                    }
                }
                let Some(access_token) = state.cookie.as_ref().and_then(|c| c.token.to_owned())
                else {
                    return Err(ClewdrError::UnexpectedNone {
                        msg: "No access token found in cookie",
                    });
                };
                state
                    .send_chat(access_token.access_token.to_owned(), p)
                    .await
            }
            .instrument(tracing::info_span!(
                "claude_code",
                "cookie" = cookie.cookie.ellipse()
            ));
            match retry.await {
                Ok(res) => {
                    return Ok(res);
                }
                Err(e) => {
                    error!(
                        "[{}] {}",
                        state.cookie.as_ref().unwrap().cookie.ellipse().green(),
                        e
                    );
                    // 429 error
                    if let ClewdrError::InvalidCookie { reason } = e {
                        state.return_cookie(Some(reason.to_owned())).await;
                        continue;
                    }
                    return Err(e);
                }
            }
        }
        Err(ClewdrError::TooManyRetries)
    }

    pub async fn send_chat(
        &mut self,
        access_token: String,
        mut p: CreateMessageParams,
    ) -> Result<axum::response::Response, ClewdrError> {
        let normalized = strip_sonnet_1m_suffix(p.model.as_str());
        let wants_1m = normalized.is_some();
        if let Some(base_model) = normalized {
            p.model = base_model;
        }
        let support_hint = self.cookie.as_ref().and_then(|c| c.claude_sonnet_1m);
        let plan = plan_beta_header(p.model.as_str(), wants_1m, support_hint);

        match plan {
            BetaAttempt::Direct(beta) => {
                let api_res = self
                    .perform_chat_request(access_token.as_str(), &p, beta)
                    .await?;
                forward_response(api_res)
            }
            BetaAttempt::Probe { primary, fallback } => {
                if let Some(cookie) = &self.cookie {
                    info!(
                        "[1M] probing {} for claude-sonnet-4 1M context",
                        cookie.cookie.ellipse().yellow()
                    );
                }
                match self
                    .perform_chat_request(access_token.as_str(), &p, primary)
                    .await
                {
                    Ok(api_res) => {
                        self.persist_sonnet_support(true).await;
                        forward_response(api_res)
                    }
                    Err(err) => {
                        if is_sonnet_context_denied(&err) {
                            self.persist_sonnet_support(false).await;
                            let fallback_res = self
                                .perform_chat_request(access_token.as_str(), &p, fallback)
                                .await?;
                            return forward_response(fallback_res);
                        }
                        Err(err)
                    }
                }
            }
        }
    }
}

impl ClaudeCodeState {
    async fn perform_chat_request(
        &self,
        access_token: &str,
        params: &CreateMessageParams,
        beta_header: &str,
    ) -> Result<wreq::Response, ClewdrError> {
        self.client
            .post(format!("{}/v1/messages", self.endpoint))
            .bearer_auth(access_token)
            .header("anthropic-beta", beta_header)
            .header("anthropic-version", "2023-06-01")
            .json(params)
            .send()
            .await
            .context(WreqSnafu {
                msg: "Failed to send chat message",
            })?
            .check_claude()
            .await
    }

    async fn persist_sonnet_support(&mut self, support: bool) {
        let Some(cookie) = self.cookie.as_mut() else {
            return;
        };
        if cookie.claude_sonnet_1m == Some(support) {
            return;
        }
        cookie.claude_sonnet_1m = Some(support);
        let cookie_clone = cookie.clone();
        if let Err(e) = self.cookie_actor_handle.update_cookie(cookie_clone).await {
            error!("Failed to persist 1M context flag: {}", e);
            return;
        }
        let styled = if support {
            "enabled".green()
        } else {
            "disabled".red()
        };
        info!("[1M] {} {}", cookie.cookie.ellipse().yellow(), styled);
    }
}

fn strip_sonnet_1m_suffix(model: &str) -> Option<String> {
    if let Some(prefix) = model.strip_suffix("-1M-thinking") {
        return Some(format!("{prefix}-thinking"));
    }
    model.strip_suffix("-1M").map(|prefix| prefix.to_string())
}

fn is_claude_sonnet_model(model: &str) -> bool {
    model.starts_with("claude-sonnet-4")
}

fn plan_beta_header(model: &str, wants_1m: bool, support_hint: Option<bool>) -> BetaAttempt {
    if !wants_1m {
        return BetaAttempt::Direct(STANDARD_BETA_HEADER);
    }
    if !is_claude_sonnet_model(model) {
        return BetaAttempt::Direct(SONNET_1M_BETA_HEADER);
    }
    match support_hint {
        Some(true) => BetaAttempt::Direct(SONNET_1M_BETA_HEADER),
        Some(false) => BetaAttempt::Direct(STANDARD_BETA_HEADER),
        None => BetaAttempt::Probe {
            primary: SONNET_1M_BETA_HEADER,
            fallback: STANDARD_BETA_HEADER,
        },
    }
}

fn is_sonnet_context_denied(err: &ClewdrError) -> bool {
    use http::StatusCode;
    use serde_json::Value;

    let ClewdrError::ClaudeHttpError { code, inner } = err else {
        return false;
    };
    if !matches!(
        *code,
        StatusCode::BAD_REQUEST | StatusCode::FORBIDDEN | StatusCode::NOT_FOUND
    ) {
        return false;
    }
    let message = match &inner.message {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    };
    let message = message.to_ascii_lowercase();
    let mentions_1m = message.contains("context-1m")
        || message.contains("1m context")
        || message.contains("1m window")
        || (message.contains("1m") && message.contains("context"));
    if !mentions_1m {
        return false;
    }
    const DENIAL_PHRASES: &[&str] = &[
        "not enabled",
        "not available",
        "not allowed",
        "no access",
        "without access",
        "requires",
        "beta",
        "upgrade",
        "not found",
        "missing",
    ];
    DENIAL_PHRASES.iter().any(|phrase| message.contains(phrase))
}

const STANDARD_BETA_HEADER: &str = "oauth-2025-04-20";
const SONNET_1M_BETA_HEADER: &str = "oauth-2025-04-20,context-1m-2025-08-07";

enum BetaAttempt {
    Direct(&'static str),
    Probe {
        primary: &'static str,
        fallback: &'static str,
    },
}
