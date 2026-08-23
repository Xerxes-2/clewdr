#!/usr/bin/env bash
# Publish the two architecture images and join them into multi-arch manifests.
#
# Called by the docker job in .github/workflows/build.yml against ghcr.io, and
# by `cargo xtask verify-push` against a throwaway local registry. Both callers
# run *this* file, so the tag scheme and the crane invocations are verified
# locally rather than only in CI.
#
# crane must be on PATH (nix shell .#crane -c ...).
#
# Usage: publish-images.sh <image> <ref> <sha> <amd64-tarball> <arm64-tarball>
#   image  registry/repository, e.g. ghcr.io/xerxes-2/clewdr
#   ref    a git ref, e.g. refs/tags/v0.13.4 or refs/heads/master
#   sha    the commit sha the images were built from
set -euo pipefail

if [ "$#" -ne 5 ]; then
  echo "usage: $0 <image> <ref> <sha> <amd64-tarball> <arm64-tarball>" >&2
  exit 2
fi

image="${1,,}" # registries are case-insensitive, ghcr.io rejects uppercase
ref="$2"
sha="$3"
amd64_tarball="$4"
arm64_tarball="$5"

tags=()
case "$ref" in
  refs/tags/*) tags+=("${ref#refs/tags/}" "latest") ;;
  refs/heads/*) tags+=("${ref#refs/heads/}") ;;
  *)
    echo "$0: not a branch or tag ref: $ref" >&2
    exit 2
    ;;
esac
# sha-<7> is the scheme docker/metadata-action's type=sha produced, kept so
# anything pinning an old sha tag still resolves.
tags+=("sha-${sha:0:7}")

for tag in "${tags[@]}"; do
  crane push "$amd64_tarball" "$image:$tag-amd64"
  crane push "$arm64_tarball" "$image:$tag-arm64"
  # Children go through -m. A positional form exists but means something else
  # (crane rejects these references outright), which is how the first attempt
  # at this got a single-arch tag pushed with no index behind it.
  crane index append -t "$image:$tag" \
    -m "$image:$tag-amd64" -m "$image:$tag-arm64"
  echo "$0: published $image:$tag (amd64 + arm64)"
done
