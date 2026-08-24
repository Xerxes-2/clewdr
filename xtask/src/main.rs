//! Build automation for the ClewdR workspace.
//!
//! ClewdR is two crates that have to be built in order: `clewdr-frontend`
//! compiles to WebAssembly and Trunk writes the result into `static/`, which
//! the server then serves. `static/` is gitignored, so a fresh clone that runs
//! `cargo run` gets a server with no UI on the default `external-resource`
//! feature, and a compile error on `embed-resource`, where `include_dir!`
//! needs the directory to exist. Everything here exists to make that ordering
//! automatic rather than folklore.
//!
//! Run `cargo xtask` for the command list.

use std::{
    env,
    ffi::OsStr,
    fmt::Write as _,
    fs,
    path::{Path, PathBuf},
    process::{Child, Command, ExitCode, Stdio},
    thread,
    time::Duration,
};

mod ci;
mod dev;
mod verify_push;

/// Feature combinations that actually compile.
///
/// `embed-resource`/`external-resource` and `portable`/`xdg` are two
/// mutually-exclusive pairs, each enforced by a `compile_error!` in
/// `build.rs`. That makes `--all-features` unusable: it fails the build and
/// takes every real warning down with it, so each valid pair is checked
/// separately.
const FEATURE_COMBINATIONS: [&str; 4] = [
    "external-resource,portable",
    "embed-resource,portable",
    "external-resource,xdg",
    "embed-resource,xdg",
];

/// The wasm target the frontend is built for.
const WASM_TARGET: &str = "wasm32-unknown-unknown";

/// Port the backend listens on during `xtask dev`.
///
/// Pinned rather than read from the user's config because `Trunk.toml`'s proxy
/// has to point somewhere fixed. Forced on the child through `CLEWDR_PORT`, so
/// a different port in `clewdr.toml` cannot desynchronise the two.
const DEV_BACKEND_PORT: u16 = 8484;

/// Port Trunk's dev server listens on. Must match `[serve]` in `Trunk.toml`.
const DEV_FRONTEND_PORT: u16 = 3000;

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    let (command, rest) = args
        .split_first()
        .map_or(("help", &[][..]), |(first, rest)| (first.as_str(), rest));

    let result = match command {
        "dev" => dev::run(rest.contains(&"--release".to_string())),
        "build" => build(),
        "lint" => lint(),
        "fmt" => fmt(rest.contains(&"--check".to_string())),
        "test" => test(),
        "check" => Toolchain::detect().report(),
        "ci" => ci::run(),
        "verify-push" => verify_push::run_verification(),
        "help" | "--help" | "-h" => {
            print_help();
            Ok(())
        }
        other => Err(format!("unknown command `{other}`\n\n{HELP}")),
    };

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("\nerror: {message}");
            ExitCode::FAILURE
        }
    }
}

const HELP: &str = "\
Usage: cargo xtask <command>

Commands:
  dev [--release]   Run backend and frontend together with hot reload
  build             Release build of the frontend and the server
  lint              Clippy over every valid feature combination
  fmt [--check]     Format the workspace (always via nightly)
  test              Run the workspace test suite
  check             Report on the required toolchain pieces
  ci                fmt --check, lint and test, as CI runs them
  verify-push       Publish the images to a local registry and check them
";

fn print_help() {
    print!("{HELP}");
}

// ---------------------------------------------------------------------------
// Tasks
// ---------------------------------------------------------------------------

/// Release build: frontend first, then the server that embeds or serves it.
fn build() -> Result<(), String> {
    Toolchain::detect().require_frontend()?;

    step("Building frontend (release)");
    trunk(&["build", "--release"])?;

    step("Building server (release)");
    cargo(&["build", "--release", "-p", "clewdr"])?;

    let binary = if cfg!(windows) {
        "target/release/clewdr.exe"
    } else {
        "target/release/clewdr"
    };
    println!("\nBuilt {binary}");
    Ok(())
}

