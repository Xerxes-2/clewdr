use std::{fmt::Debug, mem, sync::LazyLock};

use axum::{
    Json,
    body::Body,
    extract::{FromRequestParts, State},
    response::{IntoResponse, Response},
};
use colored::Colorize;
use eventsource_stream::Eventsource;
use rquest::{StatusCode, header::ACCEPT};
use scopeguard::defer;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::spawn;
use tracing::{debug, info, warn};

use crate::{
    client::AppendHeaders,
    error::{ClewdrError, check_res_err},
    state::AppState,
    text::merge_sse,
    types::message::{ContentBlock, ImageSource, Message, Role},
    utils::{print_out_json, print_out_text},
};

/// Exact test message send by SillyTavern
pub static TEST_MESSAGE: LazyLock<Message> = LazyLock::new(|| {
    Message::new_blocks(
        Role::User,
        vec![ContentBlock::Text {
            text: "Hi".to_string(),
        }],
    )
});

/// Claude.ai attachment
#[derive(Deserialize, Serialize, Debug)]
pub struct Attachment {
    extracted_content: String,
    file_name: String,
    file_type: String,
    file_size: u64,
}

impl Attachment {
    pub fn new(content: String) -> Self {
        Attachment {
            file_size: content.len() as u64,
            extracted_content: content,
            file_name: "paste.txt".to_string(),
            file_type: "txt".to_string(),
        }
    }
}

/// Request body to be sent to the Claude.ai
#[derive(Deserialize, Serialize, Debug)]
pub struct RequestBody {
    pub max_tokens_to_sample: u64,
    pub attachments: Vec<Attachment>,
    pub files: Vec<String>,
    pub model: String,
    pub rendering_mode: String,
    pub prompt: String,
    pub timezone: String,
    #[serde(skip)]
    pub images: Vec<ImageSource>,
}

/// Request body sent from the client
#[derive(Deserialize, Serialize, Debug)]
pub struct ClientRequestBody {
    pub max_tokens: u64,
    pub messages: Vec<Message>,
    #[serde(default)]
    pub stop_sequences: Vec<String>,
    pub model: String,
    #[serde(default)]
    pub stream: bool,
    #[serde(default)]
    pub thinking: Option<Thinking>,
    #[serde(default)]
    pub system: Value,
    #[serde(default)]
    pub temperature: f32,
    #[serde(default)]
    pub top_p: f32,
    #[serde(default)]
    pub top_k: u64,
}

/// Thinking mode in Claude API Request
#[derive(Deserialize, Serialize, Debug)]
pub struct Thinking {
    budget_tokens: u64,
    r#type: String,
}

pub struct Auth(pub String);

impl FromRequestParts<AppState> for Auth {
    type Rejection = StatusCode;
    async fn from_request_parts(
        parts: &mut axum::http::request::Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let key = parts
            .headers
            .get("x-api-key")
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default();
        if !state.config.auth(key) {
            warn!("Invalid password: {}", key);
            return Err(StatusCode::UNAUTHORIZED);
        }
        Ok(Auth(key.to_string()))
    }
}

