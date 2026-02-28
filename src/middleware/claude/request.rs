use std::{
    hash::{DefaultHasher, Hash, Hasher},
    mem,
    sync::LazyLock,
    vec,
};

use axum::{
    Json,
    extract::{FromRequest, Request},
};
use http::HeaderMap;
use serde_json::{Value, json};

use crate::{
    config::CLEWDR_CONFIG,
    error::ClewdrError,
    middleware::claude::{ClaudeApiFormat, ClaudeContext},
    types::{
        claude::{
            ContentBlock, CreateMessageParams, Message, MessageContent, Role, Thinking, Usage,
        },
        oai::CreateMessageParams as OaiCreateMessageParams,
    },
};

/// A custom extractor that unifies different API formats
///
/// This extractor processes incoming requests, handling differences between
/// Claude and OpenAI API formats, and applies preprocessing to ensure consistent
/// handling throughout the application. It also detects and handles test messages
/// from client applications.
///
/// # Functionality
///
/// - Extracts and normalizes message parameters from different API formats
/// - Detects and processes "thinking mode" requests by modifying model names
/// - Identifies test messages and handles them appropriately
/// - Attempts to retrieve responses from cache before processing requests
/// - Provides format information via the FormatInfo extension
pub struct ClaudeWebPreprocess(pub CreateMessageParams, pub ClaudeContext);

/// Contains information about the API format and streaming status
///
/// This structure is passed through the request pipeline to inform
/// handlers and response processors about the API format being used
/// and whether the response should be streamed.
#[derive(Debug, Clone)]
pub struct ClaudeWebContext {
    /// Whether the response should be streamed
    pub(super) stream: bool,
    /// The API format being used (Claude or OpenAI)
    pub(super) api_format: ClaudeApiFormat,
    /// The stop sequence used for the request
    pub(super) stop_sequences: Vec<String>,
    /// User information about input and output tokens
    pub(super) usage: Usage,
}

/// Predefined test message in Claude format for connection testing
///
/// This is a standard test message sent by clients like SillyTavern
/// to verify connectivity. The system detects these messages and
/// responds with a predefined test response to confirm service availability.
static TEST_MESSAGE_CLAUDE: LazyLock<Message> =
    LazyLock::new(|| Message::new_blocks(Role::User, vec![ContentBlock::text("Hi")]));

/// Predefined test message in OpenAI format for connection testing
static TEST_MESSAGE_OAI: LazyLock<Message> = LazyLock::new(|| Message::new_text(Role::User, "Hi"));

struct NormalizeRequest(CreateMessageParams, ClaudeApiFormat);

fn strip_ephemeral_scope_from_system(system: &mut Value) {
    let Some(items) = system.as_array_mut() else {
        return;
    };

    for item in items {
        let Some(obj) = item.as_object_mut() else {
            continue;
        };
        let Some(cache_control) = obj.get_mut("cache_control") else {
            continue;
        };
        let Some(cache_obj) = cache_control.as_object_mut() else {
            continue;
        };

        if let Some(ephemeral) = cache_obj.get_mut("ephemeral")
            && let Some(ephemeral_obj) = ephemeral.as_object_mut()
        {
            ephemeral_obj.remove("scope");
        }

        if matches!(cache_obj.get("type"), Some(Value::String(t)) if t == "ephemeral") {
            cache_obj.remove("scope");
        }
    }
}

fn extract_anthropic_beta_header(headers: &HeaderMap) -> Option<String> {
    let mut parts = Vec::new();
    for value in headers.get_all("anthropic-beta") {
        if let Ok(raw) = value.to_str() {
            for token in raw.split(',') {
                let token = token.trim();
                if !token.is_empty() {
                    parts.push(token.to_string());
                }
            }
        }
    }

    if parts.is_empty() {
        None
    } else {
        Some(parts.join(","))
    }
}

fn sanitize_messages(msgs: Vec<Message>) -> Vec<Message> {
    msgs.into_iter()
        .filter_map(|m| {
            let role = m.role;
            let content = match m.content {
                MessageContent::Text { content } => {
                    let trimmed = content.trim().to_string();
                    if role == Role::Assistant && trimmed.is_empty() {
                        return None;
                    }
                    MessageContent::Text { content: trimmed }
                }
                MessageContent::Blocks { content } => {
                    let mut new_blocks: Vec<ContentBlock> = content
                        .into_iter()
                        .filter_map(|b| match b {
                            ContentBlock::Text { text, .. } => {
                                let t = text.trim().to_string();
                                if t.is_empty() {
                                    None
                                } else {
                                    Some(ContentBlock::text(t))
                                }
                            }
                            other => Some(other),
                        })
                        .collect();
                    if role == Role::Assistant && new_blocks.is_empty() {
                        return None;
                    }
                    MessageContent::Blocks {
                        content: mem::take(&mut new_blocks),
                    }
                }
            };
            Some(Message { role, content })
        })
        .collect()
}