/// Clippy across every valid feature combination, plus the wasm frontend.
fn lint() -> Result<(), String> {
    let toolchain = Toolchain::detect();
    ensure_static_dir()?;

    for features in FEATURE_COMBINATIONS {
        step(&format!("Clippy [{features}]"));
        cargo(&[
            "clippy",
            "--workspace",
            "--all-targets",
            "--no-default-features",
            "--features",
            features,
            "--",
            "-D",
            "warnings",
        ])?;
    }

    // The frontend's real target is wasm; linting it only for the host would
    // miss anything behind a `target_arch` gate.
    if toolchain.wasm_target {
        step("Clippy [frontend, wasm]");
        cargo(&[
            "clippy",
            "-p",
            "clewdr-frontend",
            "--target",
            WASM_TARGET,
            "--",
            "-D",
            "warnings",
        ])?;
    } else {
        warn(&format!(
            "skipping the wasm frontend lint; install the target with\n    rustup target add {WASM_TARGET}"
        ));
    }

    // The workflows are the one part of the build that cannot be reproduced
    // locally, so the little of it that *is* checkable statically is worth
    // checking. actionlint bundles shellcheck for the `run:` blocks.
    if toolchain.actionlint {
        let workflows = workflow_files();
        if workflows.is_empty() {
            warn("skipping the workflow lint; no workflow files found");
        } else {
            step("Actionlint [workflows]");
            let mut args = vec!["-no-color".to_string()];
            args.extend(workflows);
            run("actionlint", &args, &workspace_root())
                .map_err(|_| "workflow lint failed".to_string())?;
        }
    } else {
        warn("skipping the workflow lint; actionlint is not on PATH");
    }

    Ok(())
}

/// The workflow files, relative to the workspace root.
///
/// Listed explicitly rather than letting actionlint discover them: its own
/// discovery walks up to a `.git` directory, which the nix sandbox has no copy
/// of, and it fails outright when it finds none.
fn workflow_files() -> Vec<String> {
    let dir = workspace_root().join(".github/workflows");
    let Ok(entries) = fs::read_dir(&dir) else {
        return Vec::new();
    };
    let mut files: Vec<String> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .is_some_and(|ext| ext == "yml" || ext == "yaml")
        })
        .filter_map(|path| path.to_str().map(str::to_string))
        .collect();
    // Sorted, so the lint reports in the same order everywhere.
    files.sort();
    files
}

/// Format the workspace.
///
/// Always routed through nightly: `.rustfmt.toml` sets `imports_granularity`
/// and `group_imports`, which stable rustfmt ignores *silently*. Formatting
/// with stable therefore looks like it worked while leaving imports untouched.
///
/// The nightly toolchain is found in one of two ways: normally through
/// `rustup run nightly`, and, when rustup is absent (e.g. inside a nix
/// devShell or sandbox), through `CLEWDR_NIGHTLY_CARGO`, which must point at
/// a nightly `cargo` binary.
fn fmt(check_only: bool) -> Result<(), String> {
    let toolchain = Toolchain::detect();
    if !toolchain.nightly {
        return Err(format!(
            "nightly rustfmt is required, because .rustfmt.toml uses nightly-only options\n\
             ({}).\n    rustup toolchain install nightly\n\
             or set CLEWDR_NIGHTLY_CARGO to a nightly cargo binary",
            nightly_only_options().join(", ")
        ));
    }

    step(if check_only {
        "Checking formatting (nightly)"
    } else {
        "Formatting (nightly)"
    });

    let mut args = vec!["fmt", "--all"];
    if check_only {
        args.push("--check");
    }
    if let Some(nightly_cargo) = nightly_cargo() {
        // A nightly cargo binary; no rustup shim in between.
        //
        // Its directory has to lead PATH, because cargo resolves the `fmt`
        // subcommand through PATH rather than next to its own binary. Without
        // this, a stable `cargo-fmt` and `rustfmt` found earlier on PATH
        // formatted the workspace with stable rustfmt, which ignores every
        // nightly-only option in .rustfmt.toml and says so only in a warning:
        // the CI check job was doing exactly that, 18 warnings per run, from
        // the day the nix checks derivation was introduced.
        let bin_dir = Path::new(&nightly_cargo)
            .parent()
            .ok_or_else(|| format!("CLEWDR_NIGHTLY_CARGO has no directory: {nightly_cargo}"))?
            .to_path_buf();
        run_with_path_prefix(&nightly_cargo, &args, &workspace_root(), &bin_dir)
            .map_err(|_| fmt_failed(check_only))?;
    } else {
        // Invoked through `rustup run` rather than `cargo +nightly`: CARGO points
        // at a concrete toolchain binary, and `+toolchain` is a rustup shim
        // feature that such a binary rejects outright.
        let mut rustup_args = vec!["run", "nightly", "cargo"];
        rustup_args.extend_from_slice(&args);
        run("rustup", &rustup_args, &workspace_root()).map_err(|_| fmt_failed(check_only))?;
    }
    Ok(())
}

fn fmt_failed(check_only: bool) -> String {
    if check_only {
        "formatting check failed; run `cargo xtask fmt`".to_string()
    } else {
        "formatting failed".to_string()
    }
}

/// Run the workspace tests.
fn test() -> Result<(), String> {
    ensure_static_dir()?;
    step("Running tests");
    cargo(&["test", "--workspace"])
}

// ---------------------------------------------------------------------------
// Toolchain probing
// ---------------------------------------------------------------------------