/// Axum handler for the API messages
pub async fn api_messages(
    Auth(_): Auth,
    State(mut state): State<AppState>,
    Json(p): Json<ClientRequestBody>,
) -> Response {
    // Check if the request is a test message
    if !p.stream && p.messages == vec![TEST_MESSAGE.clone()] {
        // respond with a test message
        return Json(non_stream_message(
            "Claude Reverse Proxy is working, please send a real message.".to_string(),
        ))
        .into_response();
    }

    let stream = p.stream;
    let stopwatch = chrono::Utc::now();
    info!(
        "Request received, stream mode: {}, messages: {}, model: {}",
        stream.to_string().green(),
        p.messages.len().to_string().green(),
        p.model.to_string().green()
    );

    if let Err(e) = state.request_cookie().await {
        return Json(e.error_body()).into_response();
    }
    let mut state_clone = state.clone();
    defer! {
        // ensure the cookie is returned
        spawn(async move {
            let dur = chrono::Utc::now().signed_duration_since(stopwatch);
            info!(
                "Request finished, elapsed time: {} seconds",
                dur.num_seconds().to_string().green()
            );
            state_clone.return_cookie(None).await;
        });
    }
    // check if request is successful
    match state.bootstrap().await.and(state.try_message(p).await) {
        Ok(b) => {
            if let Err(e) = state.delete_chat().await {
                warn!("Failed to delete chat: {}", e);
            }
            b.into_response()
        }
        Err(e) => {
            // delete chat after an error
            if let Err(e) = state.delete_chat().await {
                warn!("Failed to delete chat: {}", e);
            }
            warn!("Error: {}", e);
            // 429 error
            match e {
                ClewdrError::InvalidCookie(ref r) => {
                    state.return_cookie(Some(r.clone())).await;
                }
                ClewdrError::OtherHttpError(c, e) => {
                    state.return_cookie(None).await;
                    return (c, Json(e)).into_response();
                }
                _ => {
                    state.return_cookie(None).await;
                }
            }
            if stream {
                // stream the error as a response
                Body::from_stream(e.error_stream()).into_response()
            } else {
                // return the error as a response
                Json(e.error_body()).into_response()
            }
        }
    }
}

impl AppState {
    /// Try to send a message to the Claude API
    async fn try_message(&mut self, p: ClientRequestBody) -> Result<Response, ClewdrError> {
        print_out_json(&p, "0.req.json");
        let stream = p.stream;
        let proxy = self.config.rquest_proxy.clone();
        let Some(ref org_uuid) = self.org_uuid else {
            return Ok(Json(non_stream_message(
                "No organization found, please check your cookie.".to_string(),
            ))
            .into_response());
        };

        // Create a new conversation
        let new_uuid = uuid::Uuid::new_v4().to_string();
        self.conv_uuid = Some(new_uuid.to_string());
        let endpoint = format!(
            "{}/api/organizations/{}/chat_conversations",
            self.config.endpoint(),
            org_uuid
        );
        let mut body = json!({
            "uuid": new_uuid,
            "name":""
        });

        // enable thinking mode
        if p.thinking.is_some() {
            body["paprika_mode"] = "extended".into();
            body["model"] = p.model.clone().into();
        }
        let api_res = self
            .client
            .post(endpoint)
            .json(&body)
            .append_headers("", proxy.clone())
            .send()
            .await?;
        debug!("New conversation created: {}", new_uuid);

        check_res_err(api_res).await?;

        // generate the request body
        // check if the request is empty
        let Some(mut body) = self.transform(p) else {
            return Ok(Json(non_stream_message(
                "Empty request, please send a message.".to_string(),
            ))
            .into_response());
        };

        // check images
        let images = mem::take(&mut body.images);

        // upload images
        let files = self.upload_images(images).await;
        body.files = files;

        // send the request
        print_out_json(&body, "4.req.json");
        let endpoint = format!(
            "{}/api/organizations/{}/chat_conversations/{}/completion",
            self.config.endpoint(),
            org_uuid,
            new_uuid
        );

        let api_res = self
            .client
            .post(endpoint)
            .json(&body)
            .append_headers("", proxy)
            .header_append(ACCEPT, "text/event-stream")
            .send()
            .await?;

        let api_res = check_res_err(api_res).await?;

        // if not streaming, return the response
        if !stream {
            let stream = api_res.bytes_stream().eventsource();
            let text = merge_sse(stream).await;
            print_out_text(&text, "non_stream.txt");
            return Ok(Json(non_stream_message(text)).into_response());
        }

        // stream the response
        let input_stream = api_res.bytes_stream();
        Ok(Body::from_stream(input_stream).into_response())
    }
}

/// Transform a string to a message
pub fn non_stream_message(str: String) -> Message {
    Message::new_blocks(Role::Assistant, vec![ContentBlock::Text { text: str }])
}
