use axum::response::sse::Event;
use futures::{Stream, TryStreamExt};
use serde::Serialize;
use serde_json::Value;

use crate::types::claude::{ContentBlock, ContentBlockDelta, CreateMessageResponse, StreamEvent};

/// Represents the data structure for streaming events in OpenAI API format
/// Contains a choices array with deltas of content
#[derive(Debug, Serialize)]
struct StreamEventData {
    choices: Vec<StreamEventDelta>,
}

impl StreamEventData {
    /// Creates a new StreamEventData with the given content
    ///
    /// # Arguments
    /// * `content` - The event content to include
    ///
    /// # Returns
    /// A new StreamEventData instance with the content wrapped in choices array
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

/// Builds an OpenAI-compatible SSE event carrying a streamed `tool_call` delta.
///
/// `id`/`name` are sent only on the opening chunk (from `content_block_start`);
/// subsequent chunks stream the JSON `arguments` fragment from `input_json_delta`.
fn build_tool_event(index: usize, id: Option<String>, name: Option<String>, arguments: &str) -> Event {
    let mut function = serde_json::json!({ "arguments": arguments });
    if let Some(name) = name {
        function["name"] = Value::String(name);
    }
    let mut tool_call = serde_json::json!({
        "index": index,
        "function": function,
    });
    if let Some(id) = id {
        tool_call["id"] = Value::String(id);
        tool_call["type"] = Value::String("function".to_string());
    }
    let data = serde_json::json!({
        "choices": [{
            "index": 0,
            "delta": { "tool_calls": [tool_call] }
        }]
    });
    Event::default().json_data(data).unwrap()
}

/// Builds the terminating OpenAI SSE chunk carrying `finish_reason`.
///
/// OpenAI clients rely on a final `finish_reason: "tool_calls"` chunk to know the
/// streamed tool call is complete and should be executed; Claude signals this via
/// `message_delta` with `stop_reason: "tool_use"`.
fn build_finish_event(reason: &str) -> Event {
    let data = serde_json::json!({
        "choices": [{
            "index": 0,
            "delta": {},
            "finish_reason": reason
        }]
    });
    Event::default().json_data(data).unwrap()
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
    // Maps a Claude content-block index to a sequential OpenAI tool_call index,
    // so parallel tool calls keep stable, gap-free indices in the OpenAI stream.
    let tool_index = std::sync::Arc::new(std::sync::Mutex::new(
        std::collections::HashMap::<usize, usize>::new(),
    ));
    s.try_filter_map(move |eventsource_stream::Event { data, .. }| {
        let tool_index = tool_index.clone();
        async move {
            let Ok(parsed) = serde_json::from_str::<StreamEvent>(&data) else {
                return Ok(None);
            };
            match parsed {
                StreamEvent::ContentBlockStart {
                    index,
                    content_block: ContentBlock::ToolUse { id, name, .. },
                } => {
                    let oai_index = {
                        let mut map = tool_index.lock().unwrap();
                        let next = map.len();
                        *map.entry(index).or_insert(next)
                    };
                    Ok(Some(build_tool_event(oai_index, Some(id), Some(name), "")))
                }
                StreamEvent::ContentBlockDelta { index, delta } => match delta {
                    ContentBlockDelta::TextDelta { text } => {
                        Ok(Some(build_event(EventContent::Content { content: text })))
                    }
                    ContentBlockDelta::ThinkingDelta { thinking } => {
                        Ok(Some(build_event(EventContent::Reasoning {
                            reasoning_content: thinking,
                        })))
                    }
                    ContentBlockDelta::InputJsonDelta { partial_json } => {
                        let oai_index = tool_index.lock().unwrap().get(&index).copied();
                        match oai_index {
                            Some(oai_index) => Ok(Some(build_tool_event(
                                oai_index,
                                None,
                                None,
                                &partial_json,
                            ))),
                            None => Ok(None),
                        }
                    }
                    _ => Ok(None),
                },
                StreamEvent::MessageDelta { delta, .. }
                    if matches!(
                        delta.stop_reason,
                        Some(crate::types::claude::StopReason::ToolUse)
                    ) =>
                {
                    Ok(Some(build_finish_event("tool_calls")))
                }
                _ => Ok(None),
            }
        }
    })
}

pub fn transforms_json(input: CreateMessageResponse) -> Value {
    let content = input
        .content
        .iter()
        .filter_map(|block| match block {
            crate::types::claude::ContentBlock::Text { text, .. } => Some(text.clone()),
            _ => None,
        })
        .collect::<String>();

    let tool_calls = input
        .content
        .iter()
        .filter_map(|block| match block {
            crate::types::claude::ContentBlock::ToolUse {
                id, name, input: args, ..
            } => Some(serde_json::json!({
                "id": id,
                "type": "function",
                "function": {
                    "name": name,
                    "arguments": serde_json::to_string(args).unwrap_or_else(|_| "{}".to_string()),
                }
            })),
            _ => None,
        })
        .collect::<Vec<_>>();

    let usage = input.usage.as_ref().map(|u| {
        serde_json::json!({
            "prompt_tokens": u.input_tokens,
            "completion_tokens": u.output_tokens,
            "total_tokens": u.input_tokens + u.output_tokens
        })
    });

    let finish_reason = match input.stop_reason {
        Some(crate::types::claude::StopReason::EndTurn) => "stop",
        Some(crate::types::claude::StopReason::MaxTokens) => "length",
        Some(crate::types::claude::StopReason::StopSequence) => "stop",
        Some(crate::types::claude::StopReason::ToolUse) => "tool_calls",
        Some(crate::types::claude::StopReason::PauseTurn) => "stop",
        Some(crate::types::claude::StopReason::Refusal) => "content_filter",
        Some(crate::types::claude::StopReason::ModelContextWindowExceeded) => "length",
        None => "stop",
    };

    let mut message = serde_json::json!({
        "role": "assistant",
        "content": content
    });
    if !tool_calls.is_empty() {
        message["tool_calls"] = Value::Array(tool_calls);
    }

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
            "message": message,
            "finish_reason": finish_reason
        }],
        "usage": usage
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::claude::CreateMessageResponse;

    #[test]
    fn tool_use_block_becomes_oai_tool_calls() {
        let json = r#"{
            "content": [
                {"type": "text", "text": "<content>scanning</content>"},
                {"type": "tool_use", "id": "toolu_1", "name": "read_file", "input": {"path": "a.txt"}}
            ],
            "id": "msg_1",
            "model": "claude-opus-4-8",
            "role": "assistant",
            "stop_reason": "tool_use",
            "stop_sequence": null,
            "type": "message",
            "usage": null
        }"#;
        let resp: CreateMessageResponse = serde_json::from_str(json).unwrap();
        let out = transforms_json(resp);

        let msg = &out["choices"][0]["message"];
        assert_eq!(out["choices"][0]["finish_reason"], "tool_calls");
        assert_eq!(msg["content"], "<content>scanning</content>");

        let tc = &msg["tool_calls"][0];
        assert_eq!(tc["id"], "toolu_1");
        assert_eq!(tc["type"], "function");
        assert_eq!(tc["function"]["name"], "read_file");
        // arguments must be a JSON *string*, not an object
        let args = tc["function"]["arguments"]
            .as_str()
            .expect("arguments must be a string");
        assert!(args.contains("a.txt"), "arguments missing payload: {args}");
    }

    #[test]
    fn plain_text_response_has_no_tool_calls_field() {
        let json = r#"{
            "content": [{"type": "text", "text": "hi"}],
            "id": "msg_2",
            "model": "claude-opus-4-8",
            "role": "assistant",
            "stop_reason": "end_turn",
            "stop_sequence": null,
            "type": "message",
            "usage": null
        }"#;
        let resp: CreateMessageResponse = serde_json::from_str(json).unwrap();
        let out = transforms_json(resp);

        assert_eq!(out["choices"][0]["finish_reason"], "stop");
        assert!(
            out["choices"][0]["message"].get("tool_calls").is_none(),
            "plain text response must not carry a tool_calls field"
        );
    }
}
