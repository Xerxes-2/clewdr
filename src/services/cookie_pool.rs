use std::{
    collections::{HashMap, HashSet, VecDeque},
    sync::{Arc, Mutex, MutexGuard, PoisonError, Weak},
    time::{Duration, Instant},
};

use chrono::Utc;
use colored::Colorize;
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

/// How many prompt hashes keep a remembered cookie
const STICKY_CAPACITY: usize = 1000;
/// How long a remembered cookie survives without being asked for again
const STICKY_IDLE: Duration = Duration::from_hours(1);

/// Remembers which cookie served a given prompt hash, so repeated requests
/// with the same system prompt keep hitting the same upstream account.
///
/// Bounded by both idleness and capacity, because the keys are attacker-
/// influenced: a caller varying the prompt would otherwise grow this forever.
/// Purely an optimisation — a miss costs a different cookie, nothing more — so
/// eviction only has to be approximately least-recently-used, which a linear
/// scan over at most [`STICKY_CAPACITY`] entries gives cheaply enough for a
/// path that already runs under the pool's mutex.
#[derive(Debug)]
struct StickyCookies {
    entries: HashMap<u64, (CookieStatus, Instant)>,
}

impl StickyCookies {
    fn new() -> Self {
        Self {
            entries: HashMap::new(),
        }
    }

    /// The cookie remembered for `hash`, renewing its idle deadline.
    fn get(&mut self, hash: u64) -> Option<CookieStatus> {
        let now = Instant::now();
        let (cookie, last_used) = self.entries.get_mut(&hash)?;
        if now.duration_since(*last_used) >= STICKY_IDLE {
            self.entries.remove(&hash);
            return None;
        }
        *last_used = now;
        Some(cookie.clone())
    }

