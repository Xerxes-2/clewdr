//! The `dev` task: backend and frontend side by side, with hot reload.
//!
//! Two processes run concurrently:
//!
//! * the server on [`DEV_BACKEND_PORT`], serving `/api` and the Claude proxy;
//! * `trunk serve` on [`DEV_FRONTEND_PORT`], which rebuilds the wasm on every
//!   source change and reloads the browser.
//!
//! The frontend calls the API with relative paths (`/api/version` and
//! friends), so those requests would otherwise hit Trunk's dev server rather
//! than the backend. `Trunk.toml` carries a `[[proxy]]` entry that forwards
//! `/api` to the backend, which is what makes the split origin work at all.
//!
//! Use [`DEV_FRONTEND_PORT`] while developing; the backend's own port serves
//! whatever wasm was last written to `static/` and will look stale.

use std::{thread, time::Duration};

use crate::{
    ChildGuard, DEV_BACKEND_PORT, DEV_FRONTEND_PORT, Toolchain, cargo_bin, info, settle, spawn,
    step, workspace_root,
};

/// How often to check whether either child has exited.
const POLL_INTERVAL: Duration = Duration::from_millis(200);

/// Run both halves until one exits or the user interrupts.
pub(crate) fn run(release: bool) -> Result<(), String> {
    Toolchain::detect().require_frontend()?;

    step("Starting backend");
    let mut backend = ChildGuard {
        name: "backend",
        child: start_backend(release)?,
    };
    settle();
    // A backend that dies immediately (port in use, bad config) would otherwise
    // leave Trunk proxying into nothing.
    if let Some(reason) = backend.finished() {
        return Err(format!("{reason} during startup"));
    }
    info(&format!("listening on http://127.0.0.1:{DEV_BACKEND_PORT}"));

    step("Starting frontend (hot reload)");
    let mut frontend = ChildGuard {
        name: "frontend",
        child: start_frontend()?,
    };
    settle();

    println!();
    info(&format!(
        "open \x1b[1mhttp://127.0.0.1:{DEV_FRONTEND_PORT}\x1b[0m"
    ));
    info("frontend changes reload automatically; restart for backend changes");
    info("press Ctrl-C to stop both");

    // Whichever exits first ends the session; the other is killed by its guard.
    loop {
        if let Some(reason) = frontend.finished() {
            return finish(&reason);
        }
        if let Some(reason) = backend.finished() {
            return finish(&reason);
        }
        thread::sleep(POLL_INTERVAL);
    }
}

/// Report how the session ended.
///
/// A clean exit is how Ctrl-C looks from here: the terminal delivers the
/// signal to the whole process group, so the children stop on their own.
fn finish(reason: &str) -> Result<(), String> {
    println!();
    if reason.contains("exit status: 0") || reason.contains("exit code: 0") {
        info("stopped");
        Ok(())
    } else {
        Err(reason.to_string())
    }
}

/// Launch the server, pinning the port the Trunk proxy expects.
fn start_backend(release: bool) -> Result<std::process::Child, String> {
    // `-p` is required: the workspace also builds a `clewdr-frontend` binary,
    // so a bare `cargo run` cannot tell which one is meant.
    let mut args = vec!["run", "-p", "clewdr"];
    if release {
        args.push("--release");
    }
    let mut command = std::process::Command::new(cargo_bin());
    command
        .args(&args)
        .current_dir(workspace_root())
        // Environment variables carry a CLEWDR_ prefix and take precedence
        // over the file, so this overrides whatever port clewdr.toml sets.
        .env("CLEWDR_PORT", DEV_BACKEND_PORT.to_string());
    command
        .spawn()
        .map_err(|e| format!("failed to start the backend: {e}"))
}

/// Launch `trunk serve`, which watches, rebuilds and live-reloads.
fn start_frontend() -> Result<std::process::Child, String> {
    spawn(
        "trunk",
        &["serve", "--port", &DEV_FRONTEND_PORT.to_string()],
        &workspace_root().join("clewdr-frontend"),
    )
}
