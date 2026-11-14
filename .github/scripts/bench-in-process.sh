#!/usr/bin/env bash
# Run the five in-process (no Docker) Criterion benches, saving their results
# under one Criterion baseline name so two runs can be compared side by side.
#
#   bench-in-process.sh <baseline-name> [<bench-id>]
#
# With a bench id (a Criterion full id such as `live_drain/sinks1/64`), only
# that one benchmark runs, matched exactly: `--exact`, so `live_drain/sinks1/6`
# never picks up `live_drain/sinks1/64`. Each bench target is still invoked;
# the ones that don't hold the id run nothing.
#
# Each bench is built with `cargo bench --no-run`, then the executable cargo
# reports is run directly (from its package directory, with `--bench`, as cargo
# does). Its path and sha256 are recorded, one `<bench>\t<path>\t<sha256>` line
# each, in `$CARGO_TARGET_DIR/bench-executables/<baseline-name>.tsv`, so the
# gate can prove which binary produced each baseline (bench-gate.sh).
#
# Results go to `$CRITERION_HOME` (default: `$CARGO_TARGET_DIR/criterion`).
#
# A bench whose source file is missing is skipped: a PR base that predates the
# script or resolve bench still yields the others. With none of the five present the
# script runs nothing and exits 78, so a caller can tell "this tree has no
# in-process benches" apart from a failure.
#
# The pgoutput, resolve and render benches sit behind each crate's `bench`
# feature, which exposes the crate-private decoder / an offline document
# builder / the renderer to them.
set -euo pipefail
baseline="${1:?usage: bench-in-process.sh <baseline-name> [<bench-id>]}"
args=(--bench --save-baseline "$baseline")
if [ -n "${2:-}" ]; then
  args+=(--exact "$2")
fi

export CARGO_TARGET_DIR
CARGO_TARGET_DIR=$(realpath -m "${CARGO_TARGET_DIR:-target}")
export CRITERION_HOME="${CRITERION_HOME:-$CARGO_TARGET_DIR/criterion}"
record="$CARGO_TARGET_DIR/bench-executables/$baseline.tsv"
mkdir -p "$(dirname "$record")"
: >"$record"

ran=0
# bench <source file> <package> <bench target> [cargo args…]
bench() {
  local file=$1 package=$2 target=$3
  shift 3
  if [ ! -f "$file" ]; then
    echo "skipping \`$target\`: $file is not in this tree"
    return
  fi
  local artifact exe dir
  artifact=$(cargo bench -p "$package" --bench "$target" "$@" --no-run --message-format=json-render-diagnostics |
    jq -c --arg t "$target" 'select(.reason == "compiler-artifact" and .target.name == $t and (.target.kind | index("bench")) and .executable != null)')
  exe=$(jq -r .executable <<<"$artifact")
  dir=$(dirname "$(jq -r .manifest_path <<<"$artifact")")
  if [ -z "$exe" ] || [ ! -x "$exe" ]; then
    echo "::error::cargo reported no executable for bench \`$target\`"
    return 1
  fi
  printf '%s\t%s\t%s\n' "$target" "$exe" "$(sha256sum "$exe" | cut -d' ' -f1)" >>"$record"
  (cd "$dir" && "$exe" "${args[@]}")
  ran=$((ran + 1))
}

bench libs/2-engine/benches/engine.rs flusso-engine engine
bench libs/2-adapters/source-postgres/benches/pgoutput.rs flusso-source-postgres pgoutput --features bench
bench libs/2-adapters/source-postgres/benches/resolve.rs flusso-source-postgres resolve --features bench
bench libs/2-adapters/sink-opensearch/benches/render.rs flusso-sink-opensearch render --features bench
bench libs/2-script/benches/scripted.rs flusso-script scripted

if [ "$ran" -eq 0 ]; then
  echo "no in-process bench exists in this tree"
  exit 78
fi
