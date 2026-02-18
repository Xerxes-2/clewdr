use std::{
    fmt::{Debug, Display},
    hash::Hash,
    ops::Deref,
    str::FromStr,
    sync::LazyLock,
};

use regex::Regex;
use serde::{Deserialize, Serialize};
use snafu::{GenerateImplicitData, Location};
use tracing::info;

use crate::{
    config::{PLACEHOLDER_COOKIE, TokenInfo},
    error::ClewdrError,
};

/// Model family for usage bucketing
#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ModelFamily {
    Sonnet,
    Opus,
    Other,
}

/// Per-model 1M context probing channel
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Claude1mChannel {
    Sonnet,
    Opus,
}

/// Per-period usage breakdown by family
#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct UsageBreakdown {
    #[serde(default)]
    pub total_input_tokens: u64,
    #[serde(default)]
    pub total_output_tokens: u64,

    #[serde(default)]
    pub sonnet_input_tokens: u64,
    #[serde(default)]
    pub sonnet_output_tokens: u64,

    #[serde(default)]
    pub opus_input_tokens: u64,
    #[serde(default)]
    pub opus_output_tokens: u64,
}

/// A struct representing a cookie
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ClewdrCookie {
    inner: String,
}

impl Serialize for ClewdrCookie {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.inner)
    }
}

impl<'de> Deserialize<'de> for ClewdrCookie {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        ClewdrCookie::from_str(&s).map_err(serde::de::Error::custom)
    }
}

/// A struct representing a cookie with its information
#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct CookieStatus {
    pub cookie: ClewdrCookie,
    #[serde(default)]
    pub token: Option<TokenInfo>,
    #[serde(default)]
    pub reset_time: Option<i64>,
    #[serde(default)]
    pub supports_claude_1m_sonnet: Option<bool>,
    #[serde(default)]
    pub supports_claude_1m_opus: Option<bool>,
    #[serde(default)]
    pub count_tokens_allowed: Option<bool>,

    // New: Per-period usage breakdown
    #[serde(default)]
    pub session_usage: UsageBreakdown,
    #[serde(default)]
    pub weekly_usage: UsageBreakdown,
    #[serde(default)]
    pub weekly_sonnet_usage: UsageBreakdown,
    #[serde(default)]
    pub weekly_opus_usage: UsageBreakdown,
    #[serde(default)]
    pub lifetime_usage: UsageBreakdown,

    // Reset boundaries for each period (epoch seconds, UTC)
    #[serde(default)]
    pub session_resets_at: Option<i64>,
    #[serde(default)]
    pub weekly_resets_at: Option<i64>,
    #[serde(default)]
    pub weekly_sonnet_resets_at: Option<i64>,
    #[serde(default)]
    pub weekly_opus_resets_at: Option<i64>,

    /// Last time we probed Anthropic console for resets_at
    #[serde(default)]
    pub resets_last_checked_at: Option<i64>,

    /// Whether the subscription exposes a reset boundary for each window
    /// None = unknown (not probed yet), Some(true) = track this window, Some(false) = no limit, never probe again
    #[serde(default)]
    pub session_has_reset: Option<bool>,
    #[serde(default)]
    pub weekly_has_reset: Option<bool>,
    #[serde(default)]
    pub weekly_sonnet_has_reset: Option<bool>,
    #[serde(default)]
    pub weekly_opus_has_reset: Option<bool>,
}

impl PartialEq for CookieStatus {
    fn eq(&self, other: &Self) -> bool {
        self.cookie == other.cookie
    }
}

impl Eq for CookieStatus {}

impl Hash for CookieStatus {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.cookie.hash(state);
    }
}

impl Ord for CookieStatus {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.cookie.cmp(&other.cookie)
    }
}

impl PartialOrd for CookieStatus {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl CookieStatus {
    /// Creates a new CookieStatus instance
    ///
    /// # Arguments
    /// * `cookie` - Cookie string
    /// * `reset_time` - Optional timestamp when the cookie can be reused
    ///
    /// # Returns
    /// A new CookieStatus instance
    pub fn new(cookie: &str, reset_time: Option<i64>) -> Result<Self, ClewdrError> {
        let cookie = ClewdrCookie::from_str(cookie)?;
        Ok(Self {
            cookie,
            token: None,
            reset_time,
            supports_claude_1m_sonnet: Some(true),
            supports_claude_1m_opus: Some(true),
            count_tokens_allowed: None,

            session_usage: UsageBreakdown::default(),
            weekly_usage: UsageBreakdown::default(),
            weekly_sonnet_usage: UsageBreakdown::default(),
            weekly_opus_usage: UsageBreakdown::default(),
            lifetime_usage: UsageBreakdown::default(),
            session_resets_at: None,
            weekly_resets_at: None,
            weekly_sonnet_resets_at: None,
            weekly_opus_resets_at: None,
            resets_last_checked_at: None,
            session_has_reset: None,
            weekly_has_reset: None,
            weekly_sonnet_has_reset: None,
            weekly_opus_has_reset: None,
        })
    }

