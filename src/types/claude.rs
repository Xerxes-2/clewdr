use serde::de;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use serde_with::{DefaultOnError, serde_as};
use tiktoken_rs::o200k_base;

#[derive(Debug)]
pub struct RequiredMessageParams {
    pub model: String,
    pub messages: Vec<Message>,
    pub max_tokens: u32,
}

pub(super) fn default_max_tokens() -> u32 {
    8192
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct OutputConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub effort: Option<OutputEffort>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub format: Option<OutputFormat>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(rename_all = "lowercase")]
pub enum OutputEffort {
    Low,
    Medium,
    High,
    Max,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(tag = "type")]
pub enum OutputFormat {
    #[serde(rename = "json_schema")]
    JsonSchema { schema: serde_json::Value },
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(rename_all = "snake_case")]
pub enum ServiceTier {
    Auto,
    StandardOnly,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct McpServer {
    pub name: String,
    #[serde(rename = "type")]
    pub type_: String,
    pub url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub authorization_token: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_configuration: Option<serde_json::Value>,
}
/// Parameters for creating a message
#[serde_as]
#[derive(Debug, Deserialize, Serialize, Default, Clone)]
pub struct CreateMessageParams {
    /// Maximum number of tokens to generate
    #[serde(default = "default_max_tokens")]
    pub max_tokens: u32,
    /// Input messages for the conversation
    pub messages: Vec<Message>,
    /// Model to use
    pub model: String,
    /// Container identifier or definition
    #[serde(skip_serializing_if = "Option::is_none")]
    pub container: Option<serde_json::Value>,
    /// Context management configuration
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context_management: Option<serde_json::Value>,
    /// MCP servers to be utilized in this request
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mcp_servers: Option<Vec<McpServer>>,
    /// System prompt
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system: Option<serde_json::Value>,
    /// Temperature for response generation
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    /// Custom stop sequences
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_sequences: Option<Vec<String>>,
    /// Whether to stream the response
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream: Option<bool>,
    /// Thinking mode configuration
    #[serde(default)]
    #[serde_as(deserialize_as = "DefaultOnError")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thinking: Option<Thinking>,
    /// Top-k sampling
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_k: Option<u32>,
    /// Top-p sampling
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f32>,
    /// Tools that the model may use
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<Tool>>,
    /// How the model should use tools
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<ToolChoice>,
    /// Request metadata
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Metadata>,
    /// Output configuration (effort hints)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_config: Option<OutputConfig>,
    /// Output format configuration (e.g. JSON schema)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_format: Option<OutputFormat>,
    /// Service tier selection
    #[serde(skip_serializing_if = "Option::is_none")]
    pub service_tier: Option<ServiceTier>,
    /// Number of completions to generate
    #[serde(skip_serializing_if = "Option::is_none")]
    pub n: Option<u32>,
}

impl CreateMessageParams {
    pub fn count_tokens(&self) -> u32 {
        let bpe = o200k_base().expect("Failed to get encoding");
        let systems = match self.system {
            Some(Value::String(ref s)) => s.to_string(),
            Some(Value::Array(ref arr)) => arr.iter().filter_map(|v| v["text"].as_str()).collect(),
            _ => String::new(),
        };
        let messages = self
            .messages
            .iter()
            .map(|msg| match msg.content {
                MessageContent::Text { ref content } => content.to_string(),
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
        bpe.encode_with_special_tokens(&systems).len() as u32
            + bpe.encode_with_special_tokens(&messages).len() as u32
    }
}

/// Thinking mode in Claude API Request
#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Thinking {
    Enabled { budget_tokens: u64 },
    Disabled,
}

impl Thinking {
    pub fn new(budget_tokens: u64) -> Self {
        Self::Enabled { budget_tokens }
    }
}

impl From<RequiredMessageParams> for CreateMessageParams {
    fn from(required: RequiredMessageParams) -> Self {
        Self {
            model: required.model,
            messages: required.messages,
            max_tokens: required.max_tokens,
            ..Default::default()
        }
    }
}

impl CreateMessageParams {
    /// Create new parameters with only required fields
    pub fn new(required: RequiredMessageParams) -> Self {
        required.into()
    }

    // Builder methods for optional parameters
    pub fn with_system(mut self, system: impl Into<String>) -> Self {
        self.system = Some(serde_json::json!(system.into()));
        self
    }

    pub fn with_temperature(mut self, temperature: f32) -> Self {
        self.temperature = Some(temperature);
        self
    }

