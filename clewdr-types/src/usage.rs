use serde::{Deserialize, Serialize};

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

/// Which per-family columns a token count also lands in.
///
/// Mirrors `ModelFamily` without depending on it, since that type lives in the
/// server crate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UsageFamily {
    Sonnet,
    Opus,
    /// Counted in the totals only.
    Other,
}

impl UsageBreakdown {
    /// Add one exchange to the totals, and to the family columns when the model
    /// belongs to a tracked family.
    ///
    /// Saturating throughout: a counter pinned at `u64::MAX` is a better
    /// failure mode than one that wraps to zero.
    pub fn add(&mut self, input: u64, output: u64, family: UsageFamily) {
        self.total_input_tokens = self.total_input_tokens.saturating_add(input);
        self.total_output_tokens = self.total_output_tokens.saturating_add(output);
        match family {
            UsageFamily::Sonnet => {
                self.sonnet_input_tokens = self.sonnet_input_tokens.saturating_add(input);
                self.sonnet_output_tokens = self.sonnet_output_tokens.saturating_add(output);
            }
            UsageFamily::Opus => {
                self.opus_input_tokens = self.opus_input_tokens.saturating_add(input);
                self.opus_output_tokens = self.opus_output_tokens.saturating_add(output);
            }
            UsageFamily::Other => {}
        }
    }

    #[must_use]
    pub fn any_nonzero(&self) -> bool {
        self.total_input_tokens > 0
            || self.total_output_tokens > 0
            || self.sonnet_input_tokens > 0
            || self.sonnet_output_tokens > 0
            || self.opus_input_tokens > 0
            || self.opus_output_tokens > 0
    }
}
