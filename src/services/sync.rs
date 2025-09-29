use std::collections::HashSet;

use crate::{
    config::{CookieStatus, Reason},
    persistence,
    services::{cookie_actor::CookieActorHandle, key_actor::KeyActorHandle},
};

/// Spawn background sync tasks for keys and cookies when DB storage is enabled.
/// Returns join handles if tasks were spawned.
pub fn spawn(
    cookie_handle: CookieActorHandle,
    key_handle: KeyActorHandle,
) -> Option<Vec<tokio::task::JoinHandle<()>>> {
    if !persistence::storage().is_enabled() {
        return None;
    }
    let mut handles = Vec::new();

    // Keys sync (add missing, remove extra)
    let k = key_handle.clone();
    handles.push(tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(30));
        loop {
            interval.tick().await;
            if let Ok(db_keys) = persistence::load_all_keys().await
                && let Ok(cur) = k.get_status().await
            {
                let db_set: HashSet<_> = db_keys.iter().cloned().collect();
                let cur_set: HashSet<_> = cur.valid.iter().cloned().collect();
                for x in db_set.difference(&cur_set) {
                    let _ = k.submit(x.clone()).await;
                }
                for x in cur_set.difference(&db_set) {
                    let _ = k.delete_key(x.clone()).await;
                }
            }
        }
    }));

    // Cookies conservative sync: add missing; reclassify exhausted/invalid; never hard-delete
    let c_handle = cookie_handle.clone();
    handles.push(tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(45));
        loop {
            interval.tick().await;
            let Ok((db_valid, db_exhausted, db_invalid)) = persistence::load_all_cookies().await
            else {
                continue;
            };
            let Ok(cur) = c_handle.get_status().await else {
                continue;
            };
            let cur_valid: HashSet<_> = cur.valid.iter().map(|x| x.cookie.to_string()).collect();
            let cur_exh: HashSet<_> = cur.exhausted.iter().map(|x| x.cookie.to_string()).collect();
            let cur_inv: HashSet<_> = cur.invalid.iter().map(|x| x.cookie.to_string()).collect();

            // Add missing valid cookies
            for v in db_valid.iter() {
                let key = v.cookie.to_string();
                if !(cur_valid.contains(&key) || cur_exh.contains(&key) || cur_inv.contains(&key)) {
                    let _ = c_handle.submit(v.clone()).await;
                }
            }

            // Reclassify exhausted not present as exhausted in actor
            for e in db_exhausted.iter() {
                let key = e.cookie.to_string();
                if !cur_exh.contains(&key) {
                    let ts = e
                        .reset_time
                        .unwrap_or(chrono::Utc::now().timestamp() + 3600);
                    let mut tmp: CookieStatus = e.clone();
                    tmp.reset_time = Some(ts);
                    let _ = c_handle
                        .return_cookie(tmp, Some(Reason::TooManyRequest(ts)))
                        .await;
                }
            }

            // Reclassify invalid not present as invalid in actor
            for u in db_invalid.iter() {
                let key = u.cookie.to_string();
                if !cur_inv.contains(&key) {
                    let tmp = CookieStatus {
                        cookie: u.cookie.clone(),
                        token: None,
                        reset_time: None,
                        supports_claude_1m: None,
                        count_tokens_allowed: None,
                        total_input_tokens: 0,
                        total_output_tokens: 0,
                        window_input_tokens: 0,
                        window_output_tokens: 0,
                    };
                    let _ = c_handle.return_cookie(tmp, Some(u.reason.clone())).await;
                }
            }
        }
    }));

    Some(handles)
}
