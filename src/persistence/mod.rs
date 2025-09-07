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

static STORAGE: LazyLock<std::sync::Arc<dyn StorageLayer>> = LazyLock::new(|| {
    #[cfg(feature = "db")]
    {
        if crate::config::CLEWDR_CONFIG.load().is_db_mode() {
            return std::sync::Arc::new(internal::DbLayer);
        }
    }
    std::sync::Arc::new(FileLayer)
});

pub fn storage() -> &'static dyn StorageLayer { &**STORAGE }

// Public helpers for read-only snapshots (DB mode only will return data)
#[allow(unused_imports)]
pub use internal::{load_all_cookies, load_all_keys};
#[cfg(feature = "db")]
pub use internal::persist_key_upsert;

#[cfg(not(feature = "db"))]
mod internal {
    use super::*;
    pub async fn bootstrap_from_db_if_enabled() -> Result<(), ClewdrError> { Ok(()) }

    // Stubs for read helpers when DB disabled
    pub async fn load_all_keys() -> Result<Vec<KeyStatus>, ClewdrError> { Ok(vec![]) }
    pub async fn load_all_cookies() -> Result<(Vec<CookieStatus>, Vec<CookieStatus>, Vec<UselessCookie>), ClewdrError> {
        Ok((vec![], vec![], vec![]))
    }
}

#[cfg(feature = "db")]
mod internal {
    use super::*;
    use sea_orm::{
        entity::prelude::*,
        sea_query,
        ActiveValue::Set,
        Database, Schema, DatabaseBackend, Statement,
    };
    use tracing::error;
    use std::sync::{atomic::{AtomicI64, AtomicU64, Ordering}, Mutex};

    static CONN: LazyLock<std::sync::Mutex<Option<DatabaseConnection>>> = LazyLock::new(|| std::sync::Mutex::new(None));

    async fn ensure_conn() -> Result<DatabaseConnection, ClewdrError> {
        if let Ok(g) = CONN.lock() { if let Some(db) = g.as_ref() { return Ok(db.clone()); } }
        let cfg = crate::config::CLEWDR_CONFIG.load();
        if !cfg.is_db_mode() { return Err(ClewdrError::Whatever { message: "DB mode not enabled".into(), source: None }); }
        let url = cfg.database_url().or_else(|| std::env::var("CLEWDR_DATABASE_URL").ok())
            .ok_or(ClewdrError::UnexpectedNone { msg: "Database URL not provided" })?;
        if url.starts_with("sqlite://") { if let Some(parent) = std::path::Path::new(&url["sqlite://".len()..]).parent() { let _ = std::fs::create_dir_all(parent); } }
        let db = Database::connect(&url).await.map_err(|e| ClewdrError::Whatever { message: "db_connect".into(), source: Some(Box::new(e)) })?;
        migrate(&db).await?;
        if let Ok(mut g) = CONN.lock() { *g = Some(db.clone()); }
        Ok(db)
    }

