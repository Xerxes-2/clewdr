use axum::body::Body;
use colored::{ColoredString, Colorize};
use tokio::spawn;
use tracing::error;
use wreq::{Client, Proxy};
use wreq_util::Emulation;

use crate::{
    config::{CLEWDR_CONFIG, LOG_DIR},
    error::ClewdrError,
};

/// Helper function to format a boolean value as "Enabled" or "Disabled"
#[must_use]
pub fn enabled(flag: bool) -> ColoredString {
    if flag {
        "Enabled".green()
    } else {
        "Disabled".red()
    }
}

/// Helper function to print out JSON to a file in the log directory
///
/// # Arguments
/// * `json` - The JSON object to serialize and output
/// * `file_name` - The name of the file to write in the log directory
pub fn print_out_json(json: impl serde::ser::Serialize, file_name: &str) {
    if CLEWDR_CONFIG.load().no_fs {
        return;
    }
    let text = serde_json::to_string_pretty(&json).unwrap_or_default();
    print_out_text(text, file_name);
}

/// Helper function to print out text to a file in the log directory
///
/// # Arguments
/// * `text` - The text content to write
/// * `file_name` - The name of the file to write in the log directory
pub fn print_out_text(text: String, file_name: &str) {
    if CLEWDR_CONFIG.load().no_fs {
        return;
    }
    let path = LOG_DIR.join(file_name);
    spawn(async move {
        if let Some(dir) = path.parent()
            && let Err(e) = tokio::fs::create_dir_all(dir).await
        {
            error!("Failed to create log directory {}: {}", dir.display(), e);
            return;
        }
        if let Err(e) = tokio::fs::write(&path, text).await {
            error!("Failed to write log file {}: {}", path.display(), e);
        }
    });
}

/// Build the shared HTTP client, optionally behind `proxy`.
///
/// # Errors
/// If the TLS backend or the proxy configuration cannot be initialised.
pub fn build_http_client(proxy: Option<&Proxy>) -> Result<Client, wreq::Error> {
    let mut builder = Client::builder()
        .cookie_store(true)
        .emulation(Emulation::Chrome145);
    if let Some(proxy) = proxy {
        builder = builder.proxy(proxy.to_owned());
    }
    builder.build()
}

/// Timezone for the API
pub const TIME_ZONE: &str = "America/New_York";

/// Re-wrap an upstream response as an axum response, preserving status,
/// headers and the body stream.
///
/// # Errors
/// If the response cannot be assembled from the upstream status and body.
///
/// # Panics
/// If the builder is already in an error state when its headers are taken.
/// Only the status has been set at that point, so this cannot happen.
pub fn forward_response(in_: wreq::Response) -> Result<http::Response<Body>, ClewdrError> {
    let status = in_.status();
    let header = in_.headers().to_owned();
    let stream = in_.bytes_stream();
    let mut res = http::Response::builder().status(status);

    let headers = res.headers_mut().unwrap();
    for (key, value) in header {
        if let Some(key) = key {
            headers.insert(key, value);
        }
    }

    Ok(res.body(Body::from_stream(stream))?)
}