    /// Checks if the cookie's reset time has expired
    /// If the reset time has passed, sets it to None so the cookie becomes valid again
    ///
    /// # Returns
    /// The same CookieStatus with potentially updated reset_time
    pub fn reset(self) -> Self {
        if let Some(t) = self.reset_time
            && t < chrono::Utc::now().timestamp()
        {
            info!("Cookie reset time expired");
            return Self {
                reset_time: None,
                session_usage: UsageBreakdown::default(),
                weekly_usage: UsageBreakdown::default(),
                weekly_sonnet_usage: UsageBreakdown::default(),
                weekly_opus_usage: UsageBreakdown::default(),
                ..self
            };
        }
        self
    }

    pub fn add_token(&mut self, token: TokenInfo) {
        self.token = Some(token);
    }

    pub fn claude_1m_support(&self, channel: Claude1mChannel) -> Option<bool> {
        match channel {
            Claude1mChannel::Sonnet => self.supports_claude_1m_sonnet,
            Claude1mChannel::Opus => self.supports_claude_1m_opus,
        }
    }

    pub fn set_claude_1m_support(&mut self, channel: Claude1mChannel, value: Option<bool>) {
        match channel {
            Claude1mChannel::Sonnet => self.supports_claude_1m_sonnet = value,
            Claude1mChannel::Opus => self.supports_claude_1m_opus = value,
        }
    }

    pub fn set_count_tokens_allowed(&mut self, value: Option<bool>) {
        self.count_tokens_allowed = value;
    }

    pub fn reset_window_usage(&mut self) {
        // Legacy window counters removed; reset session buckets conservatively
        self.session_usage = UsageBreakdown::default();
        self.weekly_usage = UsageBreakdown::default();
        self.weekly_sonnet_usage = UsageBreakdown::default();
        self.weekly_opus_usage = UsageBreakdown::default();
    }

    // ------------------------
    // New usage aggregation
    // ------------------------

    pub fn set_session_resets_at(&mut self, ts: Option<i64>) {
        self.session_resets_at = ts;
    }

    pub fn set_weekly_resets_at(&mut self, ts: Option<i64>) {
        self.weekly_resets_at = ts;
    }

    pub fn set_weekly_sonnet_resets_at(&mut self, ts: Option<i64>) {
        self.weekly_sonnet_resets_at = ts;
    }

    pub fn set_weekly_opus_resets_at(&mut self, ts: Option<i64>) {
        self.weekly_opus_resets_at = ts;
    }

