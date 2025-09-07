use std::sync::LazyLock;

use serde_json::json;
#[cfg(feature = "db")]
use tracing::error;

use crate::config::{ClewdrConfig, CookieStatus, KeyStatus, UselessCookie};
use crate::error::ClewdrError;

// Public facade: when DB feature is disabled, provide no-op file-based stubs

/// Storage abstraction for Clewdr persistent state.
/// Implementations may back onto a database or the filesystem.
pub trait StorageLayer: Send + Sync + 'static {
    fn is_enabled(&self) -> bool;
    fn spawn_bootstrap(&self) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), ClewdrError>> + Send>>;
    fn persist_config(&self, cfg: &ClewdrConfig) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), ClewdrError>> + Send>>;
    fn persist_cookies(
        &self,
        valid: &[CookieStatus],
        exhausted: &[CookieStatus],
        invalid: &[UselessCookie],
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), ClewdrError>> + Send>>;
    fn persist_keys(
        &self,
        keys: &[KeyStatus],
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), ClewdrError>> + Send>>;
    fn persist_cookie_upsert(
        &self,
        c: &CookieStatus,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), ClewdrError>> + Send>>;
    fn delete_cookie_row(
        &self,
        c: &CookieStatus,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), ClewdrError>> + Send>>;
    fn persist_wasted_upsert(
        &self,
        u: &UselessCookie,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), ClewdrError>> + Send>>;
    fn persist_key_upsert(
        &self,
        k: &KeyStatus,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), ClewdrError>> + Send>>;
    fn delete_key_row(
        &self,
        k: &KeyStatus,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), ClewdrError>> + Send>>;
    fn import_from_file(&self) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<serde_json::Value, ClewdrError>> + Send>>;
    fn export_to_file(&self) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<serde_json::Value, ClewdrError>> + Send>>;
    fn status(&self) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<serde_json::Value, ClewdrError>> + Send>>;
}

struct FileLayer;

impl StorageLayer for FileLayer {
    fn is_enabled(&self) -> bool {
        false
    }
    fn spawn_bootstrap(&self) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), ClewdrError>> + Send>> {
        Box::pin(async { Ok(()) })
    }
    fn persist_config(&self, _cfg: &ClewdrConfig) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), ClewdrError>> + Send>> {
        Box::pin(async { Ok(()) })
    }
    fn persist_cookies(
        &self,
        _valid: &[CookieStatus],
        _exhausted: &[CookieStatus],
        _invalid: &[UselessCookie],
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), ClewdrError>> + Send>> {
        Box::pin(async { Ok(()) })
    }
    fn persist_keys(
        &self,
        _keys: &[KeyStatus],
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), ClewdrError>> + Send>> {
        Box::pin(async { Ok(()) })
    }
    fn persist_cookie_upsert(&self, _c: &CookieStatus) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), ClewdrError>> + Send>> {
        Box::pin(async { Ok(()) })
    }
    fn delete_cookie_row(&self, _c: &CookieStatus) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), ClewdrError>> + Send>> {
        Box::pin(async { Ok(()) })
    }
    fn persist_wasted_upsert(&self, _u: &UselessCookie) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), ClewdrError>> + Send>> {
        Box::pin(async { Ok(()) })
    }
    fn persist_key_upsert(&self, _k: &KeyStatus) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), ClewdrError>> + Send>> {
        Box::pin(async { Ok(()) })
    }
    fn delete_key_row(&self, _k: &KeyStatus) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), ClewdrError>> + Send>> {
        Box::pin(async { Ok(()) })
    }
    fn import_from_file(&self) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<serde_json::Value, ClewdrError>> + Send>> {
        Box::pin(async {
            Err(ClewdrError::PathNotFound { msg: "DB feature not enabled".into() })
        })
    }
    fn export_to_file(&self) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<serde_json::Value, ClewdrError>> + Send>> {
        Box::pin(async {
            Err(ClewdrError::PathNotFound { msg: "DB feature not enabled".into() })
        })
    }
    fn status(&self) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<serde_json::Value, ClewdrError>> + Send>> {
        Box::pin(async {
            Ok(json!({
                "enabled": false,
                "mode": "file",
                "healthy": false,
            }))
        })
    }
}

static STORAGE: LazyLock<std::sync::Arc<dyn StorageLayer>> = LazyLock::new(|| {
    #[cfg(feature = "db")]
    {
        if crate::config::CLEWDR_CONFIG.load().is_db_mode() {
            return std::sync::Arc::new(internal::DbLayer);
        }
    }
    std::sync::Arc::new(FileLayer)
});

pub fn storage() -> &'static dyn StorageLayer {
    &**STORAGE
}

#[cfg(not(feature = "db"))]
mod internal {
    use super::*;

    pub async fn bootstrap_from_db_if_enabled() -> Result<(), ClewdrError> {
        // DB feature not compiled; nothing to do
        Ok(())
    }

    pub async fn persist_cookies(
        _valid: &[CookieStatus],
        _exhausted: &[CookieStatus],
        _invalid: &[UselessCookie],
    ) -> Result<(), ClewdrError> {
        Ok(())
    }

    pub async fn persist_keys(_keys: &[KeyStatus]) -> Result<(), ClewdrError> {
        Ok(())
    }

    pub async fn persist_config(_config: &ClewdrConfig) -> Result<(), ClewdrError> {
        Ok(())
    }

