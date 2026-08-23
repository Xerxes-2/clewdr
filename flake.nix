{
  description = "clewdr builds: linux binaries (gnu/musl × x86_64/aarch64), distroless OCI images, checks, dev shell";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    crane.url = "github:ipetkov/crane";
    rust-overlay.url = "github:oxalica/rust-overlay";
    rust-overlay.inputs.nixpkgs.follows = "nixpkgs";
  };

  outputs = { self, nixpkgs, crane, rust-overlay }:
    let
      localSystem = "x86_64-linux";
      pkgsNative = import nixpkgs {
        system = localSystem;
        overlays = [ (import rust-overlay) ];
        # The android NDK (Google-licensed) for the aarch64-android target.
        config.allowUnfree = true;
      };
      nativeLib = crane.mkLib pkgsNative;
      # The default cleanCargoSource keeps only cargo-relevant files; the
      # frontend also ships html/css/ico assets that trunk needs, so the
      # source is filtered with crane's common cargo sources plus web assets
      # (the same set as crane's own trunk example).
      src = pkgsNative.lib.fileset.toSource {
        root = ./.;
        fileset = pkgsNative.lib.fileset.unions [
          (nativeLib.fileset.commonCargoSources ./.)
          (pkgsNative.lib.fileset.fileFilter (
            file:
            pkgsNative.lib.any file.hasExt [
              "html"
              "css"
              "js"
              "ico"
              "png"
              "svg"
              "json"
            ]
          ) ./.)
        ];
      };

      # --- toolchains ----------------------------------------------------
      # 1.98.0 is the rustc floor: wreq 0.16 needs it. Pinned exactly, with
      # wasm32 (frontend + its lint) and clippy (xtask lint).
      stableToolchain = p:
        p.rust-bin.stable."1.98.0".default.override {
          targets = [ "wasm32-unknown-unknown" ];
          extensions = [ "clippy" ];
        };
      # Pinned nightly for rustfmt: .rustfmt.toml uses nightly-only options.
      nightlyNative = pkgsNative.rust-bin.nightly."2026-03-15".default;

      nativeCrane = nativeLib.overrideToolchain stableToolchain;

      # --- wasm-bindgen-cli --------------------------------------------------
      # The frontend's Cargo.lock pins wasm-bindgen 0.2.127 and trunk refuses
      # a mismatch; nixpkgs tops out at 0.2.126. Use the official prebuilt
      # (static musl) release, pinned by hash.
      wasmBindgenCli = pkgsNative.runCommand "wasm-bindgen-cli-0.2.127" { } ''
        mkdir -p $out/bin
        tar -xzf ${pkgsNative.fetchurl {
          url = "https://github.com/rustwasm/wasm-bindgen/releases/download/0.2.127/wasm-bindgen-0.2.127-x86_64-unknown-linux-musl.tar.gz";
          hash = "sha256-YdSn3IWs+g0jVMzAuDYZKMflKnRtF/KOuqeV7T3BYUo=";
        }}
        mv wasm-bindgen-0.2.127-x86_64-unknown-linux-musl/wasm-bindgen $out/bin/
      '';

      # --- frontend --------------------------------------------------------
      # Trunk writes to ../static (Trunk.toml), so buildTrunkPackage's default
      # install step is replaced: the output of this derivation *is* the
      # static/ directory contents.
      wasmArgs = {
        inherit src;
        pname = "clewdr-frontend";
        strictDeps = true;
        cargoExtraArgs = "--locked --package=clewdr-frontend";
        CARGO_BUILD_TARGET = "wasm32-unknown-unknown";
      };
      static = nativeCrane.buildTrunkPackage (wasmArgs // {
        wasm-bindgen-cli = wasmBindgenCli;
        preBuild = "cd clewdr-frontend";
        postBuild = "cd ..";
        installPhaseCommand = "cp -r ./static $out";
        # Built explicitly: buildTrunkPackage's auto-generated deps build
        # would inherit this installPhaseCommand and fail (a deps-only build
        # has no static/ to install).
        cargoArtifacts = nativeCrane.buildDepsOnly (wasmArgs // {
          installPhaseCommand = "mkdir -p $out";
        });
      });

      # --- cross builds -----------------------------------------------------
      mkPkgs = crossSystem:
        import nixpkgs ({
          inherit localSystem;
          overlays = [ (import rust-overlay) ];
          # The android NDK (Google-licensed) is needed for the
          # aarch64-android target; harmless for the others.
          config.allowUnfree = true;
        } // (if crossSystem == null then { } else { inherit crossSystem; }));

      # The raw NDK tree (btls-sys wants ANDROID_NDK_HOME with
      # build/cmake/android.toolchain.cmake); same version androidndkPkgs_27
      # builds its toolchain from. Accessed through androidndkPkgs rather than
      # androidenv.composeAndroidPackages directly: the direct call fails to
      # evaluate inside this flake ("attribute 'ndk-bundle' missing" from
      # within the derivation, even though the same drv evaluates fine via
      # this route — some attrset self-reference quirk in androidenv).
      androidNdk = pkgsNative.lib.head
        pkgsNative.androidndkPkgs_27.binaries.propagatedBuildInputs;
      # Where the NDK keeps the arm64 bionic libs, incl. libc++_shared.so.
      androidSysrootLibs = "${androidNdk}/libexec/android-sdk/ndk-bundle/toolchains/llvm/prebuilt/linux-x86_64/sysroot/usr/lib/aarch64-linux-android";

      # Builds the server for one target with one feature set. The C++ deps
      # (btls-sys vendored BoringSSL) need git (submodule init), libclang
      # (bindgen) and cmake/perl/ninja; bindgenHook feeds the cross clang's
      # libc cflags into BINDGEN_EXTRA_CLANG_ARGS.
      #
      # `static` (the frontend output) is injected at build time via preBuild
      # rather than wrapped into the source: wrapping the source in a
      # derivation would force its realisation at *evaluation* time, because
      # crane's dependency vendoring reads the source directory.
      mk = crossSystem: features: staticDir:
        let
          pkgs = mkPkgs crossSystem;
          isAndroid = pkgs.stdenv.hostPlatform.isAndroid;
          # The android cross set needs a different toolchain story: rust-overlay
          # cannot evaluate its toolchains inside it (the target-side splice is
          # empty, and gccForLibs pulls a from-source android gcc that fails on
          # missing bionic headers). Instead, use the native-set toolchain with
          # the android std added — rustc runs on x86_64 either way, and the
          # linker is the NDK clang from the cross stdenv's CC env.
          craneLib = if isAndroid then
            (crane.mkLib pkgs).overrideToolchain
              (pkgsNative.rust-bin.stable."1.98.0".minimal.override {
                targets = [ "aarch64-linux-android" ];
              })
          else
            (crane.mkLib pkgs).overrideToolchain (p: p.rust-bin.stable."1.98.0".minimal or null);
          buildArgs = {
            pname = "clewdr";
            version = "0.13.4";
            inherit src;
            cargoLock = ./Cargo.lock;
            cargoExtraArgs = "--no-default-features --features ${features} -p clewdr";
            doCheck = false;
            strictDeps = true;
            nativeBuildInputs = (with pkgs; [
              cmake perl gnumake ninja git
              pkgs.pkgsBuildHost.llvmPackages.libclang.lib
            ]) ++ (if isAndroid then [ ] else [ pkgs.rustPlatform.bindgenHook ]);
            LIBCLANG_PATH = "${pkgs.pkgsBuildHost.llvmPackages.libclang.lib}/lib";
          } // pkgs.lib.optionalAttrs isAndroid {
            # nixpkgs' cross stdenv exports SYSROOT (the NDK bionic sysroot),
            # and cargo's target-info probe reads it as the rustc --sysroot,
            # where rustlib does not exist — the probe then fails. Unset it in
            # preBuild; the NDK clang wrapper carries the sysroot in its own
            # cc-cflags, so the C++ builds keep it.
            preBuild = "unset SYSROOT";
            # rustc 1.98 dropped the long-name alias for the android target:
            # `--target aarch64-unknown-linux-android` fails with "could not
            # find specification". The builtin (and cargo-ndk's) name is
            # `aarch64-linux-android`; nixpkgs derives the long form, so pin
            # the short one here. The std dir is shared (rustlib/<target>),
            # and the repo's .cargo/config.toml already keys its android
            # rustflags on the short name.
            CARGO_BUILD_TARGET = "aarch64-linux-android";
            # rustc's default linker for android targets is `cc`; point it at
            # the NDK clang wrapper the cross stdenv already provides.
            CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER =
              "${pkgs.stdenv.cc}/bin/aarch64-unknown-linux-android-cc";
            # btls-sys's cmake cross path requires the NDK root (its
            # android.toolchain.cmake drives the BoringSSL build). The
            # deployAndroidPackage output nests the NDK under
            # libexec/android-sdk/ndk-bundle.
            ANDROID_NDK_HOME = "${androidNdk}/libexec/android-sdk/ndk-bundle";
            # clewdr's own build.rs android path is written for cargo-ndk: it
            # reads these to copy libc++_shared.so next to the binary (the
            # .cargo/config.toml rpath is $ORIGIN). postInstall puts the
            # library beside the installed binary, matching the release zip.
            CARGO_NDK_SYSROOT_LIBS_PATH = androidSysrootLibs;
            # Replicates nixpkgs' bindgenHook, which cannot be used here: it
            # references the cross set's clang, and for android that is a
            # from-source clang (compiler-rt) rather than the NDK toolchain.
            # The NDK cc-wrapper ships the same nix-support cflags files the
            # hook reads. ($NIX_CFLAGS_COMPILE is dropped: it is runtime env
            # and cannot be baked into an env var at eval time.)
            BINDGEN_EXTRA_CLANG_ARGS = pkgs.lib.concatStringsSep " " (
              pkgs.lib.optional
                (pkgs.lib.pathExists "${pkgs.stdenv.cc}/nix-support/cc-cflags")
                (builtins.readFile "${pkgs.stdenv.cc}/nix-support/cc-cflags")
              ++ pkgs.lib.optional
                (pkgs.lib.pathExists "${pkgs.stdenv.cc}/nix-support/libc-cflags")
                (builtins.readFile "${pkgs.stdenv.cc}/nix-support/libc-cflags")
              ++ pkgs.lib.optional
                (pkgs.lib.pathExists "${pkgs.stdenv.cc}/nix-support/libcxx-cxxflags")
                (builtins.readFile "${pkgs.stdenv.cc}/nix-support/libcxx-cxxflags")
            );
          };
          depsExpression = { }: craneLib.buildDepsOnly (buildArgs // {
            # Deliberately the *union* of the feature sets we ship, so that all
            # variants of a target share one dependency build (the release
            # artifact is portable, the image is xdg). This is sound because a
            # deps-only build compiles a dummy crate: clewdr's own code — the
            # only place portable and xdg are mutually exclusive — is never
            # compiled here. The cache is just a superset (etcetera *and*
            # self-replace/tempfile/zip).
            cargoExtraArgs =
              "--no-default-features --features embed-resource,portable,xdg -p clewdr";
            # buildDepsOnly runs `cargo check` *and* `cargo build` in its build
            # phase, so one dependency build can serve both check-style
            # consumers (clippy) and build-style ones (buildPackage). Note
            # `doCheck = false` does not turn the check off: it only drops the
            # `cargo test --no-run` pass and `--all-targets`.
            #
            # These cross artifacts have exactly one consumer, buildPackage
            # below, so the metadata-only pass is dead weight. checks.ci is a
            # separate native derivation and never inherits these.
            cargoCheckCommand = ":";
          });
          crateExpression = { }: craneLib.buildPackage (buildArgs // {
            cargoArtifacts = pkgs.callPackage depsExpression { };
            # Not in buildArgs: the deps-only build has no $out/bin to install
            # into, and must not depend on the frontend output (that would
            # invalidate the dependency cache on every frontend change).
            postInstall = pkgs.lib.optionalString isAndroid ''
              cp ${androidSysrootLibs}/libc++_shared.so $out/bin/
            '';
            # Not in buildArgs: the deps-only build must not depend on the
            # frontend output, or every frontend change would invalidate the
            # dependency cache. (buildArgs.preBuild, if any, runs first.)
            preBuild = (buildArgs.preBuild or "") + "\n" + ''
              mkdir -p static
              cp -r ${staticDir}/* static/
            '';
          });
        in
        pkgs.callPackage crateExpression { };

      # --- distroless image assembly ----------------------------------------
      # The runtime base stays gcr.io/distroless/static-debian13 (pinned by
      # digest via pullImage; bump the digests + sha256s together to update
      # the base). The nix-built static binary is layered on top, upx'd like
      # the current Dockerfile does.
      distroless = {
        amd64 = "sha256:0985f124d25d79a432b79e806764a9deb759e5c664be7c0633b9f13c3e12cbc0";
        arm64 = "sha256:15a69c654ed239b3faf5bc3725ff1dd580462eb882c7d5b9c02cdf37756657c2";
      };

      imageFor = arch: digest: hash: binary:
        let
          base = pkgsNative.dockerTools.pullImage {
            imageName = "gcr.io/distroless/static-debian13";
            imageDigest = digest;
            sha256 = hash;
            arch = arch;
            os = "linux";
            finalImageName = "clewdr";
            finalImageTag = "nix-${arch}";
          };
          compressed = pkgsNative.runCommand "clewdr-upx-${arch}" { nativeBuildInputs = [ pkgsNative.upx ]; } ''
            mkdir -p $out/usr/local/bin
            upx --best --lzma ${binary}/bin/clewdr -o $out/usr/local/bin/clewdr
          '';
          etc = pkgsNative.runCommand "clewdr-etc" { } ''
            mkdir -p $out/etc/clewdr/log
            touch $out/etc/clewdr/clewdr.toml
          '';
        in
        pkgsNative.dockerTools.buildLayeredImage {
          name = "clewdr";
          tag = "nix-${arch}";
          # Uncompressed: go-containerregistry's `crane push` reads docker
          # tarballs only, and fails on a gzipped one with "invalid tar
          # header". The registry compresses layers on the wire anyway.
          compressor = "none";
          # streamLayeredImage defaults to the host platform; the base's
          # architecture is not inherited from fromImage.
          architecture = arch;
          fromImage = base;
          contents = [ compressed etc ];
          config = {
            Env = [
              "CLEWDR_IP=0.0.0.0"
              "CLEWDR_PORT=8484"
              "CLEWDR_CHECK_UPDATE=FALSE"
              "CLEWDR_AUTO_UPDATE=FALSE"
            ];
            ExposedPorts = { "8484/tcp" = { }; };
            Volumes = { "/etc/clewdr" = { }; };
            Cmd = [
              "/usr/local/bin/clewdr"
              "--config"
              "/etc/clewdr/clewdr.toml"
              "--log-dir"
              "/etc/clewdr/log"
            ];
          };
        };

      mkImage = arch: digest: hash: mkMusl:
        imageFor arch digest hash
          (mkMusl "embed-resource,xdg" static);

      # --- checks -----------------------------------------------------------
      # Shared by the check derivation and its dependency build.
      checksArgs = {
        version = "0.13.4";
        inherit src;
        cargoLock = ./Cargo.lock;
        strictDeps = true;
        # Mirrors xtask's ensure_static_dir: the embed-resource combinations
        # only need the directory to exist.
        preBuild = ''
          mkdir -p static
          echo '<!doctype html><title>ClewdR</title>' > static/index.html
        '';
        nativeBuildInputs = [
          pkgsNative.cmake
          pkgsNative.perl
          pkgsNative.gnumake
          pkgsNative.ninja
          pkgsNative.git
          pkgsNative.llvmPackages.libclang.lib
          pkgsNative.rustPlatform.bindgenHook
        ];
        LIBCLANG_PATH = "${pkgsNative.llvmPackages.libclang.lib}/lib";
      };

      # The dependency build the checks inherit. This is the consumer crane's
      # default check+build+test passes exist for: xtask runs clippy (wants
      # metadata) and `cargo test` (wants linkable artifacts) over the same
      # dependency graph.
      #
      # Two things have to match xtask exactly or nothing is reused:
      #   - the dev profile, because xtask's clippy and test runs pass no
      #     --release (crane would otherwise cache release artifacts);
      #   - the feature set. This is the union of xtask's four combinations
      #     (FEATURE_COMBINATIONS in xtask/src/main.rs), which the two
      #     external-resource passes reuse as-is; the two embed-resource
      #     passes still recompile tower-http, which loses its fs feature
      #     there, and whatever depends on it.
      checksDeps = nativeCrane.buildDepsOnly (checksArgs // {
        pname = "clewdr-checks-deps";
        CARGO_PROFILE = "dev";
        cargoExtraArgs =
          "--locked --workspace --no-default-features"
          + " --features external-resource,embed-resource,portable,xdg";
        # clippy --all-targets and `cargo test` both need the dev-dependencies
        # built, which is what --all-targets pulls in here.
        cargoCheckExtraArgs = "--all-targets";
      });
    in
    {
      packages.${localSystem} = {
        inherit static;
        clewdr-gnu-x86_64 = mk null "embed-resource,portable" static;
        clewdr-musl-x86_64 = mk "x86_64-unknown-linux-musl" "embed-resource,portable" static;
        clewdr-gnu-aarch64 = mk "aarch64-unknown-linux-gnu" "embed-resource,portable" static;
        clewdr-musl-aarch64 = mk "aarch64-unknown-linux-musl" "embed-resource,portable" static;
        # The bare target string does not set useAndroidPrebuilt, and the
        # example ships with it false, which makes nixpkgs attempt to build
        # the whole android toolchain (compiler-rt, bionic) from source — it
        # gets stuck on missing pthread.h. With the flag on, nixpkgs uses the
        # prebuilt NDK (allowUnfree) instead.
        clewdr-android-aarch64 = mk
          (pkgsNative.lib.systems.examples.aarch64-android // { useAndroidPrebuilt = true; })
          "embed-resource,portable" static;
        image-amd64 = mkImage "amd64" distroless.amd64
          "sha256-VH/TrMOuSPGO/2KYYOXidKw47dfnT1jQrKkBl0zoOLo="
          (mk "x86_64-unknown-linux-musl");
        image-arm64 = mkImage "arm64" distroless.arm64
          "sha256-ER1dxk1umx3Va8plikllvWo/lTlvz5RlOfnB6sSYHso="
          (mk "aarch64-unknown-linux-musl");
        # CI helper: go-containerregistry crane (image push), pinned to the
        # flake's nixpkgs.
        crane = pkgsNative.crane;
      };

      checks.${localSystem} = {
        # The single gate, `cargo xtask ci` (fmt --check, lint, test), run as
        # one cached derivation. Same entry point as developers and CI.
        ci = nativeCrane.buildPackage (checksArgs // {
          pname = "clewdr-checks";
          cargoArtifacts = checksDeps;
          buildPhaseCargoCommand = "cargo xtask ci";
          doCheck = false;
          doInstallCargoArtifacts = false;
          doNotPostBuildInstallCargoBinaries = true;
          installPhaseCommand = "touch $out";
          CLEWDR_NIGHTLY_CARGO = "${nightlyNative}/bin/cargo";
          # Runs fully sandboxed: the workspace test suite is hermetic (unit
          # tests only, no network).
        });
        # Stands in for the release build on PRs: embed-resource,portable,
        # release profile, links and runs.
        smoke = self.packages.${localSystem}.clewdr-gnu-x86_64;
      };

      devShells.${localSystem}.default = nativeCrane.devShell {
        packages = [
          pkgsNative.trunk
          pkgsNative.binaryen
          pkgsNative.dart-sass
          wasmBindgenCli
          pkgsNative.cmake
          pkgsNative.perl
          pkgsNative.gnumake
          pkgsNative.ninja
          pkgsNative.git
          pkgsNative.llvmPackages.libclang.lib
        ];
        env = {
          LIBCLANG_PATH = "${pkgsNative.llvmPackages.libclang.lib}/lib";
          CLEWDR_NIGHTLY_CARGO = "${nightlyNative}/bin/cargo";
        };
      };
    };
}
