use axum::http::HeaderValue;
use rquest::{
    Client, ClientBuilder, IntoUrl, Method, Proxy, RequestBuilder,
    header::{ORIGIN, REFERER},
};
use rquest_util::Emulation;
use tracing::{debug, error};
use url::Url;

use std::sync::LazyLock;

use crate::{
    api::ApiFormat,
    config::{CLEWDR_CONFIG, CookieStatus, ENDPOINT, Reason},
    error::ClewdrError,
    services::cookie_manager::CookieEventSender,
    types::message::CreateMessageParams,
};

pub mod bootstrap;
pub mod chat;
/// Placeholder
static SUPER_CLIENT: LazyLock<Client> = LazyLock::new(Client::new);
/// State of current connection
#[derive(Clone)]
pub struct RequestContext {
    pub cookie: Option<CookieStatus>,
    cookie_header_value: HeaderValue,
    pub event_sender: CookieEventSender,
    pub org_uuid: Option<String>,
    pub conv_uuid: Option<String>,
    pub capabilities: Vec<String>,
    pub endpoint: Url,
    pub proxy: Option<Proxy>,
    pub api_format: ApiFormat,
    pub stream: bool,
    pub client: Client,
    pub key: Option<(u64, usize)>,
    pub current_request: Option<CreateMessageParams>,
}

impl RequestContext {
    /// Create a new AppState instance
    pub fn new(event_sender: CookieEventSender) -> Self {
        RequestContext {
            event_sender,
            cookie: None,
            org_uuid: None,
            conv_uuid: None,
            cookie_header_value: HeaderValue::from_static(""),
            capabilities: Vec::new(),
            endpoint: CLEWDR_CONFIG.load().endpoint(),
            proxy: CLEWDR_CONFIG.load().rquest_proxy.to_owned(),
            api_format: ApiFormat::Claude,
            stream: false,
            client: SUPER_CLIENT.to_owned(),
            key: None,
            current_request: None,
        }
    }

    /// Build a request with the current cookie and proxy settings
    pub fn build_request(&self, method: Method, url: impl IntoUrl) -> RequestBuilder {
        // let r = SUPER_CLIENT.cloned();
        self.client
            .set_cookie(&self.endpoint, &self.cookie_header_value);
        let req = self
            .client
            .request(method, url)
            .header_append(ORIGIN, ENDPOINT);
        if let Some(uuid) = self.conv_uuid.to_owned() {
            req.header_append(REFERER, format!("{}/chat/{}", ENDPOINT, uuid))
        } else {
            req.header_append(REFERER, format!("{}/new", ENDPOINT))
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
    pub async fn request_cookie(&mut self) -> Result<(), ClewdrError> {
        let res = self.event_sender.request().await?;
        self.cookie = Some(res.to_owned());
        let mut client = ClientBuilder::new()
            .cookie_store(true)
            .emulation(Emulation::Chrome135);
        if let Some(ref proxy) = self.proxy {
            client = client.proxy(proxy.to_owned());
        }
        self.client = client.build()?;
        self.cookie_header_value = HeaderValue::from_str(res.cookie.to_string().as_str())?;
        // load newest config
        self.proxy = CLEWDR_CONFIG.load().rquest_proxy.to_owned();
        self.endpoint = CLEWDR_CONFIG.load().endpoint();
        Ok(())
    }

    /// Returns the current cookie to the cookie manager
    /// Optionally provides a reason for returning the cookie (e.g., invalid, banned)
    pub async fn return_cookie(&self, reason: Option<Reason>) {
        // return the cookie to the cookie manager
        if let Some(ref cookie) = self.cookie {
            self.event_sender
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
        let _ = self.build_request(Method::DELETE, endpoint).send().await?;
        Ok(())
    }
}
