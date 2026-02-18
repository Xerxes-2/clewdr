mod chat;
mod exchange;
mod organization;
use http::{
    HeaderValue, Method,
    header::{ORIGIN, REFERER},
};
use snafu::ResultExt;
use tracing::error;
use wreq::{ClientBuilder, IntoUrl, RequestBuilder};
use wreq_util::Emulation;

use crate::{
    claude_web_state::SUPER_CLIENT,
    config::{CLAUDE_ENDPOINT, CLEWDR_CONFIG, CookieStatus, Reason},
    error::{ClewdrError, WreqSnafu},
    middleware::claude::ClaudeApiFormat,
    services::cookie_actor::CookieActorHandle,
    types::claude::Usage,
};

#[derive(Clone)]
pub struct ClaudeCodeState {
    pub cookie_actor_handle: CookieActorHandle,
    pub cookie: Option<CookieStatus>,
    pub cookie_header_value: HeaderValue,
    pub proxy: Option<wreq::Proxy>,
    pub endpoint: url::Url,
    pub client: wreq::Client,
    pub api_format: ClaudeApiFormat,
    pub stream: bool,
    pub system_prompt_hash: Option<u64>,
    pub anthropic_beta_header: Option<String>,
    pub usage: Usage,
}

impl ClaudeCodeState {
    /// Create a new ClaudeCodeState instance
    pub fn new(cookie_actor_handle: CookieActorHandle) -> Self {
        ClaudeCodeState {
            cookie_actor_handle,
            cookie: None,
            cookie_header_value: HeaderValue::from_static(""),
            proxy: CLEWDR_CONFIG.load().wreq_proxy.to_owned(),
            endpoint: CLEWDR_CONFIG.load().endpoint(),
            client: SUPER_CLIENT.to_owned(),
            api_format: ClaudeApiFormat::Claude,
            stream: false,
            system_prompt_hash: None,
            anthropic_beta_header: None,
            usage: Usage::default(),
        }
    }

    /// Build a ClaudeCodeState initialized with an existing cookie snapshot
    pub fn from_cookie(
        cookie_actor_handle: CookieActorHandle,
        cookie: CookieStatus,
    ) -> Result<Self, ClewdrError> {
        let mut state = Self::new(cookie_actor_handle);
        state.cookie = Some(cookie);
        let cookie_value = state
            .cookie
            .as_ref()
            .ok_or(ClewdrError::UnexpectedNone {
                msg: "Cookie missing while initializing state",
            })?
            .cookie
            .to_string();
        let header_value = HeaderValue::from_str(cookie_value.as_str())?;
        state.cookie_header_value = header_value.clone();
        let mut client = ClientBuilder::new()
            .cookie_store(true)
            .emulation(Emulation::Chrome136);
        if let Some(ref proxy) = state.proxy {
            client = client.proxy(proxy.to_owned());
        }
        state.client = client.build().context(WreqSnafu {
            msg: "Failed to build client for cookie",
        })?;
        state.client.set_cookie(&state.endpoint, &header_value);
        Ok(state)
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

    /// Build a request with the current cookie and proxy settings
    pub fn build_request(&self, method: Method, url: impl IntoUrl) -> RequestBuilder {
        // let r = SUPER_CLIENT.cloned();
        self.client
            .set_cookie(&self.endpoint, &self.cookie_header_value);
        self.client
            .request(method, url)
            .header(ORIGIN, CLAUDE_ENDPOINT)
            .header(REFERER, format!("{CLAUDE_ENDPOINT}new"))
    }

    /// Set the cookie header value
    pub fn set_cookie_header_value(&mut self, value: HeaderValue) {
        self.cookie_header_value = value;
    }

    /// Requests a new cookie from the cookie manager
    /// Updates the internal state with the new cookie and proxy configuration
    pub async fn request_cookie(&mut self) -> Result<CookieStatus, ClewdrError> {
        let res = self
            .cookie_actor_handle
            .request(self.system_prompt_hash)
            .await?;
        self.cookie = Some(res.to_owned());
        self.cookie_header_value = HeaderValue::from_str(res.cookie.to_string().as_str())?;
        // Always pull latest proxy/endpoint before building the client
        self.proxy = CLEWDR_CONFIG.load().wreq_proxy.to_owned();
        self.endpoint = CLEWDR_CONFIG.load().endpoint();
        let mut client = ClientBuilder::new()
            .cookie_store(true)
            .emulation(Emulation::Chrome136);
        if let Some(ref proxy) = self.proxy {
            client = client.proxy(proxy.to_owned());
        }
        self.client = client.build().context(WreqSnafu {
            msg: "Failed to build client with new cookie",
        })?;
        Ok(res)
    }

    pub fn check_token(&self) -> TokenStatus {
        let Some(CookieStatus {
            token: Some(token_info),
            ..
        }) = &self.cookie
        else {
            return TokenStatus::None;
        };
        if token_info.is_expired() {
            TokenStatus::Expired
        } else {
            TokenStatus::Valid
        }
    }
}

pub enum TokenStatus {
    None,
    Expired,
    Valid,
}
