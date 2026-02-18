use colored::Colorize;
use futures::TryFutureExt;
use serde_json::json;
use snafu::ResultExt;
use tracing::{Instrument, debug, error, info, info_span, warn};
use wreq::{Method, Response, header::ACCEPT};

use super::ClaudeWebState;
use crate::{
    config::CLEWDR_CONFIG,
    error::{CheckClaudeErr, ClewdrError, WreqSnafu},
    types::claude::CreateMessageParams,
    utils::print_out_json,
};

impl ClaudeWebState {
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
            // check if request is successful
            let web_res = async { state.bootstrap().await.and(state.send_chat(p).await) };
            let transform_res = web_res
                .and_then(async |r| self.transform_response(r).await)
                .instrument(info_span!("claude_web", "cookie" = cookie.cookie.ellipse()));

            match transform_res.await {
                Ok(b) => {
                    if let Err(e) = state.clean_chat().await {
                        warn!("Failed to clean chat: {}", e);
                    }
                    return Ok(b);
                }
                Err(e) => {
                    // delete chat after an error
                    if let Err(e) = state.clean_chat().await {
                        warn!("Failed to clean chat: {}", e);
                    }
                    error!("{e}");
                    // 429 error
                    if let ClewdrError::InvalidCookie { reason } = e {
                        state.return_cookie(Some(reason.to_owned())).await;
                        continue;
                    }
                    return Err(e);
                }
            }
        }
        error!("Max retries exceeded");
        Err(ClewdrError::TooManyRetries)
    }

    /// Sends a message to the Claude API by creating a new conversation and processing the request
    ///
    /// This method performs several key operations:
    /// - Creates a new conversation with a unique UUID
    /// - Configures thinking mode if applicable
    /// - Transforms the client request to the Claude API format
    /// - Handles image uploads if present
    /// - Sends the request to the Claude API endpoint
    ///
    /// The method properly manages conversation state, including creating a new conversation,
    /// configuring its settings, and sending the actual message content. It handles special
    /// features like thinking mode for Pro accounts and image uploads for multimodal requests.
    ///
    /// # Arguments
    /// * `p` - The client request body containing messages and configuration
    ///
    /// # Returns
    /// * `Result<Response, ClewdrError>` - Response from Claude or error
    async fn send_chat(&mut self, p: CreateMessageParams) -> Result<Response, ClewdrError> {
        let org_uuid = self
            .org_uuid
            .to_owned()
            .ok_or(ClewdrError::UnexpectedNone {
                msg: "Organization UUID is not set",
            })?;

        // Create a new conversation
        let new_uuid = uuid::Uuid::new_v4().to_string();
        let endpoint = self
            .endpoint
            .join(&format!(
                "api/organizations/{}/chat_conversations",
                org_uuid
            ))
            .expect("Url parse error");
        let body = json!({
            "uuid": new_uuid,
            "name": format!("ClewdR-{}", chrono::Utc::now().format("%Y-%m-%d %H:%M:%S")),
        });

        self.build_request(Method::POST, endpoint)
            .json(&body)
            .send()
            .await
            .context(WreqSnafu {
                msg: "Failed to create new conversation",
            })?
            .check_claude()
            .await?;
        self.conv_uuid = Some(new_uuid.to_string());
        debug!("New conversation created: {}", new_uuid);

        // preserve original params for possible post-call token accounting
        self.last_params = Some(p.clone());
        let mut body = json!({});
        // enable thinking mode
        body["settings"]["paprika_mode"] = if p.thinking.is_some() && self.is_pro() {
            "extended".into()
        } else {
            json!(null)
        };

        let endpoint = self
            .endpoint
            .join(&format!(
                "api/organizations/{}/chat_conversations/{}",
                org_uuid, new_uuid
            ))
            .expect("Url parse error");
        let _ = self
            .build_request(Method::PUT, endpoint)
            .json(&body)
            .send()
            .await;
        // generate the request body
        // check if the request is empty
        let mut body = self.transform_request(p).ok_or(ClewdrError::BadRequest {
            msg: "Request body is empty",
        })?;

        // check images
        let images = body.images.drain(..).collect::<Vec<_>>();

        // upload images
        let files = self.upload_images(images).await;
        body.files = files;

        // send the request
        print_out_json(&body, "claude_web_clewdr_req.json");
        let endpoint = self
            .endpoint
            .join(&format!(
                "api/organizations/{}/chat_conversations/{}/completion",
                org_uuid, new_uuid
            ))
            .expect("Url parse error");

        self.build_request(Method::POST, endpoint)
            .json(&body)
            .header(ACCEPT, "text/event-stream")
            .send()
            .await
            .context(WreqSnafu {
                msg: "Failed to send chat request",
            })?
            .check_claude()
            .await
    }
}
