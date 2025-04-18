use clap::Parser;
use clewdr::{
    self, BANNER, config::Config, cookie::CookieManager, error::ClewdrError, state::AppState,
    utils::config_dir,
};
use colored::Colorize;
use tokio::{spawn, sync::mpsc};
use tracing::warn;
use tracing_subscriber::{
    Registry,
    fmt::{self, time::ChronoLocal},
    layer::SubscriberExt,
};

/// Async main function using tokio runtime
#[tokio::main]
async fn main() -> Result<(), ClewdrError> {
    enable_ansi_support::enable_ansi_support()?;
    // parse command line arguments
    clewdr::Args::parse();
    // set up logging time format
    let timer = ChronoLocal::new("%H:%M:%S%.3f".to_string());
    // set up logging
    // create log directory if it doesn't exist
    let path = config_dir()?;
    let log_dir = path.join("log");
    if !log_dir.exists() {
        std::fs::create_dir_all(&log_dir)?
    }
    // create log file
    let file_appender = tracing_appender::rolling::daily(log_dir, "clewdr.log");
    let (file_writer, _guard) = tracing_appender::non_blocking(file_appender);

    let subscriber = Registry::default()
        .with(
            fmt::Layer::default()
                .with_writer(file_writer)
                .with_timer(timer.clone()),
        )
        .with(
            fmt::Layer::default()
                .with_writer(std::io::stdout)
                .with_timer(timer),
        );

    tracing::subscriber::set_global_default(subscriber).expect("unable to set global subscriber");

    println!("{}", *BANNER);
    // load config from file
    let config = Config::load()?;

    let updater = clewdr::update::Updater::new(config.clone())?;
    if let Err(e) = updater.check_for_updates().await {
        warn!("Update check failed: {}", e);
    }

    // print the address
    let addr = format!("http://{}/v1", config.address());
    if let Ok(dir) = config_dir() {
        println!(
            "Config dir: {}",
            dir.join("config.toml").display().to_string().blue()
        );
    }
    println!("Listening on {}", addr.green());
    println!("{}", config);

    // initialize the application state
    let (req_tx, req_rx) = mpsc::channel(config.max_connections);
    let (ret_tx, ret_rx) = mpsc::channel(config.max_connections);
    let (submit_tx, submit_rx) = mpsc::channel(config.max_connections);
    let state = AppState::new(config.clone(), req_tx, ret_tx, submit_tx);
    let cm = CookieManager::new(config, req_rx, ret_rx, submit_rx);
    // build axum router
    // create a TCP listener
    let addr = state.config.address().to_string();
    let listener = tokio::net::TcpListener::bind(addr).await?;
    let router = clewdr::router::RouterBuilder::new(state).build();
    // serve the application
    spawn(cm.run());
    axum::serve(listener, router).await?;
    Ok(())
}
