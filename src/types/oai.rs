use anthropic_wire::{
    ContentBlock, CreateMessageParams as ClaudeCreateMessageParams, ImageSource, Message,
    MessageContent, Metadata, OutputConfig, OutputEffort, Role, Thinking, Tool, ToolChoice,
    default_max_tokens,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tiktoken_rs::o200k_base;

/// Convert OAI `ImageUrl` to Claude Image format
fn normalize_block(block: ContentBlock) -> Option<ContentBlock> {
    match block {
        ContentBlock::ImageUrl { image_url } => {
            ImageSource::from_image_url(&image_url.url).map(|source| ContentBlock::Image {
                source,
                cache_control: None,
            })
        }
        // Text, Image and everything else pass through untouched.
        _ => Some(block),
    }
}

/// Normalize all blocks in a message content
/// Returns None if the message becomes empty after filtering
fn normalize_message(msg: Message) -> Option<Message> {
    let content = match msg.content {
        MessageContent::Blocks { content } => {
            let blocks: Vec<_> = content.into_iter().filter_map(normalize_block).collect();
            // skip empty messages
            if blocks.is_empty() {
                return None;
            }
            MessageContent::Blocks { content: blocks }
        }
        other @ MessageContent::Text { .. } => other,
    };
    Some(Message {
        role: msg.role,
        content,
    })
}

/// OpenAI-style `reasoning_effort`.
///
/// Claude expresses the same idea two different ways depending on the model:
/// `output_config.effort` on Opus 4.5 and later, and a legacy thinking budget
/// on everything else. Both are derived here and [`crate::types::model`] drops
/// whichever one the target model does not accept.
#[derive(Debug, Serialize, Deserialize, Default, Clone, Copy)]
#[serde(rename_all = "snake_case")]
pub enum Effort {
    /// OpenAI's `minimal`/`none`; Claude has no rung below `low`.
    #[serde(alias = "none")]
    Minimal,
    Low,
    #[default]
    Medium,
    High,
    #[serde(rename = "xhigh")]
    XHigh,
    Max,
}

impl Effort {
    /// The matching rung on Claude's effort ladder.
    #[must_use]
    pub fn output_effort(self) -> OutputEffort {
        match self {
            Self::Minimal | Self::Low => OutputEffort::Low,
            Self::Medium => OutputEffort::Medium,
            Self::High => OutputEffort::High,
            Self::XHigh => OutputEffort::XHigh,
            Self::Max => OutputEffort::Max,
        }
    }

    /// Equivalent budget for models that only understand extended thinking.
    ///
    /// Never below the API floor of 1024 tokens.
    #[must_use]
    pub fn budget_tokens(self) -> u64 {
        match self {
            Self::Minimal => 1024,
            Self::Low => 2048,
            Self::Medium => 4096,
            Self::High => 16384,
            Self::XHigh => 32768,
            Self::Max => 65536,
        }
    }
}

impl From<CreateMessageParams> for ClaudeCreateMessageParams {
    fn from(params: CreateMessageParams) -> Self {
        let (systems, messages): (Vec<Message>, Vec<Message>) = params
            .messages
            .into_iter()
            .partition(|m| m.role == Role::System);
        let systems = systems
            .into_iter()
            .map(|m| m.content)
            .flat_map(|c| match c {
                MessageContent::Text { content } => vec![ContentBlock::text(content)],
                MessageContent::Blocks { content } => content,
            })
            .filter(|b| matches!(b, ContentBlock::Text { .. }))
            .map(|b| json!(b))
            .collect::<Vec<_>>();
        let system = (!systems.is_empty()).then(|| json!(systems));
        // normalize messages (convert ImageUrl to Image, skip empty messages)
        let messages = messages.into_iter().filter_map(normalize_message).collect();
        Self {
            max_tokens: (params.max_tokens.or(params.max_completion_tokens))
                .unwrap_or_else(default_max_tokens),
            system,
            messages,
            model: params.model,
            container: None,
            context_management: None,
            mcp_servers: None,
            stop_sequences: params.stop,
            thinking: params.thinking.or_else(|| {
                params
                    .reasoning_effort
                    .map(|e| Thinking::new(e.budget_tokens()))
            }),
            temperature: params.temperature,
            stream: params.stream,
            top_k: params.top_k,
            top_p: params.top_p,
            tools: params.tools,
            tool_choice: params.tool_choice,
            metadata: params.metadata,
            output_config: params.reasoning_effort.map(|e| OutputConfig {
                effort: Some(e.output_effort()),
                format: None,
            }),
            output_format: None,
            service_tier: None,
            n: params.n,
            // Anthropic-only, with nothing on the OpenAI side to map from. The
            // literal is left exhaustive on purpose: a new field upstream
            // should fail this build so the mapping gets considered, rather
            // than defaulting to None unnoticed.
            cache_control: None,
            inference_geo: None,
            speed: None,
            diagnostics: None,
            fallbacks: None,
            fallback_credit_token: None,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Default, Clone)]
pub struct CreateMessageParams {
    /// Maximum number of tokens to generate
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_completion_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
    /// Input messages for the conversation
    pub messages: Vec<Message>,
    /// Model to use
    pub model: String,
    /// Reasoning effort for response generation
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_effort: Option<Effort>,
    /// Frequency penalty for response generation
    #[serde(skip_serializing_if = "Option::is_none")]
    pub frequency_penalty: Option<f32>,
    /// Temperature for response generation
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    /// Custom stop sequences
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop: Option<Vec<String>>,
    /// Whether to stream the response
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream: Option<bool>,
    /// Thinking mode configuration
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thinking: Option<Thinking>,
    /// Top-k sampling
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_k: Option<u32>,
    /// Top-p sampling
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f32>,
    /// Logit bias for token generation
    #[serde(skip_serializing_if = "Option::is_none")]
    pub logit_bias: Option<Value>,
    /// Tools that the model may use
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<Tool>>,
    /// How the model should use tools
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<ToolChoice>,
    /// Request metadata
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Metadata>,
    /// Number of completions to generate
    #[serde(skip_serializing_if = "Option::is_none")]
    pub n: Option<u32>,
}

impl CreateMessageParams {
    /// Estimate the prompt's token count with the `o200k_base` encoding.
    ///
    /// This is an approximation: Anthropic does not publish its tokenizer, so
    /// the count only informs local accounting.
    ///
    /// # Panics
    /// If the bundled `o200k_base` encoding fails to load, which would mean
    /// the tiktoken data compiled into the binary is corrupt.
    #[must_use]
    pub fn count_tokens(&self) -> u32 {
        let bpe = o200k_base().expect("Failed to get encoding");
        let messages = self
            .messages
            .iter()
            .map(|msg| match msg.content {
                MessageContent::Text { ref content } => content.clone(),
                MessageContent::Blocks { ref content } => content
                    .iter()
                    .map(|block| match block {
                        ContentBlock::Text { text, .. } => text,
                        _ => "",
                    })
                    .collect::<String>(),
            })
            .collect::<Vec<_>>()
            .join("\n");
        u32::try_from(bpe.encode_with_special_tokens(&messages).len()).unwrap_or(u32::MAX)
    }
}
