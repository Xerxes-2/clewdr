use std::{
    collections::HashSet,
    fmt::{Debug, Display},
    net::{IpAddr, SocketAddr},
    path::Path,
    sync::LazyLock,
};

use axum::http::{Uri, uri::Scheme};
use clap::Parser;
use colored::Colorize;
use figment::{
    Figment,
    providers::{Env, Format, Toml},
};
use http::uri::Authority;
use passwords::PasswordGenerator;
use serde::{Deserialize, Serialize};
use tokio::{io::AsyncWriteExt, spawn};
use tracing::error;
use url::Url;
use wreq::Proxy;

use super::{CONFIG_PATH, ENDPOINT_URL};

/// Serializes writers to [`CONFIG_PATH`]. Held across the whole
/// write-flush-rename sequence, so two concurrent savers cannot interleave and
/// the file always reflects one of them in full.
static SAVE_LOCK: LazyLock<tokio::sync::Mutex<()>> =
    LazyLock::new(|| tokio::sync::Mutex::new(()));

/// Writes `data` to `tmp`, flushes it to disk, then renames it over `dst`.
///
/// The flush has to happen before the rename: without it a crash can leave the
/// rename durable but the contents not, which is exactly the truncated-config
/// case the temp file is meant to prevent.
async fn write_then_rename(tmp: &Path, dst: &Path, data: &[u8]) -> Result<(), ClewdrError> {
    let mut file = create_private(tmp).await?;
    file.write_all(data).await?;
    file.sync_all().await?;
    drop(file);
    tokio::fs::rename(tmp, dst).await?;
    Ok(())
}

/// Creates `path` truncated and owner-only where the platform supports it.
///
/// The mode is set at creation rather than chmod-ed afterwards so the config,
/// which holds the admin password and cookies, is never briefly world-readable.
/// The mode carries through the rename onto the real config file.
async fn create_private(path: &Path) -> std::io::Result<tokio::fs::File> {
    let mut opts = tokio::fs::OpenOptions::new();
    opts.write(true).create(true).truncate(true);
    #[cfg(unix)]
    opts.mode(0o600);
    opts.open(path).await
}
use crate::{
    Args,
    config::{
        CC_CLIENT_ID, CookieStatus, UselessCookie, default_check_update, default_ip,
        default_max_retries, default_port, default_skip_cool_down, default_use_real_roles,
    },
    error::ClewdrError,
    utils::enabled,
};

/// Generates a random password for authentication
/// Creates a secure 64-character password with mixed character types
///
/// # Returns
/// A random password string
fn generate_password() -> String {
    let pg = PasswordGenerator {
        length: 64,
        numbers: true,
        lowercase_letters: true,
        uppercase_letters: true,
        symbols: false,
        spaces: false,
        exclude_similar_characters: true,
        strict: true,
    };

    println!("{}", "Generating random password......".green());
    pg.generate_one().unwrap()
}

