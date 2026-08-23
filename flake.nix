{
  description = "SPIKE: nix builds of clewdr for the four linux targets (gnu/musl × x86_64/aarch64). See wiki/nix-convergence.md.";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    crane.url = "github:ipetkov/crane";
    rust-overlay.url = "github:oxalica/rust-overlay";
    rust-overlay.inputs.nixpkgs.follows = "nixpkgs";
  };

  outputs = { self, nixpkgs, crane, rust-overlay }:
    let
      localSystem = "x86_64-linux";
      pkgsNative = import nixpkgs { system = localSystem; };
      nativeLib = crane.mkLib pkgsNative;
      src = nativeLib.cleanCargoSource self;

      mkPkgs = crossSystem:
        import nixpkgs ({
          inherit localSystem;
          overlays = [ (import rust-overlay) ];
        } // (if crossSystem == null then { } else { inherit crossSystem; }));

      mk = crossSystem:
        let
          pkgs = mkPkgs crossSystem;
          craneLib = (crane.mkLib pkgs).overrideToolchain (p: p.rust-bin.stable."1.98.0".minimal);
          crateExpression = { }:
            craneLib.buildPackage {
              pname = "clewdr";
              version = "0.13.4";
              inherit src;
              cargoLock = ./Cargo.lock;
              cargoExtraArgs = "--no-default-features --features external-resource,portable -p clewdr";
              # buildDepsOnly/mkDummySrc is disabled: builtins.fromTOML (TOML
              # 1.0) cannot parse the multiline inline tables in Cargo.toml.
              # See wiki/nix-convergence.md. Re-enable after converting those
              # tables to standard form.
              cargoArtifacts = null;
              doCheck = false;
              strictDeps = true;
              nativeBuildInputs = with pkgs; [
                cmake perl gnumake ninja git
                pkgs.pkgsBuildHost.llvmPackages.libclang.lib
                pkgs.rustPlatform.bindgenHook
              ];
              LIBCLANG_PATH = "${pkgs.pkgsBuildHost.llvmPackages.libclang.lib}/lib";
            };
        in
        pkgs.callPackage crateExpression { };

      # --- distroless image assembly -------------------------------------
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
    in
    {
      packages.${localSystem} = {
        clewdr-gnu-x86_64 = mk null;
        clewdr-musl-x86_64 = mk "x86_64-unknown-linux-musl";
        clewdr-gnu-aarch64 = mk "aarch64-unknown-linux-gnu";
        clewdr-musl-aarch64 = mk "aarch64-unknown-linux-musl";
        image-amd64 = imageFor "amd64" distroless.amd64
          "sha256-VH/TrMOuSPGO/2KYYOXidKw47dfnT1jQrKkBl0zoOLo="
          self.packages.${localSystem}.clewdr-musl-x86_64;
        image-arm64 = imageFor "arm64" distroless.arm64
          "sha256-ER1dxk1umx3Va8plikllvWo/lTlvz5RlOfnB6sSYHso="
          self.packages.${localSystem}.clewdr-musl-aarch64;
      };
    };
}
