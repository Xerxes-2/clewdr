use std::{
    collections::{HashSet, VecDeque},
    sync::{Arc, Mutex, MutexGuard, PoisonError, Weak},
    time::Duration,
};

use chrono::Utc;
use colored::Colorize;
use moka::sync::Cache;
use serde::Serialize;
use tracing::{error, info, warn};

use crate::{
    config::{
        CLEWDR_CONFIG, CookieSnapshot, CookieStatus, Reason, UsageBreakdown, UselessCookie,
        take_global_cookies,
    },
    error::ClewdrError,
};

const INTERVAL: u64 = 300;
const SESSION_WINDOW_SECS: i64 = 5 * 60 * 60; // 5h
const WEEKLY_WINDOW_SECS: i64 = 7 * 24 * 60 * 60; // 7d

#[derive(Debug, Serialize, Clone)]
pub struct CookieStatusInfo {
    pub valid: Vec<CookieStatus>,
    pub exhausted: Vec<CookieStatus>,
    pub invalid: Vec<UselessCookie>,
}

/// The cookie pool's data, and the only owner of it.
///
/// Every method here is synchronous and runs under [`CookiePool`]'s mutex, so
/// operations are serialized the same way a single-threaded owner would
/// serialize them. Nothing in here may block or await.
#[derive(Debug)]
struct PoolState {
    valid: VecDeque<CookieStatus>,
    exhausted: HashSet<CookieStatus>,
    invalid: HashSet<UselessCookie>,
    moka: Cache<u64, CookieStatus>,
}

impl PoolState {
    /// Builds the initial pool by taking ownership of the config's cookies
    fn from_config() -> Self {
        let loaded = take_global_cookies();
        // A cookie with a reset time is still cooling down; the rest are usable.
        let (usable, cooling): (Vec<_>, Vec<_>) = loaded
            .cookies
            .into_iter()
            .partition(|c| c.reset_time.is_none());
        let valid = VecDeque::from(usable);
        let exhausted = cooling.into_iter().collect::<HashSet<_>>();
        let invalid = loaded.wasted;

        let moka = Cache::builder()
            .max_capacity(1000)
            .time_to_idle(Duration::from_hours(1))
            .build();

        Self {
            valid,
            exhausted,
            invalid,
            moka,
        }
    }

    /// The pool's cookies in the shape the config file expects
    fn snapshot(&self) -> CookieSnapshot {
        CookieSnapshot {
            cookies: self
                .valid
                .iter()
                .chain(self.exhausted.iter())
                .cloned()
                .collect(),
            wasted: self.invalid.clone(),
        }
    }

    /// Persists the pool alongside the current settings, in the background.
    ///
    /// Spawned rather than awaited because callers hold the pool lock, which
    /// must not be held across file I/O.
    fn save(&self) {
        let cookies = self.snapshot();
        tokio::spawn(async move {
            if let Err(e) = CLEWDR_CONFIG.load().save(&cookies).await {
                error!("Failed to save config: {}", e);
            }
        });
    }

    /// Logs the current state of cookie collections
    fn log(&self) {
        info!(
            "Valid: {}, Exhausted: {}, Invalid: {}",
            self.valid.len().to_string().green(),
            self.exhausted.len().to_string().yellow(),
            self.invalid.len().to_string().red(),
        );
    }

    /// Checks and resets cookies that have passed their reset time
    fn reset(&mut self) {
        let mut reset_cookies = Vec::new();
        self.exhausted.retain(|cookie| {
            let reset_cookie = cookie.clone().reset();
            if reset_cookie.reset_time.is_none() {
                reset_cookies.push(reset_cookie);
                false
            } else {
                true
            }
        });
        if reset_cookies.is_empty() {
            return;
        }
        // 将重置的 cookies 放回 valid，并进行增量 upsert
        for c in reset_cookies {
            self.valid.push_back(c);
        }
        self.save();
        self.log();
    }

