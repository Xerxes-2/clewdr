use serde_json::json;
use tracing::error;

use sea_orm::{entity::prelude::*, ActiveValue::Set, sea_query};

use crate::config::{ClewdrConfig, CookieStatus, KeyStatus, UselessCookie};
use crate::error::ClewdrError;

use super::{conn::ensure_conn, entities::*, metrics::*};

pub async fn bootstrap_from_db_if_enabled() -> Result<(), ClewdrError> {
    if !crate::config::CLEWDR_CONFIG.load().is_db_mode() { return Ok(()); }
    let db = ensure_conn().await?;
    if let Ok(Some(row)) = EntityConfig::find_by_id("main").one(&db).await {
        match toml::from_str::<ClewdrConfig>(&row.data) {
            Ok(mut cfg) => { cfg = cfg.validate(); crate::config::CLEWDR_CONFIG.store(std::sync::Arc::new(cfg)); }
            Err(e) => { error!("Failed to parse config from DB: {}", e); }
        }
    }
    Ok(())
}

pub async fn persist_config(config: &ClewdrConfig) -> Result<(), ClewdrError> {
    if !crate::config::CLEWDR_CONFIG.load().is_db_mode() { return Ok(()); }
    let db = ensure_conn().await?;
    let data = toml::to_string_pretty(config)?;
    use sea_orm::sea_query::OnConflict;
    let am = ActiveModelConfig { k: Set("main".to_string()), data: Set(data), updated_at: Set(Some(chrono::Utc::now().timestamp())), ..Default::default() };
    let start = std::time::Instant::now();
    let res = EntityConfig::insert(am)
        .on_conflict(OnConflict::column(ColumnConfig::K).update_columns([ColumnConfig::Data, ColumnConfig::UpdatedAt]).to_owned())
        .exec(&db).await;
    match res {
        Ok(_) => { record_duration(start); mark_write_ok(); }
        Err(e) => { record_error_msg(&e); mark_write_err(); return Err(ClewdrError::Whatever { message: "save_config".into(), source: Some(Box::new(e)) }); }
    }
    Ok(())
}

pub async fn persist_cookie_upsert(c: &CookieStatus) -> Result<(), ClewdrError> {
    if !crate::config::CLEWDR_CONFIG.load().is_db_mode() { return Ok(()); }
    let db = ensure_conn().await?;
    use sea_orm::sea_query::OnConflict;
    let (acc, rtk, exp_at, exp_in, org) = if let Some(t) = &c.token {
        (Some(t.access_token.clone()), Some(t.refresh_token.clone()), Some(t.expires_at.timestamp()), Some(t.expires_in.as_secs() as i64), Some(t.organization.uuid.clone()))
    } else { (None, None, None, None, None) };
    let am = ActiveModelCookie {
        cookie: Set(c.cookie.to_string()),
        reset_time: Set(c.reset_time),
        token_access: Set(acc),
        token_refresh: Set(rtk),
        token_expires_at: Set(exp_at),
        token_expires_in: Set(exp_in),
        token_org_uuid: Set(org),
    };
    let start = std::time::Instant::now();
    let res = EntityCookie::insert(am)
        .on_conflict(OnConflict::column(ColumnCookie::Cookie).update_columns([
            ColumnCookie::ResetTime,
            ColumnCookie::TokenAccess,
            ColumnCookie::TokenRefresh,
            ColumnCookie::TokenExpiresAt,
            ColumnCookie::TokenExpiresIn,
            ColumnCookie::TokenOrgUuid,
        ]).to_owned())
        .exec(&db).await;
    match res {
        Ok(_) => { record_duration(start); mark_write_ok(); }
        Err(e) => { record_error_msg(&e); mark_write_err(); return Err(ClewdrError::Whatever { message: "upsert_cookie".into(), source: Some(Box::new(e)) }); }
    }
    Ok(())
}

