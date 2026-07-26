use serde::{Deserialize, Serialize};

// The bool fields are a flat wire format shared with the frontend. Grouping
// them into sub-structs, as the lint suggests, would change the JSON schema.
#[expect(
    clippy::struct_excessive_bools,
    reason = "mirrors the config API wire format"
)]
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ConfigApi {
    #[serde(default)]
    pub ip: String,
    #[serde(default)]
    pub port: u16,
    #[serde(default)]
    pub check_update: bool,
    #[serde(default)]
    pub auto_update: bool,
    #[serde(default)]
    pub password: String,
    #[serde(default)]
    pub admin_password: String,
    pub proxy: Option<String>,
    pub rproxy: Option<String>,
    #[serde(default)]
    pub max_retries: usize,
    #[serde(default)]
    pub preserve_chats: bool,
    #[serde(default)]
    pub web_search: bool,
    #[serde(default)]
    pub enable_web_count_tokens: bool,
    #[serde(default)]
    pub sanitize_messages: bool,
    #[serde(default)]
    pub skip_first_warning: bool,
    #[serde(default)]
    pub skip_second_warning: bool,
    #[serde(default)]
    pub skip_restricted: bool,
    #[serde(default)]
    pub skip_non_pro: bool,
    #[serde(default)]
    pub skip_rate_limit: bool,
    #[serde(default)]
    pub skip_normal_pro: bool,
    #[serde(default)]
    pub use_real_roles: bool,
    pub custom_h: Option<String>,
    pub custom_a: Option<String>,
    #[serde(default)]
    pub custom_prompt: String,
    pub claude_code_client_id: Option<String>,
    pub custom_system: Option<String>,
}
