# Release Notes

- build(docker): stop using musl targets in Docker image build; compile native GNU target directly to avoid `boring-sys2` `open64/fopen64/stat64` link failures
- build(docker): switch runtime base image to `debian:trixie-slim` and include required runtime libs (`ca-certificates`, `libgcc-s1`, `libstdc++6`)
- ci(docker): disable buildx cache for Docker publish workflow (`no-cache: true`) to avoid stale-layer interference during multi-arch builds