/// A struct representing the configuration of the application
// The bool fields are flat keys in the user's TOML. Grouping them into
// sub-structs, as the lint suggests, would break every existing config file.
#[expect(
    clippy::struct_excessive_bools,
    reason = "mirrors the on-disk config format"
)]
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ClewdrConfig {
    // key configurations
    #[serde(default)]
    pub cookie_array: HashSet<CookieStatus>,
    #[serde(default)]
    pub wasted_cookie: HashSet<UselessCookie>,

    // Server settings, cannot hot reload
    #[serde(default = "default_ip")]
    ip: IpAddr,
    #[serde(default = "default_port")]
    port: u16,

    // App settings, can hot reload, but meaningless
    #[serde(default = "default_check_update")]
    pub check_update: bool,
    #[serde(default)]
    pub auto_update: bool,
    #[serde(default)]
    pub no_fs: bool,
    #[serde(default)]
    pub log_to_file: bool,

    // Network settings, can hot reload
    #[serde(default)]
    password: String,
    #[serde(default)]
    admin_password: String,
    #[serde(default)]
    pub proxy: Option<String>,
    #[serde(default)]
    pub rproxy: Option<Url>,

    // Api settings, can hot reload
    #[serde(default = "default_max_retries")]
    pub max_retries: usize,
    #[serde(default)]
    pub preserve_chats: bool,
    #[serde(default)]
    pub web_search: bool,
    #[serde(default)]
    pub enable_web_count_tokens: bool,
    #[serde(default)]
    pub sanitize_messages: bool,

    // Cookie settings, can hot reload
    #[serde(default)]
    pub skip_first_warning: bool,
    #[serde(default)]
    pub skip_second_warning: bool,
    #[serde(default)]
    pub skip_restricted: bool,
    #[serde(default)]
    pub skip_non_pro: bool,
    #[serde(default = "default_skip_cool_down")]
    pub skip_rate_limit: bool,
    #[serde(default)]
    pub skip_normal_pro: bool,

    // Prompt configurations, can hot reload
    #[serde(default = "default_use_real_roles")]
    pub use_real_roles: bool,
    #[serde(default)]
    pub custom_h: Option<String>,
    #[serde(default)]
    pub custom_a: Option<String>,
    #[serde(default)]
    pub custom_prompt: String,

    // Claude Code settings, can hot reload
    #[serde(default)]
    pub claude_code_client_id: Option<String>,
    #[serde(default)]
    pub custom_system: Option<String>,

    // Skip field, can hot reload
    #[serde(skip)]
    pub wreq_proxy: Option<Proxy>,
}

impl Default for ClewdrConfig {
    fn default() -> Self {
        Self {
            max_retries: default_max_retries(),
            check_update: default_check_update(),
            auto_update: false,
            cookie_array: HashSet::new(),
            wasted_cookie: HashSet::new(),
            password: String::new(),
            admin_password: String::new(),
            proxy: None,
            ip: default_ip(),
            port: default_port(),
            rproxy: None,
            use_real_roles: default_use_real_roles(),
            custom_prompt: String::new(),
            custom_h: None,
            custom_a: None,
            wreq_proxy: None,
            preserve_chats: false,
            web_search: false,
            enable_web_count_tokens: false,
            sanitize_messages: false,
            skip_first_warning: false,
            skip_second_warning: false,
            skip_restricted: false,
            skip_non_pro: false,
            skip_rate_limit: default_skip_cool_down(),
            skip_normal_pro: false,
            claude_code_client_id: None,
            custom_system: None,
            no_fs: false,
            log_to_file: false,
        }
    }
}

impl Display for ClewdrConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // one line per field
        let authority = self.address();
        let authority: Authority = authority.to_string().parse().map_err(|_| std::fmt::Error)?;
        let api_url = Uri::builder()
            .scheme(Scheme::HTTP)
            .authority(authority.clone())
            .path_and_query("/v1")
            .build()
            .map_err(|_| std::fmt::Error)?;
        let web_url = Uri::builder()
            .scheme(Scheme::HTTP)
            .authority(authority.to_string())
            // Must be "/", not "": http rejects an empty path-and-query with
            // InvalidUri(Empty), which surfaces as a panic in the `println!`
            // that prints this config at startup. Both render as a trailing
            // slash, so the output is unchanged.
            .path_and_query("/")
            .build()
            .map_err(|_| std::fmt::Error)?;
        write!(
            f,
            "Claude(Claude and OpenAI format) Endpoint: {}\n\
            Claude Code(Claude and OpenAI format) Endpoint: {}\n\
            API Password: {}\n\
            Web Admin Endpoint: {}\n\
            Web Admin Password: {}\n",
            api_url.to_string().green().underline(),
            (web_url.to_string() + "code/v1").green().underline(),
            self.password.yellow(),
            web_url.to_string().green().underline(),
            self.admin_password.yellow(),
        )?;
        if let Some(ref proxy) = self.proxy {
            writeln!(f, "Proxy: {}", proxy.clone().blue())?;
        }
        if let Some(ref rproxy) = self.rproxy {
            writeln!(f, "Reverse Proxy: {}", rproxy.to_string().blue())?;
        }
        writeln!(f, "Skip Free: {}", enabled(self.skip_non_pro))?;
        writeln!(f, "Skip restricted: {}", enabled(self.skip_restricted))?;
        writeln!(
            f,
            "Skip second warning: {}",
            enabled(self.skip_second_warning)
        )?;
        writeln!(
            f,
            "Skip first warning: {}",
            enabled(self.skip_first_warning)
        )?;
        writeln!(f, "Skip normal Pro: {}", enabled(self.skip_normal_pro))?;
        writeln!(f, "Skip rate limit: {}", enabled(self.skip_rate_limit))?;
        writeln!(
            f,
            "Web count_tokens: {}",
            enabled(self.enable_web_count_tokens)
        )?;
        Ok(())
    }
}

