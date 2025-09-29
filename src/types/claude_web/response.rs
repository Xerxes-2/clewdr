use axum::{Json, response::IntoResponse};
use bytes::Bytes;
use eventsource_stream::{EventStream, Eventsource};
use futures::{Stream, TryStreamExt};
use serde::Deserialize;

use crate::{
    claude_web_state::ClaudeWebState,
    error::ClewdrError,
    types::claude::{ContentBlock, CreateMessageResponse, Message, Role},
    utils::{forward_response, print_out_text},
};

/// Merges server-sent events (SSE) from a stream into a single string
/// Extracts and concatenates completion data from events
///
/// # Arguments
/// * `stream` - Event stream to process
///
/// # Returns
/// Combined completion text from all events
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

impl<S> From<S> for Message
where
    S: Into<String>,
{
    /// Converts a string into a Message with assistant role
    ///
    /// # Arguments
    /// * `str` - The text content for the message
    ///
    /// # Returns
    /// * `Message` - A message with assistant role and text content
    fn from(str: S) -> Self {
        Message::new_blocks(
            Role::Assistant,
            vec![ContentBlock::Text { text: str.into() }],
        )
    }
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
    pub async fn transform_response(
        &mut self,
        wreq_res: wreq::Response,
    ) -> Result<axum::response::Response, ClewdrError> {
        if self.stream {
            self.persist_usage_totals(self.usage.input_tokens as u64, 0)
                .await;
            return forward_response(wreq_res);
        }

        let stream = wreq_res.bytes_stream();
        let stream = stream.eventsource();
        let text = merge_sse(stream).await?;
        print_out_text(text.to_owned(), "claude_web_non_stream.txt");
        let mut response =
            CreateMessageResponse::text(text, Default::default(), self.usage.to_owned());
        let output_tokens = response.count_tokens();
        let mut usage = self.usage.to_owned();
        usage.output_tokens = output_tokens;
        response.usage = Some(usage.clone());
        self.persist_usage_totals(usage.input_tokens as u64, output_tokens as u64)
            .await;
        Ok(Json(response).into_response())
    }
}
