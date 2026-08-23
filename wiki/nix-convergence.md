# Nix convergence: can one flake replace the Actions / Dockerfile / local-build trio?

Research notes on replacing `build.yml` + `docker-build.yml` + `Dockerfile` + local
rustup setup with a single nix flake covering `x86_64-linux` and `aarch64-linux`,
glibc and musl. Sources are primary only: official docs and README/source of the
repos in question. Claims I could not verify from a primary source are marked
**UNVERIFIED** with the test that would settle them. Where I verified nixpkgs
attributes by evaluation, that was done with `nix eval --impure` against the
nixpkgs snapshot installed on this machine (Determinate Systems'
`nixpkgs-weekly`); the file paths cited are nixpkgs `master` on GitHub.

**Verdict up front: yes — settled empirically on 2026-08-01 by a working spike
flake (`flake.nix` in the repo root, `SPIKE`-marked).** All four linux
combinations (glibc/musl × x86_64/aarch64) build hermetically from one
x86_64 machine in the nix sandbox, including btls-sys's vendored BoringSSL;
the musl outputs are fully static (no PT_INTERP). Remaining open items: (a)
the frontend wasm build (crane has a trunk example; the wasm-bindgen-cli in
nixpkgs is a version behind the lockfile, so it must be built from the
lockfile), (b) replacing the `rustup run nightly` seam in `xtask`, (c)
assembling/pushing multi-arch OCI output without a docker daemon, and (d) a
`builtins.fromTOML` limitation (TOML 1.0) that currently forces
`cargoArtifacts = null` in the spike (§7). Details and tests below.

## 1. Crane (ipetkov/crane)

### 1a. Cross-compilation: supported, two documented patterns

Crane's docs ship two cross examples, both built in crane's own CI matrix
(`.github/workflows/test.yml`, job `examples-linux`, matrix entries
`cross-musl`, `cross-rust-overlay`, `cross-windows`, `trunk`, `trunk-workspace`).

1. **Rust-target-only pattern** (`examples/cross-musl/flake.nix`): override the
   toolchain with the musl rust target and set env vars, no C cross toolchain:

   ```nix
   craneLib = (crane.mkLib pkgs).overrideToolchain (
     p: p.rust-bin.stable.latest.default.override {
       targets = [ "x86_64-unknown-linux-musl" ]; });
   my-crate = craneLib.buildPackage {
     src = craneLib.cleanCargoSource ./.;
     strictDeps = true;
     CARGO_BUILD_TARGET = "x86_64-unknown-linux-musl";
     CARGO_BUILD_RUSTFLAGS = "-C target-feature=+crt-static";
   };
   ```

   This works only for pure-Rust crates — there is no musl C/C++ compiler in
   that example. Crane issue #838 shows the failure mode: adding `aws-lc-sys`
   to that example fails with `cannot find -lc` (closed).

2. **pkgsCross / splicing pattern** (`examples/cross-rust-overlay/flake.nix`):
   `import nixpkgs { crossSystem = "aarch64-linux"; }`, then wrap the
   `buildPackage` call in `pkgs.callPackage` "so that Nix can 'splice' the
   packages ... for the appropriate targets", with `nativeBuildInputs`
   (build-platform, e.g. `pkg-config`) and `buildInputs` (host-platform, e.g.
   `openssl`) split by `strictDeps = true` (comments in the flake itself).
   This is the pattern clewdr needs, because wreq→btls-sys compiles C++.

The docs pages `docs/examples/cross-musl.md` and
`docs/examples/cross-rust-overlay.md` are one-liners pointing at these flakes
("To build a cargo project with musl to crate statically linked binaries ...").

### 1b. buildDepsOnly

`docs/API.md` (§`craneLib.buildDepsOnly`): "Create a derivation which will only
build all dependencies of a cargo workspace. Useful for splitting up cargo
projects into two derivations: one which only builds dependencies and needs to
be rebuilt when a Cargo.lock file changes, and another which inherits the cargo
artifacts from the first and (quickly) builds just the application itself."
This replaces cargo-chef's `prepare`/`cook` split; the project's current
Dockerfile uses `cargo chef prepare --recipe-path recipe.json` for exactly
this purpose.

### 1c. Trunk/wasm under the sandbox: yes, a full example

`examples/trunk-workspace/flake.nix` builds a wasm client with
`craneLib.buildTrunkPackage` and a native server in one workspace. What it
does: toolchain overridden with `targets = [ "wasm32-unknown-unknown" ]`
("required for trunk"); `wasmArgs` sets `CARGO_BUILD_TARGET =
"wasm32-unknown-unknown"` and `cargoExtraArgs = "--package=client"`;
`buildDepsOnly` for each side; and crucially `wasm-bindgen-cli =
pkgs.wasm-bindgen-cli_0_2_114;` with the comment "The version of
wasm-bindgen-cli here must match the one from Cargo.lock." (`lib/buildTrunkPackage.nix`
has a warning message for this ("The version of the tool `wasm-bindgen-cli` ...
must match the version of the `wasm-bindgen` (Rust library, check your
Cargo.lock)"). `buildTrunkPackage` adds `binaryen`
(wasm-opt), `dart-sass`, `trunk`, and `wasm-bindgen-cli` to `nativeBuildInputs`
and sets `TRUNK_SKIP_VERSION_CHECK=true`, `TRUNK_TOOLS_SASS/WASM_BINDGEN/WASM_OPT`
to the nixpkgs-provided binaries. Cargo deps are vendored (crane writes a
`.cargo/config.toml` replacing the `crates-io` source,
`lib/vendorCargoRegistries.nix:100-109`, applied via
`lib/setupHooks/configureCargoVendoredDepsHook.sh`), so the build needs no
network. Docs: `docs/examples/trunk.md` states the four constraints (wasm
target, `CARGO_BUILD_TARGET` for buildDepsOnly, source filtering for html/css
assets, wasm-bindgen-cli version). Note clewdr's frontend is trunk-based (the
Dockerfile runs `trunk build --release` in `clewdr-frontend`), so this example
maps directly.