impl From<&ClewdrConfig> for clewdr_types::ConfigApi {
    fn from(c: &ClewdrConfig) -> Self {
        Self {
            ip: c.ip.to_string(),
            port: c.port,
            check_update: c.check_update,
            auto_update: c.auto_update,
            password: c.password.clone(),
            admin_password: c.admin_password.clone(),
            proxy: c.proxy.clone(),
            rproxy: c.rproxy.as_ref().map(std::string::ToString::to_string),
            max_retries: c.max_retries,
            preserve_chats: c.preserve_chats,
            web_search: c.web_search,
            enable_web_count_tokens: c.enable_web_count_tokens,
            sanitize_messages: c.sanitize_messages,
            skip_first_warning: c.skip_first_warning,
            skip_second_warning: c.skip_second_warning,
            skip_restricted: c.skip_restricted,
            skip_non_pro: c.skip_non_pro,
            skip_rate_limit: c.skip_rate_limit,
            skip_normal_pro: c.skip_normal_pro,
            use_real_roles: c.use_real_roles,
            custom_h: c.custom_h.clone(),
            custom_a: c.custom_a.clone(),
            custom_prompt: c.custom_prompt.clone(),
            claude_code_client_id: c.claude_code_client_id.clone(),
            custom_system: c.custom_system.clone(),
        }
    }
}

impl From<clewdr_types::ConfigApi> for ClewdrConfig {
    fn from(c: clewdr_types::ConfigApi) -> Self {
        Self {
            ip: c.ip.parse().unwrap_or(default_ip()),
            port: c.port,
            check_update: c.check_update,
            auto_update: c.auto_update,
            password: c.password,
            admin_password: c.admin_password,
            proxy: c.proxy,
            rproxy: c.rproxy.and_then(|s| Url::parse(&s).ok()),
            max_retries: c.max_retries,
            preserve_chats: c.preserve_chats,
            web_search: c.web_search,
            enable_web_count_tokens: c.enable_web_count_tokens,
            sanitize_messages: c.sanitize_messages,
            skip_first_warning: c.skip_first_warning,
            skip_second_warning: c.skip_second_warning,
            skip_restricted: c.skip_restricted,
            skip_non_pro: c.skip_non_pro,
            skip_rate_limit: c.skip_rate_limit,
            skip_normal_pro: c.skip_normal_pro,
            use_real_roles: c.use_real_roles,
            custom_h: c.custom_h,
            custom_a: c.custom_a,
            custom_prompt: c.custom_prompt,
            claude_code_client_id: c.claude_code_client_id,
            custom_system: c.custom_system,
            ..Default::default()
        }
    }
}

impl ClewdrConfig {
    pub fn user_auth(&self, key: &str) -> bool {
        key == self.password
    }

    pub fn admin_auth(&self, key: &str) -> bool {
        key == self.admin_password
    }

    pub fn cc_client_id(&self) -> String {
        self.claude_code_client_id
            .as_deref()
            .unwrap_or(CC_CLIENT_ID)
            .to_string()
    }

