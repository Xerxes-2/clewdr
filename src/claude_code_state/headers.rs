//! Messages and `count_tokens` use the CLI's API identity, rather than its
//! axios/profile user agent or the browser identity used to authorize cookies.

use http::HeaderMap;

use super::chat::CLAUDE_API_VERSION;
use crate::config::CLAUDE_CODE_API_USER_AGENT;

pub(super) fn api_request(
    client: &wreq::Client,
    url: &str,
    token: &str,
    beta: &str,
) -> wreq::RequestBuilder {
    client
        .post(url)
        .bearer_auth(token)
        .headers(api_headers())
        .header("anthropic-version", CLAUDE_API_VERSION)
        .header("anthropic-beta", beta)
}

fn api_headers() -> HeaderMap {
    [
        ("user-agent", CLAUDE_CODE_API_USER_AGENT),
        ("accept", "application/json"),
        ("content-type", "application/json"),
        ("accept-encoding", "gzip, deflate, br, zstd"),
        ("anthropic-dangerous-direct-browser-access", "true"),
        ("x-app", "cli"),
        ("x-stainless-retry-count", "0"),
        ("x-stainless-timeout", "600"),
        ("x-stainless-lang", "js"),
        ("x-stainless-package-version", "0.128.0"),
        ("x-stainless-runtime", "node"),
        ("x-stainless-runtime-version", "v26.12.1"),
        ("x-stainless-os", stainless_os()),
        ("x-stainless-arch", stainless_arch()),
    ]
    .into_iter()
    .map(|(key, value)| {
        (
            http::HeaderName::from_static(key),
            http::HeaderValue::from_static(value),
        )
    })
    .collect()
}

fn stainless_os() -> &'static str {
    match std::env::consts::OS {
        "ios" => "iOS",
        "android" => "Android",
        "macos" => "MacOS",
        "windows" => "Windows",
        "freebsd" => "FreeBSD",
        "openbsd" => "OpenBSD",
        "linux" => "Linux",
        _ => "Unknown",
    }
}

fn stainless_arch() -> &'static str {
    match std::env::consts::ARCH {
        "x86_64" => "x64",
        "aarch64" => "arm64",
        "x86" => "x32",
        "arm" => "arm",
        _ => "unknown",
    }
}
