use axum::{Json, body::Body, response::IntoResponse};
use bytes::Bytes;
use eventsource_stream::{EventStream, Eventsource};
use futures::{Stream, StreamExt, pin_mut};
use serde::Deserialize;

use crate::{
    services::cache::CACHE,
    context::RequestContext,
    types::message::{ContentBlock, Message, Role},
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
pub async fn merge_sse(
    stream: EventStream<impl Stream<Item = Result<Bytes, rquest::Error>>>,
) -> String {
    #[derive(Deserialize)]
    struct Data {
        completion: String,
    }
    pin_mut!(stream);
    let mut w = String::new();
    while let Some(event) = stream.next().await {
        let Ok(event) = event else {
            continue;
        };
        let data = event.data;
        let Ok(data) = serde_json::from_str::<Data>(&data) else {
            continue;
        };
        w += data.completion.as_str();
    }
    w
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

impl RequestContext {
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
        &self,
        input: impl Stream<Item = Result<Bytes, rquest::Error>> + Send + 'static,
    ) -> axum::response::Response {
        // response is used for caching
        if let Some((key, id)) = self.key {
            CACHE.push(input, key, id);
            // return whatever, not used
            return Body::empty().into_response();
        }
        // response is used for returning
        // not streaming
        if !self.stream {
            let stream = input.eventsource();
            let text = merge_sse(stream).await;
            print_out_text(&text, "non_stream.txt");
            let mut resp = Json(Message::from(text)).into_response();
            // Add RequestContext to response extensions
            resp.extensions_mut().insert(self.clone());
            return resp;
        }

        // stream the response
        let mut resp = Body::from_stream(input).into_response();
        // Add RequestContext to response 
        
        resp.extensions_mut().insert(self.clone());
        resp
    }
}
