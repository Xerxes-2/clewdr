use std::collections::{HashSet, VecDeque};

use chrono::Utc;
use colored::Colorize;
use moka::sync::Cache;
use ractor::{Actor, ActorProcessingErr, ActorRef, RpcReplyPort};
use serde::Serialize;
use snafu::{GenerateImplicitData, Location};
use tracing::{error, info, warn};

use crate::{
    config::{CLEWDR_CONFIG, ClewdrConfig, CookieStatus, Reason, UsageBreakdown, UselessCookie},
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

/// Messages that the CookieActor can handle
#[derive(Debug)]
enum CookieActorMessage {
    /// Return a Cookie
    Return(CookieStatus, Option<Reason>),
    /// Submit a new Cookie
    Submit(CookieStatus),
    /// Check for timed out Cookies
    CheckReset,
    /// Request to get a Cookie
    Request(Option<u64>, RpcReplyPort<Result<CookieStatus, ClewdrError>>),
    /// Get all Cookie status information
    GetStatus(RpcReplyPort<CookieStatusInfo>),
    /// Delete a Cookie
    Delete(CookieStatus, RpcReplyPort<Result<(), ClewdrError>>),
    /// Update 1M support flags on an existing cookie
    Update1mSupport(CookieStatus, RpcReplyPort<Result<(), ClewdrError>>),
}

/// CookieActor state - manages collections of cookies
#[derive(Debug)]
struct CookieActorState {
    valid: VecDeque<CookieStatus>,
    exhausted: HashSet<CookieStatus>,
    invalid: HashSet<UselessCookie>,
    moka: Cache<u64, CookieStatus>,
}

/// Cookie actor that handles cookie distribution, collection, and status tracking using Ractor
struct CookieActor;

impl CookieActor {
    /// Saves the current state of cookies to the configuration
    fn save(state: &CookieActorState) {
        CLEWDR_CONFIG.rcu(|config| {
            let mut config = ClewdrConfig::clone(config);
            config.cookie_array = state
                .valid
                .iter()
                .chain(state.exhausted.iter())
                .cloned()
                .collect();
            config.wasted_cookie = state.invalid.clone();
            config
        });

        // Persist config file/DB config row only（不再全量重写 cookies）
        tokio::spawn(async move {
            let result = CLEWDR_CONFIG.load().save().await;
            match result {
                Ok(_) => info!("Configuration saved successfully"),
                Err(e) => error!("Save task panicked: {}", e),
            }
        });
    }

    /// Logs the current state of cookie collections
    fn log(state: &CookieActorState) {
        info!(
            "Valid: {}, Exhausted: {}, Invalid: {}",
            state.valid.len().to_string().green(),
            state.exhausted.len().to_string().yellow(),
            state.invalid.len().to_string().red(),
        );
    }

    /// Checks and resets cookies that have passed their reset time
    fn reset(state: &mut CookieActorState) {
        let mut reset_cookies = Vec::new();
        state.exhausted.retain(|cookie| {
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
        for c in reset_cookies.into_iter() {
            state.valid.push_back(c.clone());
        }
        Self::log(state);
    }

    /// Reset in-memory usage buckets when local reset boundaries have elapsed.
    /// This avoids stale counters when cooldown windows expire between requests.
    fn refresh_usage_windows(state: &mut CookieActorState) -> bool {
        fn reset_if_due(
            has_reset: Option<bool>,
            resets_at: &mut Option<i64>,
            usage: &mut UsageBreakdown,
            window_secs: i64,
            now: i64,
        ) -> bool {
            if has_reset == Some(true) && resets_at.map(|ts| now >= ts).unwrap_or(false) {
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
            cookie_changed |= reset_if_due(
                cookie.weekly_opus_has_reset,
                &mut cookie.weekly_opus_resets_at,
                &mut cookie.weekly_opus_usage,
                WEEKLY_WINDOW_SECS,
                now,
            );
            cookie_changed
        };

        for cookie in state.valid.iter_mut() {
            changed |= apply_resets(cookie);
        }

        if !state.exhausted.is_empty() {
            let mut new_exhausted = HashSet::with_capacity(state.exhausted.len());
            for mut cookie in state.exhausted.drain() {
                changed |= apply_resets(&mut cookie);
                new_exhausted.insert(cookie);
            }
            state.exhausted = new_exhausted;
        }

        changed
    }

    /// Dispatches a cookie for use
    fn dispatch(
        &self,
        state: &mut CookieActorState,
        hash: Option<u64>,
    ) -> Result<CookieStatus, ClewdrError> {
        Self::reset(state);
        if let Some(hash) = hash
            && let Some(cookie) = state.moka.get(&hash)
            && let Some(cookie) = state.valid.iter().find(|&c| c == &cookie)
        {
            // renew moka cache
            state.moka.insert(hash, cookie.clone());
            return Ok(cookie.clone());
        }
        let cookie = state
            .valid
            .pop_front()
            .ok_or(ClewdrError::NoCookieAvailable)?;
        state.valid.push_back(cookie.clone());
        if let Some(hash) = hash {
            state.moka.insert(hash, cookie.clone());
        }
        Ok(cookie)
    }

    /// Collects a returned cookie and processes it based on the return reason
    fn collect(state: &mut CookieActorState, mut cookie: CookieStatus, reason: Option<Reason>) {
        let Some(reason) = reason else {
            if let Some(existing) = state.valid.iter_mut().find(|c| **c == cookie) {
                *existing = cookie;
                Self::save(state);
            }
            return;
        };
        let mut find_remove = |cookie: &CookieStatus| {
            state.valid.retain(|c| c != cookie);
        };
        match reason {
            Reason::NormalPro => {
                return;
            }
            Reason::TooManyRequest(i) => {
                find_remove(&cookie);
                cookie.reset_time = Some(i);
                cookie.reset_window_usage();
                if !state.exhausted.insert(cookie) {
                    return;
                }
            }
            Reason::Restricted(i) => {
                find_remove(&cookie);
                cookie.reset_time = Some(i);
                cookie.reset_window_usage();
                if !state.exhausted.insert(cookie) {
                    return;
                }
            }
            Reason::Free => {
                find_remove(&cookie);
                let mut removed = cookie.clone();
                removed.reset_window_usage();
                if !state
                    .invalid
                    .insert(UselessCookie::new(removed.cookie.clone(), reason))
                {
                    return;
                }
            }
            _ => {
                find_remove(&cookie);
                let mut removed = cookie.clone();
                removed.reset_window_usage();
                if !state
                    .invalid
                    .insert(UselessCookie::new(removed.cookie.clone(), reason))
                {
                    return;
                }
            }
        }
        Self::save(state);
        Self::log(state);
    }

    /// Accepts a new cookie into the valid collection
    fn accept(state: &mut CookieActorState, cookie: CookieStatus) {
        if CLEWDR_CONFIG.load().cookie_array.contains(&cookie)
            || CLEWDR_CONFIG
                .load()
                .wasted_cookie
                .iter()
                .any(|c| *c == cookie)
        {
            warn!("Cookie already exists");
            return;
        }
        state.valid.push_back(cookie);
        Self::save(state);
        Self::log(state);
    }

    /// Creates a report of all cookie statuses
    fn report(state: &CookieActorState) -> CookieStatusInfo {
        CookieStatusInfo {
            valid: state.valid.clone().into(),
            exhausted: state.exhausted.iter().cloned().collect(),
            invalid: state.invalid.iter().cloned().collect(),
        }
    }

    /// Deletes a cookie from all collections
    fn delete(state: &mut CookieActorState, cookie: CookieStatus) -> Result<(), ClewdrError> {
        let mut found = false;
        state.valid.retain(|c| {
            found |= *c == cookie;
            *c != cookie
        });
        let useless = UselessCookie::new(cookie.cookie.clone(), Reason::Null);
        found |= state.exhausted.remove(&cookie) | state.invalid.remove(&useless);

        if found {
            Self::save(state);
            Self::log(state);
            Ok(())
        } else {
            Err(ClewdrError::UnexpectedNone {
                msg: "Delete operation did not find the cookie",
            })
        }
    }

    /// Updates 1M support flags for an existing cookie in valid/exhausted collections
    fn update_1m_support(
        state: &mut CookieActorState,
        cookie: CookieStatus,
    ) -> Result<(), ClewdrError> {
        if let Some(existing) = state.valid.iter_mut().find(|c| **c == cookie) {
            existing.supports_claude_1m_sonnet = cookie.supports_claude_1m_sonnet;
            existing.supports_claude_1m_opus = cookie.supports_claude_1m_opus;
            Self::save(state);
            return Ok(());
        }

        if !state.exhausted.is_empty() {
            let mut updated = false;
            let mut new_exhausted = HashSet::with_capacity(state.exhausted.len());
            for mut existing in state.exhausted.drain() {
                if existing == cookie {
                    existing.supports_claude_1m_sonnet = cookie.supports_claude_1m_sonnet;
                    existing.supports_claude_1m_opus = cookie.supports_claude_1m_opus;
                    updated = true;
                }
                new_exhausted.insert(existing);
            }
            state.exhausted = new_exhausted;
            if updated {
                Self::save(state);
                return Ok(());
            }
        }

        Err(ClewdrError::UnexpectedNone {
            msg: "Update operation did not find the cookie",
        })
    }
}

impl Actor for CookieActor {
    type Msg = CookieActorMessage;
    type State = CookieActorState;
    type Arguments = ();

    async fn pre_start(
        &self,
        _myself: ActorRef<Self::Msg>,
        _arguments: Self::Arguments,
    ) -> Result<Self::State, ActorProcessingErr> {
        let normalize_1m_defaults = |mut cookie: CookieStatus| {
            if cookie.supports_claude_1m_sonnet.is_none() {
                cookie.supports_claude_1m_sonnet = Some(true);
            }
            if cookie.supports_claude_1m_opus.is_none() {
                cookie.supports_claude_1m_opus = Some(true);
            }
            cookie
        };

        let valid = VecDeque::from_iter(
            CLEWDR_CONFIG
                .load()
                .cookie_array
                .iter()
                .filter(|c| c.reset_time.is_none())
                .cloned()
                .map(normalize_1m_defaults),
        );
        let exhausted = HashSet::from_iter(
            CLEWDR_CONFIG
                .load()
                .cookie_array
                .iter()
                .filter(|c| c.reset_time.is_some())
                .cloned()
                .map(normalize_1m_defaults),
        );
        let invalid = HashSet::from_iter(CLEWDR_CONFIG.load().wasted_cookie.iter().cloned());

        let moka = Cache::builder()
            .max_capacity(1000)
            .time_to_idle(std::time::Duration::from_secs(60 * 60))
            .build();

        let state = CookieActorState {
            valid,
            exhausted,
            invalid,
            moka,
        };

        CookieActor::log(&state);
        Ok(state)
    }

    async fn handle(
        &self,
        _myself: ActorRef<Self::Msg>,
        message: Self::Msg,
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        match message {
            CookieActorMessage::Return(cookie, reason) => {
                Self::collect(state, cookie, reason);
            }
            CookieActorMessage::Submit(cookie) => {
                Self::accept(state, cookie);
            }
            CookieActorMessage::CheckReset => {
                let changed = Self::refresh_usage_windows(state);
                if changed {
                    Self::save(state);
                }
                Self::reset(state);
            }
            CookieActorMessage::Request(cache_hash, reply_port) => {
                let result = self.dispatch(state, cache_hash);
                reply_port.send(result)?;
            }
            CookieActorMessage::GetStatus(reply_port) => {
                let changed = Self::refresh_usage_windows(state);
                if changed {
                    Self::save(state);
                }
                let status_info = Self::report(state);
                reply_port.send(status_info)?;
            }
            CookieActorMessage::Delete(cookie, reply_port) => {
                let result = Self::delete(state, cookie.clone());
                reply_port.send(result)?;
            }
            CookieActorMessage::Update1mSupport(cookie, reply_port) => {
                let result = Self::update_1m_support(state, cookie);
                reply_port.send(result)?;
            }
        }
        Ok(())
    }

    async fn post_stop(
        &self,
        _myself: ActorRef<Self::Msg>,
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        CookieActor::save(state);
        Ok(())
    }
}

/// Handle for interacting with the CookieActor
#[derive(Clone)]
pub struct CookieActorHandle {
    actor_ref: ActorRef<CookieActorMessage>,
}

impl CookieActorHandle {
    /// Create a new CookieActor and return a handle to it
    pub async fn start() -> Result<Self, ractor::SpawnErr> {
        let (actor_ref, _join_handle) = Actor::spawn(None, CookieActor, ()).await?;

        // Start the timeout checker
        let handle = Self {
            actor_ref: actor_ref.clone(),
        };
        handle.spawn_timeout_checker().await;

        Ok(handle)
    }

    /// Spawns a timeout checker task
    async fn spawn_timeout_checker(&self) {
        let actor_ref = self.actor_ref.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(INTERVAL));
            loop {
                interval.tick().await;
                if ractor::cast!(actor_ref, CookieActorMessage::CheckReset).is_err() {
                    break;
                }
            }
        });
    }

    /// Request a cookie from the cookie actor
    pub async fn request(&self, cache_hash: Option<u64>) -> Result<CookieStatus, ClewdrError> {
        ractor::call!(self.actor_ref, CookieActorMessage::Request, cache_hash).map_err(|e| {
            ClewdrError::RactorError {
                loc: Location::generate(),
                msg: format!("Failed to communicate with CookieActor for request operation: {e}"),
            }
        })?
    }

    /// Return a cookie to the cookie actor
    pub async fn return_cookie(
        &self,
        cookie: CookieStatus,
        reason: Option<Reason>,
    ) -> Result<(), ClewdrError> {
        ractor::cast!(self.actor_ref, CookieActorMessage::Return(cookie, reason)).map_err(|e| {
            ClewdrError::RactorError {
                loc: Location::generate(),
                msg: format!("Failed to communicate with CookieActor for return operation: {e}"),
            }
        })
    }

    /// Submit a new cookie to the cookie actor
    pub async fn submit(&self, cookie: CookieStatus) -> Result<(), ClewdrError> {
        ractor::cast!(self.actor_ref, CookieActorMessage::Submit(cookie)).map_err(|e| {
            ClewdrError::RactorError {
                loc: Location::generate(),
                msg: format!("Failed to communicate with CookieActor for submit operation: {e}"),
            }
        })
    }

    /// Get status information about all cookies
    pub async fn get_status(&self) -> Result<CookieStatusInfo, ClewdrError> {
        ractor::call!(self.actor_ref, CookieActorMessage::GetStatus).map_err(|e| {
            ClewdrError::RactorError {
                loc: Location::generate(),
                msg: format!(
                    "Failed to communicate with CookieActor for get status operation: {e}"
                ),
            }
        })
    }

    /// Delete a cookie from the cookie actor
    pub async fn delete_cookie(&self, cookie: CookieStatus) -> Result<(), ClewdrError> {
        ractor::call!(self.actor_ref, CookieActorMessage::Delete, cookie).map_err(|e| {
            ClewdrError::RactorError {
                loc: Location::generate(),
                msg: format!("Failed to communicate with CookieActor for delete operation: {e}"),
            }
        })?
    }

    /// Update 1M support flags on an existing cookie
    pub async fn update_cookie_1m_support(&self, cookie: CookieStatus) -> Result<(), ClewdrError> {
        ractor::call!(self.actor_ref, CookieActorMessage::Update1mSupport, cookie).map_err(|e| {
            ClewdrError::RactorError {
                loc: Location::generate(),
                msg: format!("Failed to communicate with CookieActor for update operation: {e}"),
            }
        })?
    }
}
