use std::sync::LazyLock;

use axum::response::Response;
use colored::Colorize;
use http::header::CONTENT_TYPE;
use hyper_util::client::legacy::connect::HttpConnector;
use serde::Serialize;
use serde_json::Value;
use snafu::ResultExt;
use strum::Display;
use tokio::spawn;
use tracing::{error, info};
use wreq::{Client, ClientBuilder, header::AUTHORIZATION};
use yup_oauth2::{CustomHyperClientBuilder, ServiceAccountAuthenticator, ServiceAccountKey};

use crate::{
    config::{CLEWDR_CONFIG, GEMINI_ENDPOINT, KeyStatus},
    error::{CheckGeminiErr, ClewdrError, InvalidUriSnafu, WreqSnafu},
    middleware::gemini::*,
    services::key_actor::KeyActorHandle,
    types::gemini::response::{FinishReason, GeminiResponse},
    utils::forward_response,
};

#[derive(Clone, Display, PartialEq, Eq)]
pub enum GeminiApiFormat {
    Gemini,
    OpenAI,
}

static DUMMY_CLIENT: LazyLock<Client> = LazyLock::new(Client::new);

// TODO: replace yup-oauth2 with oauth2 crate
async fn get_token(sa_key: ServiceAccountKey) -> Result<String, ClewdrError> {
    const SCOPES: [&str; 1] = ["https://www.googleapis.com/auth/cloud-platform"];
    let token = if let Some(proxy) = CLEWDR_CONFIG.load().proxy.to_owned() {
        let proxy = proxy
            .trim_start_matches("http://")
            .trim_start_matches("https://")
            .trim_start_matches("socks5://");
        let proxy = format!("http://{proxy}");
        let proxy_uri = proxy.parse().context(InvalidUriSnafu {
            uri: proxy.to_owned(),
        })?;
        let proxy = hyper_http_proxy::Proxy::new(hyper_http_proxy::Intercept::All, proxy_uri);
        let connector = HttpConnector::new();
        let proxy_connector = hyper_http_proxy::ProxyConnector::from_proxy(connector, proxy)?;
        let client =
            hyper_util::client::legacy::Client::builder(hyper_util::rt::TokioExecutor::new())
                .pool_max_idle_per_host(0)
                .build(proxy_connector);
        let client_builder = CustomHyperClientBuilder::from(client);
        let auth = ServiceAccountAuthenticator::with_client(sa_key, client_builder)
            .build()
            .await?;
        auth.token(&SCOPES).await?
    } else {
        let auth = ServiceAccountAuthenticator::builder(sa_key).build().await?;
        auth.token(&SCOPES).await?
    };
    let token = token.token().ok_or(ClewdrError::UnexpectedNone {
        msg: "Oauth token is None",
    })?;
    Ok(token.into())
}

#[derive(Clone)]
pub struct GeminiState {
    pub model: String,
    pub vertex: bool,
    pub path: String,
    pub key: Option<KeyStatus>,
    pub stream: bool,
    pub query: GeminiArgs,
    pub key_handle: KeyActorHandle,
    pub cli_handle: crate::services::cli_token_actor::CliTokenActorHandle,
    pub api_format: GeminiApiFormat,
    pub client: Client,
    pub cli_mode: bool,
    pub auth_bearer: Option<String>,
}

impl GeminiState {
    /// Create a new AppState instance
    pub fn new(tx: KeyActorHandle, cli: crate::services::cli_token_actor::CliTokenActorHandle) -> Self {
        GeminiState {
            model: String::new(),
            vertex: false,
            path: String::new(),
            query: GeminiArgs::default(),
            stream: false,
            key: None,
            key_handle: tx,
            cli_handle: cli,
            api_format: GeminiApiFormat::Gemini,
            client: DUMMY_CLIENT.to_owned(),
            cli_mode: false,
            auth_bearer: None,
        }
    }

    pub async fn report_403(&self) -> Result<(), ClewdrError> {
        if let Some(mut key) = self.key.to_owned() {
            key.count_403 += 1;
            self.key_handle.return_key(key).await?;
        }
        Ok(())
    }

    pub async fn request_key(&mut self) -> Result<(), ClewdrError> {
        let key = self.key_handle.request().await?;
        self.key = Some(key.to_owned());
        let client = ClientBuilder::new();
        let client = if let Some(proxy) = CLEWDR_CONFIG.load().proxy.to_owned() {
            client.proxy(proxy)
        } else {
            client
        };
        self.client = client.build().context(WreqSnafu {
            msg: "Failed to build Gemini client",
        })?;
        Ok(())
    }

    pub fn update_from_ctx(&mut self, ctx: &GeminiContext) {
        self.path = ctx.path.to_owned();
        self.stream = ctx.stream.to_owned();
        self.query = ctx.query.to_owned();
        self.model = ctx.model.to_owned();
        self.vertex = ctx.vertex.to_owned();
        self.api_format = ctx.api_format.to_owned();
        self.cli_mode = ctx.cli_mode;
        self.auth_bearer = ctx.auth_bearer.to_owned();
    }