    pub fn with_stop_sequences(mut self, stop_sequences: Vec<String>) -> Self {
        self.stop_sequences = Some(stop_sequences);
        self
    }

    pub fn with_stream(mut self, stream: bool) -> Self {
        self.stream = Some(stream);
        self
    }

    pub fn with_top_k(mut self, top_k: u32) -> Self {
        self.top_k = Some(top_k);
        self
    }

    pub fn with_top_p(mut self, top_p: f32) -> Self {
        self.top_p = Some(top_p);
        self
    }

    pub fn with_tools(mut self, tools: Vec<Tool>) -> Self {
        self.tools = Some(tools);
        self
    }

    pub fn with_tool_choice(mut self, tool_choice: ToolChoice) -> Self {
        self.tool_choice = Some(tool_choice);
        self
    }

    pub fn with_metadata(mut self, metadata: Metadata) -> Self {
        self.metadata = Some(metadata);
        self
    }
}

/// Message in a conversation
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Hash)]
pub struct Message {
    /// Role of the message sender
    pub role: Role,
    /// Content of the message (either string or array of content blocks)
    #[serde(flatten)]
    pub content: MessageContent,
}

/// Role of a message sender
#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Default, Hash)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    System,
    User,
    #[default]
    Assistant,
}

/// Content of a message
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Hash)]
#[serde(untagged)]
pub enum MessageContent {
    /// Simple text content
    Text { content: String },
    /// Structured content blocks
    Blocks { content: Vec<ContentBlock> },
}

/// Content block in a message
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Hash)]
#[serde(tag = "type")]
pub enum ContentBlock {
    /// Text content
    #[serde(rename = "text")]
    Text {
        text: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        cache_control: Option<CacheControlEphemeral>,
        #[serde(skip_serializing_if = "Option::is_none")]
        citations: Option<Vec<Citation>>,
    },
    /// Image content
    #[serde(rename = "image")]
    Image {
        source: ImageSource,
        #[serde(skip_serializing_if = "Option::is_none")]
        cache_control: Option<CacheControlEphemeral>,
    },
    #[serde(rename = "image_url")]
    ImageUrl { image_url: ImageUrl },
    /// Document content
    #[serde(rename = "document")]
    Document {
        source: DocumentSource,
        #[serde(skip_serializing_if = "Option::is_none")]
        cache_control: Option<CacheControlEphemeral>,
        #[serde(skip_serializing_if = "Option::is_none")]
        citations: Option<CitationsConfig>,
        #[serde(skip_serializing_if = "Option::is_none")]
        context: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        title: Option<String>,
    },
    /// Search result content
    #[serde(rename = "search_result")]
    SearchResult {
        content: Vec<ContentBlock>,
        source: String,
        title: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        cache_control: Option<CacheControlEphemeral>,
        #[serde(skip_serializing_if = "Option::is_none")]
        citations: Option<CitationsConfig>,
    },
    /// Thinking content
    #[serde(rename = "thinking")]
    Thinking { signature: String, thinking: String },
    /// Redacted thinking content
    #[serde(rename = "redacted_thinking")]
    RedactedThinking { data: String },
    /// Tool use content
    #[serde(rename = "tool_use")]
    ToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
        #[serde(skip_serializing_if = "Option::is_none")]
        cache_control: Option<CacheControlEphemeral>,
        #[serde(skip_serializing_if = "Option::is_none")]
        caller: Option<ToolCaller>,
    },
    /// Tool result content
    #[serde(rename = "tool_result")]
    ToolResult {
        tool_use_id: String,
        content: serde_json::Value,
        #[serde(skip_serializing_if = "Option::is_none")]
        cache_control: Option<CacheControlEphemeral>,
        #[serde(skip_serializing_if = "Option::is_none")]
        is_error: Option<bool>,
    },
    /// Tool reference content
    #[serde(rename = "tool_reference")]
    ToolReference {
        tool_name: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        cache_control: Option<CacheControlEphemeral>,
    },
    /// Server tool use content
    #[serde(rename = "server_tool_use")]
    ServerToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
        #[serde(skip_serializing_if = "Option::is_none")]
        cache_control: Option<CacheControlEphemeral>,
        #[serde(skip_serializing_if = "Option::is_none")]
        caller: Option<ToolCaller>,
    },
    /// Web search tool result content
    #[serde(rename = "web_search_tool_result")]
    WebSearchToolResult {
        tool_use_id: String,
        content: serde_json::Value,
        #[serde(skip_serializing_if = "Option::is_none")]
        cache_control: Option<CacheControlEphemeral>,
    },
    /// Web fetch tool result content
    #[serde(rename = "web_fetch_tool_result")]
    WebFetchToolResult {
        tool_use_id: String,
        content: serde_json::Value,
        #[serde(skip_serializing_if = "Option::is_none")]
        cache_control: Option<CacheControlEphemeral>,
    },
    /// Code execution tool result content
    #[serde(rename = "code_execution_tool_result")]
    CodeExecutionToolResult {
        tool_use_id: String,
        content: serde_json::Value,
        #[serde(skip_serializing_if = "Option::is_none")]
        cache_control: Option<CacheControlEphemeral>,
    },
    /// Bash code execution tool result content
    #[serde(rename = "bash_code_execution_tool_result")]
    BashCodeExecutionToolResult {
        tool_use_id: String,
        content: serde_json::Value,
        #[serde(skip_serializing_if = "Option::is_none")]
        cache_control: Option<CacheControlEphemeral>,
    },
    /// Text editor tool result content
    #[serde(rename = "text_editor_code_execution_tool_result")]
    TextEditorCodeExecutionToolResult {
        tool_use_id: String,
        content: serde_json::Value,
        #[serde(skip_serializing_if = "Option::is_none")]
        cache_control: Option<CacheControlEphemeral>,
    },
    /// Tool search tool result content
    #[serde(rename = "tool_search_tool_result")]
    ToolSearchToolResult {
        tool_use_id: String,
        content: serde_json::Value,
        #[serde(skip_serializing_if = "Option::is_none")]
        cache_control: Option<CacheControlEphemeral>,
    },
    /// MCP tool use content
    #[serde(rename = "mcp_tool_use")]
    McpToolUse {
        id: String,
        name: String,
        server_name: String,
        input: serde_json::Value,
        #[serde(skip_serializing_if = "Option::is_none")]
        cache_control: Option<CacheControlEphemeral>,
    },
    /// MCP tool result content
    #[serde(rename = "mcp_tool_result")]
    McpToolResult {
        tool_use_id: String,
        content: serde_json::Value,
        #[serde(skip_serializing_if = "Option::is_none")]
        cache_control: Option<CacheControlEphemeral>,
        #[serde(skip_serializing_if = "Option::is_none")]
        is_error: Option<bool>,
    },
    /// Container upload content
    #[serde(rename = "container_upload")]
    ContainerUpload {
        file_id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        cache_control: Option<CacheControlEphemeral>,
    },
}

