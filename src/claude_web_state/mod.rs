use std::sync::LazyLock;

use axum::http::{HeaderValue, header::COOKIE};
use serde_json::Value;
use snafu::ResultExt;
use tracing::{error, warn};
use url::Url;
use wreq::{
    Client, Method, Proxy, RequestBuilder,
    header::{ORIGIN, REFERER},
};

use crate::{
    config::{CLAUDE_ENDPOINT, CLEWDR_CONFIG, CookieStatus, Reason},
    error::{ClewdrError, WreqSnafu},
    middleware::claude::ClaudeApiFormat,
    services::cookie_actor::CookieActorHandle,
    types::claude::{CreateMessageParams, Usage},
    utils::build_http_client,
};

pub mod bootstrap;
pub mod chat;
mod transform;
/// Placeholder
pub static SUPER_CLIENT: LazyLock<Client> = LazyLock::new(Client::new);

/// State of current connection
#[derive(Clone)]
pub struct ClaudeWebState {
    pub cookie: Option<CookieStatus>,
    cookie_header_value: HeaderValue,
    pub cookie_actor_handle: CookieActorHandle,
    pub org_uuid: Option<String>,
    pub conv_uuid: Option<String>,
    pub capabilities: Vec<String>,
    pub endpoint: Url,
    pub proxy: Option<Proxy>,
    pub api_format: ClaudeApiFormat,
    pub stream: bool,
    pub client: Client,
    pub key: Option<(u64, usize)>,
    pub usage: Usage,
    // keep the last request params for potential post-call token accounting
    pub last_params: Option<CreateMessageParams>,
}

impl ClaudeWebState {
    /// Create a new `AppState` instance
    pub fn new(cookie_actor_handle: CookieActorHandle) -> Self {
        ClaudeWebState {
            cookie_actor_handle,
            cookie: None,
            org_uuid: None,
            conv_uuid: None,
            cookie_header_value: HeaderValue::from_static(""),
            capabilities: Vec::new(),
            endpoint: CLEWDR_CONFIG.load().endpoint(),
            proxy: CLEWDR_CONFIG.load().wreq_proxy.clone(),
            api_format: ClaudeApiFormat::Claude,
            stream: false,
            client: SUPER_CLIENT.to_owned(),
            key: None,
            usage: Usage::default(),
            last_params: None,
        }
    }

    #[must_use]
    pub fn with_claude_format(mut self) -> Self {
        self.api_format = ClaudeApiFormat::Claude;
        self
    }

    #[must_use]
    pub fn with_openai_format(mut self) -> Self {
        self.api_format = ClaudeApiFormat::OpenAI;
        self
    }

    fn build_client(proxy: Option<&Proxy>) -> Result<Client, wreq::Error> {
        build_http_client(proxy)
    }

    /// Build a request with the current cookie and proxy settings
    pub fn build_request(&self, method: Method, url: &impl ToString) -> RequestBuilder {
        // let r = SUPER_CLIENT.cloned();
        let mut req = self
            .client
            .request(method, url.to_string())
            .header(ORIGIN, CLAUDE_ENDPOINT);
        if !self.cookie_header_value.as_bytes().is_empty() {
            req = req.header(COOKIE, self.cookie_header_value.clone());
        }
        if let Some(uuid) = self.conv_uuid.clone() {
            req.header(
                REFERER,
                self.endpoint.join(&format!("chat/{uuid}")).map_or_else(
                    |_| format!("{CLAUDE_ENDPOINT}chat/{uuid}"),
                    std::convert::Into::into,
                ),
            )
        } else {
            req.header(
                REFERER,
                self.endpoint.join("new").map_or_else(
                    |_| format!("{CLAUDE_ENDPOINT}new"),
                    std::convert::Into::into,
                ),
            )
        }
    }

    /// Checks if the current user has pro capabilities
    /// Returns true if any capability contains "pro", "enterprise", "raven", or "max"
    pub fn is_pro(&self) -> bool {
        self.capabilities.iter().any(|c| {
            c.contains("pro")
                || c.contains("enterprise")
                || c.contains("raven")
                || c.contains("max")
        })
    }

