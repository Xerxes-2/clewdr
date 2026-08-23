# Working on clewdr

## Version control

This repo is jj, colocated with git. Both `.jj/` and `.git/` exist, so git
tooling can read the state, but **no git command may write.** Reading is fine:
`git log`, `git status`, `git diff`, `git show`, `git blame`. Everything else
is jj's:

| not this            | this                                  |
| ------------------- | ------------------------------------- |
| `git mv a b`        | `mv a b`                              |
| `git rm f`          | `rm f`                                |
| `git add`           | nothing — jj snapshots automatically  |
| `git commit`        | `jj commit -m "..."`                  |
| `git checkout`      | `jj new` / `jj edit`                  |
| `git reset`         | `jj undo`                             |
| `git restore f`     | `jj restore f`                        |
| `git stash`         | `jj new` — the working copy is a commit |
| `git rebase`        | `jj rebase`                           |
| `git pull` / `push` | `jj git fetch` / `jj git push`        |

The trap is not `git commit`; that one announces itself as version control.
It is `git mv` and `git rm` — operations that feel like touching files rather
than touching history, and so never prompt the thought "check how this repo
does VCS". Those are the ones that actually slip through. Note what the first
two rows are saying: there is no `jj mv`, because jj snapshots the working copy
on every command and detects renames by content afterwards. Plain `mv` is not a
workaround, it is the whole procedure, and it is shorter than the git spelling
you were reaching for.

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

CI runs exactly that command, inside one cached nix derivation
(`nix build .#checks.x86_64-linux.ci`). Locally, either:

    cargo xtask ci       # with rustup-managed toolchains, or
    nix develop -c cargo xtask ci

`nix develop` gives the pinned toolchain set: stable rustc 1.98 with wasm32
and clippy, nightly rustfmt via `CLEWDR_NIGHTLY_CARGO`, trunk, wasm-bindgen
0.2.127, libclang and the build tools. xtask discovers the wasm target through
the toolchain sysroot when rustup is absent, and nightly through the env var;
both paths behave identically under rustup.

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

## Reproducing a CI failure

Every linux job is a nix derivation, so reproduce it locally instead of
pushing a commit to watch what happens:

    nix build -L .#checks.x86_64-linux.ci      # the check job
    nix build -L .#clewdr-musl-aarch64         # any build job's artifact
    nix build -L .#image-arm64                 # either image

When a build fails inside a derivation, `--keep-failed` leaves the build
directory in `/tmp/nix-build-*`, where the failing `cargo` invocation can be
re-run by hand with the same environment; `nix log <store-path>` replays a
build that already ran. `nix flake check --no-build` catches evaluation
mistakes in seconds without building anything, and `nix build --rebuild`
answers whether an output is reproducible.

The docker job's publish step is not a nix build, so it has its own harness:

    nix develop -c cargo xtask verify-push

That runs `scripts/publish-images.sh` — the same script the job runs — against
a registry in a throwaway container, then checks the tag scheme and that each
tag resolves to an index of linux/amd64 + linux/arm64. Needs podman or docker.

What none of this covers, i.e. what still needs a push to test: GitHub Actions
itself (action versions, artifact upload/download, ghcr auth) and the
windows/macOS matrix rows, which do not go through nix.

## Layout

    src/                 the server
    clewdr-types/        types shared with the frontend
    clewdr-frontend/     leptos UI, compiled to wasm and served by the backend
    xtask/               the build tool; not part of default-members

`CookiePool` (`src/services/cookie_pool.rs`) owns the cookies outright. They
are moved out of the config at startup and only rejoin it when the file is
written, so `CLEWDR_CONFIG`'s cookie fields are empty at runtime. Anything that
saves the config has to pass a snapshot from the pool.

## Cross-compiling and images

All four linux targets (gnu/musl × x86_64/aarch64) plus android cross-build
from one x86_64-linux machine through nix:

    nix build .#clewdr-musl-x86_64      # and -gnu-x86_64, -musl-aarch64, -gnu-aarch64
    nix build .#clewdr-android-aarch64  # aarch64-linux-android, NDK r27 from nixpkgs
    nix build .#image-amd64 .#image-arm64

The TLS stack is BoringSSL, via `wreq` → `btls-sys`, and it contains C++ —
the historical reason the old CI needed `rust-musl-cross` docker images and
per-architecture runners. nix's cross stdenv supplies the musl g++ and static
libstdc++ itself, and `rustPlatform.bindgenHook` feeds the cross clang's libc
cflags into `BINDGEN_EXTRA_CLANG_ARGS`, which is the part the old images got
wrong (see wiki/nix-convergence.md for the full story). btls-sys also needs
`git` in the sandbox (it initialises its vendored BoringSSL submodule), so a
new nativeBuildInput must not drop it.

The android target needs six things the other targets do not; all of them are
in `flake.nix`, and each one fails in a way that points somewhere else:

- **`useAndroidPrebuilt = true`** on the crossSystem. Without it nixpkgs tries
  to build the whole android toolchain from source and dies in compiler-rt on
  a missing `pthread.h`.
- **The toolchain comes from the native package set**, not from the cross set:
  rust-overlay defines nothing for `pkgsTargetTarget` of an android cross (no
  rustc runs *on* android), and crane's `spliceToolchain` merges that empty
  set over the real one. The native toolchain with the android std added does
  the job — rustc runs on x86_64 either way.
- **`bindgenHook` cannot be used** (it references the cross set's clang, which
  is the from-source one). The flake reads the same cflags out of the NDK
  cc-wrapper's `nix-support` files instead.
- **`SYSROOT` must be unset** before cargo runs. nixpkgs' cross stdenv exports
  it (the bionic sysroot) and cargo's target-info probe passes it to rustc as
  `--sysroot`, where `rustlib` does not exist.
- **The short target name.** rustc 1.98 dropped the
  `aarch64-unknown-linux-android` alias; only the builtin
  `aarch64-linux-android` resolves (this is also the name cargo-ndk uses, and
  the one `.cargo/config.toml` keys its rpath rustflags on). nixpkgs derives
  the long form, so `CARGO_BUILD_TARGET` is pinned by hand.
- **cargo-ndk's env contract.** `build.rs` copies `libc++_shared.so` next to
  the binary using `CARGO_NDK_SYSROOT_LIBS_PATH`, and btls-sys wants
  `ANDROID_NDK_HOME` pointing at the NDK root
  (`…/libexec/android-sdk/ndk-bundle` in the nixpkgs layout). The linker must
  also be pointed at the NDK clang wrapper, since rustc defaults to `cc`.

The images keep `gcr.io/distroless/static-debian13` as their runtime base,
pinned by digest in the flake (`pullImage` + `buildLayeredImage`); the static
musl binary is upx'd and layered on top, with the frontend embedded
(`embed-resource,xdg`). Multi-arch manifests are assembled by
`crane index append` without a docker daemon; see the `docker` job in
`.github/workflows/build.yml`.

Windows and macOS stay on their native CI paths — nix does not run on Windows
runners, this nixpkgs has dropped x86_64-darwin, and linux→darwin cross does
not exist. Neither is where the failures were.
