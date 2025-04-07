use std::sync::LazyLock;

use clap::Parser;
use figlet_rs::FIGfont;

pub mod bootstrap;
pub mod client;
pub mod config;
pub mod cookie;
pub mod error;
pub mod messages;
pub mod router;
pub mod state;
pub mod submit;
pub mod text;
pub mod types;
pub mod utils;
pub mod update;

/// Header for the application
pub static BANNER: LazyLock<String> = LazyLock::new(|| {
    let standard_font = FIGfont::standard().unwrap();
    let figure = standard_font.convert("ClewdR");
    let banner = figure.unwrap().to_string();
    format!(
        "{}\nv{} by {}\n",
        banner,
        env!("CARGO_PKG_VERSION"),
        env!("CARGO_PKG_AUTHORS")
    )
});

/// Command line arguments for the application
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
pub struct Args {
    /// Path to the cookie file
    pub cookie_file: Option<String>,
}
