use async_stream::try_stream;
use axum::{
    BoxError, Json,
    response::{IntoResponse, Sse, sse::Event as SseEvent},
};
use bytes::Bytes;
use clewdr_anthropic::{
    CountMessageTokensResponse, CreateMessageParams, CreateMessageResponse, Message, Role,
};
use eventsource_stream::{EventStream, Eventsource};
use futures::{Stream, TryStreamExt};
use serde::Deserialize;
use url::Url;
use wreq::Proxy;

use crate::{
    claude_code_state::ClaudeCodeState,
    claude_web_state::ClaudeWebState,
    error::{CheckClaudeErr, ClewdrError},
    utils::print_out_text,
};

/// Merges server-sent events (SSE) from a stream into a single string
/// Extracts and concatenates completion data from events
///
/// # Arguments
/// * `stream` - Event stream to process
///
/// # Returns
/// Combined completion text from all events
///
/// # Errors
/// If the stream yields a transport error, or an event's payload is not the
/// expected JSON shape.
pub async fn merge_sse(
    stream: EventStream<impl Stream<Item = Result<Bytes, wreq::Error>>>,
) -> Result<String, ClewdrError> {
    #[derive(Deserialize)]
    struct Data {
        completion: String,
    }
    Ok(stream
        .try_filter_map(async |event| {
            Ok(serde_json::from_str::<Data>(&event.data)
                .map(|data| data.completion)
                .ok())
        })
        .try_collect()
        .await?)
}

impl ClaudeWebState {
    /// Converts the response from the Claude Web into Claude API or OpenAI API format
    ///
    /// This method transforms streams of bytes from Claude's web response into the appropriate
    /// format based on the client's requested API format (Claude or OpenAI). It handles both
    /// streaming and non-streaming responses, and manages caching for responses.
    ///
    /// # Arguments
    /// * `input` - The response stream from the Claude Web API
    ///
    /// # Returns
    /// * `axum::response::Response` - Transformed response in the requested format
    ///
    /// # Errors
    /// If the upstream body cannot be read or does not parse into the expected
    /// shape.
    pub async fn transform_response(
        &mut self,
        wreq_res: wreq::Response,
    ) -> Result<axum::response::Response, ClewdrError> {
        if self.stream {
            return self.transform_stream_response(wreq_res).await;
        }
        self.transform_buffered_response(wreq_res).await
    }

    /// Stream the upstream SSE through to the client, accumulating usage as it
    /// goes and persisting it once the stream ends.
    async fn transform_stream_response(
        &mut self,
        wreq_res: wreq::Response,
    ) -> Result<axum::response::Response, ClewdrError> {
        // Stream through while accumulating completion text; persist usage at end
        let mut input_tokens = u64::from(self.usage.input_tokens);
        let handle = self.cookie_pool.clone();
        let cookie = self.cookie.clone();
        let enable_precise = crate::config::CLEWDR_CONFIG.load().enable_web_count_tokens;
        let last_params = self.last_params.clone();
        let endpoint = self.endpoint.clone();
        let proxy = self.proxy.clone();
        let client = self.client.clone();
        // try to get precise input tokens via Claude Code count_tokens if enabled
        if crate::config::CLEWDR_CONFIG.load().enable_web_count_tokens
            && let Some(tokens) = self.try_code_count_tokens().await
        {
            input_tokens = u64::from(tokens);
        }

        let stream = wreq_res
            .bytes_stream()
            .eventsource()
            .map_err(axum::Error::new);
        let stream = try_stream! {
            let mut acc = String::new();
            #[derive(serde::Deserialize)]
            struct Data { completion: String }
            futures::pin_mut!(stream);
            while let Some(event) = stream.try_next().await? {
                if let Ok(d) = serde_json::from_str::<Data>(&event.data) {
                    acc.push_str(&d.completion);
                }
                let e = SseEvent::default().event(event.event).id(event.id);
                let e = if let Some(retry) = event.retry { e.retry(retry) } else { e };
                yield e.data(event.data);
            }
            // on end of stream, compute output tokens and persist totals
            if !acc.is_empty() {
                // Prefer official count_tokens if enabled and possible; else estimate locally
                let mut out = None;
                if enable_precise
                    && let Some(model) = last_params.as_ref().map(|p| p.model.clone())
                {
                    out = count_code_output_tokens_for_text(
                        cookie.clone(), endpoint.clone(), proxy.clone(), client.clone(),
                        model, acc.clone(), handle.clone()
                    ).await.map(u64::from);
                }
                let out = out.unwrap_or_else(|| {
                    let usage = clewdr_anthropic::Usage {
                        input_tokens: u32::try_from(input_tokens).unwrap_or(u32::MAX),
                        output_tokens: 0,
                    };
                    let resp = clewdr_anthropic::CreateMessageResponse::text(acc.clone(), String::default(), usage);
                    u64::from(resp.count_tokens())
                });
                if let Some(mut c) = cookie.clone() {
                    let family = last_params
                        .as_ref()
                        .map(|p| p.model.as_str())
                        .map_or(crate::config::ModelFamily::Other, |m| {
                            let m = m.to_ascii_lowercase();
                            if m.contains("opus") {
                                crate::config::ModelFamily::Opus
                            } else if m.contains("sonnet") {
                                crate::config::ModelFamily::Sonnet
                            } else {
                                crate::config::ModelFamily::Other
                            }
                        });
                    c.add_and_bucket_usage(input_tokens, out, family);
                    handle.return_cookie(c, None);
                }
            } else if let Some(mut c) = cookie.clone() {
                // still persist input tokens to maintain parity
                let family = last_params
                    .as_ref()
                    .map(|p| p.model.as_str())
                    .map_or(crate::config::ModelFamily::Other, |m| {
                        let m = m.to_ascii_lowercase();
                        if m.contains("opus") {
                            crate::config::ModelFamily::Opus
                        } else if m.contains("sonnet") {
                            crate::config::ModelFamily::Sonnet
                        } else {
                            crate::config::ModelFamily::Other
                        }
                    });
                c.add_and_bucket_usage(input_tokens, 0, family);
                handle.return_cookie(c, None);
            }
        };
        // normalize error type for axum SSE
        let stream = stream.map_err(|e: axum::Error| -> BoxError { e.into() });
        Ok(Sse::new(stream)
            .keep_alive(axum::response::sse::KeepAlive::default())
            .into_response())
    }