    pub async fn import_config_from_file() -> Result<serde_json::Value, ClewdrError> {
        Ok(json!({
            "status": "db_feature_not_enabled",
        }))
    }

    pub async fn export_config_to_file() -> Result<serde_json::Value, ClewdrError> {
        Ok(json!({
            "status": "db_feature_not_enabled",
        }))
    }
}

#[cfg(feature = "db")]
mod internal {
    use std::{str::FromStr, time::Duration};

    use super::*;
    use crate::config::{CLEWDR_CONFIG};
    use sqlx::{AnyPool};
    use std::sync::{atomic::{AtomicI64, AtomicU64, Ordering}, Mutex};
    use std::time::Instant;

    static POOL: LazyLock<std::sync::Mutex<Option<AnyPool>>> = LazyLock::new(|| std::sync::Mutex::new(None));

    fn pool() -> Option<AnyPool> {
        POOL.lock().ok().and_then(|g| g.clone())
    }

    async fn ensure_pool() -> Result<AnyPool, ClewdrError> {
        if let Some(p) = pool() {
            return Ok(p);
        }
        // Ensure Any drivers are registered for non-binary contexts (tests, libs)
        sqlx::any::install_default_drivers();
        let cfg = CLEWDR_CONFIG.load();
        if !cfg.is_db_mode() {
            return Err(ClewdrError::Whatever {
                message: "DB mode not enabled".into(),
                source: None,
            });
        }
        // Derive URL: prefer explicit database_url; for sqlite, allow sqlite_path
        let url = cfg
            .database_url()
            .or_else(|| std::env::var("CLEWDR_DATABASE_URL").ok())
            .ok_or(ClewdrError::UnexpectedNone {
                msg: "Database URL not provided",
            })?;
        // Best-effort create parent dir for sqlite file URLs
        if url.starts_with("sqlite://") {
            let path = &url["sqlite://".len()..];
            // handle absolute and relative paths
            let path = std::path::Path::new(path);
            if let Some(parent) = path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
        }

        let pool = sqlx::any::AnyPoolOptions::new()
            .max_connections(8)
            .idle_timeout(Duration::from_secs(300))
            .after_connect(|conn, _meta| {
                Box::pin(async move {
                    // pragmas for sqlite performance (safe defaults). Ignore errors on other drivers
                    let _ = sqlx::query("PRAGMA journal_mode=WAL;").execute(&mut *conn).await;
                    let _ = sqlx::query("PRAGMA synchronous=NORMAL;").execute(&mut *conn).await;
                    Ok(())
                })
            })
            .connect(&url)
            .await
            .map_err(|e| ClewdrError::Whatever { message: "db_connect".into(), source: Some(Box::new(e)) })?;

        // Set global
        if let Ok(mut g) = POOL.lock() {
            *g = Some(pool.clone());
        }
        // Migrate minimal schema
        migrate(&pool).await?;
        Ok(pool)
    }

