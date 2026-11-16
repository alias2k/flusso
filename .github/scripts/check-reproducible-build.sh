#!/usr/bin/env bash
# Build one executable twice from the same tree and fail if the bytes differ.
#
#   check-reproducible-build.sh [--full] scripted   # the `scripted` in-process bench
#   check-reproducible-build.sh [--full] cli        # the `flusso` release binary
#
# Two modes:
#
# - default: covers Rhai's hash seed only (issue #355). It builds in the
#   caller's target dir, cleans `ahash` and `const-random-macro`, and builds
#   again, so `ahash`, `rhai` and every crate that depends on them are
#   recompiled there. A build-time random value from any other crate is not
#   rebuilt, so this mode can't see it.
# - `--full`: covers every dependency. Builds from an empty
#   `$CARGO_TARGET_DIR/reproducible-check`, keeps the executable, deletes that
#   directory, and builds from empty again at the same path (one path, so
#   paths a build script embeds match). Each build is a full release build.
#
# Needs `jq`. Builds at cargo's default parallelism (set `CARGO_BUILD_JOBS` to
# change it). Runs at the workspace root, wherever it is called from. `$CARGO`
# names the cargo to run (default `cargo`). Cargo's errors go to stderr.
#
# A manual check, not a CI step: what it costs and when to run it is in
# docs/src/contribute/benchmarks.md. CI's cheap guard against the same
# regression is check-ahash-seed.py.
set -euo pipefail

full=false
if [ "${1:-}" = "--full" ]; then
  full=true
  shift
fi
what="${1:?usage: check-reproducible-build.sh [--full] scripted|cli}"
cargo="${CARGO:-cargo}"

case "$what" in
  scripted)
    build=(bench -p flusso-script --bench scripted --no-run --message-format=json)
    select='select(.reason=="compiler-artifact" and .target.name=="scripted" and .executable != null) | .executable'
    ;;
  cli)
    build=(build --release -p flusso-cli --message-format=json)
    select='select(.reason=="compiler-artifact" and .target.name=="flusso" and .executable != null) | .executable'
    ;;
  *)
    echo "unknown target: $what (scripted|cli)" >&2
    exit 2
    ;;
esac

cd "$(dirname "$0")"
cd "$(git rev-parse --show-toplevel)"

target=$(realpath -m "${CARGO_TARGET_DIR:-target}")

# Prints the executable's sha256; cargo's errors stay on stderr.
digest() {
  local exe
  if ! exe=$("$cargo" "${build[@]}" | jq -r "$select" | tail -n 1); then
    echo "build of $what failed: cargo's errors are above" >&2
    exit 1
  fi
  [ -n "$exe" ] || { echo "build of $what reported no executable" >&2; exit 1; }
  sha256sum "$exe" | cut -d' ' -f1
}

if $full; then
  scratch="$target/reproducible-check"
  trap 'rm -rf "$scratch"' EXIT
  rm -rf "$scratch"
  first=$(CARGO_TARGET_DIR="$scratch" digest)
  rm -rf "$scratch"
  second=$(CARGO_TARGET_DIR="$scratch" digest)
  how="from an empty target dir"
else
  export CARGO_TARGET_DIR="$target"
  first=$(digest)
  "$cargo" clean --release -p ahash -p const-random-macro >/dev/null
  second=$(digest)
  how="after rebuilding ahash and const-random-macro"
fi

echo "$what: $first"
echo "$what: $second ($how)"
if [ "$first" != "$second" ]; then
  echo "$what is not reproducible: two builds of the same tree differ" >&2
  exit 1
fi