    // SeaORM entities (one module per table; each defines `Model`)
    mod entity_config {
        use sea_orm::entity::prelude::*;
        #[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
        #[sea_orm(table_name = "clewdr_config")]
        pub struct Model {
            #[sea_orm(primary_key)]
            pub k: String,
            pub data: String,
            #[sea_orm(column_type = "BigInteger", nullable)]
            pub updated_at: Option<i64>,
        }
        #[derive(Copy, Clone, Debug, EnumIter)]
        pub enum Relation {}
        impl RelationTrait for Relation { fn def(&self) -> RelationDef { panic!() } }
        impl ActiveModelBehavior for ActiveModel {}
    }
    mod entity_cookie {
        use sea_orm::entity::prelude::*;
        #[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
        #[sea_orm(table_name = "cookies")]
        pub struct Model {
            #[sea_orm(primary_key)]
            pub cookie: String,
            #[sea_orm(column_type = "BigInteger", nullable)]
            pub reset_time: Option<i64>,
            #[sea_orm(nullable)]
            pub token_access: Option<String>,
            #[sea_orm(nullable)]
            pub token_refresh: Option<String>,
            #[sea_orm(column_type = "BigInteger", nullable)]
            pub token_expires_at: Option<i64>,
            #[sea_orm(column_type = "BigInteger", nullable)]
            pub token_expires_in: Option<i64>,
            #[sea_orm(nullable)]
            pub token_org_uuid: Option<String>,
        }
        #[derive(Copy, Clone, Debug, EnumIter)]
        pub enum Relation {}
        impl RelationTrait for Relation { fn def(&self) -> RelationDef { panic!() } }
        impl ActiveModelBehavior for ActiveModel {}
    }
    mod entity_wasted {
        use sea_orm::entity::prelude::*;
        #[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
        #[sea_orm(table_name = "wasted_cookies")]
        pub struct Model {
            #[sea_orm(primary_key)]
            pub cookie: String,
            pub reason: String,
        }
        #[derive(Copy, Clone, Debug, EnumIter)]
        pub enum Relation {}
        impl RelationTrait for Relation { fn def(&self) -> RelationDef { panic!() } }
        impl ActiveModelBehavior for ActiveModel {}
    }
    mod entity_key {
        use sea_orm::entity::prelude::*;
        #[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
        #[sea_orm(table_name = "keys")]
        pub struct Model {
            #[sea_orm(primary_key)]
            pub key: String,
            pub count_403: i64,
        }
        #[derive(Copy, Clone, Debug, EnumIter)]
        pub enum Relation {}
        impl RelationTrait for Relation { fn def(&self) -> RelationDef { panic!() } }
        impl ActiveModelBehavior for ActiveModel {}
    }

    // Convenient aliases to match previous names used in code
    use entity_config::{Entity as EntityConfig, Column as ColumnConfig, ActiveModel as ActiveModelConfig};
    use entity_cookie::{Entity as EntityCookie, Column as ColumnCookie, ActiveModel as ActiveModelCookie};
    use entity_wasted::{Entity as EntityWasted, Column as ColumnWasted, ActiveModel as ActiveModelWasted};
    use entity_key::{Entity as EntityKeyRow, Column as ColumnKeyRow, ActiveModel as ActiveModelKeyRow};

    async fn migrate(db: &DatabaseConnection) -> Result<(), ClewdrError> {
        let backend = db.get_database_backend();
        let schema = Schema::new(backend);
        let stmt: sea_query::TableCreateStatement = schema.create_table_from_entity(EntityConfig);
        db.execute(backend.build(&stmt)).await.ok();
        let stmt = schema.create_table_from_entity(EntityCookie);
        db.execute(backend.build(&stmt)).await.ok();
        let stmt = schema.create_table_from_entity(EntityWasted);
        db.execute(backend.build(&stmt)).await.ok();
        let stmt = schema.create_table_from_entity(EntityKeyRow);
        db.execute(backend.build(&stmt)).await.ok();
        // indexes
        use sea_query::Index;
        // cookies(token_org_uuid)
        let idx = Index::create()
            .name("idx_cookies_org_uuid")
            .table(EntityCookie)
            .col(ColumnCookie::TokenOrgUuid)
            .to_owned();
        db.execute(backend.build(&idx)).await.ok();
        // cookies(reset_time)
        let idx = Index::create()
            .name("idx_cookies_reset")
            .table(EntityCookie)
            .col(ColumnCookie::ResetTime)
            .to_owned();
        db.execute(backend.build(&idx)).await.ok();
        // keys(count_403)
        let idx = Index::create()
            .name("idx_keys_count")
            .table(EntityKeyRow)
            .col(ColumnKeyRow::Count403)
            .to_owned();
        db.execute(backend.build(&idx)).await.ok();
        Ok(())
    }

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

