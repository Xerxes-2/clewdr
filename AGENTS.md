# Working on clewdr

## Version control

This repo is jj, colocated with git. Both `.jj/` and `.git/` exist, so git
tooling can read the state, but every mutation goes through jj — a stray
`git commit` or `git checkout` desynchronises the operation log.

End a unit of work with a single command:

    jj commit -m "..."

Not `jj describe -m`. Both set the message, but `describe` leaves `@` open,
and jj snapshots the working copy on every command, so the next file you edit
silently amends a commit you already considered finished. `jj commit` closes
the change and opens an empty one in the same step, which leaves nothing to
remember at the start of the next task.

If something did land in the wrong change, `jj split -r <rev> <paths>` pulls it
back out by path, and `jj undo` reverses the last operation.

Never pass `-i`/`--interactive` to anything, and never run `jj resolve`: they
open a TUI that hangs a non-interactive session.

## Checks

`cargo xtask ci` is what CI runs, and it is the single gate to satisfy:

    cargo xtask ci        # fmt --check, then lint, then test

The pieces are available separately as `cargo xtask fmt|lint|test`, and
`cargo xtask check` reports which of the three optional toolchain pieces
(nightly, wasm target, trunk) are installed.

Two things about it are easy to trip over:

- **Formatting requires nightly.** `.rustfmt.toml` uses nightly-only options,
  so `cargo xtask fmt` always shells out to the nightly toolchain. Without it
  installed, CI fails on a diff you cannot reproduce with stable `cargo fmt`.
- **Lint covers four feature combinations**, the cross product of
  `external-resource`/`embed-resource` with `portable`/`xdg`, each with
  `-D warnings`. Cargo reuses most of the work, so this is far cheaper than it
  looks. The frontend is linted separately against `wasm32-unknown-unknown`,
  because linting it for the host would skip everything behind
  `#[cfg(target_arch = "wasm32")]`.

Clippy runs at `pedantic`. The workspace lint table has no `allow` entries by
design: exceptions go at the site as `#[expect(..., reason = "...")]`, so each
one carries its justification.

## Layout

    src/                 the server
    clewdr-types/        types shared with the frontend
    clewdr-frontend/     leptos UI, compiled to wasm and served by the backend
    xtask/               the build tool; not part of default-members

`CookiePool` (`src/services/cookie_pool.rs`) owns the cookies outright. They
are moved out of the config at startup and only rejoin it when the file is
written, so `CLEWDR_CONFIG`'s cookie fields are empty at runtime. Anything that
saves the config has to pass a snapshot from the pool.

## Cross-compiling

The TLS stack is BoringSSL, via `wreq` → `btls-sys`, and it contains C++. That
single fact drives the whole musl story: Debian's `musl-tools` provides only a
`musl-gcc` wrapper script with no C++ compiler and no musl `libstdc++.a`, so
BoringSSL cannot be built with it. The failure surfaces from CMake as
`Could NOT find Threads`, which points nowhere near the cause.

Use an image built with `musl-cross-make`, which ships a real
`x86_64-unknown-linux-musl-g++`:

    ghcr.io/rust-cross/rust-musl-cross:<arch>-musl

No `CC`/`CXX`/`RUSTFLAGS` overrides are needed there. In particular the
`BORING_BSSL_RUST_CPPLIB=static=stdc++` workaround seen in wreq-python is for
building a shared Python extension; a musl binary links `crt-static` and picks
up the archive on its own.

Build each architecture on a runner of that architecture. Cross-building
arm64 from an amd64 host fails in bindgen, which reaches for the host's glibc
headers (`bits/libc-header-start.h`) because the image sets no
`BINDGEN_EXTRA_CLANG_ARGS`.
