use std::fmt::Display;

use axum::{
    Json,
    extract::rejection::{JsonRejection, PathRejection, QueryRejection},
    response::IntoResponse,
};
use chrono::Utc;
use colored::Colorize;
use oauth2::{RequestTokenError, StandardErrorResponse, basic::BasicErrorResponseType};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use snafu::Location;
use strum::IntoStaticStr;
use tokio::sync::oneshot;
use tracing::{debug, error};
use wreq::{Response, StatusCode, header::InvalidHeaderValue};

use crate::{config::Reason, types::claude::Message};

#[derive(Debug, IntoStaticStr, snafu::Snafu)]
#[snafu(visibility(pub(crate)))]
#[strum(serialize_all = "snake_case")]
pub enum ClewdrError {
    #[snafu(display("HTTP error: {}, at: {}", source, loc))]
    #[snafu(context(false))]
    HttpError {
        #[snafu(implicit)]
        loc: Location,
        source: http::Error,
    },
    #[snafu(display("Ractor error: {}", msg))]
    RactorError {
        #[snafu(implicit)]
        loc: Location,
        msg: String,
    },
    #[snafu(display("Error requesting token: {}", source))]
    #[snafu(context(false))]
    RequestTokenError {
        #[snafu(implicit)]
        loc: Location,
        source: RequestTokenError<
            oauth2::HttpClientError<wreq::Error>,
            StandardErrorResponse<BasicErrorResponseType>,
        >,
    },
    #[snafu(display("URL parse error: {}, at: {}", source, loc))]
    UrlError {
        #[snafu(implicit)]
        loc: Location,
        url: String,
        source: url::ParseError,
    },
    #[snafu(display("Parse cookie error: {}, at: {}", msg, loc))]
    ParseCookieError {
        #[snafu(implicit)]
        loc: Location,
        msg: &'static str,
    },
    #[snafu(display("Invalid URI: {}", uri))]
    InvalidUri {
        uri: String,
        source: http::uri::InvalidUri,
    },
    #[snafu(display("Empty choices"))]
    EmptyChoices,
    #[snafu(display("JSON error: {}", source))]
    #[snafu(context(false))]
    JsonError { source: serde_json::Error },
    #[snafu(transparent)]
    PathRejection { source: PathRejection },
    #[snafu(transparent)]
    QueryRejection { source: QueryRejection },
    #[snafu(display("Test Message"))]
    TestMessage,
    #[snafu(display("FmtError: {}", source))]
    #[snafu(context(false))]
    FmtError {
        #[snafu(implicit)]
        loc: Location,
        source: std::fmt::Error,
    },
    #[snafu(display("Invalid header value: {}", source))]
    #[snafu(context(false))]
    InvalidHeaderValue { source: InvalidHeaderValue },
    #[snafu(display("Bad request: {}", msg))]
    BadRequest { msg: &'static str },
    #[snafu(display("Retries exceeded"))]
    TooManyRetries,
    #[snafu(display("EventSource error: {}", source))]
    #[snafu(context(false))]
    EventSourceAxumError {
        source: eventsource_stream::EventStreamError<axum::Error>,
    },
    #[snafu(context(false))]
    EventSourceRquestError {
        source: eventsource_stream::EventStreamError<wreq::Error>,
    },
    #[snafu(display("Zip error: {}", source))]
    #[snafu(context(false))]
    #[cfg(feature = "portable")]
    ZipError { source: zip::result::ZipError },
    #[snafu(display("Asset Error: {}", msg))]
    AssetError { msg: String },
    #[snafu(display("Invalid version: {}", version))]
    InvalidVersion { version: String },
    #[snafu(display("ParseInt error: {}", source))]
    #[snafu(context(false))]
    ParseIntError { source: std::num::ParseIntError },
    #[snafu(display("Cookie dispatch error: {}", source))]
    #[snafu(context(false))]
    CookieDispatchError { source: oneshot::error::RecvError },
    #[snafu(display("No cookie available"))]
    NoCookieAvailable,
    #[snafu(display("Invalid Cookie: {}", reason))]
    #[snafu(context(false))]
    InvalidCookie {
        #[snafu(source)]
        reason: Reason,
    },
    #[snafu(display("Failed to parse TOML: {}", source))]
    #[snafu(context(false))]
    TomlDeError { source: toml::de::Error },
    #[snafu(transparent)]
    TomlSeError { source: toml::ser::Error },
    #[snafu(transparent)]
    JsonRejection { source: JsonRejection },
    #[snafu(display("Rquest error: {}, source: {}", msg, source))]
    WreqError {
        msg: &'static str,
        source: wreq::Error,
    },
    #[snafu(display("UTF-8 error: {}", source))]
    #[snafu(context(false))]
    UTF8Error {
        #[snafu(implicit)]
        loc: Location,
        source: std::string::FromUtf8Error,
    },
    #[snafu(display("Http error: code: {}, body: {}", code.to_string().red(), inner.to_string()))]
    ClaudeHttpError {
        code: StatusCode,
        inner: ClaudeErrorBody,
    },
    #[snafu(display("Unexpected None: {}", msg))]
    UnexpectedNone { msg: &'static str },
    #[snafu(display("IO error: {}", source))]
    #[snafu(context(false))]
    IoError {
        #[snafu(implicit)]
        loc: Location,
        source: std::io::Error,
    },
    #[snafu(display("{}", msg))]
    PathNotFound { msg: String },
    #[snafu(display("Invalid timestamp: {}", timestamp))]
    TimestampError { timestamp: i64 },
    #[snafu(display("Key/Password Invalid"))]
    InvalidAuth,
    #[snafu(whatever, display("{}: {}", message, source.as_ref().map_or_else(|| "Unknown error".into(), |e| e.to_string())))]
    Whatever {
        message: String,
        #[snafu(source(from(Box<dyn std::error::Error + Send>, Some)))]
        source: Option<Box<dyn std::error::Error + Send>>,
    },
}

impl IntoResponse for ClewdrError {
    fn into_response(self) -> axum::response::Response {
        let (status, msg) = match self {
            ClewdrError::UrlError {
                loc,
                source,
                ref url,
            } => (
                StatusCode::BAD_REQUEST,
                json!(format!("{}: {} (URL: {})", loc, source, url)),
            ),
            ClewdrError::ParseCookieError { .. } => {
                (StatusCode::BAD_REQUEST, json!(self.to_string()))
            }
            ClewdrError::InvalidUri { .. } => (StatusCode::BAD_REQUEST, json!(self.to_string())),
            ClewdrError::PathRejection { ref source } => {
                (source.status(), json!(source.body_text()))
            }
            ClewdrError::QueryRejection { ref source } => {
                (source.status(), json!(source.body_text()))
            }
            ClewdrError::ClaudeHttpError { code, inner } => {
                return (code, Json(ClaudeError { error: inner })).into_response();
            }
            ClewdrError::TestMessage => {
                return (
                    StatusCode::OK,
                    Json(Message::from(
                        "Claude Reverse Proxy is working, please send a real message.",
                    )),
                )
                    .into_response();
            }
            ClewdrError::JsonRejection { ref source } => {
                (source.status(), json!(source.body_text()))
            }
            ClewdrError::TooManyRetries => (StatusCode::GATEWAY_TIMEOUT, json!(self.to_string())),
            ClewdrError::InvalidCookie { .. } => (StatusCode::BAD_REQUEST, json!(self.to_string())),
            ClewdrError::PathNotFound { .. } => (StatusCode::NOT_FOUND, json!(self.to_string())),
            ClewdrError::InvalidAuth => (StatusCode::UNAUTHORIZED, json!(self.to_string())),
            ClewdrError::BadRequest { .. } => (StatusCode::BAD_REQUEST, json!(self.to_string())),
            ClewdrError::InvalidHeaderValue { .. } => {
                (StatusCode::BAD_REQUEST, json!(self.to_string()))
            }
            ClewdrError::EmptyChoices => (StatusCode::NO_CONTENT, json!(self.to_string())),
            _ => (StatusCode::INTERNAL_SERVER_ERROR, json!(self.to_string())),
        };
        let err = ClaudeError {
            error: ClaudeErrorBody {
                message: msg,
                r#type: <&str>::from(self).into(),
                code: Some(status.as_u16()),
            },
        };
        (status, Json(err)).into_response()
    }
}

/// HTTP error response
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ClaudeError {
    pub error: ClaudeErrorBody,
}

/// Inner HTTP error response
#[derive(Debug, Serialize, Clone)]
pub struct ClaudeErrorBody {
    pub message: Value,
    pub r#type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<u16>,
}

/// Raw Inner HTTP error response
#[derive(Debug, Deserialize)]
struct RawBody {
    pub message: String,
    pub r#type: String,
}

impl<'de> Deserialize<'de> for ClaudeErrorBody {
    /// when message is a json string, try parse it as a object
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let raw = RawBody::deserialize(deserializer)?;
        if let Ok(message) = serde_json::from_str::<Value>(&raw.message) {
            return Ok(ClaudeErrorBody {
                message,
                r#type: raw.r#type,
                code: None,
            });
        }
        Ok(ClaudeErrorBody {
            message: json!(raw.message),
            r#type: raw.r#type,
            code: None,
        })
    }
}

impl Display for ClaudeErrorBody {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        serde_json::to_string_pretty(self)
            .map_err(|_| std::fmt::Error)?
            .fmt(f)
    }
}

