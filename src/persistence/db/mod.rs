pub mod conn;
pub mod entities;
pub mod metrics;
pub mod repo;

use crate::{
    config::{ClewdrConfig, CookieStatus, KeyStatus, UselessCookie},
    error::ClewdrError,
    persistence::StorageLayer,
};

pub struct DbLayer;

impl StorageLayer for DbLayer {
    fn is_enabled(&self) -> bool { true }
    fn spawn_bootstrap(&self) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), ClewdrError>> + Send>> {
        Box::pin(async { repo::bootstrap_from_db_if_enabled().await })
    }
    fn persist_config(&self, cfg: &ClewdrConfig) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), ClewdrError>> + Send>> {
        let c = cfg.clone();
        Box::pin(async move { repo::persist_config(&c).await })
    }
    fn persist_cookies(
        &self,
        valid: &[CookieStatus],
        exhausted: &[CookieStatus],
        invalid: &[UselessCookie],
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), ClewdrError>> + Send>> {
        let v = valid.to_vec();
        let e = exhausted.to_vec();
        let i = invalid.to_vec();
        Box::pin(async move { repo::persist_cookies(&v, &e, &i).await })
    }
    fn persist_keys(&self, keys: &[KeyStatus]) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), ClewdrError>> + Send>> {
        let k = keys.to_vec();
        Box::pin(async move { repo::persist_keys(&k).await })
    }
    fn persist_cookie_upsert(&self, c: &CookieStatus) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), ClewdrError>> + Send>> {
        let cc = c.clone();
        Box::pin(async move { repo::persist_cookie_upsert(&cc).await })
    }
    fn delete_cookie_row(&self, c: &CookieStatus) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), ClewdrError>> + Send>> {
        let cc = c.clone();
        Box::pin(async move { repo::delete_cookie_row(&cc).await })
    }
    fn persist_wasted_upsert(&self, u: &UselessCookie) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), ClewdrError>> + Send>> {
        let uu = u.clone();
        Box::pin(async move { repo::persist_wasted_upsert(&uu).await })
    }
    fn persist_key_upsert(&self, k: &KeyStatus) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), ClewdrError>> + Send>> {
        let kk = k.clone();
        Box::pin(async move { repo::persist_key_upsert(&kk).await })
    }
    fn delete_key_row(&self, k: &KeyStatus) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), ClewdrError>> + Send>> {
        let kk = k.clone();
        Box::pin(async move { repo::delete_key_row(&kk).await })
    }
    fn import_from_file(&self) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<serde_json::Value, ClewdrError>> + Send>> {
        Box::pin(async move { repo::import_config_from_file().await })
    }
    fn export_to_file(&self) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<serde_json::Value, ClewdrError>> + Send>> {
        Box::pin(async move { repo::export_config_to_file().await })
    }
    fn status(&self) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<serde_json::Value, ClewdrError>> + Send>> {
        Box::pin(async move { repo::status_json().await })
    }
}