    async fn migrate(pool: &AnyPool) -> Result<(), ClewdrError> {
        // schema_migrations for explicit migrations
        sqlx::query("CREATE TABLE IF NOT EXISTS schema_migrations (version TEXT PRIMARY KEY, applied_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP)")
            .execute(pool)
            .await
            .map_err(|e| ClewdrError::Whatever { message: "migrate_migrations".into(), source: Some(Box::new(e)) })?;

        // define migrations
        struct Mig(&'static str, &'static [&'static str]);
        let m1 = Mig("001_init", &[
            "CREATE TABLE IF NOT EXISTS clewdr_config (k TEXT PRIMARY KEY, data TEXT NOT NULL, updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP)",
            "CREATE TABLE IF NOT EXISTS cookies (cookie TEXT PRIMARY KEY, reset_time INTEGER NULL)",
            "CREATE TABLE IF NOT EXISTS wasted_cookies (cookie TEXT PRIMARY KEY, reason TEXT NOT NULL)",
            "CREATE TABLE IF NOT EXISTS keys (key TEXT PRIMARY KEY, count_403 INTEGER NOT NULL)",
        ]);
        let m2 = Mig("002_cookies_token_columns", &[
            "ALTER TABLE cookies ADD COLUMN token_access TEXT",
            "ALTER TABLE cookies ADD COLUMN token_refresh TEXT",
            "ALTER TABLE cookies ADD COLUMN token_expires_at INTEGER",
            "ALTER TABLE cookies ADD COLUMN token_expires_in INTEGER",
            "ALTER TABLE cookies ADD COLUMN token_org_uuid TEXT",
        ]);
        let m3 = Mig("003_indexes", &[
            "CREATE INDEX IF NOT EXISTS idx_cookies_org_uuid ON cookies(token_org_uuid)",
            "CREATE INDEX IF NOT EXISTS idx_cookies_reset ON cookies(reset_time)",
            "CREATE INDEX IF NOT EXISTS idx_keys_count ON keys(count_403)",
        ]);

        for Mig(ver, ddls) in [m1, m2, m3] {
            let sel = match *DB_KIND {
                DbKind::Postgres => "SELECT version FROM schema_migrations WHERE version = $1",
                _ => "SELECT version FROM schema_migrations WHERE version = ?",
            };
            let applied = sqlx::query_scalar::<_, Option<String>>(sel)
                .bind(ver)
                .fetch_optional(pool)
                .await
                .map_err(|e| ClewdrError::Whatever { message: "check_migration".into(), source: Some(Box::new(e)) })?;
            if applied.is_none() {
                for ddl in ddls {
                    let _ = sqlx::query(ddl).execute(pool).await;
                }
                let ins = match *DB_KIND {
                    DbKind::Postgres => "INSERT INTO schema_migrations(version) VALUES($1)",
                    _ => "INSERT INTO schema_migrations(version) VALUES(?)",
                };
                let _ = sqlx::query(ins).bind(ver).execute(pool).await;
            }
        }
        Ok(())
    }

    pub async fn bootstrap_from_db_if_enabled() -> Result<(), ClewdrError> {
        // Fast path: only when compiled and config chooses DB
        if !CLEWDR_CONFIG.load().is_db_mode() {
            return Ok(());
        }
        let apool = ensure_pool().await?;

        // Try load config from DB; if absent, seed with current config
        let row: Option<(String,)> = sqlx::query_as("SELECT data FROM clewdr_config WHERE k='main'")
            .fetch_optional(&apool)
            .await
            .map_err(|e| ClewdrError::Whatever { message: "load_config".into(), source: Some(Box::new(e)) })?;
        let mut cfg = if let Some((data,)) = row {
            match toml::from_str::<ClewdrConfig>(&data) {
                Ok(c) => c,
                Err(e) => {
                    error!("Failed to parse config TOML from DB: {}", e);
                    CLEWDR_CONFIG.load().as_ref().clone()
                }
            }
        } else {
            // Seed DB with current config
            let current = CLEWDR_CONFIG.load().as_ref().clone();
            let data = toml::to_string_pretty(&current)
                .map_err(|e| ClewdrError::Whatever { message: "toml_serialize".into(), source: Some(Box::new(e)) })?;
            sqlx::query("INSERT OR REPLACE INTO clewdr_config (k, data) VALUES ('main', ?)")
                .bind(data)
                .execute(&apool)
                .await
                .map_err(|e| ClewdrError::Whatever { message: "seed_config".into(), source: Some(Box::new(e)) })?;
            current
        };

        // Load cookies
        let cookie_rows = sqlx::query_as::<_, (String, Option<i64>, Option<String>, Option<String>, Option<i64>, Option<i64>, Option<String>)>(
            "SELECT cookie, reset_time, token_access, token_refresh, token_expires_at, token_expires_in, token_org_uuid FROM cookies",
        )
        .fetch_all(&apool)
        .await
        .map_err(|e| ClewdrError::Whatever { message: "load_cookies".into(), source: Some(Box::new(e)) })?;
        let mut cookie_array = std::collections::HashSet::new();
        for (cookie, reset_time, token_access, token_refresh, token_expires_at, token_expires_in, token_org_uuid) in cookie_rows {
            let mut c = CookieStatus::new(&cookie, reset_time).unwrap_or_default();
            if let Some(acc) = token_access {
                let expires_at = token_expires_at
                    .and_then(|s| chrono::DateTime::from_timestamp(s, 0))
                    .unwrap_or_else(|| chrono::Utc::now());
                let expires_in = std::time::Duration::from_secs(token_expires_in.unwrap_or_default() as u64);
                c.token = Some(crate::config::TokenInfo {
                    access_token: acc,
                    refresh_token: token_refresh.unwrap_or_default(),
                    organization: crate::config::Organization { uuid: token_org_uuid.unwrap_or_default() },
                    expires_at,
                    expires_in,
                });
            }
            cookie_array.insert(c);
        }
        cfg.cookie_array = cookie_array;

        // Load wasted_cookies
        let wasted_rows = sqlx::query_as::<_, (String, String)>(
            "SELECT cookie, reason FROM wasted_cookies",
        )
        .fetch_all(&apool)
        .await
        .map_err(|e| ClewdrError::Whatever { message: "load_wasted".into(), source: Some(Box::new(e)) })?;
        let mut wasted = std::collections::HashSet::new();
        for (cookie, reason) in wasted_rows {
            if let Ok(reason) = serde_json::from_str(&reason) {
                if let Ok(cc) = <crate::config::ClewdrCookie as FromStr>::from_str(&cookie) {
                    wasted.insert(UselessCookie::new(cc, reason));
                }
            }
        }
        cfg.wasted_cookie = wasted;

        // Load keys
        let key_rows = sqlx::query_as::<_, (String, i64)>("SELECT key, count_403 FROM keys")
            .fetch_all(&apool)
            .await
            .map_err(|e| ClewdrError::Whatever { message: "load_keys".into(), source: Some(Box::new(e)) })?;
        let mut keys = std::collections::HashSet::new();
        for (k, c403) in key_rows {
            keys.insert(KeyStatus {
                key: k.into(),
                count_403: c403 as u32,
            });
        }
        cfg.gemini_keys = keys;

        // Publish the config to global
        CLEWDR_CONFIG.store(std::sync::Arc::new(cfg.validate()));
        Ok(())
    }

    pub async fn persist_cookies(
        valid: &[CookieStatus],
        exhausted: &[CookieStatus],
        invalid: &[UselessCookie],
    ) -> Result<(), ClewdrError> {
        if !CLEWDR_CONFIG.load().is_db_mode() {
            return Ok(());
        }
        let pool = ensure_pool().await?;
        let mut tx = pool.begin().await.map_err(|e| ClewdrError::Whatever { message: "tx_begin".into(), source: Some(Box::new(e)) })?;
        // Replace contents: clear and insert
        sqlx::query("DELETE FROM cookies").execute(&mut *tx).await.map_err(|e| ClewdrError::Whatever { message: "clear_cookies".into(), source: Some(Box::new(e)) })?;
        sqlx::query("DELETE FROM wasted_cookies").execute(&mut *tx).await.map_err(|e| ClewdrError::Whatever { message: "clear_wasted".into(), source: Some(Box::new(e)) })?;

        for c in valid.iter().chain(exhausted.iter()) {
            let (acc, rtk, exp_at, exp_in, org) = if let Some(t) = &c.token {
                (
                    Some(t.access_token.to_owned()),
                    Some(t.refresh_token.to_owned()),
                    Some(t.expires_at.timestamp()),
                    Some(t.expires_in.as_secs() as i64),
                    Some(t.organization.uuid.to_owned()),
                )
            } else {
                (None, None, None, None, None)
            };
            sqlx::query(
                "INSERT INTO cookies(cookie, reset_time, token_access, token_refresh, token_expires_at, token_expires_in, token_org_uuid) VALUES(?, ?, ?, ?, ?, ?, ?)"
            )
            .bind(c.cookie.to_string())
            .bind(c.reset_time)
            .bind(acc)
            .bind(rtk)
            .bind(exp_at)
            .bind(exp_in)
            .bind(org)
            .execute(&mut *tx)
            .await
            .map_err(|e| ClewdrError::Whatever { message: "insert_cookie".into(), source: Some(Box::new(e)) })?;
        }

        for u in invalid {
            let reason = serde_json::to_string(&u.reason).unwrap_or_else(|_| "\"Unknown\"".to_string());
            sqlx::query("INSERT INTO wasted_cookies(cookie, reason) VALUES(?, ?)")
                .bind(u.cookie.to_string())
                .bind(reason)
                .execute(&mut *tx)
                .await
                .map_err(|e| ClewdrError::Whatever { message: "insert_wasted".into(), source: Some(Box::new(e)) })?;
        }

        tx.commit().await.map_err(|e| ClewdrError::Whatever { message: "tx_commit".into(), source: Some(Box::new(e)) })?;
        Ok(())
    }

    pub async fn persist_keys(keys: &[KeyStatus]) -> Result<(), ClewdrError> {
        if !CLEWDR_CONFIG.load().is_db_mode() {
            return Ok(());
        }
        let pool = ensure_pool().await?;
        let mut tx = pool.begin().await.map_err(|e| ClewdrError::Whatever { message: "tx_begin".into(), source: Some(Box::new(e)) })?;
        sqlx::query("DELETE FROM keys").execute(&mut *tx).await.map_err(|e| ClewdrError::Whatever { message: "clear_keys".into(), source: Some(Box::new(e)) })?;
        for k in keys {
            sqlx::query("INSERT INTO keys(key, count_403) VALUES(?, ?)")
                .bind(k.key.to_string())
                .bind(k.count_403 as i64)
                .execute(&mut *tx)
                .await
                .map_err(|e| ClewdrError::Whatever { message: "insert_key".into(), source: Some(Box::new(e)) })?;
        }
        tx.commit().await.map_err(|e| ClewdrError::Whatever { message: "tx_commit".into(), source: Some(Box::new(e)) })?;
        Ok(())
    }

    pub async fn persist_cookie_upsert(c: &CookieStatus) -> Result<(), ClewdrError> {
        if !CLEWDR_CONFIG.load().is_db_mode() {
            return Ok(());
        }
        let pool = ensure_pool().await?;
        let (acc, rtk, exp_at, exp_in, org) = if let Some(t) = &c.token {
            (
                Some(t.access_token.to_owned()),
                Some(t.refresh_token.to_owned()),
                Some(t.expires_at.timestamp()),
                Some(t.expires_in.as_secs() as i64),
                Some(t.organization.uuid.to_owned()),
            )
        } else {
            (None, None, None, None, None)
        };
        let sql = match *DB_KIND {
            DbKind::Mysql =>
                "INSERT INTO cookies(cookie, reset_time, token_access, token_refresh, token_expires_at, token_expires_in, token_org_uuid)
                 VALUES(?, ?, ?, ?, ?, ?, ?)
                 ON DUPLICATE KEY UPDATE
                   reset_time=VALUES(reset_time),
                   token_access=VALUES(token_access),
                   token_refresh=VALUES(token_refresh),
                   token_expires_at=VALUES(token_expires_at),
                   token_expires_in=VALUES(token_expires_in),
                   token_org_uuid=VALUES(token_org_uuid)",
            DbKind::Postgres =>
                "INSERT INTO cookies(cookie, reset_time, token_access, token_refresh, token_expires_at, token_expires_in, token_org_uuid)
                 VALUES($1, $2, $3, $4, $5, $6, $7)
                 ON CONFLICT(cookie) DO UPDATE SET
                   reset_time=excluded.reset_time,
                   token_access=excluded.token_access,
                   token_refresh=excluded.token_refresh,
                   token_expires_at=excluded.token_expires_at,
                   token_expires_in=excluded.token_expires_in,
                   token_org_uuid=excluded.token_org_uuid",
            _ =>
                "INSERT INTO cookies(cookie, reset_time, token_access, token_refresh, token_expires_at, token_expires_in, token_org_uuid)
                 VALUES(?, ?, ?, ?, ?, ?, ?)
                 ON CONFLICT(cookie) DO UPDATE SET
                   reset_time=excluded.reset_time,
                   token_access=excluded.token_access,
                   token_refresh=excluded.token_refresh,
                   token_expires_at=excluded.token_expires_at,
                   token_expires_in=excluded.token_expires_in,
                   token_org_uuid=excluded.token_org_uuid",
        };
        // build and execute with retry below
        let start = Instant::now();
        let mut res = sqlx::query::<sqlx::Any>(sql)
            .bind(c.cookie.to_string())
            .bind(c.reset_time)
            .bind(acc.clone())
            .bind(rtk.clone())
            .bind(exp_at.clone())
            .bind(exp_in.clone())
            .bind(org.clone())
            .execute(&pool)
            .await;
        if res.is_err() {
            RETRY_COUNT.fetch_add(1, Ordering::Relaxed);
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            res = sqlx::query::<sqlx::Any>(sql)
                .bind(c.cookie.to_string())
                .bind(c.reset_time)
                .bind(acc)
                .bind(rtk)
                .bind(exp_at)
                .bind(exp_in)
                .bind(org)
                .execute(&pool)
                .await;
        }
        if let Err(e) = res {
            record_error_msg(&e);
            mark_write_err();
            return Err(ClewdrError::Whatever { message: "upsert_cookie".into(), source: Some(Box::new(e)) });
        }
        record_duration(start);
        mark_write_ok();

        // Remove from wasted if present
        let del_wasted = match *DB_KIND { DbKind::Postgres => "DELETE FROM wasted_cookies WHERE cookie = $1", _ => "DELETE FROM wasted_cookies WHERE cookie = ?" };
        let _ = sqlx::query(del_wasted)
            .bind(c.cookie.to_string())
            .execute(&pool)
            .await;
        Ok(())
    }

    pub async fn delete_cookie_row(c: &CookieStatus) -> Result<(), ClewdrError> {
        if !CLEWDR_CONFIG.load().is_db_mode() {
            return Ok(());
        }
        let pool = ensure_pool().await?;
        let start = Instant::now();
        let del_cookie = match *DB_KIND { DbKind::Postgres => "DELETE FROM cookies WHERE cookie = $1", _ => "DELETE FROM cookies WHERE cookie = ?" };
        let mut res = sqlx::query(del_cookie)
            .bind(c.cookie.to_string())
            .execute(&pool)
            .await;
        if res.is_err() {
            RETRY_COUNT.fetch_add(1, Ordering::Relaxed);
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            res = sqlx::query(del_cookie)
                .bind(c.cookie.to_string())
                .execute(&pool)
                .await;
        }
        if let Err(e) = res {
            record_error_msg(&e);
            mark_write_err();
            return Err(ClewdrError::Whatever { message: "delete_cookie".into(), source: Some(Box::new(e)) });
        }
        record_duration(start);
        mark_write_ok();
        let del_wasted = match *DB_KIND { DbKind::Postgres => "DELETE FROM wasted_cookies WHERE cookie = $1", _ => "DELETE FROM wasted_cookies WHERE cookie = ?" };
        let _ = sqlx::query(del_wasted)
            .bind(c.cookie.to_string())
            .execute(&pool)
            .await;
        Ok(())
    }

    pub async fn persist_wasted_upsert(u: &UselessCookie) -> Result<(), ClewdrError> {
        if !CLEWDR_CONFIG.load().is_db_mode() {
            return Ok(());
        }
        let pool = ensure_pool().await?;
        let reason = serde_json::to_string(&u.reason).unwrap_or_else(|_| "\"Unknown\"".to_string());
        let sql = match *DB_KIND {
            DbKind::Mysql =>
                "INSERT INTO wasted_cookies(cookie, reason) VALUES(?, ?)
                 ON DUPLICATE KEY UPDATE reason=VALUES(reason)",
            DbKind::Postgres =>
                "INSERT INTO wasted_cookies(cookie, reason) VALUES($1, $2)
                 ON CONFLICT(cookie) DO UPDATE SET reason=excluded.reason",
            _ =>
                "INSERT INTO wasted_cookies(cookie, reason) VALUES(?, ?)
                 ON CONFLICT(cookie) DO UPDATE SET reason=excluded.reason",
        };
        let start = Instant::now();
        let mut res = sqlx::query::<sqlx::Any>(sql)
            .bind(u.cookie.to_string())
            .bind(reason.clone())
            .execute(&pool)
            .await;
        if res.is_err() {
            RETRY_COUNT.fetch_add(1, Ordering::Relaxed);
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            res = sqlx::query::<sqlx::Any>(sql)
                .bind(u.cookie.to_string())
                .bind(reason)
                .execute(&pool)
                .await;
        }
        if let Err(e) = res {
            record_error_msg(&e);
            mark_write_err();
            return Err(ClewdrError::Whatever { message: "upsert_wasted".into(), source: Some(Box::new(e)) });
        }
        record_duration(start);
        mark_write_ok();
        // ensure removed from active cookies
        let del_cookie = match *DB_KIND { DbKind::Postgres => "DELETE FROM cookies WHERE cookie = $1", _ => "DELETE FROM cookies WHERE cookie = ?" };
        let _ = sqlx::query(del_cookie)
            .bind(u.cookie.to_string())
            .execute(&pool)
            .await;
        Ok(())
    }

    pub async fn persist_key_upsert(k: &KeyStatus) -> Result<(), ClewdrError> {
        if !CLEWDR_CONFIG.load().is_db_mode() {
            return Ok(());
        }
        let pool = ensure_pool().await?;
        let sql = match *DB_KIND {
            DbKind::Mysql =>
                "INSERT INTO keys(`key`, count_403) VALUES(?, ?)
                 ON DUPLICATE KEY UPDATE count_403=VALUES(count_403)",
            DbKind::Postgres =>
                "INSERT INTO keys(key, count_403) VALUES($1, $2)
                 ON CONFLICT(key) DO UPDATE SET count_403=excluded.count_403",
            _ =>
                "INSERT INTO keys(key, count_403) VALUES(?, ?)
                 ON CONFLICT(key) DO UPDATE SET count_403=excluded.count_403",
        };
        let start = Instant::now();
        let mut res = sqlx::query::<sqlx::Any>(sql)
            .bind(k.key.to_string())
            .bind(k.count_403 as i64)
            .execute(&pool)
            .await;
        if res.is_err() {
            RETRY_COUNT.fetch_add(1, Ordering::Relaxed);
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            res = sqlx::query::<sqlx::Any>(sql)
                .bind(k.key.to_string())
                .bind(k.count_403 as i64)
                .execute(&pool)
                .await;
        }
        if let Err(e) = res {
            record_error_msg(&e);
            mark_write_err();
            return Err(ClewdrError::Whatever { message: "upsert_key".into(), source: Some(Box::new(e)) });
        }
        record_duration(start);
        mark_write_ok();
        Ok(())
    }

    pub async fn delete_key_row(k: &KeyStatus) -> Result<(), ClewdrError> {
        if !CLEWDR_CONFIG.load().is_db_mode() {
            return Ok(());
        }
        let pool = ensure_pool().await?;
        let start = Instant::now();
        let del_key = match *DB_KIND { DbKind::Postgres => "DELETE FROM keys WHERE key = $1", _ => "DELETE FROM keys WHERE key = ?" };
        let mut res = sqlx::query(del_key)
            .bind(k.key.to_string())
            .execute(&pool)
            .await;
        if res.is_err() {
            RETRY_COUNT.fetch_add(1, Ordering::Relaxed);
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            res = sqlx::query(del_key)
                .bind(k.key.to_string())
                .execute(&pool)
                .await;
        }
        if let Err(e) = res {
            record_error_msg(&e);
            mark_write_err();
            return Err(ClewdrError::Whatever { message: "delete_key".into(), source: Some(Box::new(e)) });
        }
        record_duration(start);
        mark_write_ok();
        Ok(())
    }

    pub async fn persist_config(config: &ClewdrConfig) -> Result<(), ClewdrError> {
        if !CLEWDR_CONFIG.load().is_db_mode() {
            return Ok(());
        }
        let pool = ensure_pool().await?;
        let data = toml::to_string_pretty(config)?;
        sqlx::query("INSERT OR REPLACE INTO clewdr_config (k, data, updated_at) VALUES('main', ?, CURRENT_TIMESTAMP)")
            .bind(data)
            .execute(&pool)
            .await
            .map_err(|e| ClewdrError::Whatever { message: "save_config".into(), source: Some(Box::new(e)) })?;
        Ok(())
    }

    pub async fn import_config_from_file() -> Result<serde_json::Value, ClewdrError> {
        // Read TOML file and push into DB tables
        let pool = ensure_pool().await?;
        // Load file-only without env overlay
        let toml_text = tokio::fs::read_to_string(crate::config::CONFIG_PATH.as_path()).await?;
        let cfg: ClewdrConfig = toml::from_str(&toml_text)
            .map_err(|e| ClewdrError::Whatever { message: "toml_parse".into(), source: Some(Box::new(e)) })?;

        persist_config(&cfg).await?;

        // Cookies
        // Decompose valid/exhausted based on reset_time presence
        let mut valid = vec![];
        let mut exhausted = vec![];
        for c in cfg.cookie_array.iter().cloned() {
            if c.reset_time.is_some() {
                exhausted.push(c);
            } else {
                valid.push(c);
            }
        }
        let invalid: Vec<UselessCookie> = cfg.wasted_cookie.iter().cloned().collect();
        persist_cookies(&valid, &exhausted, &invalid).await?;

        // Keys
        let keys: Vec<KeyStatus> = cfg.gemini_keys.iter().cloned().collect();
        persist_keys(&keys).await?;

        Ok(json!({
            "status": "ok",
            "imported_cookies": valid.len() + exhausted.len(),
            "imported_wasted": invalid.len(),
            "imported_keys": keys.len(),
        }))
    }

    pub async fn export_config_to_file() -> Result<serde_json::Value, ClewdrError> {
        // Load from DB and write to file regardless of no_fs
        let pool = ensure_pool().await?;
        let row: Option<(String,)> = sqlx::query_as("SELECT data FROM clewdr_config WHERE k='main'")
            .fetch_optional(&pool)
            .await
            .map_err(|e| ClewdrError::Whatever { message: "load_config".into(), source: Some(Box::new(e)) })?;
        let mut cfg = if let Some((data,)) = row {
            toml::from_str::<ClewdrConfig>(&data)
                .map_err(|e| ClewdrError::Whatever { message: "toml_parse".into(), source: Some(Box::new(e)) })?
        } else {
            CLEWDR_CONFIG.load().as_ref().clone()
        };

        // Merge cookies
        let cookie_rows = sqlx::query_as::<_, (String, Option<i64>, Option<String>, Option<String>, Option<i64>, Option<i64>, Option<String>)>(
            "SELECT cookie, reset_time, token_access, token_refresh, token_expires_at, token_expires_in, token_org_uuid FROM cookies",
        )
        .fetch_all(&pool)
        .await
        .map_err(|e| ClewdrError::Whatever { message: "load_cookies".into(), source: Some(Box::new(e)) })?;
        cfg.cookie_array.clear();
        for (cookie, reset_time, token_access, token_refresh, token_expires_at, token_expires_in, token_org_uuid) in cookie_rows {
            let mut c = CookieStatus::new(&cookie, reset_time).unwrap_or_default();
            if let Some(acc) = token_access {
                let expires_at = token_expires_at
                    .and_then(|s| chrono::DateTime::from_timestamp(s, 0))
                    .unwrap_or_else(|| chrono::Utc::now());
                let expires_in = std::time::Duration::from_secs(token_expires_in.unwrap_or_default() as u64);
                c.token = Some(crate::config::TokenInfo {
                    access_token: acc,
                    refresh_token: token_refresh.unwrap_or_default(),
                    organization: crate::config::Organization { uuid: token_org_uuid.unwrap_or_default() },
                    expires_at,
                    expires_in,
                });
            }
            cfg.cookie_array.insert(c);
        }
        let wasted_rows = sqlx::query_as::<_, (String, String)>(
            "SELECT cookie, reason FROM wasted_cookies",
        )
        .fetch_all(&pool)
        .await
        .map_err(|e| ClewdrError::Whatever { message: "load_wasted".into(), source: Some(Box::new(e)) })?;
        cfg.wasted_cookie.clear();
        for (cookie, reason) in wasted_rows {
            if let Ok(reason) = serde_json::from_str(&reason) {
                if let Ok(cc) = <crate::config::ClewdrCookie as FromStr>::from_str(&cookie) {
                    cfg.wasted_cookie.insert(UselessCookie::new(cc, reason));
                }
            }
        }
        let key_rows = sqlx::query_as::<_, (String, i64)>("SELECT key, count_403 FROM keys")
            .fetch_all(&pool)
            .await
            .map_err(|e| ClewdrError::Whatever { message: "load_keys".into(), source: Some(Box::new(e)) })?;
        cfg.gemini_keys.clear();
        for (k, c403) in key_rows {
            cfg.gemini_keys.insert(KeyStatus { key: k.into(), count_403: c403 as u32 });
        }

        // Write to file
        let toml_text = toml::to_string_pretty(&cfg)
            .map_err(|e| ClewdrError::Whatever { message: "toml_serialize".into(), source: Some(Box::new(e)) })?;
        if let Some(parent) = crate::config::CONFIG_PATH.parent() {
            if !parent.exists() {
                tokio::fs::create_dir_all(parent).await?;
            }
        }
        tokio::fs::write(crate::config::CONFIG_PATH.as_path(), toml_text).await?;
        Ok(json!({"status": "ok"}))
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum DbKind { Sqlite, Postgres, Mysql, Unknown }

    static DB_KIND: LazyLock<DbKind> = LazyLock::new(|| {
        let url = CLEWDR_CONFIG.load().database_url()
            .or_else(|| std::env::var("CLEWDR_DATABASE_URL").ok())
            .unwrap_or_default();
        if url.starts_with("sqlite:") { DbKind::Sqlite }
        else if url.starts_with("postgres:") || url.starts_with("postgresql:") { DbKind::Postgres }
        else if url.starts_with("mysql:") || url.starts_with("mariadb:") { DbKind::Mysql }
        else { DbKind::Unknown }
    });

    pub struct DbLayer;

    impl super::StorageLayer for DbLayer {
        fn is_enabled(&self) -> bool { true }
        fn spawn_bootstrap(&self) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), ClewdrError>> + Send>> {
            Box::pin(async { bootstrap_from_db_if_enabled().await })
        }
        fn persist_config(&self, cfg: &ClewdrConfig) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), ClewdrError>> + Send>> {
            let c = cfg.clone();
            Box::pin(async move { persist_config(&c).await })
        }
        fn persist_cookies(&self, valid: &[CookieStatus], exhausted: &[CookieStatus], invalid: &[UselessCookie]) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), ClewdrError>> + Send>> {
            let v = valid.to_vec();
            let e = exhausted.to_vec();
            let i = invalid.to_vec();
            Box::pin(async move { persist_cookies(&v, &e, &i).await })
        }
        fn persist_keys(&self, keys: &[KeyStatus]) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), ClewdrError>> + Send>> {
            let k = keys.to_vec();
            Box::pin(async move { persist_keys(&k).await })
        }
        fn persist_cookie_upsert(&self, c: &CookieStatus) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), ClewdrError>> + Send>> {
            let cc = c.clone();
            Box::pin(async move { persist_cookie_upsert(&cc).await })
        }
        fn delete_cookie_row(&self, c: &CookieStatus) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), ClewdrError>> + Send>> {
            let cc = c.clone();
            Box::pin(async move { delete_cookie_row(&cc).await })
        }
        fn persist_wasted_upsert(&self, u: &UselessCookie) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), ClewdrError>> + Send>> {
            let uu = u.clone();
            Box::pin(async move { persist_wasted_upsert(&uu).await })
        }
        fn persist_key_upsert(&self, k: &KeyStatus) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), ClewdrError>> + Send>> {
            let kk = k.clone();
            Box::pin(async move { persist_key_upsert(&kk).await })
        }
        fn delete_key_row(&self, k: &KeyStatus) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), ClewdrError>> + Send>> {
            let kk = k.clone();
            Box::pin(async move { delete_key_row(&kk).await })
        }
        fn import_from_file(&self) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<serde_json::Value, ClewdrError>> + Send>> {
            Box::pin(async move { import_config_from_file().await })
        }
        fn export_to_file(&self) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<serde_json::Value, ClewdrError>> + Send>> {
            Box::pin(async move { export_config_to_file().await })
        }
        fn status(&self) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<serde_json::Value, ClewdrError>> + Send>> {
            Box::pin(async move {
                let mut healthy = false;
                let mut err_str: Option<String> = None;
                let mut latency_ms: Option<u128> = None;
                match ensure_pool().await {
                    Ok(pool) => {
                        let start = Instant::now();
                        match sqlx::query("SELECT 1").execute(&pool).await {
                            Ok(_) => { healthy = true; latency_ms = Some(start.elapsed().as_millis()); },
                            Err(e) => { err_str = Some(e.to_string()); }
                        }
                    }
                    Err(e) => {
                        err_str = Some(e.to_string());
                    }
                }
                let mode = if CLEWDR_CONFIG.load().is_db_mode() { format!("{}", match *DB_KIND { DbKind::Sqlite=>"sqlite", DbKind::Postgres=>"postgres", DbKind::Mysql=>"mysql", DbKind::Unknown=>"unknown" }) } else { "file".into() };
                let cfg = CLEWDR_CONFIG.load();
                // redact database_url
                let mut redacted_url: Option<String> = None;
                if let Some(url) = &cfg.persistence.database_url {
                    redacted_url = Some(mask_url(url));
                }
                let details = json!({
                    "sqlite_path": cfg.persistence.sqlite_path,
                    "database_url": redacted_url,
                    "driver": mode,
                    "latency_ms": latency_ms,
                });
                let total = TOTAL_WRITES.load(Ordering::Relaxed);
                let errors = WRITE_ERROR_COUNT.load(Ordering::Relaxed);
                let nanos = TOTAL_WRITE_NANOS.load(Ordering::Relaxed);
                let avg_ms = if total > 0 { (nanos as f64 / total as f64) / 1_000_000.0 } else { 0.0 };
                let ratio = if total > 0 { errors as f64 / total as f64 } else { 0.0 };
                let last_error = LAST_ERROR.lock().ok().and_then(|g| g.clone());
                Ok(json!({
                    "enabled": true,
                    "mode": mode,
                    "healthy": healthy,
                    "details": details,
                    "last_write_ts": LAST_WRITE_TS.load(Ordering::Relaxed),
                    "write_error_count": errors,
                    "total_writes": total,
                    "avg_write_ms": avg_ms,
                    "failure_ratio": ratio,
                    "retry_count": RETRY_COUNT.load(Ordering::Relaxed),
                    "error": err_str,
                    "last_error": last_error,
                }))
            })
        }
    }

    fn mask_url(url: &str) -> String {
        // very basic redaction: replace password in postgres://user:pass@host/db
        if let Ok(u) = url::Url::parse(url) {
            let mut u2 = u.clone();
            if u.password().is_some() {
                let _ = u2.set_password(Some("***"));
            }
            return u2.to_string();
        }
        "***".into()
    }

    // metrics
    static LAST_WRITE_TS: LazyLock<AtomicI64> = LazyLock::new(|| AtomicI64::new(0));
    static WRITE_ERROR_COUNT: LazyLock<AtomicU64> = LazyLock::new(|| AtomicU64::new(0));
    static TOTAL_WRITES: LazyLock<AtomicU64> = LazyLock::new(|| AtomicU64::new(0));
    static TOTAL_WRITE_NANOS: LazyLock<AtomicU64> = LazyLock::new(|| AtomicU64::new(0));
    static RETRY_COUNT: LazyLock<AtomicU64> = LazyLock::new(|| AtomicU64::new(0));
    static LAST_ERROR: LazyLock<Mutex<Option<String>>> = LazyLock::new(|| Mutex::new(None));

    fn mark_write_ok() {
        let now = chrono::Utc::now().timestamp();
        LAST_WRITE_TS.store(now, Ordering::Relaxed);
    }
    fn mark_write_err() {
        WRITE_ERROR_COUNT.fetch_add(1, Ordering::Relaxed);
    }

    fn record_duration(start: Instant) {
        let dur = start.elapsed();
        TOTAL_WRITES.fetch_add(1, Ordering::Relaxed);
        TOTAL_WRITE_NANOS.fetch_add(dur.as_nanos() as u64, Ordering::Relaxed);
    }

    fn record_error_msg(e: &dyn std::error::Error) {
        if let Ok(mut g) = LAST_ERROR.lock() {
            *g = Some(e.to_string());
        }
    }

    async fn exec_with_retry<F, T>(mut f: F) -> Result<T, sqlx::Error>
    where
        F: FnMut() -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<T, sqlx::Error>> + Send>>,
    {
        match f().await {
            Ok(v) => Ok(v),
            Err(_e) => {
                RETRY_COUNT.fetch_add(1, Ordering::Relaxed);
                // small backoff
                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                f().await.map_err(|e2| e2)
            }
        }
    }
}

pub use internal::*;