    /// Loads configuration from files and environment variables
    /// Combines settings from config.toml, clewdr.toml, and environment variables
    /// Also loads cookies from a file if specified
    ///
    /// # Returns
    /// * Config instance
    pub fn new() -> Self {
        // Load config from TOML then override with environment variables.
        // Use double underscore "__" to map nested keys.
        let mut config: ClewdrConfig = Figment::from(Toml::file(CONFIG_PATH.as_path()))
            .admerge(Env::prefixed("CLEWDR_").split("__"))
            .extract_lossy()
            .inspect_err(|e| {
                error!("Failed to load config: {}", e);
            })
            .unwrap_or_default();
        if let Some(ref f) = Args::try_parse().ok().and_then(|a| a.file) {
            // load cookies from file
            if f.exists() {
                if let Ok(cookies) = std::fs::read_to_string(f) {
                    let cookies = cookies
                        .lines()
                        .filter_map(|line| CookieStatus::new(line, None).ok());
                    config.cookie_array.extend(cookies);
                } else {
                    error!("Failed to read cookie file: {}", f.display());
                }
            } else {
                error!("Cookie file not found: {}", f.display());
            }
        }
        let config = config.validate();
        if !config.no_fs {
            let config_clone = config.clone();
            spawn(async move {
                config_clone.save().await.unwrap_or_else(|e| {
                    error!("Failed to save config: {}", e);
                });
            });
        }
        config
    }

