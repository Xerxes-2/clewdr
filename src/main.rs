use clewdr::{
    self, FIG, IS_DEBUG, VERSION_INFO,
    config::{CLEWDR_CONFIG, CONFIG_PATH, LOG_DIR},
    error::ClewdrError,
};
use colored::Colorize;
#[cfg(feature = "mimalloc")]
use mimalloc::MiMalloc;
use tracing::Subscriber;
use tracing_subscriber::{
    Layer, Registry,
    fmt::{self, time::ChronoLocal},
    layer::SubscriberExt,
    registry::LookupSpan,
};

#[cfg(feature = "mimalloc")]
#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;
#[cfg(feature = "dhat-heap")]
#[global_allocator]
static ALLOC: dhat::Alloc = dhat::Alloc;

fn setup_subscriber<S>(subscriber: S)
where
    S: Subscriber + for<'span> LookupSpan<'span> + Send + Sync + 'static,
{
    #[cfg(feature = "tokio-console")]
    let subscriber = {
        // enable tokio console
        use std::str::FromStr;
        let tokio_console_filter =
            tracing_subscriber::filter::Targets::from_str("tokio=trace,runtime=trace")
                .expect("Failed to parse filter");
        let console_layer = console_subscriber::ConsoleLayer::builder()
            // set the address the server is bound to
            .server_addr(([0, 0, 0, 0], 6669))
            .spawn();
        subscriber.with(console_layer.with_filter(tokio_console_filter))
    };
    tracing::subscriber::set_global_default(subscriber).expect("unable to set global subscriber");
}

/// Application entry point
/// Sets up logging, checks for updates, initializes the application state,
/// creates the router, and starts the server
///
/// # Returns
/// Result indicating success or failure of the application execution
#[tokio::main]
async fn main() -> Result<(), ClewdrError> {
    // DB drivers setup is handled by SeaORM (via sqlx features) when compiled with db-*
    #[cfg(feature = "dhat-heap")]
    let _profiler = dhat::Profiler::new_heap();
    #[cfg(windows)]
    {
        _ = enable_ansi_support::enable_ansi_support();
    }
    // set up logging time format
    let timer = ChronoLocal::new("%H:%M:%S%.3f".to_string());
    // set up logging
    let filter = if IS_DEBUG {
        tracing_subscriber::filter::LevelFilter::DEBUG
    } else {
        tracing_subscriber::filter::LevelFilter::INFO
    };
    let env_filter = tracing_subscriber::EnvFilter::builder()
        .with_default_directive(filter.into())
        .from_env_lossy();
    let subscriber = Registry::default().with(
        fmt::Layer::default()
            .with_writer(std::io::stdout)
            .with_timer(timer.to_owned())
            .with_filter(env_filter),
    );
    let _guard = if !CLEWDR_CONFIG.load().no_fs && CLEWDR_CONFIG.load().log_to_file {
        std::fs::create_dir_all(LOG_DIR.as_path()).expect("Failed to create log directory");
        let file_appender = tracing_appender::rolling::daily(LOG_DIR.as_path(), "clewdr.log");
        let (file_writer, guard) = tracing_appender::non_blocking(file_appender);
        let filter = tracing_subscriber::EnvFilter::builder()
            .with_default_directive(filter.into())
            .from_env_lossy();
        let subscriber = subscriber.with(
            fmt::Layer::default()
                .with_writer(file_writer)
                .with_timer(timer)
                .with_filter(filter),
        );
        setup_subscriber(subscriber);
        Some(guard)
    } else {
        setup_subscriber(subscriber);
        None
    };

    println!("{}\n{}", FIG, *VERSION_INFO);

    #[cfg(feature = "portable")]
    {
        use tracing::warn;
        let updater = clewdr::services::update::ClewdrUpdater::new()?;
        if let Err(e) = updater.check_for_updates().await {
            warn!("Update check failed: {}", e);
        }
    }

    if let Err(e) = clewdr::persistence::storage().spawn_bootstrap().await {
        use tracing::warn;
        warn!("DB bootstrap skipped or failed: {}", e);
    }

    // print info
    println!("Config dir: {}", CONFIG_PATH.display().to_string().blue());
    println!("{}", *CLEWDR_CONFIG);

    // build axum router
    // create a TCP listener
    let addr = CLEWDR_CONFIG.load().address();
    let listener = tokio::net::TcpListener::bind(addr).await?;
    let router = clewdr::router::RouterBuilder::new()
        .await
        .with_default_setup()
        .build();
    // serve the application
    Ok(axum::serve(listener, router)
        .with_graceful_shutdown(async {
            tokio::signal::ctrl_c()
                .await
                .expect("Failed to install Ctrl-C handler");
        })
        .await?)
}