pub async fn delete_cookie_row(c: &CookieStatus) -> Result<(), ClewdrError> {
    if !crate::config::CLEWDR_CONFIG.load().is_db_mode() { return Ok(()); }
    let db = ensure_conn().await?;
    let start = std::time::Instant::now();
    let res = EntityCookie::delete_by_id(c.cookie.to_string()).exec(&db).await;
    match res {
        Ok(_) => { record_duration(start); mark_write_ok(); }
        Err(e) => { record_error_msg(&e); mark_write_err(); return Err(ClewdrError::Whatever { message: "delete_cookie".into(), source: Some(Box::new(e)) }); }
    }
    EntityWasted::delete_by_id(c.cookie.to_string()).exec(&db).await.ok();
    Ok(())
}

pub async fn persist_wasted_upsert(u: &UselessCookie) -> Result<(), ClewdrError> {
    if !crate::config::CLEWDR_CONFIG.load().is_db_mode() { return Ok(()); }
    let db = ensure_conn().await?;
    use sea_orm::sea_query::OnConflict;
    let am = ActiveModelWasted { cookie: Set(u.cookie.to_string()), reason: Set(serde_json::to_string(&u.reason).unwrap_or_else(|_| "\"Unknown\"".to_string())) };
    let start = std::time::Instant::now();
    let res = EntityWasted::insert(am)
        .on_conflict(OnConflict::column(ColumnWasted::Cookie).update_columns([ColumnWasted::Reason]).to_owned())
        .exec(&db).await;
    match res {
        Ok(_) => { record_duration(start); mark_write_ok(); }
        Err(e) => { record_error_msg(&e); mark_write_err(); return Err(ClewdrError::Whatever { message: "upsert_wasted".into(), source: Some(Box::new(e)) }); }
    }
    Ok(())
}

pub async fn persist_keys(keys: &[KeyStatus]) -> Result<(), ClewdrError> {
    if !crate::config::CLEWDR_CONFIG.load().is_db_mode() { return Ok(()); }
    let db = ensure_conn().await?;
    EntityKeyRow::delete_many().exec(&db).await.ok(); // bulk reset (non-critical errors ignored)
    for k in keys {
        let am = ActiveModelKeyRow { key: Set(k.key.to_string()), count_403: Set(k.count_403 as i64) };
        let start = std::time::Instant::now();
        match EntityKeyRow::insert(am).exec(&db).await {
            Ok(_) => { record_duration(start); mark_write_ok(); }
            Err(e) => { record_error_msg(&e); mark_write_err(); error!("insert key failed: {}", e); }
        }
    }
    Ok(())
}

pub async fn persist_key_upsert(k: &KeyStatus) -> Result<(), ClewdrError> {
    if !crate::config::CLEWDR_CONFIG.load().is_db_mode() { return Ok(()); }
    let db = ensure_conn().await?;
    use sea_orm::sea_query::OnConflict;
    let am = ActiveModelKeyRow { key: Set(k.key.to_string()), count_403: Set(k.count_403 as i64) };
    let start = std::time::Instant::now();
    let res = EntityKeyRow::insert(am)
        .on_conflict(OnConflict::column(ColumnKeyRow::Key).update_columns([ColumnKeyRow::Count403]).to_owned())
        .exec(&db).await;
    match res {
        Ok(_) => { record_duration(start); mark_write_ok(); }
        Err(e) => { record_error_msg(&e); mark_write_err(); return Err(ClewdrError::Whatever { message: "upsert_key".into(), source: Some(Box::new(e)) }); }
    }
    Ok(())
}

pub async fn delete_key_row(k: &KeyStatus) -> Result<(), ClewdrError> {
    if !crate::config::CLEWDR_CONFIG.load().is_db_mode() { return Ok(()); }
    let db = ensure_conn().await?;
    let start = std::time::Instant::now();
    let res = EntityKeyRow::delete_by_id(k.key.to_string()).exec(&db).await;
    match res {
        Ok(_) => { record_duration(start); mark_write_ok(); }
        Err(e) => { record_error_msg(&e); mark_write_err(); return Err(ClewdrError::Whatever { message: "delete_key".into(), source: Some(Box::new(e)) }); }
    }
    Ok(())
}

