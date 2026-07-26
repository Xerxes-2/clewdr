use axum::response::sse::Event;
use futures::{Stream, TryStreamExt};
use serde::Serialize;
use serde_json::Value;

use crate::types::claude::{ContentBlockDelta, CreateMessageResponse, StopReason, StreamEvent};

/// Represents the data structure for streaming events in OpenAI API format
/// Contains a choices array with deltas of content
#[derive(Debug, Serialize)]
struct StreamEventData {
    choices: Vec<StreamEventDelta>,
}

impl StreamEventData {
    /// Creates a new `StreamEventData` with the given content
    ///
    /// # Arguments
    /// * `content` - The event content to include
    ///
    /// # Returns
    /// A new `StreamEventData` instance with the content wrapped in choices array
    fn new(content: EventContent) -> Self {
        Self {
            choices: vec![StreamEventDelta { delta: content }],
        }
    }
}

/// Represents a delta update in a streaming response
/// Contains the content change for the current chunk
#[derive(Debug, Serialize)]
struct StreamEventDelta {
    delta: EventContent,
}

/// Content of an event, either regular content or reasoning (thinking mode)
/// Uses untagged enum to handle different response formats
#[derive(Debug, Serialize)]
#[serde(untagged)]
pub enum EventContent {
    Content { content: String },
    Reasoning { reasoning_content: String },
}

/// Creates an SSE event with the given content in OpenAI format
///
/// # Arguments
/// * `content` - The event content to include
///
/// # Returns
/// A formatted SSE Event ready to be sent to the client
pub fn build_event(content: EventContent) -> Event {
    let event = Event::default();
    let data = StreamEventData::new(content);
    event.json_data(data).unwrap()
}

/// Transforms a Claude.ai event stream into an OpenAI-compatible event stream
///
/// Extracts content from Claude events and reformats them to match OpenAI's streaming format.
/// This function processes each event in the stream, identifying the delta content type
/// (text or thinking), and converting it to the appropriate OpenAI-compatible event format.
///
/// # Arguments
/// * `s` - The input stream of Claude.ai events
///
/// # Returns
/// A stream of OpenAI-compatible SSE events
///
/// # Type Parameters
/// * `I` - The input stream type
/// * `E` - The error type for the stream
pub fn transform_stream<I, E>(s: I) -> impl Stream<Item = Result<Event, E>>
where
    I: Stream<Item = Result<eventsource_stream::Event, E>>,
{
    s.try_filter_map(async |eventsource_stream::Event { data, .. }| {
        let Ok(parsed) = serde_json::from_str::<StreamEvent>(&data) else {
            return Ok(None);
        };
        let StreamEvent::ContentBlockDelta { delta, .. } = parsed else {
            return Ok(None);
        };
        match delta {
            ContentBlockDelta::TextDelta { text } => {
                Ok(Some(build_event(EventContent::Content { content: text })))
            }
            ContentBlockDelta::ThinkingDelta { thinking } => {
                Ok(Some(build_event(EventContent::Reasoning {
                    reasoning_content: thinking,
                })))
            }
            _ => Ok(None),
        }
    })
}

pub fn transforms_json(input: &CreateMessageResponse) -> Value {
    let content = input
        .content
        .iter()
        .filter_map(|block| match block {
            crate::types::claude::ContentBlock::Text { text, .. } => Some(text.clone()),
            _ => None,
        })
        .collect::<String>();

    let usage = input.usage.as_ref().map(|u| {
        serde_json::json!({
            "prompt_tokens": u.input_tokens,
            "completion_tokens": u.output_tokens,
            "total_tokens": u.input_tokens + u.output_tokens
        })
    });

    let finish_reason = match input.stop_reason {
        Some(StopReason::MaxTokens | StopReason::ModelContextWindowExceeded) => "length",
        Some(StopReason::ToolUse) => "tool_calls",
        Some(StopReason::Refusal) => "content_filter",
        Some(StopReason::EndTurn | StopReason::StopSequence | StopReason::PauseTurn) | None => {
            "stop"
        }
    };

    serde_json::json!({
        "id": input.id,
        "object": "chat.completion",
        "created": std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs(),
        "model": input.model,
        "choices": [{
            "index": 0,
            "message": {
                "role": "assistant",
                "content": content
            },
            "finish_reason": finish_reason
        }],
        "usage": usage
    })
}
