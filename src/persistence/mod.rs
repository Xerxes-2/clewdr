use std::sync::LazyLock;

use serde_json::json;
use crate::config::{ClewdrConfig, CookieStatus, KeyStatus, UselessCookie};
use crate::error::ClewdrError;

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
    fn is_enabled(&self) -> bool { false }
    fn spawn_bootstrap(&self) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), ClewdrError>> + Send>> { Box::pin(async { Ok(()) }) }
    fn persist_config(&self, _cfg: &ClewdrConfig) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), ClewdrError>> + Send>> { Box::pin(async { Ok(()) }) }
    fn persist_cookies(&self, _valid: &[CookieStatus], _exhausted: &[CookieStatus], _invalid: &[UselessCookie]) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), ClewdrError>> + Send>> { Box::pin(async { Ok(()) }) }
    fn persist_keys(&self, _keys: &[KeyStatus]) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), ClewdrError>> + Send>> { Box::pin(async { Ok(()) }) }
    fn persist_cookie_upsert(&self, _c: &CookieStatus) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), ClewdrError>> + Send>> { Box::pin(async { Ok(()) }) }
    fn delete_cookie_row(&self, _c: &CookieStatus) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), ClewdrError>> + Send>> { Box::pin(async { Ok(()) }) }
    fn persist_wasted_upsert(&self, _u: &UselessCookie) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), ClewdrError>> + Send>> { Box::pin(async { Ok(()) }) }
    fn persist_key_upsert(&self, _k: &KeyStatus) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), ClewdrError>> + Send>> { Box::pin(async { Ok(()) }) }
    fn delete_key_row(&self, _k: &KeyStatus) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), ClewdrError>> + Send>> { Box::pin(async { Ok(()) }) }
    fn import_from_file(&self) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<serde_json::Value, ClewdrError>> + Send>> { Box::pin(async { Err(ClewdrError::PathNotFound { msg: "DB feature not enabled".into() }) }) }
    fn export_to_file(&self) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<serde_json::Value, ClewdrError>> + Send>> { Box::pin(async { Err(ClewdrError::PathNotFound { msg: "DB feature not enabled".into() }) }) }
    fn status(&self) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<serde_json::Value, ClewdrError>> + Send>> { Box::pin(async { Ok(json!({ "enabled": false, "mode": "file", "healthy": false })) }) }
}

// Feature-gated DB module providing actual implementation
#[cfg(feature = "db")]
pub mod db;

static STORAGE: LazyLock<std::sync::Arc<dyn StorageLayer>> = LazyLock::new(|| {
    #[cfg(feature = "db")]
    {
        if crate::config::CLEWDR_CONFIG.load().is_db_mode() {
            return std::sync::Arc::new(db::DbLayer);
        }
    }
    std::sync::Arc::new(FileLayer)
});

pub fn storage() -> &'static dyn StorageLayer { &**STORAGE }

// Public helpers for read-only snapshots used by background sync
#[cfg(feature = "db")]
pub use db::repo::{load_all_cookies, load_all_keys, persist_key_upsert};

#[cfg(not(feature = "db"))]
pub async fn load_all_keys() -> Result<Vec<KeyStatus>, ClewdrError> { Ok(vec![]) }
#[cfg(not(feature = "db"))]
pub async fn load_all_cookies() -> Result<(Vec<CookieStatus>, Vec<CookieStatus>, Vec<UselessCookie>), ClewdrError> {
    Ok((vec![], vec![], vec![]))
}