impl<S> FromRequest<S> for NormalizeRequest
where
    S: Send + Sync,
{
    type Rejection = ClewdrError;

    async fn from_request(req: Request, _: &S) -> Result<Self, Self::Rejection> {
        let uri = req.uri().to_string();
        let format = if uri.contains("chat/completions") {
            ClaudeApiFormat::OpenAI
        } else {
            ClaudeApiFormat::Claude
        };
        let Json(mut body) = match format {
            ClaudeApiFormat::OpenAI => {
                let Json(json) = Json::<OaiCreateMessageParams>::from_request(req, &()).await?;
                Json(json.into())
            }
            ClaudeApiFormat::Claude => Json::<CreateMessageParams>::from_request(req, &()).await?,
        };
        if CLEWDR_CONFIG.load().sanitize_messages {
            // Trim whitespace and drop empty assistant turns when enabled.
            body.messages = sanitize_messages(body.messages);
        }
        if body.model.ends_with("-thinking") {
            body.model = body.model.trim_end_matches("-thinking").to_string();
            body.thinking.get_or_insert(Thinking::new(4096));
        }
        Ok(Self(body, format))
    }
}

impl<S> FromRequest<S> for ClaudeWebPreprocess
where
    S: Send + Sync,
{
    type Rejection = ClewdrError;

    async fn from_request(req: Request, _: &S) -> Result<Self, Self::Rejection> {
        let NormalizeRequest(body, format) = NormalizeRequest::from_request(req, &()).await?;

        // Check for test messages and respond appropriately
        if !body.stream.unwrap_or_default()
            && (body.messages == vec![TEST_MESSAGE_CLAUDE.to_owned()]
                || body.messages == vec![TEST_MESSAGE_OAI.to_owned()])
        {
            // Respond with a test message
            return Err(ClewdrError::TestMessage);
        }

        // Determine streaming status and API format
        let stream = body.stream.unwrap_or_default();

        let input_tokens = body.count_tokens();
        let info = ClaudeWebContext {
            stream,
            api_format: format,
            stop_sequences: body.stop_sequences.to_owned().unwrap_or_default(),
            usage: Usage {
                input_tokens,
                output_tokens: 0, // Placeholder for output token count
            },
        };

        Ok(Self(body, ClaudeContext::Web(info)))
    }
}

#[derive(Debug, Clone)]
pub struct ClaudeCodeContext {
    /// Whether the response should be streamed
    pub(super) stream: bool,
    /// The API format being used (Claude or OpenAI)
    pub(super) api_format: ClaudeApiFormat,
    /// The hash of the system messages for caching purposes
    pub(super) system_prompt_hash: Option<u64>,
    /// Optional anthropic-beta header forwarded from client request
    pub(super) anthropic_beta: Option<String>,
    // Usage information for the request
    pub(super) usage: Usage,
}

pub struct ClaudeCodePreprocess(pub CreateMessageParams, pub ClaudeContext);

impl<S> FromRequest<S> for ClaudeCodePreprocess
where
    S: Send + Sync,
{
    type Rejection = ClewdrError;

    async fn from_request(req: Request, _: &S) -> Result<Self, Self::Rejection> {
        let anthropic_beta = extract_anthropic_beta_header(req.headers());
        let NormalizeRequest(mut body, format) = NormalizeRequest::from_request(req, &()).await?;
        // Handle thinking mode by modifying the model name
        if  body.temperature.is_some()
        {
            body.top_p = None; // temperature and top_p cannot be used together in Opus-4.x
        }

        // Check for test messages and respond appropriately
        if !body.stream.unwrap_or_default()
            && (body.messages == vec![TEST_MESSAGE_CLAUDE.to_owned()]
                || body.messages == vec![TEST_MESSAGE_OAI.to_owned()])
        {
            // Respond with a test message
            return Err(ClewdrError::TestMessage);
        }

        // Determine streaming status and API format
        let stream = body.stream.unwrap_or_default();

        if let Some(custom_system) = CLEWDR_CONFIG
            .load()
            .custom_system
            .clone()
            .filter(|s| !s.trim().is_empty())
        {
            let custom_system_block = ContentBlock::text(custom_system);
            match body.system {
                Some(Value::String(ref text)) => {
                    let text_content = ContentBlock::text(text.to_owned());
                    body.system = Some(json!([custom_system_block, text_content]));
                }
                Some(Value::Array(ref mut systems)) => {
                    systems.insert(0, json!(custom_system_block));
                }
                _ => {
                    body.system = Some(json!([custom_system_block]));
                }
            }
        }

        if let Some(system) = body.system.as_mut() {
            strip_ephemeral_scope_from_system(system);
        }

        let cache_systems = body
            .system
            .as_ref()
            .ok_or(ClewdrError::BadRequest {
                msg: "Empty system prompt",
            })?
            .as_array()
            .ok_or(ClewdrError::BadRequest {
                msg: "System prompt is not an array",
            })?
            .iter()
            .filter(|s| s["cache_control"].as_object().is_some())
            .collect::<Vec<_>>();
        let system_prompt_hash = (!cache_systems.is_empty()).then(|| {
            let mut hasher = DefaultHasher::new();
            cache_systems.hash(&mut hasher);
            hasher.finish()
        });

        let input_tokens = body.count_tokens();

        let info = ClaudeCodeContext {
            stream,
            api_format: format,
            system_prompt_hash,
            anthropic_beta,
            usage: Usage {
                input_tokens,
                output_tokens: 0, // Placeholder for output token count
            },
        };

        Ok(Self(body, ClaudeContext::Code(info)))
    }
}