    // metrics
    static LAST_WRITE_TS: LazyLock<AtomicI64> = LazyLock::new(|| AtomicI64::new(0));
    static WRITE_ERROR_COUNT: LazyLock<AtomicU64> = LazyLock::new(|| AtomicU64::new(0));
    static TOTAL_WRITES: LazyLock<AtomicU64> = LazyLock::new(|| AtomicU64::new(0));
    static TOTAL_WRITE_NANOS: LazyLock<AtomicU64> = LazyLock::new(|| AtomicU64::new(0));
    static LAST_ERROR: LazyLock<Mutex<Option<String>>> = LazyLock::new(|| Mutex::new(None));

    fn mark_write_ok() {
        let now = chrono::Utc::now().timestamp();
        LAST_WRITE_TS.store(now, Ordering::Relaxed);
    }
    fn mark_write_err() { WRITE_ERROR_COUNT.fetch_add(1, Ordering::Relaxed); }
    fn record_duration(start: std::time::Instant) {
        let dur = start.elapsed();
        TOTAL_WRITES.fetch_add(1, Ordering::Relaxed);
        TOTAL_WRITE_NANOS.fetch_add(dur.as_nanos() as u64, Ordering::Relaxed);
    }
    fn record_error_msg(e: &dyn std::error::Error) {
        if let Ok(mut g) = LAST_ERROR.lock() { *g = Some(e.to_string()); }
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
        // Reconstruct latest runtime config from DB rows (like旧版逻辑)
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

    pub struct DbLayer;
    impl super::StorageLayer for DbLayer {
        fn is_enabled(&self) -> bool { true }
        fn spawn_bootstrap(&self) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), ClewdrError>> + Send>> { Box::pin(async { bootstrap_from_db_if_enabled().await }) }
        fn persist_config(&self, cfg: &ClewdrConfig) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), ClewdrError>> + Send>> { let c = cfg.clone(); Box::pin(async move { persist_config(&c).await }) }
        fn persist_cookies(&self, valid: &[CookieStatus], exhausted: &[CookieStatus], invalid: &[UselessCookie]) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), ClewdrError>> + Send>> { let v = valid.to_vec(); let e = exhausted.to_vec(); let i = invalid.to_vec(); Box::pin(async move { persist_cookies(&v, &e, &i).await }) }
        fn persist_keys(&self, keys: &[KeyStatus]) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), ClewdrError>> + Send>> { let k = keys.to_vec(); Box::pin(async move { persist_keys(&k).await }) }
        fn persist_cookie_upsert(&self, c: &CookieStatus) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), ClewdrError>> + Send>> { let cc = c.clone(); Box::pin(async move { persist_cookie_upsert(&cc).await }) }
        fn delete_cookie_row(&self, c: &CookieStatus) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), ClewdrError>> + Send>> { let cc = c.clone(); Box::pin(async move { delete_cookie_row(&cc).await }) }
        fn persist_wasted_upsert(&self, u: &UselessCookie) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), ClewdrError>> + Send>> { let uu = u.clone(); Box::pin(async move { persist_wasted_upsert(&uu).await }) }
        fn persist_key_upsert(&self, k: &KeyStatus) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), ClewdrError>> + Send>> { let kk = k.clone(); Box::pin(async move { persist_key_upsert(&kk).await }) }
        fn delete_key_row(&self, k: &KeyStatus) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), ClewdrError>> + Send>> { let kk = k.clone(); Box::pin(async move { delete_key_row(&kk).await }) }
        fn import_from_file(&self) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<serde_json::Value, ClewdrError>> + Send>> { Box::pin(async move { import_config_from_file().await }) }
        fn export_to_file(&self) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<serde_json::Value, ClewdrError>> + Send>> { Box::pin(async move { export_config_to_file().await }) }
        fn status(&self) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<serde_json::Value, ClewdrError>> + Send>> {
            Box::pin(async move {
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
                let nanos = TOTAL_WRITE_NANOS.load(Ordering::Relaxed);
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
            })
        }
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
}