    async fn vertex_response(
        &mut self,
        p: impl Sized + Serialize,
    ) -> Result<wreq::Response, ClewdrError> {
        let client = ClientBuilder::new();
        let client = if let Some(proxy) = CLEWDR_CONFIG.load().proxy.to_owned() {
            client.proxy(proxy)
        } else {
            client
        };
        self.client = client.build().context(WreqSnafu {
            msg: "Failed to build Gemini client",
        })?;
        let method = if self.stream {
            "streamGenerateContent"
        } else {
            "generateContent"
        };

        // Get an access token
        let Some(cred) = CLEWDR_CONFIG.load().vertex.credential.to_owned() else {
            return Err(ClewdrError::BadRequest {
                msg: "Vertex credential not found",
            });
        };

        let access_token = get_token(cred.to_owned()).await?;
        let bearer = format!("Bearer {access_token}");
        let res = match self.api_format {
            GeminiApiFormat::Gemini => {
                let endpoint = format!(
                    "https://aiplatform.googleapis.com/v1/projects/{}/locations/global/publishers/google/models/{}:{method}",
                    cred.project_id.unwrap_or_default(),
                    self.model
                );
                let query_vec = self.query.to_vec();
                self
                    .client
                    .post(endpoint)
                    .query(&query_vec)
                    .header(AUTHORIZATION, bearer)
                    .json(&p)
                    .send()
                    .await
                    .context(WreqSnafu {
                        msg: "Failed to send request to Gemini Vertex API",
                    })?
            }
            GeminiApiFormat::OpenAI => {
                self.client
                    .post(format!(
                        "https://aiplatform.googleapis.com/v1beta1/projects/{}/locations/global/endpoints/openapi/chat/completions",
                        cred.project_id.unwrap_or_default(),
                    ))
                    .header(AUTHORIZATION, bearer)
                    .json(&p)
                    .send()
                    .await
                    .context(WreqSnafu {
                        msg: "Failed to send request to Gemini Vertex OpenAI API",
                    })?
            }
        };
        let res = res.check_gemini().await?;
        Ok(res)
    }

    pub async fn send_chat(
        &mut self,
        p: impl Sized + Serialize,
    ) -> Result<wreq::Response, ClewdrError> {
        if self.vertex {
            let res = self.vertex_response(p).await?;
            return Ok(res);
        }
        // If CLI mode and a user bearer exists, or saved CLI tokens exist, prefer OAuth bearer
        let res = if self.cli_mode {
            if !(self.auth_bearer.is_some() || !CLEWDR_CONFIG.load().cli_tokens.is_empty()) {
                return Err(ClewdrError::BadRequest { msg: "CLI requires OAuth credentials (Bearer or saved CLI token)" });
            }
            // Use user OAuth (ya29...) to call Code Assist endpoint like gcli2api
            // 1) obtain token (with optional refresh)
            let (token, project_id) = {
                if let Some(b) = self.auth_bearer.to_owned() {
                    (b, CLEWDR_CONFIG.load().vertex.model_id.to_owned()) // project_id may be missing when bearer is provided directly
                } else {
                    let mut t = self.cli_handle.request().await?;
                    let mut result_token = t.token.to_string();
                    if let Some(exp) = t.expiry {
                        let now = chrono::Utc::now();
                        let near = exp - chrono::Duration::seconds(300);
                        if now >= near {
                            if let Some(meta) = t.meta.clone() {
                                if let (Some(client_id), Some(client_secret), Some(refresh_token), Some(token_uri)) = (meta.client_id, meta.client_secret, meta.refresh_token, meta.token_uri) {
                                    let form = [
                                        ("grant_type", "refresh_token"),
                                        ("client_id", client_id.as_str()),
                                        ("client_secret", client_secret.as_str()),
                                        ("refresh_token", refresh_token.as_str()),
                                    ];
                                    let resp = self.client
                                        .post(token_uri)
                                        .form(&form)
                                        .send()
                                        .await
                                        .context(WreqSnafu { msg: "Failed to refresh CLI OAuth token" })?;
                                    let v: serde_json::Value = resp.json().await.context(WreqSnafu { msg: "Failed to parse refreshed token" })?;
                                    if let Some(acc) = v.get("access_token").and_then(|x| x.as_str()) {
                                        result_token = acc.to_string();
                                        t.token = result_token.clone().into();
                                        if let Some(ei) = v.get("expires_in").and_then(|x| x.as_i64()) {
                                            t.expiry = Some(chrono::Utc::now() + chrono::Duration::seconds(ei));
                                        }
                                        let _ = self.cli_handle.return_token(t.clone()).await;
                                    }
                                }
                            }
                        }
                    }
                    let project = t.meta.as_ref().and_then(|m| m.project_id.clone());
                    info!("[CLI TOKEN] {}", result_token.chars().take(10).collect::<String>());
                    (result_token, project)
                }
            };
            let bearer = format!("Bearer {}", token);
            match self.api_format {
                GeminiApiFormat::Gemini => {
                    // Build Code Assist payload: { model, project, request }
                    let method = if self.stream { "v1internal:streamGenerateContent" } else { "v1internal:generateContent" };
                    let mut endpoint = format!("https://cloudcode-pa.googleapis.com/{method}");
                    if self.stream { endpoint.push_str("?alt=sse"); }
                    let payload = serde_json::json!({
                        "model": self.model,
                        "project": project_id.unwrap_or_default(),
                        "request": p,
                    });
                    self.client
                        .post(endpoint)
                        .header(AUTHORIZATION, bearer)
                        .json(&payload)
                        .send()
                        .await
                        .context(WreqSnafu { msg: "Failed to send request to Code Assist API (CLI bearer)" })?
                }
                GeminiApiFormat::OpenAI => {
                    // Keep OAI path to generativelanguage OpenAI endpoint with bearer
                    self.client
                        .post(format!("{GEMINI_ENDPOINT}/v1beta/openai/chat/completions"))
                        .header(AUTHORIZATION, bearer)
                        .json(&p)
                        .send()
                        .await
                        .context(WreqSnafu { msg: "Failed to send request to Gemini OpenAI API (CLI bearer)" })?
                }
            }
        } else {
            // Default: use API key pool
            self.request_key().await?;
            let Some(key) = self.key.to_owned() else {
                return Err(ClewdrError::UnexpectedNone { msg: "Key is None, did you request a key?" });
            };
            info!("[KEY] {}", key.key.ellipse().green());
            let key = key.key.to_string();
            match self.api_format {
                GeminiApiFormat::Gemini => {
                    let mut query_vec = self.query.to_vec();
                    query_vec.push(("key", key.as_str()));
                    self.client
                        .post(format!("{}/v1beta/{}", GEMINI_ENDPOINT, self.path))
                        .query(&query_vec)
                        .json(&p)
                        .send()
                        .await
                        .context(WreqSnafu { msg: "Failed to send request to Gemini API" })?
                }
                GeminiApiFormat::OpenAI => self
                    .client
                    .post(format!("{GEMINI_ENDPOINT}/v1beta/openai/chat/completions",))
                    .header(AUTHORIZATION, format!("Bearer {key}"))
                    .json(&p)
                    .send()
                    .await
                    .context(WreqSnafu { msg: "Failed to send request to Gemini OpenAI API" })?,
            }
        };
        let res = res.check_gemini().await?;
        Ok(res)
    }

