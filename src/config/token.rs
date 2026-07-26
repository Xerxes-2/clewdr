use std::time::Duration;

use chrono::{DateTime, Utc};
use oauth2::{EmptyExtraTokenFields, StandardTokenResponse, TokenResponse, basic::BasicTokenType};
use serde::{Deserialize, Serialize};
use serde_with::{DurationSeconds, TimestampSecondsWithFrac, serde_as};
use tracing::debug;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]

pub struct Organization {
    pub uuid: String,
}

#[serde_as]
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TokenInfo {
    pub access_token: String,
    #[serde_as(as = "DurationSeconds")]
    pub expires_in: Duration,
    pub organization: Organization,
    pub refresh_token: String,
    #[serde_as(as = "TimestampSecondsWithFrac")]
    pub expires_at: DateTime<Utc>,
}

impl TokenInfo {
    pub fn new(
        raw: &StandardTokenResponse<EmptyExtraTokenFields, BasicTokenType>,
        organization_uuid: String,
    ) -> Self {
        let expires_at = Utc::now() + raw.expires_in().unwrap_or_default();
        Self {
            access_token: raw.access_token().secret().clone(),
            expires_in: raw.expires_in().unwrap_or_default(),
            organization: Organization {
                uuid: organization_uuid,
            },
            refresh_token: raw
                .refresh_token()
                .map_or_else(Default::default, |rt| rt.secret().clone()),
            expires_at,
        }
    }

    pub fn is_expired(&self) -> bool {
        debug!("Expires at: {}", self.expires_at.to_rfc3339());
        Utc::now() >= self.expires_at - Duration::from_mins(5)
    }
}