### 1d. bindgen / cmake under crane

Crane has **no documentation mentioning LIBCLANG_PATH** (grep of `docs/`,
`lib/`, `examples/` finds nothing). What exists:

- Issue #250 (closed, OpenCV + bindgen): the maintainer's answer is the
  standard pattern — put `LIBCLANG_PATH = "${pkgs.libclang.lib}/lib"` on the
  derivation, and the same env var works in a devShell via `pkgs.mkShell`.
- Issue #654 (closed): bindgen can't find C headers (`stdarg.h`) in
  dependency builds — the failure was missing libc headers for clang, so
  `LIBCLANG_PATH` alone isn't enough; clang's libc headers must be
  available too. The nixpkgs-native fix is
  `rustPlatform.bindgenHook` (`pkgs/build-support/rust/hooks/rust-bindgen-hook.sh`),
  which sets `LIBCLANG_PATH=@libclang@/lib` and propagates the clang toolchain's
  cflags via `BINDGEN_EXTRA_CLANG_ARGS`.
- `docs/faq/rebuilds-bindgen.md`: bindgen rebuilds in dep-only builds because
  nixpkgs' reproducible-builds hook changes `-frandom-seed` per derivation;
  workaround `NIX_OUTPATH_USED_AS_RANDOM_SEED = "aaaaaaaaaa"` on all
  artifact-sharing derivations.
- `docs/faq/cross-compiling-aws-lc-sys.md`: the canonical cross + cmake + C
  recipe — `nativeBuildInputs = [ buildPackages.cmake ... ]`, per-target env
  overrides like `CC_x86_64-pc-windows-gnu`, and `CFLAGS` fixes. (aws-lc-sys
  is the closest analogue to BoringSSL/btls-sys: cmake-driven C build.)

Nothing in crane's docs covers libclang under cross specifically.
**SETTLED 2026-08-01** by the spike (§7): btls-sys's bindgen needs both
`LIBCLANG_PATH` (pointing at build-platform libclang) and nixpkgs'
`rustPlatform.bindgenHook` (populates `BINDGEN_EXTRA_CLANG_ARGS` from the
cross clang's libc cflags, which is what makes `stdlib.h` visible for the
target). Without the hook, bindgen fails with `stdlib.h file not found` on
all four targets.

### 1e. Cargo.lock handling and --locked

Crane reads the existing `Cargo.lock` directly: `cargoLock = args.cargoLock or
(src + "/Cargo.lock")`, parsed with `builtins.fromTOML` to vendor deps
(`lib/vendorCargoDeps.nix:29-46`; API.md documents `cargoLock`,
`cargoLockContents`, `cargoLockParsed` at lines 1252ff and 1380ff). A missing
lockfile is a hard error (same file, "unable to find Cargo.lock"). All cargo
invocations default to `cargoExtraArgs ? "--locked"`
(`lib/cargoBuild.nix:7`, `lib/buildPackage.nix:14`), so a stale lockfile fails
the build rather than silently re-resolving — matching the project's
`--locked` discipline. If a git dependency updates under you, override
`cargoExtraArgs` per-derivation; there is no known crane issue about `--locked`
itself.

## 2. Nixpkgs cross-compilation

### 2a. Exact cross attrs

From `lib/systems/examples.nix` (nixpkgs master):

- `musl64 = { config = "x86_64-unknown-linux-musl"; };`
- `aarch64-multiplatform = { config = "aarch64-unknown-linux-gnu"; };`
- `aarch64-multiplatform-musl = { config = "aarch64-unknown-linux-musl"; };`
- (`gnu64 = { config = "x86_64-unknown-linux-gnu"; };` — native builds need no
  entry.)

Local eval confirms: `pkgs.pkgsCross.musl64.stdenv.cc` is
`x86_64-unknown-linux-musl-gcc-wrapper-15.3.0`,
`pkgsCross.aarch64-multiplatform.stdenv.cc.targetPrefix` is
`aarch64-unknown-linux-gnu-`, `aarch64-multiplatform-musl` is
`aarch64-unknown-linux-musl-`. The four targets = `gnu64` (native x86_64),
`musl64`, `aarch64-multiplatform`, `aarch64-multiplatform-musl`.

### 2b. Musl cross stdenv: working g++ and libstdc++