    pub async fn try_chat(&mut self, p: impl Serialize + Clone) -> Result<Response, ClewdrError> {
        let mut err = None;
        for i in 0..CLEWDR_CONFIG.load().max_retries + 1 {
            if i > 0 {
                info!("[RETRY] attempt: {}", i.to_string().green());
            }
            let mut state = self.to_owned();
            let p = p.to_owned();

            match state.send_chat(p).await {
                Ok(resp) => match state.check_empty_choices(resp).await {
                    Ok(resp) => return Ok(resp),
                    Err(e) => {
                        error!("Failed to check empty choices: {}", e);
                        err = Some(e);
                        continue;
                    }
                },
                Err(e) => {
                    if let Some(key) = state.key.to_owned() {
                        error!("[{}] {}", key.key.ellipse().green(), e);
                    } else {
                        error!("{}", e);
                    }
                    match e {
                        ClewdrError::GeminiHttpError { code, .. } => {
                            if code == 403 {
                                spawn(async move {
                                    state.report_403().await.unwrap_or_else(|e| {
                                        error!("Failed to report 403: {}", e);
                                    });
                                });
                            }
                            err = Some(e);
                            continue;
                        }
                        e => return Err(e),
                    }
                }
            }
        }
        error!("Max retries exceeded");
        if let Some(e) = err {
            return Err(e);
        }
        Err(ClewdrError::TooManyRetries)
    }

    async fn check_empty_choices(&self, resp: wreq::Response) -> Result<Response, ClewdrError> {
        if self.stream || self.cli_mode {
            return forward_response(resp);
        }
        let bytes = resp.bytes().await.context(WreqSnafu {
            msg: "Failed to get bytes from Gemini response",
        })?;

        match self.api_format {
            GeminiApiFormat::Gemini => {
                let res = serde_json::from_slice::<GeminiResponse>(&bytes)?;
                if res.candidates.is_empty() {
                    return Err(ClewdrError::EmptyChoices);
                }
                if res.candidates[0].finishReason == Some(FinishReason::OTHER) {
                    return Err(ClewdrError::EmptyChoices);
                }
            }
            GeminiApiFormat::OpenAI => {
                let res = serde_json::from_slice::<Value>(&bytes)?;
                if res["choices"].as_array().is_some_and(|v| v.is_empty()) {
                    return Err(ClewdrError::EmptyChoices);
                }
                if res["choices"][0]["finish_reason"] == "OTHER" {
                    return Err(ClewdrError::EmptyChoices);
                }
            }
        }
        Ok(Response::builder()
            .header(CONTENT_TYPE, "application/json")
            .body(bytes.into())?)
    }
}
