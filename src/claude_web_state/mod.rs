use std::sync::LazyLock;

use axum::http::HeaderValue;
use snafu::ResultExt;
use tracing::{debug, error};
use url::Url;
use wreq::{
    Client, ClientBuilder, IntoUrl, Method, Proxy, RequestBuilder,
    header::{ORIGIN, REFERER},
};
use wreq_util::Emulation;

use crate::{
    config::{CLAUDE_ENDPOINT, CLEWDR_CONFIG, CookieStatus, Reason},
    error::{ClewdrError, WreqSnafu},
    middleware::claude::ClaudeApiFormat,
    services::cookie_actor::CookieActorHandle,
    types::claude::Usage,
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
}

impl ClaudeWebState {
    /// Create a new AppState instance
    pub fn new(cookie_actor_handle: CookieActorHandle) -> Self {
        ClaudeWebState {
            cookie_actor_handle,
            cookie: None,
            org_uuid: None,
            conv_uuid: None,
            cookie_header_value: HeaderValue::from_static(""),
            capabilities: Vec::new(),
            endpoint: CLEWDR_CONFIG.load().endpoint(),
            proxy: CLEWDR_CONFIG.load().wreq_proxy.to_owned(),
            api_format: ClaudeApiFormat::Claude,
            stream: false,
            client: SUPER_CLIENT.to_owned(),
            key: None,
            usage: Usage::default(),
        }
    }

    pub fn with_claude_format(mut self) -> Self {
        self.api_format = ClaudeApiFormat::Claude;
        self
    }

    pub fn with_openai_format(mut self) -> Self {
        self.api_format = ClaudeApiFormat::OpenAI;
        self
    }

    /// Build a request with the current cookie and proxy settings
    pub fn build_request(&self, method: Method, url: impl IntoUrl) -> RequestBuilder {
        // let r = SUPER_CLIENT.cloned();
        self.client
            .set_cookie(&self.endpoint, &self.cookie_header_value);
        let req = self
            .client
            .request(method, url)
            .header(ORIGIN, CLAUDE_ENDPOINT);
        if let Some(uuid) = self.conv_uuid.to_owned() {
            req.header(REFERER, format!("{CLAUDE_ENDPOINT}/chat/{uuid}"))
        } else {
            req.header(REFERER, format!("{CLAUDE_ENDPOINT}/new"))
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
    pub async fn request_cookie(&mut self) -> Result<CookieStatus, ClewdrError> {
        let res = self.cookie_actor_handle.request(None).await?;
        self.cookie = Some(res.to_owned());
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

    /// Deletes or renames the current chat conversation based on configuration
    /// If preserve_chats is true, the chat is renamed rather than deleted
    pub async fn clean_chat(&self) -> Result<(), ClewdrError> {
        if CLEWDR_CONFIG.load().preserve_chats {
            return Ok(());
        }
        let Some(ref org_uuid) = self.org_uuid else {
            return Ok(());
        };
        let Some(ref conv_uuid) = self.conv_uuid else {
            return Ok(());
        };
        let endpoint = format!(
            "{}/api/organizations/{}/chat_conversations/{}",
            self.endpoint, org_uuid, conv_uuid
        );
        debug!("Deleting chat: {}", conv_uuid);
        let _ = self
            .build_request(Method::DELETE, endpoint)
            .send()
            .await
            .context(WreqSnafu {
                msg: "Failed to delete chat conversation",
            });
        Ok(())
    }
}