    /// Gets the API endpoint for the Claude service
    /// Returns the reverse proxy URL if configured, otherwise the default endpoint
    ///
    /// # Returns
    /// The URL for the API endpoint
    pub fn ip(&self) -> IpAddr {
        self.ip
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    pub fn password(&self) -> &str {
        &self.password
    }

    pub fn admin_password(&self) -> &str {
        &self.admin_password
    }

    pub fn endpoint(&self) -> Url {
        if let Some(ref proxy) = self.rproxy {
            return proxy.to_owned();
        }
        ENDPOINT_URL.to_owned()
    }

    /// address of proxy
    pub fn address(&self) -> SocketAddr {
        SocketAddr::new(self.ip, self.port)
    }

    /// Save the configuration to a file
    ///
    /// The new contents go to a sibling temporary file which is flushed and
    /// then renamed over the target, so a concurrent reader or an interrupted
    /// run sees either the previous config or the new one, never a partial
    /// write. Concurrent savers are serialized by [`SAVE_LOCK`].
    ///
    /// A no-op when running with `no_fs`.
    ///
    /// # Errors
    /// If the config directory cannot be created, the config cannot be
    /// serialized, or the file cannot be written or renamed.
    pub async fn save(&self) -> Result<(), ClewdrError> {
        if self.no_fs {
            return Ok(());
        }
        let data = toml::ser::to_string_pretty(self)?;
        let path = CONFIG_PATH.as_path();

        let _guard = SAVE_LOCK.lock().await;

        if let Some(parent) = path.parent()
            && !parent.exists()
        {
            tokio::fs::create_dir_all(parent).await?;
        }
        // Sibling of the target, so the rename stays on one filesystem. The
        // pid keeps two clewdr processes sharing a config dir off each other's
        // temporary file; SAVE_LOCK covers savers within this process.
        let tmp = path.with_extension(format!("tmp.{}", std::process::id()));
        let result = write_then_rename(&tmp, path, data.as_bytes()).await;
        if result.is_err() {
            // Best effort: a leftover temp file would otherwise linger next to
            // the config forever.
            drop(tokio::fs::remove_file(&tmp).await);
        }
        result
    }

    /// Validate the configuration
    #[must_use]
    pub fn validate(mut self) -> Self {
        if self.password.trim().is_empty() {
            self.password = generate_password();
        }
        if self.admin_password.trim().is_empty() {
            self.admin_password = generate_password();
        }
        self.cookie_array = self
            .cookie_array
            .into_iter()
            .map(super::cookie::CookieStatus::reset)
            .collect();
        self.wreq_proxy = self.proxy.clone().and_then(|p| {
            Proxy::all(p)
                .inspect_err(|e| {
                    self.proxy = None;
                    error!("Failed to parse proxy: {}", e);
                })
                .ok()
        });
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `main` prints the config with `println!`, and a `Display` impl that
    /// returns `Err` makes that panic rather than fail gracefully. The URLs are
    /// built through `http`, which rejects some inputs that look harmless --
    /// an empty `path_and_query` among them.
    #[test]
    fn display_never_fails() {
        let config = ClewdrConfig::default();
        let rendered = config.to_string();
        assert!(rendered.contains("Web Admin Endpoint"));
        assert!(rendered.contains("Claude Code"));
    }

    /// Unique scratch directory, removed on drop.
    struct TempDir(std::path::PathBuf);

    impl TempDir {
        fn new(tag: &str) -> Self {
            use std::sync::atomic::{AtomicU32, Ordering};
            static COUNTER: AtomicU32 = AtomicU32::new(0);
            let n = COUNTER.fetch_add(1, Ordering::Relaxed);
            let dir =
                std::env::temp_dir().join(format!("clewdr-test-{tag}-{}-{n}", std::process::id()));
            std::fs::create_dir_all(&dir).expect("create temp dir");
            Self(dir)
        }

        fn join(&self, name: &str) -> std::path::PathBuf {
            self.0.join(name)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            drop(std::fs::remove_dir_all(&self.0));
        }
    }

    /// Overwriting in place can leave the tail of a longer previous file
    /// behind. Going through a fresh temp file plus rename must not.
    #[tokio::test]
    async fn write_then_rename_replaces_content_wholesale() {
        let dir = TempDir::new("replace");
        let dst = dir.join("clewdr.toml");
        let tmp = dir.join("clewdr.tmp");

        write_then_rename(&tmp, &dst, &b"x".repeat(4096))
            .await
            .expect("first write");
        write_then_rename(&tmp, &dst, b"short")
            .await
            .expect("second write");

        assert_eq!(std::fs::read(&dst).expect("read back"), b"short");
        assert!(!tmp.exists(), "temp file should have been renamed away");
    }

    /// A reader must never observe a partially written config, no matter how
    /// many savers are racing.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn concurrent_writes_are_never_torn() {
        const WRITERS: usize = 8;
        // Large enough that a non-atomic write would be caught mid-flight.
        const LEN: usize = 512 * 1024;

        let dir = TempDir::new("torn");
        let dst = dir.join("clewdr.toml");
        write_then_rename(&dir.join("seed.tmp"), &dst, &vec![b'a'; LEN])
            .await
            .expect("seed");

        let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let reader = tokio::spawn({
            let dst = dst.clone();
            let stop = stop.clone();
            async move {
                while !stop.load(std::sync::atomic::Ordering::Relaxed) {
                    let seen = std::fs::read(&dst).expect("read during writes");
                    // Every writer writes one repeated byte, so any mixture of
                    // two payloads (or a truncated one) fails these checks.
                    assert_eq!(seen.len(), LEN, "observed a partially written file");
                    let first = seen[0];
                    assert!(
                        seen.iter().all(|b| *b == first),
                        "observed a mix of two payloads"
                    );
                    tokio::task::yield_now().await;
                }
            }
        });

        let writers = (0..WRITERS).map(|i| {
            let tmp = dir.join(&format!("w{i}.tmp"));
            let dst = dst.clone();
            tokio::spawn(async move {
                let byte = b'a' + u8::try_from(i).expect("writer index fits");
                for _ in 0..10 {
                    write_then_rename(&tmp, &dst, &vec![byte; LEN])
                        .await
                        .expect("concurrent write");
                }
            })
        });
        for w in writers {
            w.await.expect("writer task");
        }

        stop.store(true, std::sync::atomic::Ordering::Relaxed);
        reader.await.expect("reader task");
    }

    /// The config holds the admin password and cookies, so it must never be
    /// readable by other users -- not even briefly between create and chmod.
    #[cfg(unix)]
    #[tokio::test]
    async fn written_config_is_owner_only() {
        use std::os::unix::fs::PermissionsExt;

        let dir = TempDir::new("perms");
        let dst = dir.join("clewdr.toml");
        write_then_rename(&dir.join("clewdr.tmp"), &dst, b"secret")
            .await
            .expect("write");

        let mode = std::fs::metadata(&dst).expect("stat").permissions().mode();
        assert_eq!(mode & 0o777, 0o600, "config should be owner read/write only");
    }
}
