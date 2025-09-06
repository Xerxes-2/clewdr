use std::{
    fmt::{Debug, Display},
    hash::Hash,
};

use colored::Colorize;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use super::CookieStatus;
use crate::config::ClewdrCookie;

/// Reason why a cookie is considered useless
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Hash, Error)]
pub enum Reason {
    NormalPro,
    NonPro,
    Disabled,
    Banned,
    Null,
    /// Upstream returned 403 Forbidden; message provides additional context
    Forbidden(String),
    Restricted(i64),
    TooManyRequest(i64),
}

impl Display for Reason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let colored_time = |secs: i64| {
            chrono::DateTime::from_timestamp(secs, 0)
                .map(|t| t.format("UTC %Y-%m-%d %H:%M:%S").to_string())
                .unwrap_or("Invalid date".to_string())
                .yellow()
        };
        match self {
            Reason::NormalPro => write!(f, "Normal Pro account"),
            Reason::Disabled => write!(f, "Organization Disabled"),
            Reason::NonPro => write!(f, "Free account"),
            Reason::Banned => write!(f, "Banned"),
            Reason::Null => write!(f, "Null"),
            Reason::Forbidden(msg) => write!(f, "403 Forbidden: {}", msg),
            Reason::Restricted(i) => {
                write!(f, "Restricted/Warning: until {}", colored_time(*i))
            }
            Reason::TooManyRequest(i) => {
                write!(f, "429 Too many request: until {}", colored_time(*i))
            }
        }
    }
}

/// A struct representing a cookie that can't be used
/// Contains the cookie and the reason why it's considered unusable
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct UselessCookie {
    pub cookie: ClewdrCookie,
    pub reason: Reason,
}

impl PartialEq<CookieStatus> for UselessCookie {
    fn eq(&self, other: &CookieStatus) -> bool {
        self.cookie == other.cookie
    }
}

impl PartialEq for UselessCookie {
    fn eq(&self, other: &Self) -> bool {
        self.cookie == other.cookie
    }
}

impl Eq for UselessCookie {}

impl Hash for UselessCookie {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.cookie.hash(state);
    }
}

impl UselessCookie {
    /// Creates a new UselessCookie instance
    ///
    /// # Arguments
    /// * `cookie` - The cookie that is unusable
    /// * `reason` - The reason why the cookie is unusable
    ///
    /// # Returns
    /// A new UselessCookie instance
    pub fn new(cookie: ClewdrCookie, reason: Reason) -> Self {
        Self { cookie, reason }
    }
}
