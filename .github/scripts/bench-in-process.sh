#!/usr/bin/env bash
# Run the six in-process (no Docker) Criterion benches, saving their results
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
# Two more records sit beside it, read by bench_identity.py (the gate gives no
# verdict to a bench that is identical on both sides):
#
# - `<baseline>.identity.tsv`, `<bench>\t<identity>`: a sha256 over the
#   executable's sha256 and each run-time input bench-inputs.tsv declares for
#   that bench (a file or directory of the tree, or `env:NAME`);
# - `<baseline>.ids.tsv`, `<bench>\t<Criterion id>`: what the executable's
#   `--list` prints, so each id is tied to the binary that measures it (the
#   in-process set only; a bench that fails to list records no id).
#
# Results go to `$CRITERION_HOME` (default: `$CARGO_TARGET_DIR/criterion`).
#
# A bench whose source file is missing is skipped: a PR base that predates the
# script, resolve or documents bench still yields the others. With none of the six present the
# script runs nothing and exits 78, so a caller can tell "this tree has no
# in-process benches" apart from a failure.
#
# Three knobs for review-ab.sh (all unset in the PR gate):
#
# - BENCH_SET=docker picks the Docker-backed benches (postgres, cases,
#   pipeline, and `review`, review-ab's review-mode harness) instead of the
#   six in-process ones;
# - BENCH_BUILD_ONLY=1 builds and records the executables without running
#   their benchmarks, so the caller can run base and head interleaved (an
#   in-process executable still runs once, with `--list`, to record its ids);
# - BENCH_ONLY=<target,…> builds only those bench targets of the set.
#
# And one for the PR gate: BENCH_SKIP_SAME_AS=<the other side's identity.tsv>
# builds and records every bench but runs none whose identity matches the
# other side's and whose ids were listed (bench.yml builds both sides first).
#
# The pgoutput, resolve, documents and render benches sit behind each crate's `bench`
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
identities="$CARGO_TARGET_DIR/bench-executables/$baseline.identity.tsv"
ids="$CARGO_TARGET_DIR/bench-executables/$baseline.ids.tsv"
inputs="$(cd "$(dirname "$0")" && pwd)/bench-inputs.tsv"
mkdir -p "$(dirname "$record")"
: >"$record"
: >"$identities"
: >"$ids"

# digest <file or directory>: a sha256 of its content, `missing` when absent.
# A directory's symlinks count by where they point.
digest() {
  if [ -d "$1" ]; then
    (cd "$1" && {
      find . -type f -print0 | LC_ALL=C sort -z | xargs -0r sha256sum
      find . -type l -printf '%p -> %l\n' | LC_ALL=C sort
    }) | sha256sum | cut -d' ' -f1
  elif [ -f "$1" ]; then
    sha256sum "$1" | cut -d' ' -f1
  else
    echo missing
  fi
}

# identity <bench target> <executable sha256>
identity() {
  {
    echo "executable $2"
    if [ -f "$inputs" ]; then
      local bench input name
      while IFS=$'\t' read -r bench input; do
        [ "$bench" = "$1" ] || continue
        case $input in
          env:*)
            name=${input#env:}
            if [ -n "${!name+set}" ]; then
              echo "$input $(printf '%s' "${!name}" | sha256sum | cut -d' ' -f1)"
            else
              echo "$input unset"
            fi
            ;;
          *) echo "$input $(digest "$input")" ;;
        esac
      done < <(grep -v '^#' "$inputs" | LC_ALL=C sort)
    fi
  } | sha256sum | cut -d' ' -f1
}

ran=0
# bench <source file> <package> <bench target> [cargo args…]
bench() {
  local file=$1 package=$2 target=$3
  shift 3
  if [ -n "${BENCH_ONLY:-}" ] && [[ ",$BENCH_ONLY," != *",$target,"* ]]; then
    return
  fi
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
  local sha mine listed=0 listing
  sha=$(sha256sum "$exe" | cut -d' ' -f1)
  mine=$(identity "$target" "$sha")
  printf '%s\t%s\t%s\n' "$target" "$exe" "$sha" >>"$record"
  printf '%s\t%s\n' "$target" "$mine" >>"$identities"
  if [ "${BENCH_SET:-in-process}" != docker ]; then
    if listing=$(cd "$dir" && "$exe" --bench --list 2>/dev/null); then
      listing=$(sed -n 's/: benchmark$//p' <<<"$listing")
      if [ -n "$listing" ]; then
        awk -v t="$target" '{ print t "\t" $0 }' <<<"$listing" >>"$ids"
        listed=1
      fi
    fi
    [ "$listed" -eq 1 ] || echo "::warning::\`$target\` listed no benchmark; its results are compared unless the other side lists them under identical bytes"
  fi
  ran=$((ran + 1))
  if [ -n "${BENCH_SKIP_SAME_AS:-}" ] && [ "$listed" -eq 1 ] && [ -f "$BENCH_SKIP_SAME_AS" ] &&
    [ "$(awk -F'\t' -v t="$target" '$1 == t { print $2 }' "$BENCH_SKIP_SAME_AS")" = "$mine" ]; then
    echo "not running \`$target\`: identical binary and run-time inputs on both sides"
    return
  fi
  if [ "${BENCH_BUILD_ONLY:-}" != 1 ]; then
    (cd "$dir" && "$exe" "${args[@]}")
  fi
}

if [ "${BENCH_SET:-in-process}" = docker ]; then
  bench libs/2-adapters/source-postgres/benches/postgres.rs flusso-source-postgres postgres
  bench libs/2-adapters/source-postgres/benches/cases.rs flusso-source-postgres cases
  bench libs/2-engine/benches/pipeline.rs flusso-engine pipeline
  bench libs/2-adapters/source-postgres/benches/review.rs flusso-source-postgres review
else
  bench libs/2-engine/benches/engine.rs flusso-engine engine
  bench libs/2-adapters/source-postgres/benches/pgoutput.rs flusso-source-postgres pgoutput --features bench
  bench libs/2-adapters/source-postgres/benches/resolve.rs flusso-source-postgres resolve --features bench
  bench libs/2-adapters/source-postgres/benches/documents.rs flusso-source-postgres documents --features bench
  bench libs/2-adapters/sink-opensearch/benches/render.rs flusso-sink-opensearch render --features bench
  bench libs/2-script/benches/scripted.rs flusso-script scripted
fi

if [ "$ran" -eq 0 ]; then
  echo "no in-process bench exists in this tree"
  exit 78
fi