    /// Reset in-memory usage buckets when local reset boundaries have elapsed.
    /// This avoids stale counters when cooldown windows expire between requests.
    fn refresh_usage_windows(&mut self) -> bool {
        fn reset_if_due(
            has_reset: Option<bool>,
            resets_at: &mut Option<i64>,
            usage: &mut UsageBreakdown,
            window_secs: i64,
            now: i64,
        ) -> bool {
            if has_reset == Some(true) && resets_at.is_some_and(|ts| now >= ts) {
                *usage = UsageBreakdown::default();
                *resets_at = Some(now + window_secs);
                return true;
            }
            false
        }

        let now = Utc::now().timestamp();
        let mut changed = false;

        let apply_resets = |cookie: &mut CookieStatus| {
            let mut cookie_changed = reset_if_due(
                cookie.session_has_reset,
                &mut cookie.session_resets_at,
                &mut cookie.session_usage,
                SESSION_WINDOW_SECS,
                now,
            );
            cookie_changed |= reset_if_due(
                cookie.weekly_has_reset,
                &mut cookie.weekly_resets_at,
                &mut cookie.weekly_usage,
                WEEKLY_WINDOW_SECS,
                now,
            );
            cookie_changed |= reset_if_due(
                cookie.weekly_sonnet_has_reset,
                &mut cookie.weekly_sonnet_resets_at,
                &mut cookie.weekly_sonnet_usage,
                WEEKLY_WINDOW_SECS,
                now,
            );
            cookie_changed
        };

        for cookie in &mut self.valid {
            changed |= apply_resets(cookie);
        }

        if !self.exhausted.is_empty() {
            let mut new_exhausted = HashSet::with_capacity(self.exhausted.len());
            for mut cookie in self.exhausted.drain() {
                changed |= apply_resets(&mut cookie);
                new_exhausted.insert(cookie);
            }
            self.exhausted = new_exhausted;
        }

        changed
    }

    /// Refreshes elapsed usage windows, persisting only if something moved
    fn refresh_usage_windows_and_save(&mut self) {
        if self.refresh_usage_windows() {
            self.save();
        }
    }

    /// Dispatches a cookie for use
    fn dispatch(&mut self, hash: Option<u64>) -> Result<CookieStatus, ClewdrError> {
        self.reset();
        if let Some(hash) = hash
            && let Some(cookie) = self.moka.get(&hash)
            && let Some(cookie) = self.valid.iter().find(|&c| c == &cookie)
        {
            // renew moka cache
            let cookie = cookie.clone();
            self.moka.insert(hash, cookie.clone());
            return Ok(cookie);
        }
        let cookie = self
            .valid
            .pop_front()
            .ok_or(ClewdrError::NoCookieAvailable)?;
        self.valid.push_back(cookie.clone());
        if let Some(hash) = hash {
            self.moka.insert(hash, cookie.clone());
        }
        Ok(cookie)
    }

    /// Collects a returned cookie and processes it based on the return reason
    fn collect(&mut self, mut cookie: CookieStatus, reason: Option<Reason>) {
        let Some(reason) = reason else {
            if let Some(existing) = self.valid.iter_mut().find(|c| **c == cookie) {
                *existing = cookie;
                self.save();
            }
            return;
        };
        match reason {
            Reason::NormalPro => {
                return;
            }
            // Both carry a reset timestamp, so the cookie is only parked.
            Reason::TooManyRequest(i) | Reason::Restricted(i) => {
                self.valid.retain(|c| *c != cookie);
                cookie.reset_time = Some(i);
                cookie.reset_window_usage();
                if !self.exhausted.insert(cookie) {
                    return;
                }
            }
            // Everything else retires the cookie for good.
            _ => {
                self.valid.retain(|c| *c != cookie);
                let mut removed = cookie.clone();
                removed.reset_window_usage();
                if !self
                    .invalid
                    .insert(UselessCookie::new(removed.cookie.clone(), reason))
                {
                    return;
                }
            }
        }
        self.save();
        self.log();
    }

    /// Accepts a new cookie into the valid collection
    fn accept(&mut self, cookie: CookieStatus) {
        if self.valid.contains(&cookie)
            || self.exhausted.contains(&cookie)
            || self.invalid.iter().any(|c| *c == cookie)
        {
            warn!("Cookie already exists");
            return;
        }
        self.valid.push_back(cookie);
        self.save();
        self.log();
    }

