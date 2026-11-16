#!/usr/bin/env bash
# Build one executable twice from the same tree and fail if the bytes differ.
#
#   check-reproducible-build.sh scripted   # the `scripted` in-process bench
#   check-reproducible-build.sh cli        # the `flusso` release binary
#
# Between the two builds, `ahash` and `const-random-macro` are cleaned, so the
# second build recompiles everything that could bake a build-time random
# value into the executable (issue #355: Rhai's `ahash` did, through
# `const-random`, until `rhai` kept its default `ahash/runtime-rng`).
#
# A manual check, not a CI step: it costs a second link of the target (about
# 30 s for `scripted`, a minute or two for `cli`). Run it after changing
# Rhai's or `ahash`'s features, or adding a dependency that generates values
# at build time.
#
# Uses `$CARGO_TARGET_DIR` (default `target`).
set -euo pipefail
what="${1:?usage: check-reproducible-build.sh scripted|cli}"

case "$what" in
  scripted)
    build=(cargo bench -p flusso-script --bench scripted --no-run --message-format=json)
    select='select(.reason=="compiler-artifact" and .target.name=="scripted") | .executable'
    ;;
  cli)
    build=(cargo build --release -p flusso-cli --message-format=json)
    select='select(.reason=="compiler-artifact" and .target.name=="flusso" and .executable != null) | .executable'
    ;;
  *)
    echo "unknown target: $what (scripted|cli)" >&2
    exit 2
    ;;
esac

digest() {
  local exe
  exe=$("${build[@]}" 2>/dev/null | jq -r "$select" | tail -n 1)
  [ -n "$exe" ] || { echo "no executable for $what" >&2; exit 1; }
  sha256sum "$exe" | cut -d' ' -f1
}

first=$(digest)
cargo clean --release -p ahash -p const-random-macro >/dev/null 2>&1
second=$(digest)

echo "$what: $first"
echo "$what: $second (after rebuilding ahash and const-random-macro)"
if [ "$first" != "$second" ]; then
  echo "$what is not reproducible: two builds of the same tree differ" >&2
  exit 1
fi