pub async fn import_config_from_file() -> Result<serde_json::Value, ClewdrError> {
    let text = tokio::fs::read_to_string(crate::config::CONFIG_PATH.as_path()).await?;
    let cfg: ClewdrConfig = toml::from_str(&text)?;
    persist_config(&cfg).await?;
    let mut valid = vec![]; let mut exhausted = vec![];
    for c in cfg.cookie_array.iter().cloned() { if c.reset_time.is_some() { exhausted.push(c) } else { valid.push(c) } }
    let invalid: Vec<UselessCookie> = cfg.wasted_cookie.iter().cloned().collect();
    persist_cookies(&valid, &exhausted, &invalid).await?;
    let keys: Vec<KeyStatus> = cfg.gemini_keys.iter().cloned().collect();
    persist_keys(&keys).await?;
    Ok(json!({"status":"ok"}))
}

pub async fn export_config_to_file() -> Result<serde_json::Value, ClewdrError> {
    // Reconstruct latest runtime config from DB rows
    let db = ensure_conn().await?;
    // base config from DB row or current
    let mut cfg = if let Ok(Some(row)) = EntityConfig::find_by_id("main").one(&db).await {
        toml::from_str::<ClewdrConfig>(&row.data).unwrap_or_else(|_| crate::config::CLEWDR_CONFIG.load().as_ref().clone())
    } else { crate::config::CLEWDR_CONFIG.load().as_ref().clone() };
    // cookies
    let cookie_rows = EntityCookie::find().all(&db).await.unwrap_or_default();
    cfg.cookie_array.clear();
    for r in cookie_rows {
        let mut c = CookieStatus::new(&r.cookie, r.reset_time).unwrap_or_default();
        if let Some(acc) = r.token_access {
            let expires_at = r.token_expires_at.and_then(|s| chrono::DateTime::from_timestamp(s, 0)).unwrap_or_else(|| chrono::Utc::now());
            let expires_in = std::time::Duration::from_secs(r.token_expires_in.unwrap_or_default() as u64);
            c.token = Some(crate::config::TokenInfo {
                access_token: acc,
                refresh_token: r.token_refresh.unwrap_or_default(),
                organization: crate::config::Organization { uuid: r.token_org_uuid.unwrap_or_default() },
                expires_at,
                expires_in,
            });
        }
        cfg.cookie_array.insert(c);
    }
    // wasted
    let wasted_rows = EntityWasted::find().all(&db).await.unwrap_or_default();
    cfg.wasted_cookie.clear();
    for r in wasted_rows {
        if let Ok(reason) = serde_json::from_str(&r.reason) {
            if let Ok(cc) = <crate::config::ClewdrCookie as std::str::FromStr>::from_str(&r.cookie) {
                cfg.wasted_cookie.insert(UselessCookie::new(cc, reason));
            }
        }
    }
    // keys
    let key_rows = EntityKeyRow::find().all(&db).await.unwrap_or_default();
    cfg.gemini_keys.clear();
    for r in key_rows { cfg.gemini_keys.insert(KeyStatus { key: r.key.into(), count_403: r.count_403 as u32 }); }

    // write file
    if let Some(parent) = crate::config::CONFIG_PATH.parent() { if !parent.exists() { tokio::fs::create_dir_all(parent).await?; } }
    tokio::fs::write(crate::config::CONFIG_PATH.as_path(), toml::to_string_pretty(&cfg)?).await?;
    Ok(json!({"status":"ok"}))
}

pub async fn persist_cookies(valid: &[CookieStatus], exhausted: &[CookieStatus], invalid: &[UselessCookie]) -> Result<(), ClewdrError> {
    if !crate::config::CLEWDR_CONFIG.load().is_db_mode() { return Ok(()); }
    let db = ensure_conn().await?;
    EntityCookie::delete_many().exec(&db).await.ok();
    EntityWasted::delete_many().exec(&db).await.ok();
    for c in valid.iter().chain(exhausted.iter()) { let _ = persist_cookie_upsert(c).await; }
    for u in invalid { let _ = persist_wasted_upsert(u).await; }
    Ok(())
}

