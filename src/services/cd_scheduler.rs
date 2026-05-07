use std::collections::HashMap;

use chrono::Utc;
use tracing::{info, warn};

use crate::{
    claude_code_state::ClaudeCodeState,
    config::{CLEWDR_CONFIG, ClewdrCookie},
    services::cookie_actor::CookieActorHandle,
};

const CD_CHECK_INTERVAL_SECS: u64 = 60;

struct WindowState {
    session: Option<i64>,
    weekly: Option<i64>,
    weekly_sonnet: Option<i64>,
    weekly_opus: Option<i64>,
    /// Whether we have already triggered CD for the current expired state.
    /// Prevents duplicate triggers when re-fetch fails and we store expired timestamps.
    triggered_for_current: bool,
}

pub struct CdScheduler;

fn parse_resets_at(usage: &serde_json::Value, key: &str) -> Option<i64> {
    usage
        .get(key)
        .and_then(|o| o.get("resets_at"))
        .and_then(|v| v.as_str())
        .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
        .map(|dt| dt.timestamp())
}

/// Check if a window has expired since last check.
/// Returns true only when the window transitions from "active" (future) to "expired" (past).
fn window_expired_since_last_check(current: Option<i64>, previous: Option<i64>, now: i64) -> bool {
    match (current, previous) {
        // Same timestamp, was in the future, now expired
        (Some(curr), Some(prev)) if curr == prev && now >= curr => true,
        // Timestamp changed and the new one is also expired
        (Some(curr), Some(prev)) if curr != prev && now >= curr => true,
        _ => false,
    }
}

impl CdScheduler {
    pub fn spawn(handle: CookieActorHandle) {
        tokio::spawn(async move {
            let mut triggered: HashMap<ClewdrCookie, WindowState> = HashMap::new();
            let mut interval =
                tokio::time::interval(tokio::time::Duration::from_secs(CD_CHECK_INTERVAL_SECS));
            loop {
                interval.tick().await;
                if !CLEWDR_CONFIG.load().auto_trigger_cd {
                    continue;
                }
                Self::check_and_trigger(&handle, &mut triggered).await;
            }
        });
    }

    async fn check_and_trigger(
        handle: &CookieActorHandle,
        triggered: &mut HashMap<ClewdrCookie, WindowState>,
    ) {
        let status = match handle.get_status().await {
            Ok(s) => s,
            Err(e) => {
                warn!("CD scheduler: failed to get cookie status: {}", e);
                return;
            }
        };

        let valid_keys: Vec<ClewdrCookie> =
            status.valid.iter().map(|c| c.cookie.clone()).collect();

        for cookie in status.valid.iter() {
            Self::check_cookie(handle, triggered, cookie.clone()).await;
        }

        triggered.retain(|k, _| valid_keys.contains(k));
    }

    async fn check_cookie(
        handle: &CookieActorHandle,
        triggered: &mut HashMap<ClewdrCookie, WindowState>,
        cookie: crate::config::CookieStatus,
    ) {
        let cookie_key = cookie.cookie.clone();
        let display_key = cookie.cookie.ellipse();

        let mut state = match ClaudeCodeState::from_cookie(handle.clone(), cookie) {
            Ok(s) => s,
            Err(e) => {
                warn!(
                    "CD scheduler: failed to create state for {}: {}",
                    display_key, e
                );
                return;
            }
        };

        let usage = match state.fetch_usage_metrics().await {
            Ok(u) => u,
            Err(e) => {
                warn!(
                    "CD scheduler: failed to fetch usage for {}: {}",
                    display_key, e
                );
                return;
            }
        };

        let now = Utc::now().timestamp();
        let current = WindowState {
            session: parse_resets_at(&usage, "five_hour"),
            weekly: parse_resets_at(&usage, "seven_day"),
            weekly_sonnet: parse_resets_at(&usage, "seven_day_sonnet"),
            weekly_opus: parse_resets_at(&usage, "seven_day_opus"),
            triggered_for_current: false,
        };

        let needs_trigger = if let Some(prev) = triggered.get(&cookie_key) {
            // If we already triggered and timestamps haven't changed, skip
            if prev.triggered_for_current && Self::same_timestamps(&current, prev) {
                false
            } else {
                window_expired_since_last_check(current.session, prev.session, now)
                    || window_expired_since_last_check(current.weekly, prev.weekly, now)
                    || window_expired_since_last_check(
                        current.weekly_sonnet,
                        prev.weekly_sonnet,
                        now,
                    )
                    || window_expired_since_last_check(current.weekly_opus, prev.weekly_opus, now)
            }
        } else {
            // First time seeing this cookie.
            // Only trigger if the session (five_hour) window has no active timer,
            // meaning the cookie has never been used or its session window has expired.
            // We intentionally check only session to avoid false positives from
            // accounts that legitimately lack other window types.
            current.session.map(|ts| now >= ts).unwrap_or(true)
        };

        if needs_trigger {
            info!("CD scheduler: triggering CD for cookie {}...", display_key);
            match state.trigger_cd().await {
                Ok(_) => {
                    info!(
                        "CD scheduler: trigger sent successfully for {}",
                        display_key
                    );
                }
                Err(e) => {
                    warn!("CD scheduler: trigger failed for {}: {}", display_key, e);
                }
            }
            // Re-fetch usage to get updated timestamps after trigger
            if let Ok(new_usage) = state.fetch_usage_metrics().await {
                let new_state = WindowState {
                    session: parse_resets_at(&new_usage, "five_hour"),
                    weekly: parse_resets_at(&new_usage, "seven_day"),
                    weekly_sonnet: parse_resets_at(&new_usage, "seven_day_sonnet"),
                    weekly_opus: parse_resets_at(&new_usage, "seven_day_opus"),
                    // If timestamps are still expired after trigger, mark as triggered
                    // to prevent duplicate. Will reset when new future timestamps appear.
                    triggered_for_current: true,
                };
                triggered.insert(cookie_key, new_state);
            } else {
                // Re-fetch failed; mark as triggered to prevent duplicate on next tick
                let mut fallback = current;
                fallback.triggered_for_current = true;
                triggered.insert(cookie_key, fallback);
            }
        } else {
            triggered.insert(cookie_key, current);
        }

        // Do NOT call return_cookie — the cookie was obtained from a read-only
        // get_status() snapshot, not borrowed from the actor. Returning it would
        // overwrite concurrent updates (token refreshes, usage counters) made
        // by real requests.
    }

    fn same_timestamps(a: &WindowState, b: &WindowState) -> bool {
        a.session == b.session
            && a.weekly == b.weekly
            && a.weekly_sonnet == b.weekly_sonnet
            && a.weekly_opus == b.weekly_opus
    }
}