pub trait CheckClaudeErr
where
    Self: Sized,
{
    fn check_claude(self) -> impl Future<Output = Result<Self, ClewdrError>>;
}

impl CheckClaudeErr for Response {
    /// Checks response from Claude Web API for errors
    /// Validates HTTP status codes and parses error messages from responses
    ///
    /// # Arguments
    /// * `res` - The HTTP response to check
    ///
    /// # Returns
    /// * `Ok(Response)` if the request was successful
    /// * `Err(ClewdrError)` if the request failed, with details about the failure
    async fn check_claude(self) -> Result<Self, ClewdrError> {
        let status = self.status();
        if status.is_success() {
            return Ok(self);
        }
        let reset_header = self
            .headers()
            .get("anthropic-ratelimit-unified-reset")
            .cloned();
        debug!("Error response status: {}", status);
        if status == 302 {
            // blocked by cloudflare
            let error = ClaudeErrorBody {
                message: json!("Blocked, check your IP address"),
                r#type: "error".to_string(),
                code: Some(status.as_u16()),
            };
            return Err(ClewdrError::ClaudeHttpError {
                code: status,
                inner: error,
            });
        }
        let text = match self.text().await {
            Ok(text) => text,
            Err(err) => {
                let error = ClaudeErrorBody {
                    message: json!(err.to_string()),
                    r#type: "error_get_error_body".to_string(),
                    code: Some(status.as_u16()),
                };
                return Err(ClewdrError::ClaudeHttpError {
                    code: status,
                    inner: error,
                });
            }
        };
        let Ok(err) = serde_json::from_str::<ClaudeError>(&text) else {
            let error = ClaudeErrorBody {
                message: format!("Unknown error: {text}").into(),
                r#type: "error_parse_error_body".to_string(),
                code: Some(status.as_u16()),
            };
            return Err(ClewdrError::ClaudeHttpError {
                code: status,
                inner: error,
            });
        };
        if status == 400 && err.error.message == json!("This organization has been disabled.") {
            // account disabled
            return Err(Reason::Disabled.into());
        }
        if status == 401 {
            return Err(Reason::Null.into());
        }
        const OAUTH_403_PHRASE: &str =
            "oauth authentication is currently not allowed for this organization";
        if status == 403
            && err
                .error
                .message
                .to_string()
                .to_ascii_lowercase()
                .contains(OAUTH_403_PHRASE)
        {
            return Err(Reason::Null.into());
        }
        let inner_error = err.error;
        // check if the error is a rate limit error
        if status == 429 {
            // Long-context 1M gating also uses 429; keep it as HTTP error so upper
            // retry logic can downgrade to non-1M without cooling down the cookie.
            let msg_lower = inner_error
                .message
                .as_str()
                .map(|s| s.to_ascii_lowercase())
                .unwrap_or_else(|| inner_error.message.to_string().to_ascii_lowercase());
            if msg_lower.contains("extra usage is required for long context requests") {
                return Err(ClewdrError::ClaudeHttpError {
                    code: status,
                    inner: inner_error,
                });
            }

            // get the reset time from the error message
            let ts = inner_error.message["resetsAt"]
                .as_i64()
                .or_else(|| reset_header.and_then(|h| h.to_str().ok()?.parse::<i64>().ok()));
            if let Some(ts) = ts {
                let reset_time = chrono::DateTime::from_timestamp(ts, 0)
                    .ok_or(ClewdrError::TimestampError { timestamp: ts })?
                    .to_utc();
                let now = chrono::Utc::now();
                let diff = reset_time - now;
                let mins = diff.num_minutes();
                error!(
                    "Rate limit exceeded, expires in {} hours",
                    mins as f64 / 60.0
                );
                return Err(ClewdrError::InvalidCookie {
                    reason: Reason::TooManyRequest(ts),
                });
            } else {
                error!("Rate limit exceeded, but no reset time provided");
                return Err(ClewdrError::InvalidCookie {
                    reason: Reason::TooManyRequest(Utc::now().timestamp() + 3600),
                });
            }
        }
        Err(ClewdrError::ClaudeHttpError {
            code: status,
            inner: inner_error,
        })
    }
}