    pub fn add_and_bucket_usage(&mut self, input: u64, output: u64, family: ModelFamily) {
        if input == 0 && output == 0 {
            return;
        }
        // Legacy totals/windows removed; only bucketed aggregation remains

        // session bucket (total + per family)
        self.session_usage.total_input_tokens =
            self.session_usage.total_input_tokens.saturating_add(input);
        self.session_usage.total_output_tokens = self
            .session_usage
            .total_output_tokens
            .saturating_add(output);
        match family {
            ModelFamily::Sonnet => {
                self.session_usage.sonnet_input_tokens =
                    self.session_usage.sonnet_input_tokens.saturating_add(input);
                self.session_usage.sonnet_output_tokens = self
                    .session_usage
                    .sonnet_output_tokens
                    .saturating_add(output);
            }
            ModelFamily::Opus => {
                self.session_usage.opus_input_tokens =
                    self.session_usage.opus_input_tokens.saturating_add(input);
                self.session_usage.opus_output_tokens =
                    self.session_usage.opus_output_tokens.saturating_add(output);
            }
            ModelFamily::Other => {}
        }

        // weekly bucket (total + per family)
        self.weekly_usage.total_input_tokens =
            self.weekly_usage.total_input_tokens.saturating_add(input);
        self.weekly_usage.total_output_tokens =
            self.weekly_usage.total_output_tokens.saturating_add(output);
        match family {
            ModelFamily::Sonnet => {
                self.weekly_usage.sonnet_input_tokens =
                    self.weekly_usage.sonnet_input_tokens.saturating_add(input);
                self.weekly_usage.sonnet_output_tokens = self
                    .weekly_usage
                    .sonnet_output_tokens
                    .saturating_add(output);

                // weekly_sonnet bucket (only sonnet contributes)
                self.weekly_sonnet_usage.total_input_tokens = self
                    .weekly_sonnet_usage
                    .total_input_tokens
                    .saturating_add(input);
                self.weekly_sonnet_usage.total_output_tokens = self
                    .weekly_sonnet_usage
                    .total_output_tokens
                    .saturating_add(output);
                self.weekly_sonnet_usage.sonnet_input_tokens = self
                    .weekly_sonnet_usage
                    .sonnet_input_tokens
                    .saturating_add(input);
                self.weekly_sonnet_usage.sonnet_output_tokens = self
                    .weekly_sonnet_usage
                    .sonnet_output_tokens
                    .saturating_add(output);
            }
            ModelFamily::Opus => {
                self.weekly_usage.opus_input_tokens =
                    self.weekly_usage.opus_input_tokens.saturating_add(input);
                self.weekly_usage.opus_output_tokens =
                    self.weekly_usage.opus_output_tokens.saturating_add(output);
            }
            ModelFamily::Other => {}
        }

        // weekly_opus bucket (only opus contributes)
        if matches!(family, ModelFamily::Opus) {
            self.weekly_opus_usage.total_input_tokens = self
                .weekly_opus_usage
                .total_input_tokens
                .saturating_add(input);
            self.weekly_opus_usage.total_output_tokens = self
                .weekly_opus_usage
                .total_output_tokens
                .saturating_add(output);
            self.weekly_opus_usage.opus_input_tokens = self
                .weekly_opus_usage
                .opus_input_tokens
                .saturating_add(input);
            self.weekly_opus_usage.opus_output_tokens = self
                .weekly_opus_usage
                .opus_output_tokens
                .saturating_add(output);
        }

        // lifetime bucket (total + per family)
        self.lifetime_usage.total_input_tokens =
            self.lifetime_usage.total_input_tokens.saturating_add(input);
        self.lifetime_usage.total_output_tokens = self
            .lifetime_usage
            .total_output_tokens
            .saturating_add(output);
        match family {
            ModelFamily::Sonnet => {
                self.lifetime_usage.sonnet_input_tokens = self
                    .lifetime_usage
                    .sonnet_input_tokens
                    .saturating_add(input);
                self.lifetime_usage.sonnet_output_tokens = self
                    .lifetime_usage
                    .sonnet_output_tokens
                    .saturating_add(output);
            }
            ModelFamily::Opus => {
                self.lifetime_usage.opus_input_tokens =
                    self.lifetime_usage.opus_input_tokens.saturating_add(input);
                self.lifetime_usage.opus_output_tokens = self
                    .lifetime_usage
                    .opus_output_tokens
                    .saturating_add(output);
            }
            ModelFamily::Other => {}
        }
    }
}

impl Deref for ClewdrCookie {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl Default for ClewdrCookie {
    fn default() -> Self {
        Self {
            inner: PLACEHOLDER_COOKIE.to_string(),
        }
    }
}

impl ClewdrCookie {
    pub fn ellipse(&self) -> String {
        let len = self.inner.len();
        if len > 20 {
            format!("{}...", &self.inner[..20])
        } else {
            self.inner.to_owned()
        }
    }
}

impl FromStr for ClewdrCookie {
    type Err = ClewdrError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        static RE_FULL: LazyLock<Regex> = LazyLock::new(|| {
            Regex::new(r"sk-ant-sid\d{2}-[0-9A-Za-z_-]{86,120}-[0-9A-Za-z_-]{6}AA").unwrap()
        });
        static RE_BASE: LazyLock<Regex> =
            LazyLock::new(|| Regex::new(r"^[0-9A-Za-z_-]{86,120}-[0-9A-Za-z_-]{6}AA$").unwrap());

        let cleaned = s
            .trim()
            .chars()
            .filter(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-')
            .collect::<String>();

        if let Some(found) = RE_FULL.find(&cleaned) {
            return Ok(Self {
                inner: found.as_str().to_string(),
            });
        }

        if RE_BASE.is_match(&cleaned) {
            return Ok(Self { inner: cleaned });
        }

        Err(ClewdrError::ParseCookieError {
            loc: Location::generate(),
            msg: "Invalid cookie format",
        })
    }
}

impl Display for ClewdrCookie {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "sessionKey={}", self.inner)
    }
}

impl Debug for ClewdrCookie {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        Display::fmt(self, f)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_base_cookie_with_len(prefix_len: usize) -> String {
        format!("{}-{}AA", "a".repeat(prefix_len), "b".repeat(6))
    }

    #[test]
    fn test_sk_cookie_from_str() {
        let base = make_base_cookie_with_len(86);
        let full = format!("sk-ant-sid01-{base}");
        let cookie = ClewdrCookie::from_str(&full).unwrap();
        assert_eq!(cookie.inner, full);
    }

    #[test]
    fn test_cookie_from_str() {
        let base = make_base_cookie_with_len(86);
        let cookie = ClewdrCookie::from_str(&base).unwrap();
        assert_eq!(cookie.inner, base);
    }

    #[test]
    fn test_long_cookie_from_str() {
        let base = make_base_cookie_with_len(109);
        let full = format!("sk-ant-sid02-{base}");
        let cookie = ClewdrCookie::from_str(&full).unwrap();
        assert_eq!(cookie.inner, full);
    }

    #[test]
    fn test_invalid_cookie() {
        let result = ClewdrCookie::from_str("invalid-cookie");
        assert!(result.is_err());
    }
}
