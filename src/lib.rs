use std::{path::PathBuf, sync::LazyLock};

use clap::Parser;
use colored::Colorize;

use crate::config::CLEWDR_CONFIG;

pub mod api;
pub mod claude_code_state;
pub mod claude_web_state;
pub mod config;
pub mod error;
pub mod gemini_state;
pub mod middleware;
pub mod router;
pub mod routes;
pub mod services;
pub mod persistence;
pub mod types;
pub mod utils;

pub const IS_DEBUG: bool = cfg!(debug_assertions);
pub static IS_DEV: LazyLock<bool> = LazyLock::new(|| std::env::var("CARGO_MANIFEST_DIR").is_ok());

pub static VERSION_INFO: LazyLock<String> = LazyLock::new(|| {
    format!(
        "v{} by {}\n| profile: {}\n| mode: {}\n| no_fs: {}",
        env!("CARGO_PKG_VERSION"),
        env!("CARGO_PKG_AUTHORS"),
        if IS_DEBUG {
            "debug".yellow()
        } else {
            "release".green()
        },
        if *IS_DEV {
            "dev".yellow()
        } else {
            "prod".green()
        },
        if CLEWDR_CONFIG.load().no_fs {
            "true".yellow()
        } else {
            "false".green()
        }
    )
});

pub const FIG: &str = r#"
    //   ) )                                    //   ) ) 
   //        //  ___                   ___   / //___/ /  
  //        // //___) ) //  / /  / / //   ) / / ___ (    
 //        // //       //  / /  / / //   / / //   | |    
((____/ / // ((____   ((__( (__/ / ((___/ / //    | |    
"#;

/// Reverse Proxy API for Claude and Gemini
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
pub struct Args {
    #[cfg(feature = "portable")]
    #[arg(short, long)]
    /// Force update of the application
    pub update: bool,
    #[arg(short, long)]
    /// load cookie from file
    pub file: Option<PathBuf>,
    /// Alternative config file
    #[arg(short, long)]
    pub config: Option<PathBuf>,
    #[arg(short, long)]
    /// Alternative log directory
    pub log_dir: Option<PathBuf>,
}
