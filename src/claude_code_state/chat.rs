use axum::{Json, response::IntoResponse};
use colored::Colorize;
use snafu::{GenerateImplicitData, ResultExt};
use tracing::{Instrument, error, info, warn};

use crate::{
    claude_code_state::{ClaudeCodeState, TokenStatus},
    config::CLEWDR_CONFIG,
    error::{CheckClaudeErr, ClewdrError, WreqSnafu},
    types::claude::{CountMessageTokensResponse, CreateMessageParams},
    utils::forward_response,
};

const CLAUDE_BETA_BASE: &str = "oauth-2025-04-20";
const CLAUDE_BETA_CONTEXT_1M: &str = "oauth-2025-04-20,context-1m-2025-08-07";
const CLAUDE_SONNET_4_PREFIX: &str = "claude-sonnet-4-20250514";

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
        let (base_model, requested_1m) = match p.model.strip_suffix("-1M") {
            Some(stripped) => (stripped.to_string(), true),
            None => (p.model.clone(), false),
        };

        let is_sonnet = Self::is_sonnet4_model(&base_model);
        let cookie_support = self
            .cookie
            .as_ref()
            .and_then(|cookie| cookie.supports_claude_1m);

        let attempts: Vec<bool> = if is_sonnet {
            match cookie_support {
                Some(true) => vec![true],
                Some(false) => vec![false],
                None => vec![true, false],
            }
        } else if requested_1m {
            vec![true, false]
        } else {
            vec![false]
        };

        p.model = base_model;

        let mut last_err: Option<ClewdrError> = None;
        for (idx, use_1m) in attempts.iter().copied().enumerate() {
            match self.execute_claude_request(&access_token, &p, use_1m).await {
                Ok(response) => {
                    return self
                        .handle_success_response(response, is_sonnet && use_1m)
                        .await;
                }
                Err(err) => {
                    let is_last_attempt = idx + 1 == attempts.len();
                    let should_retry = use_1m
                        && is_sonnet
                        && !is_last_attempt
                        && Self::is_context_1m_forbidden(&err);

                    if should_retry {
                        warn!(
                            "1M context not available for current cookie, disabling automatic 1M attempts"
                        );
                        self.persist_claude_1m_support(false).await;
                        last_err = Some(err);
                        continue;
                    }
                    return Err(err);
                }
            }
        }

        Err(last_err.unwrap_or_else(|| ClewdrError::TooManyRetries))
    }

    async fn execute_claude_request(
        &mut self,
        access_token: &str,
        body: &CreateMessageParams,
        use_context_1m: bool,
    ) -> Result<wreq::Response, ClewdrError> {
        let beta_header = if use_context_1m {
            CLAUDE_BETA_CONTEXT_1M
        } else {
            CLAUDE_BETA_BASE
        };

        self.client
            .post(format!("{}/v1/messages", self.endpoint))
            .bearer_auth(access_token)
            .header("anthropic-beta", beta_header)
            .header("anthropic-version", "2023-06-01")
            .json(body)
            .send()
            .await
            .context(WreqSnafu {
                msg: "Failed to send chat message",
            })?
            .check_claude()
            .await
    }

    async fn persist_claude_1m_support(&mut self, value: bool) {
        if let Some(cookie) = self.cookie.as_mut() {
            if cookie.supports_claude_1m == Some(value) {
                return;
            }
            cookie.set_claude_1m_support(Some(value));
            let cloned = cookie.clone();
            if let Err(err) = self.cookie_actor_handle.return_cookie(cloned, None).await {
                warn!("Failed to persist Claude 1M support state: {}", err);
            }
        }
    }

    async fn persist_count_tokens_allowed(&mut self, value: bool) {
        if let Some(cookie) = self.cookie.as_mut() {
            if cookie.count_tokens_allowed == Some(value) {
                return;
            }
            cookie.set_count_tokens_allowed(Some(value));
            let cloned = cookie.clone();
            if let Err(err) = self.cookie_actor_handle.return_cookie(cloned, None).await {
                warn!("Failed to persist count_tokens permission: {}", err);
            }
        }
    }

    pub async fn try_count_tokens(
        &mut self,
        p: CreateMessageParams,
        for_web: bool,
    ) -> Result<axum::response::Response, ClewdrError> {
        for i in 0..CLEWDR_CONFIG.load().max_retries + 1 {
            if i > 0 {
                info!("[TOKENS][RETRY] attempt: {}", i.to_string().green());
            }
            let mut state = self.to_owned();
            let p = p.to_owned();

            let cookie = state.request_cookie().await?;
            let web_attempt_allowed = CLEWDR_CONFIG.load().enable_web_count_tokens;
            let cookie_disallows = matches!(cookie.count_tokens_allowed, Some(false));
            if cookie_disallows || (for_web && !web_attempt_allowed) {
                if cookie_disallows {
                    state.persist_count_tokens_allowed(false).await;
                }
                return Ok(Self::local_count_tokens_response(&p));
            }
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
                        info!("Token is valid, proceeding with count_tokens");
                    }
                }
                let Some(access_token) = state.cookie.as_ref().and_then(|c| c.token.to_owned())
                else {
                    return Err(ClewdrError::UnexpectedNone {
                        msg: "No access token found in cookie",
                    });
                };
                state
                    .perform_count_tokens(access_token.access_token.to_owned(), p, for_web)
                    .await
            }
            .instrument(tracing::info_span!(
                "claude_code_tokens",
                "cookie" = cookie.cookie.ellipse()
            ));
            match retry.await {
                Ok(res) => {
                    return Ok(res);
                }
                Err(e) => {
                    error!(
                        "[{}][TOKENS] {}",
                        state.cookie.as_ref().unwrap().cookie.ellipse().green(),
                        e
                    );
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

    async fn perform_count_tokens(
        &mut self,
        access_token: String,
        mut p: CreateMessageParams,
        allow_fallback: bool,
    ) -> Result<axum::response::Response, ClewdrError> {
        p.stream = Some(false);
        let (base_model, requested_1m) = match p.model.strip_suffix("-1M") {
            Some(stripped) => (stripped.to_string(), true),
            None => (p.model.clone(), false),
        };

        let is_sonnet = Self::is_sonnet4_model(&base_model);
        let cookie_support = self
            .cookie
            .as_ref()
            .and_then(|cookie| cookie.supports_claude_1m);

        let attempts: Vec<bool> = if is_sonnet {
            match cookie_support {
                Some(true) => vec![true],
                Some(false) => vec![false],
                None => vec![true, false],
            }
        } else if requested_1m {
            vec![true, false]
        } else {
            vec![false]
        };

        p.model = base_model;

        let mut last_err: Option<ClewdrError> = None;
        for (idx, use_1m) in attempts.iter().copied().enumerate() {
            match self
                .execute_claude_count_tokens_request(&access_token, &p, use_1m)
                .await
            {
                Ok(response) => {
                    self.persist_count_tokens_allowed(true).await;
                    if is_sonnet && use_1m {
                        self.persist_claude_1m_support(true).await;
                    }
                    let (resp, _) = Self::materialize_non_stream_response(response).await?;
                    return Ok(resp);
                }
                Err(err) => {
                    let unauthorized = Self::is_count_tokens_unauthorized(&err);
                    if unauthorized {
                        self.persist_count_tokens_allowed(false).await;
                        if allow_fallback {
                            return Ok(Self::local_count_tokens_response(&p));
                        }
                    }
                    let is_last_attempt = idx + 1 == attempts.len();
                    let should_retry = use_1m
                        && is_sonnet
                        && !is_last_attempt
                        && Self::is_context_1m_forbidden(&err);

                    if should_retry {
                        warn!(
                            "1M context not available for current cookie, disabling automatic 1M attempts"
                        );
                        self.persist_claude_1m_support(false).await;
                        last_err = Some(err);
                        continue;
                    }
                    return Err(err);
                }
            }
        }

        Err(last_err.unwrap_or_else(|| ClewdrError::TooManyRetries))
    }

    async fn handle_success_response(
        &mut self,
        response: wreq::Response,
        mark_support_true: bool,
    ) -> Result<axum::response::Response, ClewdrError> {
        if !self.stream {
            let (resp, usage_pair) = Self::materialize_non_stream_response(response).await?;
            let (input, output) = usage_pair.unwrap_or((self.usage.input_tokens as u64, 0));
            self.persist_usage_totals(input, output).await;
            if mark_support_true {
                self.persist_claude_1m_support(true).await;
            }
            Ok(resp)
        } else {
            self.persist_usage_totals(self.usage.input_tokens as u64, 0)
                .await;
            if mark_support_true {
                self.persist_claude_1m_support(true).await;
            }
            forward_response(response)
        }
    }

    async fn persist_usage_totals(&mut self, input: u64, output: u64) {
        if input == 0 && output == 0 {
            return;
        }
        if let Some(cookie) = self.cookie.as_mut() {
            cookie.add_usage(input, output);
            let cloned = cookie.clone();
            if let Err(err) = self.cookie_actor_handle.return_cookie(cloned, None).await {
                warn!("Failed to persist usage statistics: {}", err);
            }
        }
    }

    async fn materialize_non_stream_response(
        response: wreq::Response,
    ) -> Result<(axum::response::Response, Option<(u64, u64)>), ClewdrError> {
        let status = response.status();
        let headers = response.headers().clone();
        let bytes = response.bytes().await.context(WreqSnafu {
            msg: "Failed to read Claude response body",
        })?;
        let usage = Self::extract_usage_from_bytes(&bytes);

        let mut builder = http::Response::builder().status(status);
        for (key, value) in headers.iter() {
            builder = builder.header(key, value);
        }
        let response =
            builder
                .body(axum::body::Body::from(bytes))
                .map_err(|e| ClewdrError::HttpError {
                    loc: snafu::Location::generate(),
                    source: e,
                })?;
        Ok((response, usage))
    }

    fn extract_usage_from_bytes(bytes: &[u8]) -> Option<(u64, u64)> {
        let value: serde_json::Value = serde_json::from_slice(bytes).ok()?;
        let usage = value.get("usage")?;
        let input = usage
            .get("input_tokens")
            .and_then(|v| v.as_u64().or_else(|| v.as_i64().map(|n| n.max(0) as u64)))?;
        let output = usage
            .get("output_tokens")
            .and_then(|v| v.as_u64().or_else(|| v.as_i64().map(|n| n.max(0) as u64)))?;
        Some((input, output))
    }

    async fn execute_claude_count_tokens_request(
        &mut self,
        access_token: &str,
        body: &CreateMessageParams,
        use_context_1m: bool,
    ) -> Result<wreq::Response, ClewdrError> {
        let beta_header = if use_context_1m {
            CLAUDE_BETA_CONTEXT_1M
        } else {
            CLAUDE_BETA_BASE
        };

        self.client
            .post(format!("{}/v1/messages/count_tokens", self.endpoint))
            .bearer_auth(access_token)
            .header("anthropic-beta", beta_header)
            .header("anthropic-version", "2023-06-01")
            .json(body)
            .send()
            .await
            .context(WreqSnafu {
                msg: "Failed to call Claude count_tokens",
            })?
            .check_claude()
            .await
    }

    fn is_sonnet4_model(model: &str) -> bool {
        model.starts_with(CLAUDE_SONNET_4_PREFIX)
    }

    fn local_count_tokens_response(body: &CreateMessageParams) -> axum::response::Response {
        let estimate = CountMessageTokensResponse {
            input_tokens: body.count_tokens(),
        };
        Json(estimate).into_response()
    }

    fn is_context_1m_forbidden(error: &ClewdrError) -> bool {
        if let ClewdrError::ClaudeHttpError { code, inner } = error
            && (code.as_u16() == 403 || code.as_u16() == 400)
        {
            let message = inner
                .message
                .as_str()
                .map(|s| s.to_ascii_lowercase())
                .unwrap_or_default();
            return message
                .contains("the long context beta is not yet available for this subscription.");
        }
        false
    }

    fn is_count_tokens_unauthorized(error: &ClewdrError) -> bool {
        if let ClewdrError::ClaudeHttpError { code, .. } = error {
            return match code.as_u16() {
                401 | 404 => true,
                403 => !Self::is_context_1m_forbidden(error),
                _ => false,
            };
        }
        false
    }
}
