use std::time::Duration;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tracing::debug;

use crate::claude_code_state::oauth::TokenResponse;

/// A [`Duration`] as whole seconds.
///
/// Matches what `serde_with`'s `DurationSeconds` wrote, so config files written
/// by earlier builds still load. The values are OAuth `expires_in` fields,
/// which are integer seconds to begin with, so nothing is lost by truncating.
mod duration_secs {
    use std::time::Duration;

    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(value: &Duration, ser: S) -> Result<S::Ok, S::Error> {
        ser.serialize_u64(value.as_secs())
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(de: D) -> Result<Duration, D::Error> {
        u64::deserialize(de).map(Duration::from_secs)
    }
}

/// A [`DateTime<Utc>`] as fractional epoch seconds.
///
/// The float encoding is the one `serde_with`'s `TimestampSecondsWithFrac`
/// produced, and it is already sitting in every deployed config file, so both
/// directions have to keep speaking it.
mod timestamp_secs {
    use chrono::{DateTime, Utc};
    use serde::{Deserialize, Deserializer, Serializer, de::Error as _};

    const NANOS_PER_SEC: f64 = 1_000_000_000.0;

    #[expect(
        clippy::cast_precision_loss,
        reason = "f64 is the wire format; it holds epoch seconds exactly for any \
                  date this program will see"
    )]
    pub fn serialize<S: Serializer>(value: &DateTime<Utc>, ser: S) -> Result<S::Ok, S::Error> {
        let secs =
            value.timestamp() as f64 + f64::from(value.timestamp_subsec_nanos()) / NANOS_PER_SEC;
        ser.serialize_f64(secs)
    }

    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "whole is floor()ed so the i64 cast is exact, and the nanos cast is \
                  from a value clamped into range on the line before"
    )]
    pub fn deserialize<'de, D: Deserializer<'de>>(de: D) -> Result<DateTime<Utc>, D::Error> {
        let secs = f64::deserialize(de)?;
        let whole = secs.floor();
        // A fraction of exactly 1.0 after rounding would be out of range for
        // the nanosecond argument, so it is clamped rather than carried.
        let nanos = (((secs - whole) * NANOS_PER_SEC).round()).clamp(0.0, NANOS_PER_SEC - 1.0);
        DateTime::from_timestamp(whole as i64, nanos as u32)
            .ok_or_else(|| D::Error::custom(format!("timestamp out of range: {secs}")))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]

pub struct Organization {
    pub uuid: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TokenInfo {
    pub access_token: String,
    #[serde(with = "duration_secs")]
    pub expires_in: Duration,
    pub organization: Organization,
    pub refresh_token: String,
    #[serde(with = "timestamp_secs")]
    pub expires_at: DateTime<Utc>,
}

impl TokenInfo {
    #[must_use]
    pub fn new(raw: &TokenResponse, organization_uuid: String) -> Self {
        let expires_in = raw.expires_in();
        Self {
            access_token: raw.access_token.clone(),
            expires_in,
            organization: Organization {
                uuid: organization_uuid,
            },
            refresh_token: raw.refresh_token.clone().unwrap_or_default(),
            expires_at: Utc::now() + expires_in,
        }
    }

    pub fn is_expired(&self) -> bool {
        debug!("Expires at: {}", self.expires_at.to_rfc3339());
        Utc::now() >= self.expires_at - Duration::from_mins(5)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A token whose timestamp has a fractional part, so the encoding is
    /// exercised rather than trivially integral.
    fn sample() -> TokenInfo {
        TokenInfo {
            access_token: "a".to_string(),
            expires_in: Duration::from_hours(1),
            organization: Organization {
                uuid: "org".to_string(),
            },
            refresh_token: "r".to_string(),
            expires_at: DateTime::from_timestamp(1_785_087_657, 250_000_000).unwrap(),
        }
    }

    /// These fields live in every user's config file on disk, written by builds
    /// that used `serde_with`. The literals here are that build's exact output:
    /// `expires_in` as integer seconds, `expires_at` as fractional epoch
    /// seconds. Changing either shape would silently invalidate saved logins.
    #[test]
    fn the_on_disk_encoding_is_unchanged() {
        let json = serde_json::to_value(sample()).unwrap();

        assert_eq!(json["expires_in"], serde_json::json!(3600));
        assert_eq!(json["expires_at"], serde_json::json!(1_785_087_657.25));
    }

    /// The config file is TOML, not JSON, and TOML keeps integers and floats
    /// apart, so the round trip has to be checked in the real format.
    #[test]
    fn a_token_round_trips_through_toml() {
        let encoded = toml::to_string(&sample()).unwrap();
        assert!(
            encoded.contains("expires_in = 3600"),
            "expires_in must stay an integer: {encoded}"
        );

        let decoded: TokenInfo = toml::from_str(&encoded).unwrap();
        assert_eq!(decoded, sample());
    }

    /// A file written by the previous build must still load. This is that
    /// build's output verbatim.
    #[test]
    fn a_config_written_by_the_previous_build_still_loads() {
        let legacy = r#"
            access_token = "a"
            expires_in = 3600
            refresh_token = "r"
            expires_at = 1785087657.25
            [organization]
            uuid = "org"
        "#;

        let decoded: TokenInfo = toml::from_str(legacy).expect("legacy config must load");

        assert_eq!(decoded.expires_in, Duration::from_hours(1));
        assert_eq!(decoded.expires_at.timestamp(), 1_785_087_657);
    }

    /// The old decoder took a bare integer for the timestamp as well as a
    /// float, and TOML types those differently, so a whole-second boundary
    /// written without a fractional part must not be rejected.
    #[test]
    fn an_integer_timestamp_is_still_accepted() {
        let legacy = r#"
            access_token = "a"
            expires_in = 3600
            refresh_token = "r"
            expires_at = 1785087657
            [organization]
            uuid = "org"
        "#;

        let decoded: TokenInfo = toml::from_str(legacy).expect("an integer timestamp must load");

        assert_eq!(decoded.expires_at.timestamp(), 1_785_087_657);
        assert_eq!(decoded.expires_at.timestamp_subsec_nanos(), 0);
    }

    /// Expiry drives whether a cookie is refreshed, so the sign and ordering
    /// have to survive the float encoding.
    #[test]
    fn expiry_is_decided_from_the_decoded_timestamp() {
        let mut token = sample();
        token.expires_at = Utc::now() + Duration::from_hours(1);
        let reloaded: TokenInfo = toml::from_str(&toml::to_string(&token).unwrap()).unwrap();
        assert!(!reloaded.is_expired());

        token.expires_at = Utc::now() - Duration::from_secs(1);
        let reloaded: TokenInfo = toml::from_str(&toml::to_string(&token).unwrap()).unwrap();
        assert!(reloaded.is_expired());
    }

    /// `is_expired` reports true inside the five-minute refresh margin, so a
    /// token is renewed before it actually lapses.
    #[test]
    fn a_token_expiring_within_the_refresh_margin_counts_as_expired() {
        let mut token = sample();
        token.expires_at = Utc::now() + Duration::from_mins(4);
        assert!(token.is_expired());
    }
}