    /// Creates a report of all cookie statuses
    fn report(&self) -> CookieStatusInfo {
        CookieStatusInfo {
            valid: self.valid.iter().cloned().collect(),
            exhausted: self.exhausted.iter().cloned().collect(),
            invalid: self.invalid.iter().cloned().collect(),
        }
    }

    /// Deletes a cookie from all collections
    fn delete(&mut self, cookie: &CookieStatus) -> Result<(), ClewdrError> {
        let mut found = false;
        self.valid.retain(|c| {
            found |= c == cookie;
            c != cookie
        });
        let useless = UselessCookie::new(cookie.cookie.clone(), Reason::Null);
        found |= self.exhausted.remove(cookie) | self.invalid.remove(&useless);

        if found {
            self.save();
            self.log();
            Ok(())
        } else {
            Err(ClewdrError::UnexpectedNone {
                msg: "Delete operation did not find the cookie",
            })
        }
    }
}

/// Shared handle to the cookie pool.
///
/// Cheap to clone; every clone talks to the same [`PoolState`]. All critical
/// sections are short and synchronous, so callers never await on the pool.
#[derive(Clone, Debug)]
pub struct CookiePool {
    state: Arc<Mutex<PoolState>>,
}

impl CookiePool {
    /// Loads the pool from the configuration and starts its reset ticker
    #[must_use]
    pub fn start() -> Self {
        let state = PoolState::from_config();
        state.log();
        let pool = Self {
            state: Arc::new(Mutex::new(state)),
        };
        pool.spawn_reset_ticker();
        pool
    }

    /// The pool is only ever poisoned by a panic in one of the short,
    /// side-effect-free critical sections above, so the state stays usable.
    fn lock(&self) -> MutexGuard<'_, PoolState> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Periodically expires usage windows and un-parks cooled-down cookies.
    /// The ticker holds only a [`Weak`] reference, so it stops on its own once
    /// the last pool handle is dropped.
    fn spawn_reset_ticker(&self) {
        let weak: Weak<Mutex<PoolState>> = Arc::downgrade(&self.state);
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(INTERVAL));
            loop {
                interval.tick().await;
                let Some(state) = weak.upgrade() else {
                    break;
                };
                {
                    let mut state = state.lock().unwrap_or_else(PoisonError::into_inner);
                    state.refresh_usage_windows_and_save();
                    state.reset();
                }
            }
        });
    }

    /// Takes a cookie out of the rotation, preferring the one previously
    /// dispatched for `cache_hash` so identical system prompts stay sticky.
    ///
    /// # Errors
    /// [`ClewdrError::NoCookieAvailable`] if the valid pool is empty.
    pub fn request(&self, cache_hash: Option<u64>) -> Result<CookieStatus, ClewdrError> {
        self.lock().dispatch(cache_hash)
    }

    /// Returns a cookie to the pool. `reason` retires or parks it; `None` just
    /// writes back the cookie's updated usage counters.
    pub fn return_cookie(&self, cookie: CookieStatus, reason: Option<Reason>) {
        self.lock().collect(cookie, reason);
    }

    /// Adds a new cookie. Duplicates are ignored with a warning.
    pub fn submit(&self, cookie: CookieStatus) {
        self.lock().accept(cookie);
    }

    /// The pool's cookies in the shape the config file expects, for callers
    /// that need to persist the config without owning the cookies themselves.
    #[must_use]
    pub fn snapshot(&self) -> CookieSnapshot {
        self.lock().snapshot()
    }

    /// Snapshot of every cookie the pool knows about
    #[must_use]
    pub fn status(&self) -> CookieStatusInfo {
        let mut state = self.lock();
        state.refresh_usage_windows_and_save();
        state.report()
    }

    /// Removes a cookie from every collection.
    ///
    /// # Errors
    /// [`ClewdrError::UnexpectedNone`] if the pool has never seen the cookie.
    pub fn delete(&self, cookie: &CookieStatus) -> Result<(), ClewdrError> {
        self.lock().delete(cookie)
    }
}
