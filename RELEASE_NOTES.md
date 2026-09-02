# Release Notes

## Fixes

- **Claude Code 2.1.258 compatibility.** Requests now identify as the current
  client instead of the legacy 2.1.76 release. Billing attribution also joins
  JavaScript UTF-16 code units 4, 7, and 20 before UTF-8 hashing, keeping the
  `cc_version` suffix correct for emoji and surrogate-pair boundaries.
- **Portable GNU/Linux archives.** Nix-built GNU binaries have their loader
  path repaired before packaging, so the release ZIPs run outside the Nix
  store on supported distributions instead of failing with “No such file or
  directory”.
- **Container publication.** Multi-architecture image publishing now preserves
  the existing `sha-<7>` tags, produces real amd64/arm64 manifest lists, and
  avoids publishing images from manual workflow dispatches.

## Changes

- **Reproducible release builds.** Linux GNU, musl, Android, frontend and
  distroless container outputs now use pinned Nix definitions. `nix develop`
  provides the matching stable/wasm/nightly/trunk toolchain, while Windows and
  macOS retain their native build paths. The HTTP stack and other dependencies
  were refreshed, including the stable wreq 0.16 release.
- **Faster, verifiable CI.** Linux targets and both container architectures now
  build in parallel; exact feature sets, shared frontend store paths and
  minimal per-target toolchains avoid redundant compilation. Workflow YAML and
  the image-push/tag scheme are also checked locally through xtask before
  publication.
