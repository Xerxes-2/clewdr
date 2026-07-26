//! The `ci` task: exactly what the CI workflow runs, runnable locally.
//!
//! Keeping this as one command means the workflow file and a developer's
//! machine cannot drift apart: CI calls `cargo xtask ci`, and so can you.

use crate::{fmt, lint, step, test};

/// Run the full check suite, stopping at the first failure.
pub fn run() -> Result<(), String> {
    fmt(true)?;
    lint()?;
    test()?;
    step("All checks passed");
    Ok(())
}