The cross stdenv's C/C++ compiler is the native gcc retargeted:
`pkgs/stdenv/cross/default.nix` sets `cc = ... buildPackages.gcc` (in the "Run
Packages" stage, `stdenv` definition). nixpkgs' gcc defaults to
`langCC ? true` (`pkgs/development/compilers/gcc/default.nix`) and its
configure flags include `--enable-static` and
`--enable-languages=c,c++,...` (`gcc/common/configure-flags.nix`); shared
libraries are controlled by `enableShared ? stdenv.targetPlatform.hasSharedLibraries`,
which is true for musl, so both `.so` and `.a` libstdc++ are produced.
Verified by eval: `pkgsCross.musl64.stdenv.cc.cc` =
`x86_64-unknown-linux-musl-gcc-15.3.0` with `langCC = true` and outputs
`["out" "man" "info" "lib" "libgcc"]` (same for
`aarch64-multiplatform-musl`). Presence of `libstdc++.a` inside the `lib`
output is implied by `--enable-static` + `langCC`; **settled in practice by
the spike (§7)**, whose musl outputs are fully static and linked against
libstdc++ with no `BORING_BSSL_RUST_CPPLIB`-style workaround. This fixes the
exact gap the Dockerfile comments describe (Debian `musl-tools` ships no musl g++ and no
`libstdc++.a`, which is why the BoringSSL CMake C++ probe failed as
"Could NOT find Threads").

### 2c. Cross-arch OCI output from dockerTools

`pkgs/build-support/docker/default.nix`: `defaultArchitecture = go.GOARCH`
("For the mapping from Nixpkgs system parameters to GOARCH, we can reuse the
mapping from the go package"), used as `architecture ? defaultArchitecture` in
both `buildImage` and (via `streamLayeredImage`) `buildLayeredImage` — so under
`pkgsCross` the image arch follows the host platform automatically. The manual
documents the `architecture` override ("Used to specify the image architecture.
This is useful for multi-architecture builds that don't need cross compiling",
nixpkgs manual, `streamLayeredImage`/`buildImage` sections) but does not spell
out a pkgsCross recipe. The behavior is exercised by nixpkgs' own test:
`nixos/tests/docker-tools-cross.nix` builds `remoteCrossPkgs.dockerTools.buildImage`
and `buildLayeredImage` cross and runs them in a docker VM (the test is
excluded from the default suite with the comment "requires remote builder").
Local eval of
`pkgsCross.aarch64-multiplatform-musl.dockerTools.buildLayeredImage { ... }`
succeeds. So: supported, tested, but documented only in source + test, not in
the manual's dockerTools chapter. **UNVERIFIED** end-to-end (no build
attempted); test: build and `docker load` the aarch64 tarball on an arm64
runner. Note `buildImage`'s `runAsRoot` needs KVM (manual "Caution: Using this
attribute requires the `kvm` device"); `buildLayeredImage` does not.

### 2d/2e. Tools: upx, skopeo, crane (go-containerregistry)

All three are packaged, verified by eval and by file position:

- `pkgs.upx` — `pkgs/by-name/up/upx/package.nix` (v5.2.0).
- `pkgs.skopeo` — `pkgs/by-name/sk/skopeo/package.nix` (v1.24.0). nixpkgs'
  `dockerTools.pullImage` already uses it internally
  (`nativeBuildInputs = [ skopeo ]` in `pkgs/build-support/docker/default.nix`).
- `pkgs.crane` = go-containerregistry crane —
  `pkgs/by-name/go/go-containerregistry/package.nix`.

Which to use for CI push: go-containerregistry `crane push` accepts a
"docker-style tarball" directly ("If the PATH is a directory, it will be read
as an OCI image layout. Otherwise, PATH is assumed to be a docker-style
tarball", `cmd/crane/doc/crane_push.md`) — exactly what dockerTools outputs —
and `crane index append` assembles a multi-arch manifest list with no daemon:
`crane index append -m registry.k8s.io/etcd-amd64:3.4.9 -m
registry.k8s.io/etcd-arm64:3.4.9 -t example.com/etcd`
(`cmd/crane/doc/crane_index_append.md`); auth via `crane auth login`
(`cmd/crane/doc/crane_auth_login.md` exists). skopeo can push the tarball too
(`skopeo copy docker-archive:... docker://...`, documented in
`docs/skopeo-copy.1.md` examples) and `--all`/`--multi-arch all` copy manifest
lists, but assembling a fresh list from two single-arch tarballs is exactly
`crane index append`'s job. **Recommendation: crane (go-containerregistry) for
both steps; skopeo as fallback.** Both are pure-Go/static, daemon-free.

## 3. Rust toolchains in nix

### 3a. fenix vs rust-overlay for pinned nightly + rustfmt

Both can express "nightly with rustfmt", with different failure modes:

- **fenix**: `toolchainOf { channel = "nightly"; date = "..."; sha256 =
  "..."; }` pins an exact manifest (README), and each toolchain has `rustfmt`
  as a component (`rustfmt = <derivation>; # alias to rustfmt-preview`). But a
  component exists only if that date's nightly manifest shipped it. Verified
  by eval: against the latest fenix snapshot, `minimal.rustfmt` **does not
  exist** (today's nightly manifest has no rustfmt), while
  `latest.rustfmt` does (`rustfmt-preview-nightly-latest-2026-08-22` — fenix's
  `latest` profile intentionally mixes component dates, "you get the latest
  version of the components, but risks a larger chance of incompatibility").
- **rust-overlay**: `rust-bin.nightly."2026-03-15".default` and
  `.rustfmt` both eval successfully here (`rustfmt-preview-1.96.0-nightly-2026-03-15`);
  the README warns "nightly toolchain may have components (like `rustfmt` or
  `rls`) missing" and offers `selectLatestNightlyWith (t: t.default.override
  { extensions = [...]; })` to pick the newest nightly that has what you need.
  Hashes are pre-fetched in-tree, so evaluation is pure (README).

For an exact-pinned nightly with rustfmt, rust-overlay is the safer choice;
fenix works too but a pinned date can silently lack rustfmt.

### 3b. Stable default + nightly rustfmt side by side

fenix documents `combine [ derivation ... ]` ("Combines a list of components
into a derivation. If the components are from the same toolchain, use
`withComponents` instead") — canonical shape:
`combine [ stable.cargo stable.clippy stable.rustc stable.rustfmt?... nightly.rustfmt ]`.
rust-overlay has no equivalent combine; the README pattern is per-toolchain
`override { extensions = [...]; targets = [...]; }`, and multiple toolchains
are joined manually. Either way, crane's `overrideToolchain` + `devShell`
(which "automatically add[s]" the toolchain's `cargo`, `clippy`, `rustc`,
`rustfmt` — `docs/API.md` §`craneLib.devShell`) gives the shell both.

**The catch:** `cargo +nightly fmt` is a rustup feature. fenix #110 (closed;
fenix contributor @figsoda's answer): "`+<toolchain>` is a rustup feature and fenix doesn't
currently support it ... You can make a wrapper for cargo ... that skips the
first argument if it starts with `+`." clewdr's xtask deliberately invokes
`rustup run nightly cargo fmt --all` ("Invoked through `rustup run` rather
than `cargo +nightly`...", `xtask/src/main.rs:188-191`) and probes for rustup
in `Toolchain::detect`. So under nix, `cargo xtask ci` needs a `rustup`
binary with a registered `nightly` toolchain. nixpkgs packages rustup
(`pkgs.rustup`, `pkgs/development/tools/rust/rustup/default.nix`; its
postInstall symlinks cargo/rustc/rustfmt/clippy proxies to the rustup binary),
and rustup resolves toolchains from `RUSTUP_HOME` (rustup book,
"Environment variables"). The community pattern is a shellHook that points
`RUSTUP_HOME` at a generated directory whose
`toolchains/nightly-x86_64-unknown-linux-gnu` is a symlink to a pinned nightly
nix toolchain — but I found **no canonical documentation of this pattern** in
fenix, rust-overlay, or crane, so it is **UNVERIFIED**; test it, or (cleaner)
make xtask locate nightly fmt via an env var (`RUSTFMT`/`CARGO_FMT_NIGHTLY`)
and drop the rustup dependency in nix builds.

### 3c. wasm32-unknown-unknown target

fenix README lists it under "only rust-std (for cross compiling)" and provides
`targets.wasm32-unknown-unknown.latest.rust-std` (verified by eval:
`rust-std-nightly-latest-2026-08-22`); fenix's crane example uses `combine [
minimal.cargo minimal.rustc targets.wasm32-unknown-unknown.latest.rust-std ]`.
rust-overlay: `rust-bin.stable.latest.default.override { targets =
["wasm32-unknown-unknown"]; }` — documented in the README cheat sheet and
verified by eval. Both fine.

## 4. GitHub Actions integration

### 4a. DeterminateSystems/nix-installer-action

README: supports "Linux (x86_64 and aarch64), macOS (aarch64), WSL, Containers,
SteamOS, GitHub Enterprise Server, GitHub Hosted, self-hosted, and long running
Actions Runners" (note: macOS x86_64 is absent). Installs Determinate Nix by
default; enables `nix-command` and `flakes` in nix.conf, sets
`auto-optimise-store`, `max-jobs = auto`; auto-configures KVM when available
(which `dockerTools.buildImage runAsRoot` would need). `extra-conf` appends to
`/etc/nix/nix.conf`. Speed: the README claims "fast, friendly, and reliable"
and "tens of thousands of installs daily" but publishes **no benchmark
numbers** — treat quantitative speed claims as UNVERIFIED.

### 4b. DeterminateSystems/magic-nix-cache

README: "uses the GitHub Actions built-in cache", claims "Save 30-50%+ of CI
time"; "follows the same semantics as the GitHub Actions cache" (link to GitHub
docs). GitHub's cache docs (primary source for semantics): caches are scoped to
the current branch with fallback to the default branch; PR runs get a
merge-ref-scoped cache that only that PR can restore; entries unused for 7 days
are removed; default 10 GB/repo with LRU eviction; rate limits of 200
uploads/min and 1500 downloads/min. magic-nix-cache handles the rate limits
gracefully ("won't cause your CI to fail"; deferred uploads go out on the next
run; `runs.post` stores paths even when the job fails). So: per-branch,
self-pruning, size-capped — good fit for a 4-target matrix, provided it's not
thrashed by huge rebuilds.

### 4c. Documented pattern for 4-cross-target CI → OCI → ghcr.io

**No single canonical end-to-end example exists.** Crane's own CI
(`.github/workflows/test.yml`) builds the cross examples (`cross-musl`,
`cross-rust-overlay`, `cross-windows`, plus `trunk`) as a matrix on
`ubuntu-latest` using cachix (not GH cache) and pushes no images; grep of
crane's examples/docs finds no dockerTools, ghcr, or skopeo usage at all. The
pieces are individually documented: `pkgsCross` + splicing (crane
cross-rust-overlay example), cross dockerTools output (nixpkgs
`docker-tools-cross` test), tarball push (`crane push`), manifest-list assembly
(`crane index append`), and ghcr auth (`crane auth login`). The assembly is
therefore compose-from-pieces, not copy-from-example: **UNVERIFIED** until the
project's first `nix build .#images.<arch>` + `crane index append` run.

## 5. Known issues

### 5a. btls-sys under nix

crates.io metadata for `btls-sys` declares **no repository** (crates.io index
entry for the crate has `repository: null`; the crates.io API agrees). The
matching upstream repo is `github.com/0x676e67/btls` (contains `btls-sys/`,
authors match, `btls-sys/build/main.rs` carries cmake cross-compilation logic
— `should_use_cmake_cross_compilation`, Android/Apple cmake param tables). A
search of that repo's issues for "nix" returns nothing relevant, and
`cloudflare/boring` (upstream BoringSSL bindings lineage) likewise has no
nix-specific open issues. **SETTLED 2026-08-01**: builds cleanly under
`pkgsCross` for all four targets in the sandbox (§7); required inputs: `git`
(BoringSSL's cmake runs `git init` in the out dir), `cmake`, `perl`,
`gnumake`, `ninja` (not all needed — see §7).

### 5b. cmake crate + nix sandbox

The historical "Could NOT find Threads" came from FindThreads' try_compile
probe: `try_compile(THREADS_HAVE_PTHREAD_ARG SOURCE_FROM_FILE
"${_threads_src}" ... CMAKE_FLAGS -DLINK_LIBRARIES:STRING=-pthread)`
(`Modules/FindThreads.cmake` in Kitware/CMake) — i.e. a broken/absent target
C++ toolchain, not a sandbox problem per se; the nix cross stdenv provides a
complete one (2b). cmake-rs enters cross mode by defining
`CMAKE_SYSTEM_NAME`/`CMAKE_SYSTEM_PROCESSOR` from cargo's target env
(`rust-lang/cmake-rs src/lib.rs:454-461`, "Set CMAKE_SYSTEM_NAME and
CMAKE_SYSTEM_PROCESSOR when cross compiling"), and the nixpkgs cmake
setup-hook pins the compiler/tools explicitly
(`pkgs/by-name/cm/cmake/setup-hook.sh`:
`-DCMAKE_CXX_COMPILER=$CXX -DCMAKE_C_COMPILER=$CC -DCMAKE_AR=...`). CMake docs
describe `CMAKE_TRY_COMPILE_TARGET_TYPE=STATIC_LIBRARY` as "intended for use
with cross-compiling toolchains that cannot link without custom flags or
linker scripts" (cmake.org variable docs). Whether modern CMake defaults
try_compile to STATIC_LIBRARY when cross-compiling without an emulator I could
not confirm from a primary source — **UNVERIFIED**; the spike (§7) got all
four BoringSSL builds through configure+compile, and its log shows
cmake-rs generating `-DCMAKE_CROSSCOMPILING=true` and picking up the nix
cross gcc wrappers (`aarch64-unknown-linux-musl-cc`, `-c++`) from `CC`/`CXX`
env, with no Threads probe failure — the exact failure mode the Dockerfile
comments describe. Open cmake-rs issues touching nix: none about the sandbox/Threads
found by search; nearest are #260 (closed, `[nixos]
-DCMAKE_POLICY_VERSION_MINIMUM=3.5`) and #278 (open, relocated CMake build
directories). Related crane issue: #188 (closed, CMakeCache path mismatch
between dummy-src and real build).

### 5c. bindgen + libclang under cross

Covered in 1d: LIBCLANG_PATH is not crane-documented but is the accepted
fix (#250, closed); nixpkgs' `rustPlatform.bindgenHook` is the native
integration (`LIBCLANG_PATH` + `BINDGEN_EXTRA_CLANG_ARGS` propagation).
**SETTLED 2026-08-01** by the spike: hook + LIBCLANG_PATH suffice for all
four targets (§7).

## 6. Local dev shell

Canonical: `craneLib.devShell { checks = self.checks.${system}; packages =
[...]; shellHook = ...; }` (crane README → `docs/local_development.md`;
`docs/API.md` §`craneLib.devShell`: toolchain's cargo/clippy/rustc/rustfmt are
added automatically, `inputsFrom`/`checks` inherit build inputs). The
trunk-workspace example's devShell adds `pkgs.trunk` and exports
`CLIENT_DIST`. For clewdr the shell is: stable toolchain with
`wasm32-unknown-unknown` + clippy (rust-overlay override or fenix combine),
nightly rustfmt (see 3b for the `rustup run nightly` seam), plus `pkgs.trunk`,
`pkgs.wasm-bindgen-cli` (version matching Cargo.lock),
`pkgs.binaryen`, `pkgs.upx`, `pkgs.crane` (go-containerregistry) or
`pkgs.skopeo`. `pkgs.cargo-ndk` exists (Android;
`pkgs/by-name/ca/cargo-ndk/package.nix`). cross-rs is **not packaged** — no
`pkgs.cross` attr exists (verified by eval; `pkgs/by-name/cr/cross` 404) — so
`cross` would need to come in as its own flake input if wanted; crane's
pkgsCross pattern makes it unnecessary.

## 7. Empirical spike results (2026-08-01)

A spike flake (`flake.nix`, repo root) builds the server for all four targets
on one x86_64 machine. Result: **all four build successfully** in the sandbox.
Binaries: aarch64-musl 10.8 MB, aarch64-gnu 11.2 MB, x86_64-musl 12.3 MB,
x86_64-gnu 11.0 MB (stripped, no upx). `readelf` shows the two musl binaries
have **no PT_INTERP** (fully static — the property the Dockerfile exists to
produce) and the gnu ones point at nix-store glibc loaders; the native binary
runs (`--help`/first-run output verified).

What the flake does (crane's canonical `cross-rust-overlay` pattern):
`import nixpkgs { crossSystem = ...; overlays = [ rust-overlay ]; }`,
`crane.mkLib pkgs` + `overrideToolchain (p: p.rust-bin.stable."1.98.0".minimal)`,
wrap in `pkgs.callPackage` for splicing, `strictDeps = true`.

Things discovered along the way, in order:

1. **rustc floor**: nixpkgs-unstable's rustc is 1.97.1; wreq 0.16 requires
   1.98 (`error: rustc 1.97.1 is not supported by ... wreq@0.16.0 requires
   rustc 1.98`). The flake pins `rust-bin.stable."1.98.0"` — one more reason
the toolchain must be overlay-pinned, not nixpkgs-default (also for nightly
rustfmt, 3a).
2. **`git` must be in nativeBuildInputs**: btls-sys's BoringSSL cmake runs
   `git init` in the out dir; without git the build dies with
   `can't run git: No such file or directory`. (btls-sys only needs it when
   the vendored tree is patched; it is, so it runs.)
3. **bindgen needs libclang AND the target's libc headers** (5c settled):
   `LIBCLANG_PATH` + `nativeBuildInputs = [ pkgs.rustPlatform.bindgenHook
   (lib.getLib llvmPackages.libclang) ]`. The hook's
   `BINDGEN_EXTRA_CLANG_ARGS` is populated from the cross clang's
   nix-support flags — target libc headers included — which is what bindgen
   needs to find `stdlib.h`. Note the hook does not add libclang to the
   sandbox itself, so libclang must also be listed explicitly.
4. **cmake cross + BoringSSL just works** (5b settled): the log shows
   cmake-rs defining `CMAKE_CROSSCOMPILING` and taking
   `aarch64-unknown-linux-musl-cc`/`-c++` from the stdenv env; no
   `Could NOT find Threads`-class failure on any of the four. The
   `patchelf: cannot find section '.dynamic'` message on the musl outputs is
   benign (static binary, expected).
5. **NEW LIMITATION — `builtins.fromTOML` is TOML 1.0**: crane's
   `buildDepsOnly`/`mkDummySrc` path parses every workspace Cargo.toml with
   nix's `builtins.fromTOML` at eval time. That parser is toml11-based (TOML
   1.0, nixpkgs `lib/fromTOML.nix` delegates to the builtin; NixOS/nix
   issue #15129 tracks TOML 1.1 support) and rejects newlines inside inline
   tables — which this repo's `Cargo.toml` uses in four places
   (`tower-http`, `tracing-subscriber`, `wreq`, `zip`; cargo itself accepts
   them). Evaluation fails with `toml::parse_inline_table: missing closing
   bracket`. The spike therefore sets `cargoArtifacts = null`, skipping the
   dependency-split (one-shot builds, no cargo-chef-style caching).
   **Fix for adoption**: convert those four entries to standard table form
   (`[dependencies.tower-http]` + `version`/`features` keys — valid TOML
   1.0), then drop `cargoArtifacts = null` and get incremental dep caching
   back. Alternative: hand-roll a `dummySrc` (build-time TOML cleaning);
   inferior to a 12-line manifest change.
6. **wasm-bindgen-cli version**: the lockfile pins wasm-bindgen 0.2.127 but
   nixpkgs' newest `wasm-bindgen-cli_0_2_126` is 0.2.126 — for the frontend
   build the cli must come from a crane build of the crate at the lockfile's
   version (the `trunk-workspace` example comment says exactly this: the cli
   version must match Cargo.lock).

Cold build wall-clock: ~25 min for all four (2 in parallel, 16 cores each,
LTO on) — most of it is the nix store's one-time cost (musl cross gcc,
clang/LLVM/libclang toolchains), not the crate build itself.

Warm rebuild (deps in store, new src hash): both musl targets in **59.5 s**
wall (cargo "Finished" 56.55 s x86_64 vs 58.91 s aarch64 — cross-aarch64
costs ~4% extra). Cross-compilation runs rustc/LLVM/gcc at native x86 speed;
only codegen differs, so there is no case for a native arm runner on
build-speed grounds.

Reference, current CI at v0.13.4 (warm caches, 4-core runners):
linux-aarch64 on `ubuntu-24.04-arm` 3.1 min; musllinux-aarch64 cross on
`ubuntu-latest` 4.5 min; musllinux-x86_64 4.3 min; linux-x86_64 3.6 min.
On GHA's 4 cores a warm nix target should land in ~2 min; four targets in
one job on one runner ≈ 4–8 min wall (or split x86_64/aarch64 into two jobs
for ~2–3 min each), with no arm-runner availability lottery. Cross-built
outputs are machine-independent, so all runners share one cache.

## 7.1 Distroless image spike (decision: distroless stays as the runtime base)

Per maintainer decision, the runtime image keeps
`gcr.io/distroless/static-debian13` as its base. In nix this is
`dockerTools.pullImage` (base pinned by manifest digest — stricter than the
current Dockerfile's floating `latest` tag) + `buildLayeredImage
{ fromImage = ...; }` layering the upx'd static binary and `/etc/clewdr` on
top; distroless's own Env (PATH, SSL_CERT_FILE) and CA-cert layers are
preserved. Images: `.#image-amd64` (7.45 MB) / `.#image-arm64` (7.05 MB),
both docker-archive tarballs. Two gotchas found:

1. `pullImage` needs a precomputed output sha256 — the standard workflow is
   build once with the wrong hash and copy the real one from the error
   (`got: sha256-...`). Base updates = bump imageDigest + sha256 together.
2. `streamLayeredImage` (what `buildLayeredImage` wraps) defaults
   `architecture` to the **host** platform and does **not** inherit it from
   `fromImage` — the arm64 image came out labelled amd64 until
   `architecture = "arm64"` was passed explicitly. Settles 2c's
   cross-image question: dockerTools assembles cross images on one machine;
   the manifest arch is a caller-supplied parameter.

Both images verified end-to-end: `podman load` + run, amd64 natively and
arm64 under qemu binfmt, with clewdr's startup logs observed in both.
(Distroless has no shell, so image debugging needs `crane`/`skopeo`
inspection rather than `docker run sh`.)

Warm rebuild (deps in store, new src hash): both musl targets in **59.5 s**
wall (cargo "Finished" 56.55 s x86_64 vs 58.91 s aarch64 — cross-aarch64
costs ~4% extra). Cross-compilation runs rustc/LLVM/gcc at native x86 speed;
only codegen differs, so there is no case for a native arm runner on
build-speed grounds.

Reference, current CI at v0.13.4 (warm caches, 4-core runners):
linux-aarch64 on `ubuntu-24.04-arm` 3.1 min; musllinux-aarch64 cross on
`ubuntu-latest` 4.5 min; musllinux-x86_64 4.3 min; linux-x86_64 3.6 min.
On GHA's 4 cores a warm nix target should land in ~2 min; four targets in
one job on one runner ≈ 4–8 min wall (or split x86_64/aarch64 into two jobs
for ~2–3 min each), with no arm-runner availability lottery. Cross-built
outputs are machine-independent, so all runners share one cache.

## 7.2 Android cross-build (2026-08-23): works, six non-obvious requirements

`nix build .#clewdr-android-aarch64` produces a real android binary:
`ELF 64-bit aarch64, interpreter /system/bin/linker64, for Android 35, built
by NDK r27 (12077973)`, `RUNPATH=$ORIGIN`, `NEEDED libc++_shared.so`, with
`libc++_shared.so` (1.77 MB) installed beside the 12.6 MB binary — the same
shape as the cargo-ndk artifact the old CI shipped. NDK r27 (27.0.12077973)
comes from nixpkgs (`androidndkPkgs_27`), which needs
`config.allowUnfree = true`.

Each requirement below was found by a failure whose message pointed
elsewhere:

1. `lib.systems.examples.aarch64-android` ships `useAndroidPrebuilt = false`
   in this nixpkgs, so nixpkgs builds the android toolchain from source and
   fails in compiler-rt with `'pthread.h' file not found`. Override the flag
   on the crossSystem.
2. rust-overlay defines **nothing** for the android cross's
   `pkgsTargetTarget` (verified: `attrNames …rust-bin == []`, while musl and
   aarch64-gnu cross sets have the full set), and crane's `spliceToolchain`
   calls the toolchain function for every splice, so `minimal` disappears.
   Pass a toolchain built from the **native** set instead (with
   `targets = [ "aarch64-linux-android" ]`); crane accepts a plain derivation
   there and fans it out to cargo/rustc/clippy/rustfmt.
3. `rustPlatform.bindgenHook` in the android cross set drags in the
   from-source android clang (→ compiler-rt again). Its whole job is to export
   `LIBCLANG_PATH` and read three `nix-support` cflags files, so read them
   from the NDK cc-wrapper instead (`cc-cflags`, `libc-cflags`,
   `libcxx-cxxflags`).
4. nixpkgs' cross stdenv exports `SYSROOT` (bionic). Cargo's `TargetInfo`
   probe (`rustc - --crate-name ___ --print=file-names …`) inherits it as
   `--sysroot`, where `lib/rustlib` does not exist, and the build dies with
   `error loading target specification` before compiling anything. `unset
   SYSROOT` in preBuild.
5. **rustc 1.98 no longer accepts `aarch64-unknown-linux-android`** — only the
   builtin `aarch64-linux-android`. nixpkgs' `rust.rustcTargetSpec` produces
   the long form, so `CARGO_BUILD_TARGET` must be pinned to the short one.
   (cargo-ndk uses the short name, which is why the old CI never hit this;
   `.cargo/config.toml` already keys its android rustflags on it.)
6. Two env contracts have to be honoured by hand, because there is no
   cargo-ndk in the loop: `ANDROID_NDK_HOME` for btls-sys's
   `android.toolchain.cmake` (the NDK root is nested at
   `…/libexec/android-sdk/ndk-bundle` in nixpkgs' layout), and
   `CARGO_NDK_SYSROOT_LIBS_PATH` for clewdr's own `build.rs`, which copies
   `libc++_shared.so`. rustc also defaults to `cc` as the android linker, so
   `CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER` must point at the NDK wrapper.

One nixpkgs oddity worth recording: calling
`androidenv.composeAndroidPackages { includeNDK = true; … }.ndk-bundle`
directly from the flake throws `attribute 'ndk-bundle' missing` when the
derivation is forced, while the *same* derivation
(`android-sdk-ndk-27.0.12077973`) evaluates fine when reached through
`androidndkPkgs_27.binaries.propagatedBuildInputs`. The flake uses the latter
route.

## 7.3 CI cutover (2026-08-23): what the first real runs measured

The migration landed on branch `nix-migration`, replacing the linux/musllinux
matrix rows, `docker-build.yml` and the `Dockerfile`. Three GitHub Actions
runs produced numbers that changed the design twice.

### Dependency artifacts: check who consumes them

`lib/buildDepsOnly.nix:77-85` (crane `692f7e9`) runs **both** commands in the
build phase:

```nix
buildPhaseCargoCommand = args.buildPhaseCargoCommand or ''
  ${cargoCheckCommand} ${cargoExtraArgs} ${cargoCheckExtraArgs}
  ${cargoBuildCommand} ${cargoExtraArgs} ${cargoBuildExtraArgs}
'';
```

`doCheck = false` does **not** remove the check pass — it only drops the
`cargo test --no-run` phase and the `--all-targets` default. Upstream's intent
is documented in [artifact reuse](https://crane.dev/introduction/artifact-reuse.html):
one deps derivation is meant to serve `cargoClippy` (wants metadata),
`buildPackage` (wants linkable artifacts) and `cargoTest` at once. The API doc
states "all artifacts from running `cargo {check,build,test}` will be cached".
No community idiom exists for turning the extra passes off; the threads about
deps build time are about sharing one deps derivation (crane #201/#213 asked
for per-crate splitting and were declined), or about the feature-set mismatch
trap in the FAQ — which is what the union-features comment in `mk` avoids.

Our topology was upside down in both directions, and both were measured:

| derivation | consumers | measured |
|---|---|---|
| 5 cross deps builds | `buildPackage` only | check pass = **159 s** of the musl deps derivation on a 4-core runner, per target |
| `checks.ci` | itself (clippy ×4, wasm clippy, test) | `cargoArtifacts = null`, so every source change recompiled the whole graph |

Fixes: `cargoCheckCommand = ":"` on the cross deps (the resulting binary is
byte-identical, sha256 `783300694b7ee5d1` before and after), and a
dev-profile deps derivation for `checks.ci`. Two things have to match xtask
or nothing is reused: the **dev profile** (xtask passes no `--release`) and
the feature set (union of `FEATURE_COMBINATIONS`). After wiring it, crates
compiled per xtask phase: 13 / 4 / 10 / 4 for the four clippy passes and 13
for the tests, against 675 in the deps derivation. A source-only change then
rebuilds `checks.ci` in 60 s locally.

### Passing the frontend between jobs: a store path, not a directory

Run 32622131714 rebuilt the frontend in **every** nix job — 4.0 min of the
musl job's 8.3 — even though `build-frontend` had fetched that exact output
from the binary cache 2 minutes earlier in 42 s. In-run substitution missed
it (732 derivations built while 312 paths were fetched), so `needs:` plus a
shared binary cache does not reliably carry a path across jobs.

`build-frontend` now also uploads `nix-store --export` of the output (773 KB)
and the nix jobs `nix-store --import` it. This is not impure: the imported
path is exactly the derivation's output, so nix treats the build as done. The
directory artifact stays for windows/macOS, which build with cargo. Result:
nix jobs 8.3 → 6.8 min, docker 12.4 → 9.9 min, frontend rebuild lines 522 → 0.

### Cross-job cache reliability

Cross-*run* substitution works (an unchanged frontend output was fetched in
42 s). Cross-*job within a run* is unreliable: in run 32623187130 the
`checks.ci` derivation was unchanged from the previous run yet the job took
16.7 min instead of 6m44s, while five matrix jobs were each pulling ~1 GiB
through the same GHA cache. Anything expensive that more than one job needs
should be handed over as an exported store path, not left to substitution.

### Other cutover findings

- `nix build -o` is single-valued: repeating it in one invocation silently
  renames the other outputs (`<name>-1-bin`). One `-o` per invocation.
- `crane index append` takes children via `-m/--manifest`, not positionally.
- `crane push` cannot read a gzipped docker tarball (`invalid tar header`);
  the image derivations set `compressor = "none"`.
- One job per target beats one job for five: `lto = true` with
  `codegen-units = 1` makes the final link effectively single-threaded (4-core
  57 s vs 32-core 55 s), so parallel jobs, not bigger runners, are the lever.
- `nix build --rebuild` reports no differences: the builds are bit-reproducible.

## Open tests (updated after the spike)

1. ~~`nix build` of a clewdr package under `pkgsCross.musl64` and
   `pkgsCross.aarch64-multiplatform-musl`~~ **done, passes** — settles
   5a/5b, and libstdc++.a presence (2b) by successful static link. Android
   (§7.2) also builds and links.
2. ~~`docker load` + run of the cross `buildLayeredImage` tarballs~~
   **done, passes**: both arches loaded (podman) and ran — amd64 natively,
   arm64 under qemu binfmt — on the distroless base (§7.1). Remaining: the
   `crane index append` push to ghcr.io (4c) and a run on real arm64
   hardware — untested.
3. ~~A `nix develop` shell running `cargo xtask ci` unmodified (3b/6)~~
   **done**: xtask now discovers nightly through `CLEWDR_NIGHTLY_CARGO` and
   the wasm target through the toolchain sysroot, so the rustup path is
   unchanged and the nix path needs no rustup. CI runs the same command
   inside `checks.ci`.
4. ~~Restore `buildDepsOnly` by converting the four multiline inline tables
   in `Cargo.toml` to standard table form (§7 item 5), and re-measure CI
   build time with dep caching.~~ **done**, and the topology re-measured in
   §7.3.
5. ~~Frontend wasm build under the sandbox: `buildTrunkPackage` +
   lockfile-pinned wasm-bindgen-cli (§7 item 6)~~ **done**: the official
   prebuilt musl wasm-bindgen 0.2.127 is pinned by hash (nixpkgs tops out at
   0.2.126), and `embed-resource` builds inject `static/` at build time via
   `preBuild` — wrapping the source in a derivation instead would force it to
   be realised at evaluation time, because crane's vendoring reads the source.
6. **Untested: the `release` job.** Tag-triggered artifact collection
   (`softprops/action-gh-release` with `fail_on_unmatched_files: true`) has
   never run against the nine nix + native artifacts. Dry-run with a
   throwaway tag before trusting a real release.

## 7.4 Binary caching in CI: measured, then declined

Three CI runs made it clear that nothing was being reused between runs. The
evidence, from run 32625720144:

| job | built | uploaded |
|---|---|---|
| Format, lint and test | new deps artifact | 385 paths / 633 s |
| each of 5 build jobs | 718 derivations | **0 paths** |
| docker | 718 derivations | **0 paths** |

`magic-nix-cache` logged no warning in either case. The repository's Actions
cache stood at **10.65 GB against a 10 GB ceiling**, 4879 entries
(`gh api repos/OWNER/REPO/actions/cache/usage`), so entries were evicting one
another. Two runs on the *same commit* (32624239965 and 32624907882)
recompiled everything, which is the "android keeps rebuilding btls"
symptom: not duplication within a run, but zero reuse across runs.

Determinate Systems have [end-of-lifed the free tier of the Magic Nix
Cache](https://determinate.systems/blog/magic-nix-cache-free-tier-eol/)
("will stop working unless you're on GitHub Enterprise Server").

### What we would need to store

Measured on this repo (`du -sc` on the store paths, per commit-state):

| what | size | changes when |
|---|---|---|
| 5 cross dependency artifacts | 5 × 188 MB = 940 MB | Cargo.lock, Cargo.toml, flake env |
| checks dependency artifact | 335 MB (616 MB before dropping dep debug info) | same |
| frontend dependency artifact | 64 MB | frontend deps |
| 5 binaries + 2 images + static | ~75 MB | any source change |
| **per commit-state** | **≈ 1.4 GB** | |
| rust-overlay toolchains (stable, nightly, per-target std) | 3.07 GB | only the pins |
| 766 vendored crate paths | 0.41 GB | Cargo.lock |

The dependency artifacts are also *not* bit-reproducible
(`nix-store -r --check` reports the target tarball differs; cargo fingerprints
carry timestamps), so each rebuild is a fresh multi-hundred-MB path rather
than a content-addressed no-op. Final binaries *are* reproducible
(sha256 7833006… across rebuilds); only the cache seed churns.

### Options, priced

- **Cachix free**: 5 GB, LRU eviction, unlimited bandwidth
  ([pricing](https://www.cachix.org/pricing)). Fits 3.5 GB of pins plus
  exactly one commit-state; the second Cargo.lock bump thrashes it. Viable
  only in explicit-push mode (`cachix push` on the deps and binary paths
  alone, ~200 MB/job), which leaves the 3.07 GB of toolchains uncached and
  costs every job 1–2 minutes of unpacking rustc.
- **FlakeHub Cache**: no free tier, $20 per organisation member per month;
  open source projects can request a free account by mail.
- **Cloudflare R2 + nix's native S3 store**: 10 GB free, egress free, no
  server to run (`nix copy --to 's3://…?endpoint=…&region=auto'`). Fits
  everything (4.9 GB) with room for several generations. Costs three secrets,
  a nix signing key, and a public read domain for substitution — and would
  also serve local developers.
- **No cache** (chosen): build jobs sit at 5–7 minutes, which is what they
  already cost with the cache broken, minus the upload time and the eviction
  churn.

The frontend is still handed between jobs as an exported store path
(`nix-store --export` → artifact → `nix-store --import`), which is not a
binary cache and saves 4 minutes per job; see §7.3.

Revisit R2 if the build jobs stop being tolerable, or if the matrix grows.