// Read helpers used by background sync
pub async fn load_all_keys() -> Result<Vec<KeyStatus>, ClewdrError> {
    let db = ensure_conn().await?;
    let rows = EntityKeyRow::find().all(&db).await
        .map_err(|e| ClewdrError::Whatever { message: "load_keys".into(), source: Some(Box::new(e)) })?;
    Ok(rows.into_iter().map(|r| KeyStatus { key: r.key.into(), count_403: r.count_403 as u32 }).collect())
}

pub async fn load_all_cookies() -> Result<(Vec<CookieStatus>, Vec<CookieStatus>, Vec<UselessCookie>), ClewdrError> {
    let db = ensure_conn().await?;
    let mut valid = Vec::new();
    let mut exhausted = Vec::new();
    let mut invalid = Vec::new();
    let rows = EntityCookie::find().all(&db).await
        .map_err(|e| ClewdrError::Whatever { message: "load_cookies".into(), source: Some(Box::new(e)) })?;
    for r in rows {
        let mut c = CookieStatus::new(&r.cookie, r.reset_time).unwrap_or_default();
        if let Some(acc) = r.token_access {
            let expires_at = r.token_expires_at.and_then(|s| chrono::DateTime::from_timestamp(s, 0)).unwrap_or_else(|| chrono::Utc::now());
            let expires_in = std::time::Duration::from_secs(r.token_expires_in.unwrap_or_default() as u64);
            c.token = Some(crate::config::TokenInfo { access_token: acc, refresh_token: r.token_refresh.unwrap_or_default(), organization: crate::config::Organization { uuid: r.token_org_uuid.unwrap_or_default() }, expires_at, expires_in });
        }
        if c.reset_time.is_some() { exhausted.push(c); } else { valid.push(c); }
    }
    let wasted = EntityWasted::find().all(&db).await
        .map_err(|e| ClewdrError::Whatever { message: "load_wasted".into(), source: Some(Box::new(e)) })?;
    for r in wasted {
        if let Ok(reason) = serde_json::from_str(&r.reason) {
            if let Ok(cc) = <crate::config::ClewdrCookie as std::str::FromStr>::from_str(&r.cookie) {
                invalid.push(UselessCookie::new(cc, reason));
            }
        }
    }
    Ok((valid, exhausted, invalid))
}

pub async fn status_json() -> Result<serde_json::Value, ClewdrError> {
    use sea_orm::{DatabaseBackend, Statement};
    use std::sync::atomic::Ordering;

    let mut healthy = false;
    let mut err_str: Option<String> = None;
    let mut latency_ms: Option<u128> = None;
    match ensure_conn().await {
        Ok(db) => {
            let backend = db.get_database_backend();
            let stmt = match backend {
                DatabaseBackend::Postgres => Statement::from_string(backend, "SELECT 1"),
                DatabaseBackend::MySql => Statement::from_string(backend, "SELECT 1"),
                DatabaseBackend::Sqlite => Statement::from_string(backend, "SELECT 1"),
            };
            let start = std::time::Instant::now();
            match db.execute(stmt).await {
                Ok(_) => { healthy = true; latency_ms = Some(start.elapsed().as_millis()); }
                Err(e) => { err_str = Some(e.to_string()); }
            }
        }
        Err(e) => { err_str = Some(e.to_string()); }
    }
    let total = TOTAL_WRITES.load(Ordering::Relaxed);
    let errors = WRITE_ERROR_COUNT.load(Ordering::Relaxed);
    let nanos = TOTAL_WRITES.load(Ordering::Relaxed);
    let avg_ms = if total > 0 { (nanos as f64 / total as f64) / 1_000_000.0 } else { 0.0 };
    let ratio = if total > 0 { errors as f64 / total as f64 } else { 0.0 };
    let last_error = LAST_ERROR.lock().ok().and_then(|g| g.clone());
    Ok(json!({
        "enabled": true,
        "mode": "db",
        "healthy": healthy,
        "latency_ms": latency_ms,
        "last_write_ts": LAST_WRITE_TS.load(Ordering::Relaxed),
        "write_error_count": errors,
        "total_writes": total,
        "avg_write_ms": avg_ms,
        "failure_ratio": ratio,
        "error": err_str,
        "last_error": last_error,
    }))
}

