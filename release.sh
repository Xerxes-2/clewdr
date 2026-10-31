#!/bin/bash
set -euo pipefail
VERSION=${1:-}
if [ -z "$VERSION" ]; then
    echo "Usage: ./release.sh <version> (e.g., 1.0.0)"
    exit 1
fi
# Keep dependency upgrades separate: the flake pins wasm-bindgen-cli to the
# version in Cargo.lock. Only the server crate carries the release version.
cargo set-version --package clewdr "$VERSION"
cargo metadata --locked --format-version 1 > /dev/null
cargo xtask ci
jj commit -m "Update to v$VERSION"
jj bookmark set master -r @-
jj git push --remote origin --bookmark master
jj tag set "v$VERSION" -r @-
jj git push --remote origin --tag "v$VERSION"
