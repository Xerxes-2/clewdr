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
      nativeLib = crane.mkLib (import nixpkgs { system = localSystem; });
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
    in
    {
      packages.${localSystem} = {
        clewdr-gnu-x86_64 = mk null;
        clewdr-musl-x86_64 = mk "x86_64-unknown-linux-musl";
        clewdr-gnu-aarch64 = mk "aarch64-unknown-linux-gnu";
        clewdr-musl-aarch64 = mk "aarch64-unknown-linux-musl";
      };
    };
}
