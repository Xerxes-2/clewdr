use std::{
    net::{IpAddr, Ipv4Addr},
    path::PathBuf,
    sync::LazyLock,
};

use arc_swap::ArcSwap;
use clap::Parser;
use url::Url;

use crate::{Args, config::ClewdrConfig};

pub const CONFIG_NAME: &str = "clewdr.toml";
pub const CLAUDE_ENDPOINT: &str = "https://api.anthropic.com";
pub const GEMINI_ENDPOINT: &str = "https://generativelanguage.googleapis.com";
pub const CC_CLIENT_ID: &str = "9d1c250a-e61b-44d9-88ed-5944d1962f5e";
pub const CC_TOKEN_URL: &str = "https://console.anthropic.com/v1/oauth/token";
pub const CC_REDIRECT_URI: &str = "https://console.anthropic.com/oauth/code/callback";

pub static ENDPOINT_URL: LazyLock<Url> = LazyLock::new(|| {
    Url::parse(CLAUDE_ENDPOINT).unwrap_or_else(|_| {
        panic!("Failed to parse endpoint URL: {CLAUDE_ENDPOINT}");
    })
});
pub static LOG_DIR: LazyLock<PathBuf> = LazyLock::new(|| {
    if let Some(path) = Args::try_parse().ok().and_then(|a| a.log_dir) {
        path
    } else {
        #[cfg(feature = "portable")]
        {
            PORTABLE_DIR.join("log")
        }
        #[cfg(feature = "xdg")]
        {
            use etcetera::{AppStrategy, AppStrategyArgs, choose_app_strategy};
            let strategy = choose_app_strategy(AppStrategyArgs {
                top_level_domain: "org".to_string(),
                author: "Xerxes-2".to_string(),
                app_name: "clewdr".to_string(),
            })
            .expect("Failed to choose app strategy");
            strategy.in_data_dir("log")
        }
    }
});
pub static CLEWDR_CONFIG: LazyLock<ArcSwap<ClewdrConfig>> = LazyLock::new(|| {
    let config = ClewdrConfig::new();
    ArcSwap::from_pointee(config)
});

pub static CONFIG_PATH: LazyLock<PathBuf> = LazyLock::new(|| {
    if let Some(path) = Args::try_parse().ok().and_then(|a| a.config) {
        path
    } else {
        #[cfg(feature = "portable")]
        {
            PORTABLE_DIR.join(CONFIG_NAME)
        }
        #[cfg(feature = "xdg")]
        {
            use etcetera::{AppStrategy, AppStrategyArgs, choose_app_strategy};
            let strategy = choose_app_strategy(AppStrategyArgs {
                top_level_domain: "org".to_string(),
                author: "Xerxes-2".to_string(),
                app_name: "clewdr".to_string(),
            })
            .expect("Failed to choose app strategy");
            strategy.in_config_dir(CONFIG_NAME)
        }
    }
});

#[cfg(feature = "portable")]
static PORTABLE_DIR: LazyLock<PathBuf> = LazyLock::new(|| {
    use crate::IS_DEV;
    if *IS_DEV {
        // In development use cargo dir
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
    } else {
        // In production use the directory of the executable
        std::env::current_exe()
            .expect("Failed to get current executable path")
            .parent()
            .expect("Failed to get dir")
            .to_path_buf()
    }
});

// Default functions
/// Default number of maximum retries for API requests
///
/// # Returns
/// * `usize` - The default value of 5
pub const fn default_max_retries() -> usize {
    5
}

/// Default IP address for the server to bind to
///
/// # Returns
/// * `String` - The default localhost IP "127.0.0.1"
pub fn default_ip() -> IpAddr {
    Ipv4Addr::new(127, 0, 0, 1).into()
}

/// Default port for the server to listen on
///
/// # Returns
/// * `u16` - The default port number 8484
pub fn default_port() -> u16 {
    8484
}

/// Default setting for using real roles in conversations
///
/// # Returns
/// * `bool` - The default value of true
pub const fn default_use_real_roles() -> bool {
    true
}

/// Default setting for checking updates on startup
///
/// # Returns
/// * `bool` - The default value of true
pub const fn default_check_update() -> bool {
    true
}
/// Default setting for skipping cool down cookies
///
/// # Returns
/// * `bool` - The default value of true
pub const fn default_skip_cool_down() -> bool {
    true
}

/// Default cookie value for testing purposes
pub const PLACEHOLDER_COOKIE: &str = "sk-ant-sid01----------------------------SET_YOUR_COOKIE_HERE----------------------------------------AAAAAAAA";