/// Which optional pieces of the toolchain are present.
#[expect(
    clippy::struct_excessive_bools,
    reason = "a detection report: one independent flag per optional tool, which is the point of grouping them"
)]
struct Toolchain {
    /// `wasm32-unknown-unknown` is installed.
    wasm_target: bool,
    /// The `trunk` binary is on `PATH`.
    trunk: bool,
    /// A nightly toolchain is installed.
    nightly: bool,
    /// The `actionlint` binary is on `PATH`.
    actionlint: bool,
}

impl Toolchain {
    fn detect() -> Self {
        Self {
            wasm_target: probe("rustup", &["target", "list", "--installed"])
                .is_some_and(|out| out.lines().any(|line| line.trim() == WASM_TARGET))
                || wasm_target_in_sysroot(),
            trunk: probe("trunk", &["--version"]).is_some(),
            nightly: probe("rustup", &["toolchain", "list"])
                .is_some_and(|out| out.contains("nightly"))
                || nightly_cargo().is_some(),
            actionlint: probe("actionlint", &["-version"]).is_some(),
        }
    }

    /// Everything needed to build the frontend.
    fn require_frontend(&self) -> Result<(), String> {
        let mut missing = String::new();
        if !self.wasm_target {
            let _ = write!(missing, "\n    rustup target add {WASM_TARGET}");
        }
        if !self.trunk {
            let _ = write!(
                missing,
                "\n    cargo binstall trunk    (or: cargo install trunk)"
            );
        }
        if missing.is_empty() {
            Ok(())
        } else {
            Err(format!(
                "missing frontend prerequisites. Install with:{missing}"
            ))
        }
    }

    /// Print what is present and what is not.
    fn report(&self) -> Result<(), String> {
        let mark = |ok: bool| if ok { "ok     " } else { "MISSING" };
        println!("{} {WASM_TARGET}", mark(self.wasm_target));
        println!("{} trunk", mark(self.trunk));
        println!("{} nightly toolchain (needed by `fmt`)", mark(self.nightly));
        println!("{} actionlint (workflow lint)", mark(self.actionlint));

        if self.wasm_target && self.trunk && self.nightly && self.actionlint {
            println!("\nEverything needed is installed.");
            return Ok(());
        }
        println!("\nInstall what is missing:");
        if !self.wasm_target {
            println!("    rustup target add {WASM_TARGET}");
        }
        if !self.trunk {
            println!("    cargo binstall trunk");
        }
        if !self.nightly {
            println!("    rustup toolchain install nightly");
        }
        if !self.actionlint {
            println!("    cargo binstall actionlint    (or: nix develop)");
        }
        Err("some prerequisites are missing".to_string())
    }
}

/// Run a command purely to read its output, returning `None` if it is absent.
fn probe(program: &str, args: &[&str]) -> Option<String> {
    let output = Command::new(program).args(args).output().ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).into_owned())
}

/// The nightly cargo binary named by `CLEWDR_NIGHTLY_CARGO`, if any.
///
/// Set by the nix devShell and the checks derivation, where rustup does not
/// exist. An empty value counts as unset, so that exporting it empty falls
/// back to rustup instead of trying to run "".
fn nightly_cargo() -> Option<String> {
    env::var("CLEWDR_NIGHTLY_CARGO")
        .ok()
        .filter(|v| !v.is_empty())
}

/// Whether the wasm std is present in the toolchain's sysroot.
///
/// The rustup-less equivalent of `rustup target list --installed`: rust
/// toolchains install target stds under `<sysroot>/lib/rustlib`, which covers
/// nix toolchains (rust-overlay/fenix) where `rustup` does not exist.
fn wasm_target_in_sysroot() -> bool {
    let Some(sysroot) = probe(&cargo_bin(), &["rustc", "--", "--print", "sysroot"]) else {
        return false;
    };
    Path::new(sysroot.trim())
        .join("lib/rustlib")
        .join(WASM_TARGET)
        .is_dir()
}

/// The nightly-only keys in `.rustfmt.toml`, for the error message.
fn nightly_only_options() -> Vec<String> {
    let Ok(contents) = fs::read_to_string(workspace_root().join(".rustfmt.toml")) else {
        return vec!["nightly-only options".to_string()];
    };
    contents
        .lines()
        .filter_map(|line| line.split('=').next())
        .map(|key| key.trim().to_string())
        .filter(|key| !key.is_empty() && !key.starts_with('#'))
        .collect()
}

// ---------------------------------------------------------------------------
// Process helpers
// ---------------------------------------------------------------------------

/// The workspace root, derived from this crate's location.
fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask always lives one level below the workspace root")
        .to_path_buf()
}