/// Source of an image
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Hash)]
#[serde(tag = "type")]
pub enum ImageSource {
    /// Base64-encoded image data
    #[serde(rename = "base64")]
    Base64 { media_type: String, data: String },
    /// Remote image URL
    #[serde(rename = "url")]
    Url { url: String },
    /// Uploaded file reference
    #[serde(rename = "file")]
    File { file_id: String },
}

// oai image
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Hash)]
pub struct ImageUrl {
    pub url: String,
}

/// Cache control breakpoint configuration.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Hash)]
pub struct CacheControlEphemeral {
    #[serde(rename = "type")]
    pub type_: CacheControlType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ttl: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CacheControlType {
    #[serde(rename = "ephemeral")]
    Ephemeral,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Hash)]
pub struct CitationsConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
}

pub type Citation = serde_json::Value;
pub type ToolCaller = serde_json::Value;
pub type DocumentSource = serde_json::Value;

/// Tool definition
///
/// Claude `tools` is a union type: it can include custom tools (which have an
/// `input_schema`) and built-in tools (e.g. Claude Code tools) that do not.
///
/// This models the documented tool variants and preserves unknown tool shapes
/// for pass-through.
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(untagged)]
pub enum Tool {
    Custom(CustomTool),
    Known(KnownTool),
    Raw(serde_json::Value),
}