    /// Remembers `cookie` for `hash`, evicting if that would exceed capacity.
    fn insert(&mut self, hash: u64, cookie: CookieStatus) {
        let now = Instant::now();
        if !self.entries.contains_key(&hash) && self.entries.len() >= STICKY_CAPACITY {
            self.entries
                .retain(|_, (_, last_used)| now.duration_since(*last_used) < STICKY_IDLE);
            if self.entries.len() >= STICKY_CAPACITY
                && let Some(&oldest) = self
                    .entries
                    .iter()
                    .min_by_key(|(_, (_, last_used))| *last_used)
                    .map(|(hash, _)| hash)
            {
                self.entries.remove(&oldest);
            }
        }
        self.entries.insert(hash, (cookie, now));
    }
}

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
    sticky: StickyCookies,
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

        Self {
            valid,
            exhausted,
            invalid,
            sticky: StickyCookies::new(),
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
            && let Some(remembered) = self.sticky.get(hash)
            && let Some(cookie) = self.valid.iter().find(|&c| c == &remembered)
        {
            // renew the sticky entry
            let cookie = cookie.clone();
            self.sticky.insert(hash, cookie.clone());
            return Ok(cookie);
        }
        let cookie = self
            .valid
            .pop_front()
            .ok_or(ClewdrError::NoCookieAvailable)?;
        self.valid.push_back(cookie.clone());
        if let Some(hash) = hash {
            self.sticky.insert(hash, cookie.clone());
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

#[cfg(test)]
mod sticky_tests {
    use super::*;

    /// A distinct, well-formed cookie per `tag`.
    fn cookie(tag: char) -> CookieStatus {
        let raw = format!(
            "sk-ant-sid01-{}-{}AA",
            str::repeat(&tag.to_string(), 86),
            "b".repeat(6)
        );
        CookieStatus::new(&raw, None).expect("valid test cookie")
    }

    #[test]
    fn a_remembered_cookie_comes_back_for_the_same_hash() {
        let mut sticky = StickyCookies::new();
        sticky.insert(1, cookie('a'));

        assert_eq!(sticky.get(1), Some(cookie('a')));
    }

    #[test]
    fn an_unknown_hash_is_a_miss() {
        let mut sticky = StickyCookies::new();
        sticky.insert(1, cookie('a'));

        assert_eq!(sticky.get(2), None);
    }

    #[test]
    fn re_inserting_a_hash_replaces_the_cookie() {
        let mut sticky = StickyCookies::new();
        sticky.insert(1, cookie('a'));
        sticky.insert(1, cookie('b'));

        assert_eq!(sticky.get(1), Some(cookie('b')));
        assert_eq!(sticky.entries.len(), 1);
    }

    /// The keys are prompt hashes, so a caller varying its system prompt
    /// controls how many there are. Without a cap this would grow without
    /// bound for as long as the process runs.
    #[test]
    fn the_map_never_exceeds_its_capacity() {
        let mut sticky = StickyCookies::new();
        for hash in 0..(STICKY_CAPACITY as u64 * 3) {
            sticky.insert(hash, cookie('a'));
        }

        assert!(
            sticky.entries.len() <= STICKY_CAPACITY,
            "grew to {} entries",
            sticky.entries.len()
        );
    }

    /// Overflowing the cap must drop the entry that has gone longest without
    /// being asked for, not an arbitrary one, or a busy prompt would lose its
    /// cookie to a burst of one-off ones.
    #[test]
    fn eviction_takes_the_least_recently_used_entry() {
        let mut sticky = StickyCookies::new();
        for hash in 0..STICKY_CAPACITY as u64 {
            sticky.insert(hash, cookie('a'));
        }
        // Touch the oldest key so it is no longer the least recently used.
        assert!(sticky.get(0).is_some());

        // One more entry than fits, forcing exactly one eviction.
        sticky.insert(u64::MAX, cookie('b'));

        assert!(sticky.get(0).is_some(), "the touched entry must survive");
        assert_eq!(sticky.get(u64::MAX), Some(cookie('b')));
        assert!(sticky.get(1).is_none(), "the untouched oldest must be gone");
    }

    /// Re-inserting an existing key is not growth, so it must not evict.
    #[test]
    fn refreshing_a_full_map_evicts_nothing() {
        let mut sticky = StickyCookies::new();
        for hash in 0..STICKY_CAPACITY as u64 {
            sticky.insert(hash, cookie('a'));
        }

        sticky.insert(0, cookie('b'));

        assert_eq!(sticky.entries.len(), STICKY_CAPACITY);
        assert_eq!(sticky.get(0), Some(cookie('b')));
        assert!(sticky.get(1).is_some(), "no other entry should have gone");
    }

    #[test]
    fn an_idle_entry_is_not_returned() {
        let mut sticky = StickyCookies::new();
        sticky.insert(1, cookie('a'));
        // Backdate the entry past the idle deadline.
        sticky.entries.get_mut(&1).unwrap().1 = Instant::now()
            .checked_sub(STICKY_IDLE + Duration::from_secs(1))
            .expect("the test clock must reach back past the idle window");

        assert_eq!(sticky.get(1), None);
        assert!(
            sticky.entries.is_empty(),
            "the stale entry should be dropped"
        );
    }

    /// Reading an entry renews it, so a prompt that keeps being asked for
    /// keeps its cookie however long the process has been up.
    #[test]
    fn reading_an_entry_renews_its_deadline() {
        let mut sticky = StickyCookies::new();
        sticky.insert(1, cookie('a'));
        // Nearly stale, but not yet.
        sticky.entries.get_mut(&1).unwrap().1 = Instant::now()
            .checked_sub(
                STICKY_IDLE
                    .checked_sub(Duration::from_secs(30))
                    .expect("the idle window is longer than 30s"),
            )
            .expect("the test clock must reach back into the idle window");

        assert!(sticky.get(1).is_some());

        // The read should have reset the clock, leaving a full idle window.
        let age = sticky.entries[&1].1.elapsed();
        assert!(
            age < Duration::from_secs(1),
            "deadline was not renewed: {age:?}"
        );
    }
}