    /// Requests a new cookie from the cookie manager
    /// Updates the internal state with the new cookie and proxy configuration
    ///
    /// # Errors
    /// If no cookie is available, or the HTTP client cannot be rebuilt for the
    /// cookie's proxy.
    pub async fn request_cookie(&mut self) -> Result<CookieStatus, ClewdrError> {
        let res = self.cookie_actor_handle.request(None).await?;
        self.cookie = Some(res.clone());
        // Always pull latest proxy/endpoint before building the client
        self.proxy.clone_from(&CLEWDR_CONFIG.load().wreq_proxy);
        self.endpoint = CLEWDR_CONFIG.load().endpoint();
        self.client = Self::build_client(self.proxy.as_ref()).context(WreqSnafu {
            msg: "Failed to build client with new cookie",
        })?;
        self.cookie_header_value = HeaderValue::from_str(res.cookie.to_string().as_str())?;
        Ok(res)
    }

    /// Returns the current cookie to the cookie manager
    /// Optionally provides a reason for returning the cookie (e.g., invalid, banned)
    pub async fn return_cookie(&self, reason: Option<Reason>) {
        // return the cookie to the cookie manager
        if let Some(ref cookie) = self.cookie {
            self.cookie_actor_handle
                .return_cookie(cookie.to_owned(), reason)
                .await
                .unwrap_or_else(|e| {
                    error!("Failed to send cookie: {}", e);
                });
        }
    }

    fn classify_model(model: &str) -> crate::config::ModelFamily {
        let m = model.to_ascii_lowercase();
        if m.contains("opus") {
            crate::config::ModelFamily::Opus
        } else if m.contains("sonnet") {
            crate::config::ModelFamily::Sonnet
        } else {
            crate::config::ModelFamily::Other
        }
    }

    pub async fn persist_usage_totals(&mut self, input: u64, output: u64) {
        if input == 0 && output == 0 {
            return;
        }
        if let Some(cookie) = self.cookie.as_mut() {
            let family = self
                .last_params
                .as_ref()
                .map_or(crate::config::ModelFamily::Other, |p| {
                    Self::classify_model(&p.model)
                });
            cookie.add_and_bucket_usage(input, output, family);
            let cloned = cookie.clone();
            if let Err(err) = self.cookie_actor_handle.return_cookie(cloned, None).await {
                warn!("Failed to persist usage statistics: {}", err);
            }
        }
    }

    /// Fetch usage data via the claude.ai web endpoint.
    /// Used as a fallback when the OAuth usage endpoint is not available (e.g. Without Claude Code Access).
    pub async fn fetch_web_usage(handle: CookieActorHandle, cookie: CookieStatus) -> Option<Value> {
        let mut state = ClaudeWebState::new(handle);
        state.cookie = Some(cookie.clone());
        state.proxy.clone_from(&CLEWDR_CONFIG.load().wreq_proxy);
        state.endpoint = CLEWDR_CONFIG.load().endpoint();
        state.client = Self::build_client(state.proxy.as_ref()).ok()?;
        state.cookie_header_value =
            HeaderValue::from_str(cookie.cookie.to_string().as_str()).ok()?;

        if let Err(e) = state.bootstrap().await {
            warn!(
                "fetch_web_usage: bootstrap failed for {}: {}",
                cookie.cookie, e
            );
            return None;
        }

        let org_uuid = state.org_uuid.as_ref()?;
        let url = state
            .endpoint
            .join(&format!("api/organizations/{org_uuid}/usage"))
            .ok()?;

        let res = state
            .build_request(Method::GET, &url)
            .send()
            .await
            .inspect_err(|e| {
                warn!(
                    "fetch_web_usage: request failed for {}: {}",
                    cookie.cookie, e
                );
            })
            .ok()?;

        res.json::<Value>()
            .await
            .inspect_err(|e| {
                warn!("fetch_web_usage: parse failed for {}: {}", cookie.cookie, e);
            })
            .ok()
    }
}