/// Custom tool definition (requires `input_schema`)
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CustomTool {
    /// Name of the tool
    pub name: String,
    /// Description of the tool
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// JSON schema for tool input
    pub input_schema: serde_json::Value,
    /// Allowed callers for the tool
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_callers: Option<Vec<String>>,
    /// Optional cache control breakpoint for this tool definition
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<CacheControlEphemeral>,
    /// Whether to defer loading this tool
    #[serde(skip_serializing_if = "Option::is_none")]
    pub defer_loading: Option<bool>,
    /// Input examples for the tool
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_examples: Option<Vec<serde_json::Value>>,
    /// Strict mode flag
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strict: Option<bool>,
    /// Optional tool type marker
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub type_: Option<CustomToolType>,
    /// Preserve additional tool fields
    #[serde(default, flatten)]
    pub extra: std::collections::HashMap<String, serde_json::Value>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CustomToolType {
    #[serde(rename = "custom")]
    Custom,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(tag = "type")]
pub enum KnownTool {
    #[serde(rename = "bash_20250124")]
    Bash20250124 {
        name: ToolNameBash,
        #[serde(skip_serializing_if = "Option::is_none")]
        cache_control: Option<CacheControlEphemeral>,
        #[serde(flatten)]
        extra: std::collections::HashMap<String, serde_json::Value>,
    },
    #[serde(rename = "text_editor_20250124")]
    TextEditor20250124 {
        name: ToolNameStrReplaceEditor,
        #[serde(skip_serializing_if = "Option::is_none")]
        cache_control: Option<CacheControlEphemeral>,
        #[serde(flatten)]
        extra: std::collections::HashMap<String, serde_json::Value>,
    },
    #[serde(rename = "text_editor_20250429")]
    TextEditor20250429 {
        name: ToolNameStrReplaceBasedEditTool,
        #[serde(skip_serializing_if = "Option::is_none")]
        cache_control: Option<CacheControlEphemeral>,
        #[serde(flatten)]
        extra: std::collections::HashMap<String, serde_json::Value>,
    },
    #[serde(rename = "text_editor_20250728")]
    TextEditor20250728 {
        name: ToolNameStrReplaceBasedEditTool,
        #[serde(skip_serializing_if = "Option::is_none")]
        cache_control: Option<CacheControlEphemeral>,
        #[serde(skip_serializing_if = "Option::is_none")]
        max_characters: Option<u32>,
        #[serde(flatten)]
        extra: std::collections::HashMap<String, serde_json::Value>,
    },
    #[serde(rename = "web_search_20250305")]
    WebSearch20250305 {
        name: ToolNameWebSearch,
        #[serde(skip_serializing_if = "Option::is_none")]
        allowed_domains: Option<Vec<String>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        blocked_domains: Option<Vec<String>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        cache_control: Option<CacheControlEphemeral>,
        #[serde(skip_serializing_if = "Option::is_none")]
        max_uses: Option<u32>,
        #[serde(skip_serializing_if = "Option::is_none")]
        user_location: Option<WebSearchUserLocation>,
        #[serde(flatten)]
        extra: std::collections::HashMap<String, serde_json::Value>,
    },
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ToolNameBash {
    #[serde(rename = "bash")]
    Bash,
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ToolNameStrReplaceEditor {
    #[serde(rename = "str_replace_editor")]
    StrReplaceEditor,
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ToolNameStrReplaceBasedEditTool {
    #[serde(rename = "str_replace_based_edit_tool")]
    StrReplaceBasedEditTool,
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ToolNameWebSearch {
    #[serde(rename = "web_search")]
    WebSearch,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct WebSearchUserLocation {
    #[serde(rename = "type")]
    pub type_: WebSearchUserLocationType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub city: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub country: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub region: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timezone: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WebSearchUserLocationType {
    #[serde(rename = "approximate")]
    Approximate,
}

/// Tool choice configuration
#[derive(Debug, Serialize, Clone)]
#[serde(tag = "type")]
pub enum ToolChoice {
    /// Let model choose whether to use tools
    #[serde(rename = "auto")]
    Auto {
        #[serde(skip_serializing_if = "Option::is_none")]
        disable_parallel_tool_use: Option<bool>,
    },
    /// Model must use one of the provided tools
    #[serde(rename = "any")]
    Any {
        #[serde(skip_serializing_if = "Option::is_none")]
        disable_parallel_tool_use: Option<bool>,
    },
    /// Model must use a specific tool
    #[serde(rename = "tool")]
    Tool {
        name: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        disable_parallel_tool_use: Option<bool>,
    },
    /// Model will not be allowed to use tools
    #[serde(rename = "none")]
    None,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
enum ToolChoiceTagged {
    #[serde(rename = "auto")]
    Auto {
        #[serde(skip_serializing_if = "Option::is_none")]
        disable_parallel_tool_use: Option<bool>,
    },
    #[serde(rename = "any")]
    Any {
        #[serde(skip_serializing_if = "Option::is_none")]
        disable_parallel_tool_use: Option<bool>,
    },
    #[serde(rename = "tool")]
    Tool {
        name: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        disable_parallel_tool_use: Option<bool>,
    },
    #[serde(rename = "none")]
    None,
}

impl From<ToolChoiceTagged> for ToolChoice {
    fn from(value: ToolChoiceTagged) -> Self {
        match value {
            ToolChoiceTagged::Auto {
                disable_parallel_tool_use,
            } => ToolChoice::Auto {
                disable_parallel_tool_use,
            },
            ToolChoiceTagged::Any {
                disable_parallel_tool_use,
            } => ToolChoice::Any {
                disable_parallel_tool_use,
            },
            ToolChoiceTagged::Tool {
                name,
                disable_parallel_tool_use,
            } => ToolChoice::Tool {
                name,
                disable_parallel_tool_use,
            },
            ToolChoiceTagged::None => ToolChoice::None,
        }
    }
}

impl<'de> Deserialize<'de> for ToolChoice {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = serde_json::Value::deserialize(deserializer)?;
        match value {
            serde_json::Value::String(choice) => match choice.as_str() {
                "auto" => Ok(ToolChoice::Auto {
                    disable_parallel_tool_use: None,
                }),
                "any" | "required" => Ok(ToolChoice::Any {
                    disable_parallel_tool_use: None,
                }),
                "none" => Ok(ToolChoice::None),
                _ => Err(de::Error::custom(format!(
                    "unsupported tool_choice string: {choice}"
                ))),
            },
            serde_json::Value::Object(_) => {
                let tagged: ToolChoiceTagged =
                    serde_json::from_value(value).map_err(de::Error::custom)?;
                Ok(tagged.into())
            }
            _ => Err(de::Error::custom(
                "tool_choice must be a string or an object",
            )),
        }
    }
}

/// Message metadata
#[derive(Debug, Serialize, Deserialize, Default, Clone)]
pub struct Metadata {
    /// Custom metadata fields
    #[serde(flatten)]
    pub fields: std::collections::HashMap<String, String>,
}

/// Response from creating a message
#[derive(Debug, Deserialize, Serialize)]
pub struct CreateMessageResponse {
    /// Content blocks in the response
    pub content: Vec<ContentBlock>,
    /// Unique message identifier
    pub id: String,
    /// Model that handled the request
    pub model: String,
    /// Role of the message (always "assistant")
    pub role: Role,
    /// Reason for stopping generation
    pub stop_reason: Option<StopReason>,
    /// Stop sequence that was generated
    pub stop_sequence: Option<String>,
    /// Type of the message
    #[serde(rename = "type")]
    pub type_: String,
    /// Usage statistics
    pub usage: Option<Usage>,
}

impl CreateMessageResponse {
    pub fn count_tokens(&self) -> u32 {
        let bpe = o200k_base().expect("Failed to get encoding");
        let content = self
            .content
            .iter()
            .map(|block| match block {
                ContentBlock::Text { text, .. } => text.as_str(),
                ContentBlock::Image {
                    source: ImageSource::Base64 { data, .. },
                    ..
                } => data.as_str(),
                ContentBlock::Image { .. } => "",
                _ => "",
            })
            .collect::<Vec<_>>()
            .join("\n");
        bpe.encode_with_special_tokens(&content).len() as u32
    }
}

impl CreateMessageResponse {
    /// Create a new response with the given content blocks
    pub fn text(content: String, model: String, usage: Usage) -> Self {
        Self {
            content: vec![ContentBlock::text(content)],
            id: uuid::Uuid::new_v4().to_string(),
            model,
            role: Role::Assistant,
            stop_reason: None,
            stop_sequence: None,
            type_: "message".into(),
            usage: Some(usage),
        }
    }
}

/// Reason for stopping message generation
#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StopReason {
    EndTurn,
    MaxTokens,
    StopSequence,
    ToolUse,
    PauseTurn,
    Refusal,
    ModelContextWindowExceeded,
}

/// Token usage statistics
#[derive(Debug, Deserialize, Serialize, Default, Clone)]
pub struct Usage {
    /// Input tokens used
    pub input_tokens: u32,
    /// Output tokens used
    pub output_tokens: u32,
}

#[derive(Debug, Deserialize, Serialize, Default)]
pub struct StreamUsage {
    /// Input tokens used (may be missing in some events)
    #[serde(default)]
    pub input_tokens: u32,
    /// Output tokens used
    pub output_tokens: u32,
}

impl Message {
    /// Create a new message with simple text content
    pub fn new_text(role: Role, text: impl Into<String>) -> Self {
        Self {
            role,
            content: MessageContent::Text {
                content: text.into(),
            },
        }
    }

    /// Create a new message with content blocks
    pub fn new_blocks(role: Role, blocks: Vec<ContentBlock>) -> Self {
        Self {
            role,
            content: MessageContent::Blocks { content: blocks },
        }
    }
}

// Helper methods for content blocks
impl ContentBlock {
    /// Create a new text block
    pub fn text(text: impl Into<String>) -> Self {
        Self::Text {
            text: text.into(),
            cache_control: None,
            citations: None,
        }
    }

    /// Create a new image block
    pub fn image(
        type_: impl Into<String>,
        media_type: impl Into<String>,
        data: impl Into<String>,
    ) -> Self {
        let type_ = type_.into();
        let source = match type_.as_str() {
            "url" => ImageSource::Url { url: data.into() },
            "file" => ImageSource::File {
                file_id: data.into(),
            },
            _ => ImageSource::Base64 {
                media_type: media_type.into(),
                data: data.into(),
            },
        };
        Self::Image {
            source,
            cache_control: None,
        }
    }
}

#[derive(Debug, Serialize, Default)]
pub struct CountMessageTokensParams {
    pub model: String,
    pub messages: Vec<Message>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct CountMessageTokensResponse {
    pub input_tokens: u32,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(tag = "type")]
pub enum StreamEvent {
    #[serde(rename = "message_start")]
    MessageStart { message: MessageStartContent },
    #[serde(rename = "content_block_start")]
    ContentBlockStart {
        index: usize,
        content_block: ContentBlock,
    },
    #[serde(rename = "content_block_delta")]
    ContentBlockDelta {
        index: usize,
        delta: ContentBlockDelta,
    },
    #[serde(rename = "content_block_stop")]
    ContentBlockStop { index: usize },
    #[serde(rename = "message_delta")]
    MessageDelta {
        delta: MessageDeltaContent,
        usage: Option<StreamUsage>,
    },
    #[serde(rename = "message_stop")]
    MessageStop,
    #[serde(rename = "ping")]
    Ping,
    #[serde(rename = "error")]
    Error { error: StreamError },
}

#[derive(Debug, Deserialize, Serialize, Default)]
pub struct MessageStartContent {
    pub id: String,
    #[serde(rename = "type")]
    pub type_: String,
    pub role: Role,
    pub content: Vec<ContentBlock>,
    pub model: String,
    pub stop_reason: Option<StopReason>,
    pub stop_sequence: Option<String>,
    pub usage: Option<Usage>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(tag = "type")]
pub enum ContentBlockDelta {
    #[serde(rename = "text_delta")]
    TextDelta { text: String },
    #[serde(rename = "input_json_delta")]
    InputJsonDelta { partial_json: String },
    #[serde(rename = "thinking_delta")]
    ThinkingDelta { thinking: String },
    #[serde(rename = "signature_delta")]
    SignatureDelta { signature: String },
}

#[derive(Debug, Deserialize, Serialize, Default)]
pub struct MessageDeltaContent {
    pub stop_reason: Option<StopReason>,
    pub stop_sequence: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct StreamError {
    #[serde(rename = "type")]
    pub type_: String,
    pub message: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn deserializes_claude_code_builtin_tools_without_input_schema() {
        let body = json!({
            "max_tokens": 1024,
            "messages": [
                { "role": "user", "content": "hi" }
            ],
            "model": "claude-sonnet-4-5-20250929",
            "tools": [
                { "name": "bash", "type": "bash_20250124" },
                { "name": "str_replace_editor", "type": "text_editor_20250124" }
            ],
            "tool_choice": { "type": "auto", "disable_parallel_tool_use": false }
        });

        let params: CreateMessageParams = serde_json::from_value(body).unwrap();
        let tools = params.tools.as_ref().expect("tools should be present");
        assert_eq!(tools.len(), 2);

        // Ensure we preserve the tool union objects when re-serializing.
        let reserialized = serde_json::to_value(&params).unwrap();
        assert_eq!(reserialized["tools"][0]["type"], "bash_20250124");
        assert_eq!(reserialized["tools"][1]["type"], "text_editor_20250124");
    }

    #[test]
    fn deserializes_tool_choice_string_auto() {
        let body = json!({
            "max_tokens": 64,
            "messages": [
                { "role": "user", "content": "hi" }
            ],
            "model": "claude-sonnet-4-5-20250929",
            "tool_choice": "auto"
        });

        let params: CreateMessageParams = serde_json::from_value(body).unwrap();
        assert!(matches!(params.tool_choice, Some(ToolChoice::Auto { .. })));
    }
}