/// Run `cargo` in the workspace root, inheriting stdio.
fn cargo(args: &[&str]) -> Result<(), String> {
    run(&cargo_bin(), args, &workspace_root())
}

/// Run `trunk` in the frontend directory.
fn trunk(args: &[&str]) -> Result<(), String> {
    run("trunk", args, &workspace_root().join("clewdr-frontend"))
}

/// The cargo binary that invoked us, so child builds reuse the same toolchain.
fn cargo_bin() -> String {
    env::var("CARGO").unwrap_or_else(|_| "cargo".to_string())
}

/// Run a command to completion, mapping a non-zero exit into an error.
fn run(program: &str, args: &[impl AsRef<OsStr>], dir: &Path) -> Result<(), String> {
    let status = spawn(program, args, dir)?
        .wait()
        .map_err(|e| format!("failed waiting for `{program}`: {e}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("`{program}` exited with {status}"))
    }
}

/// Start a child process with inherited stdio.
fn spawn(program: &str, args: &[impl AsRef<OsStr>], dir: &Path) -> Result<Child, String> {
    Command::new(program)
        .args(args)
        .current_dir(dir)
        .stdin(Stdio::inherit())
        .spawn()
        .map_err(|e| format!("failed to start `{program}`: {e}"))
}

/// Run a command with one directory prepended to `PATH`.
///
/// For toolchains that are a directory of binaries rather than a rustup shim:
/// cargo finds its subcommands (`cargo-fmt`, `cargo-clippy`) on `PATH`, so a
/// toolchain that is not on `PATH` gets its subcommands served by whichever
/// other toolchain is.
fn run_with_path_prefix(
    program: &str,
    args: &[impl AsRef<OsStr>],
    dir: &Path,
    path_prefix: &Path,
) -> Result<(), String> {
    let path = match env::var_os("PATH") {
        Some(existing) => {
            let mut dirs = vec![path_prefix.to_path_buf()];
            dirs.extend(env::split_paths(&existing));
            env::join_paths(dirs).map_err(|e| format!("cannot build PATH: {e}"))?
        }
        None => path_prefix.as_os_str().to_os_string(),
    };
    let status = Command::new(program)
        .args(args)
        .current_dir(dir)
        .env("PATH", path)
        .stdin(Stdio::inherit())
        .spawn()
        .map_err(|e| format!("failed to start `{program}`: {e}"))?
        .wait()
        .map_err(|e| format!("failed waiting for `{program}`: {e}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("`{program}` failed"))
    }
}

/// Kills its child when dropped, so one task dying does not orphan the other.
struct ChildGuard {
    name: &'static str,
    child: Child,
}

impl ChildGuard {
    /// Whether the child has exited, and with what status.
    fn finished(&mut self) -> Option<String> {
        match self.child.try_wait() {
            Ok(Some(status)) => Some(format!("{} exited with {status}", self.name)),
            Ok(None) => None,
            Err(e) => Some(format!("{} could not be polled: {e}", self.name)),
        }
    }
}

impl Drop for ChildGuard {
    fn drop(&mut self) {
        if matches!(self.child.try_wait(), Ok(None)) {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}

/// Ensure `static/` exists so `include_dir!` can expand.
///
/// The `embed-resource` feature embeds `static/` at compile time, so linting
/// that combination on a clone that has never built the frontend would fail on
/// a missing directory rather than on anything real. A placeholder keeps the
/// lint honest without a full wasm build; a real `trunk build` overwrites it.
fn ensure_static_dir() -> Result<(), String> {
    let dir = workspace_root().join("static");
    let index = dir.join("index.html");
    if index.exists() {
        return Ok(());
    }
    fs::create_dir_all(&dir).map_err(|e| format!("could not create {}: {e}", dir.display()))?;
    fs::write(
        &index,
        "<!doctype html>\n<meta charset=\"utf-8\">\n<title>ClewdR</title>\n\
         <p>Frontend not built. Run <code>cargo xtask build</code>.</p>\n",
    )
    .map_err(|e| format!("could not write {}: {e}", index.display()))?;
    warn("static/ was empty; wrote a placeholder. Run `cargo xtask build` for the real UI.");
    Ok(())
}

// ---------------------------------------------------------------------------
// Output
// ---------------------------------------------------------------------------

fn step(message: &str) {
    println!("\n\x1b[1;36m==>\x1b[0m \x1b[1m{message}\x1b[0m");
}

fn warn(message: &str) {
    println!("\x1b[1;33mwarning:\x1b[0m {message}");
}

fn info(message: &str) {
    println!("\x1b[1;32m   \x1b[0m {message}");
}

/// Give a spawned process a moment before reporting it as ready.
fn settle() {
    thread::sleep(Duration::from_millis(300));
}