    /// Collect the whole upstream response and return it as a single JSON body.
    ///
    /// # Errors
    /// If the upstream body cannot be read or does not parse into the expected
    /// shape.
    async fn transform_buffered_response(
        &mut self,
        wreq_res: wreq::Response,
    ) -> Result<axum::response::Response, ClewdrError> {
        let stream = wreq_res.bytes_stream();
        let stream = stream.eventsource();
        let text = merge_sse(stream).await?;
        print_out_text(text.clone(), "claude_web_non_stream.txt");
        let mut response =
            CreateMessageResponse::text(text.clone(), String::default(), self.usage.clone());

        // Prefer official counting if enabled
        let enable_precise = crate::config::CLEWDR_CONFIG.load().enable_web_count_tokens;
        let mut usage = self.usage.clone();
        if enable_precise && let Some(inp) = self.try_code_count_tokens().await {
            usage.input_tokens = inp;
        }
        let mut output_tokens = response.count_tokens();
        if enable_precise && let Some(model) = self.last_params.as_ref().map(|p| p.model.clone()) {
            let out = count_code_output_tokens_for_text(
                self.cookie.clone(),
                self.endpoint.clone(),
                self.proxy.clone(),
                self.client.clone(),
                model,
                text.clone(),
                self.cookie_pool.clone(),
            )
            .await;
            if let Some(v) = out {
                output_tokens = v;
            }
        }
        usage.output_tokens = output_tokens;
        response.usage = Some(usage.clone());
        self.persist_usage_totals(u64::from(usage.input_tokens), u64::from(output_tokens));
        Ok(Json(response).into_response())
    }
}

async fn bearer_count_tokens(
    state: &ClaudeCodeState,
    access_token: &str,
    body: &CreateMessageParams,
) -> Option<u32> {
    let url = state.endpoint.join("v1/messages/count_tokens").ok()?;
    let resp = state
        .client
        .post(url.to_string())
        .bearer_auth(access_token)
        .header("anthropic-version", "2023-06-01")
        .json(body)
        .send()
        .await
        .ok()?;
    let resp = resp.check_claude().await.ok()?;
    let v: CountMessageTokensResponse = resp.json().await.ok()?;
    Some(v.input_tokens)
}

impl ClaudeWebState {
    pub(crate) async fn try_code_count_tokens(&mut self) -> Option<u32> {
        self.cookie.as_ref()?;
        let params = self.last_params.as_ref()?.clone();
        let mut code = ClaudeCodeState::new(self.cookie_pool.clone());
        code.cookie = self.cookie.clone();
        code.endpoint = self.endpoint.clone();
        code.proxy = self.proxy.clone();
        code.client = self.client.clone();
        // populate cookie header for Claude code API requests
        if let Some(ref c) = self.cookie
            && let Ok(val) = http::HeaderValue::from_str(&c.cookie.to_string())
        {
            code.set_cookie_header_value(val);
        }

        // OAuth exchange to get access token
        let org = code.get_organization().await.ok()?;
        let exch = code.exchange_code(&org).await.ok()?;
        code.exchange_token(exch).await.ok()?;
        let access = code.cookie.as_ref()?.token.as_ref()?.access_token.clone();

        // prepare body
        let mut body = params.clone();
        body.stream = Some(false);

        // do count_tokens
        bearer_count_tokens(&code, &access, &body).await
    }
}

async fn count_code_output_tokens_for_text(
    cookie: Option<crate::config::CookieStatus>,
    endpoint: Url,
    proxy: Option<Proxy>,
    client: wreq::Client,
    model: String,
    text: String,
    handle: crate::services::cookie_pool::CookiePool,
) -> Option<u32> {
    let mut code = ClaudeCodeState::new(handle.clone());
    code.cookie = cookie.clone();
    code.endpoint = endpoint;
    code.proxy = proxy;
    code.client = client;
    if let Some(ref c) = cookie
        && let Ok(val) = http::HeaderValue::from_str(&c.cookie.to_string())
    {
        code.set_cookie_header_value(val);
    }
    let org = code.get_organization().await.ok()?;
    let exch = code.exchange_code(&org).await.ok()?;
    code.exchange_token(exch).await.ok()?;
    let access = code.cookie.as_ref()?.token.as_ref()?.access_token.clone();

    let body = CreateMessageParams {
        model,
        messages: vec![Message::new_text(Role::Assistant, text)],
        ..Default::default()
    };
    // do not set count_tokens_allowed flag here to avoid races; handled by try_code_count_tokens
    bearer_count_tokens(&code, &access, &body).await
}
