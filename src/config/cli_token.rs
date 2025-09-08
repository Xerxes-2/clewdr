use std::fmt::Display;
use std::hash::Hash;
use chrono::{DateTime, Utc};

use serde::{Deserialize, Serialize};
use tracing::warn;

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Hash)]
#[serde(from = "String")]
#[serde(into = "String")]
pub struct CliBearerToken {
    pub inner: String,
}

impl From<String> for CliBearerToken {
    fn from(mut original: String) -> Self {
        original = original.trim().to_string();
        if original.starts_with("Bearer ") {
            original = original[7..].to_string();
        }
        // Minimal validation: ya29 prefix is common but not guaranteed; warn if too short
        if original.len() < 20 {
            warn!("CLI token looks too short");
        }
        Self { inner: original }
    }
}

impl From<CliBearerToken> for String {
    fn from(v: CliBearerToken) -> Self { v.inner }
}

impl Display for CliBearerToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.inner)
    }
}

impl CliBearerToken {
    pub fn ellipse(&self) -> String {
        let len = self.inner.len();
        if len > 10 { format!("{}...", &self.inner[..10]) } else { self.inner.to_owned() }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Hash)]
pub struct CliTokenStatus {
    pub token: CliBearerToken,
    #[serde(default)]
    pub count_403: u32,
    #[serde(default)]
    pub expiry: Option<DateTime<Utc>>, // when the access token expires
    #[serde(default)]
    pub meta: Option<CliOAuthMeta>,    // optional refresh info
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Hash, Default)]
pub struct CliOAuthMeta {
    #[serde(default)]
    pub client_id: Option<String>,
    #[serde(default)]
    pub client_secret: Option<String>,
    #[serde(default)]
    pub refresh_token: Option<String>,
    #[serde(default)]
    pub token_uri: Option<String>,
    #[serde(default)]
    pub scopes: Option<Vec<String>>,
    #[serde(default)]
    pub project_id: Option<String>,
}

impl CliTokenStatus {
    pub fn validate(&self) -> bool { !self.token.inner.is_empty() }
}
