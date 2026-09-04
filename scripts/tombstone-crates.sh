#!/usr/bin/env bash
# Publish a final "moved" release of every crate name retired by the 0.16
# rename (ADR 0004), so crates.io points at the successor instead of going
# quiet. Run once, by hand, after the rename is on crates.io:
#
#   cargo login
#   scripts/tombstone-crates.sh            # publishes 0.15.2 of each old name
#   scripts/tombstone-crates.sh --dry-run  # builds the tombstones, publishes nothing
#
# Each tombstone is an empty library whose README names the successor. Nothing
# else is ever published under the old names.

set -euo pipefail

DRY_RUN=""
[[ "${1:-}" == "--dry-run" ]] && DRY_RUN="--dry-run"

VERSION="0.15.2"
declare -A SUCCESSOR=(
  [flusso-schema-core]=flusso-kernel
  [flusso-schema]=flusso-config
  [flusso-schema-config-toml]=flusso-config
  [flusso-schema-index-yaml]=flusso-config
  [flusso-queue-core]=flusso-stream
  [flusso-queue-channel]=flusso-stream-channel
  [flusso-sources-core]=flusso-source
  [flusso-sources-postgres]=flusso-source-postgres
  [flusso-sinks-core]=flusso-sink
  [flusso-sinks-stdout]=flusso-sink-stdout
  [flusso-sinks-opensearch]=flusso-sink-opensearch
)

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

for old in "${!SUCCESSOR[@]}"; do
  new="${SUCCESSOR[$old]}"
  dir="$work/$old"
  mkdir -p "$dir/src"
  cat >"$dir/Cargo.toml" <<EOF
[package]
name = "$old"
version = "$VERSION"
edition = "2024"
license = "Apache-2.0"
description = "Moved: this crate is now published as $new."
repository = "https://github.com/alias2k/flusso"
readme = "README.md"

[lib]
path = "src/lib.rs"
EOF
  cat >"$dir/README.md" <<EOF
# $old has moved

This crate was renamed to [\`$new\`](https://crates.io/crates/$new) in flusso 0.16, when the
library crates took the names of the seams they implement (kernel, ports, adapters, engine,
daemon). Nothing else will be published under \`$old\`; depend on \`$new\` instead.

See the flusso repository for the current crate map: <https://github.com/alias2k/flusso/blob/main/libs/README.md>.
EOF
  cat >"$dir/src/lib.rs" <<EOF
#![doc = include_str!("../README.md")]
EOF
  echo "publishing $old $VERSION → points at $new"
  (cd "$dir" && cargo publish --allow-dirty $DRY_RUN)
done
