use std::hash::Hash;

pub use clewdr_types::Reason;
use serde::{Deserialize, Serialize};

use super::CookieStatus;
use crate::config::ClewdrCookie;

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
    /// Creates a new `UselessCookie` instance
    ///
    /// # Arguments
    /// * `cookie` - The cookie that is unusable
    /// * `reason` - The reason why the cookie is unusable
    ///
    /// # Returns
    /// A new `UselessCookie` instance
    pub fn new(cookie: ClewdrCookie, reason: Reason) -> Self {
        Self { cookie, reason }
    }
}
