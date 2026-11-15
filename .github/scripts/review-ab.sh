#!/usr/bin/env bash
# Compare the working tree with a base on the Docker-backed component benches.
#
#   review-ab.sh [<base>] [--pg 14,16,18] [--bench postgres,cases,pipeline]
#                [--filter <criterion filter>] [--limit <percent>] [--own-benches]
#
# <base> defaults to origin/main. Run from the head tree (the working tree,
# uncommitted changes included). docs/src/contribute/benchmarks.md has the
# reader's guide; the steps:
#
# 1. The base is checked out into a warm tree, `$FLUSSO_REVIEW_HOME/tree`
#    (default ~/.cache/flusso/review-ab), with a private index: no git
#    worktree is registered, and only the files that differ from the last base
#    are rewritten, so cargo rebuilds only what changed. It builds into its own
#    target dir, `$FLUSSO_REVIEW_HOME/target`, seeded once from the head's
#    `release` dir (the bench profile's) with a reflink copy. The head builds
#    into its own `$CARGO_TARGET_DIR` (or ./target). One shared dir would let cargo run one tree's binary for the
#    other; bench-provenance.py checks it did not.
# 2. The head's bench sources are laid over the base (same harness, each side's
#    own code), unless --own-benches. review-ab-stamp.py decides whether the
#    warm build is current (base commit, toolchain, overlaid files, the
#    executables themselves); a current build is reused without calling cargo.
# 3. One named Postgres container per version, plus one OpenSearch when
#    `pipeline` runs. Each bench run gets a fresh database (BENCH_PG_URL) and an
#    emptied cluster (BENCH_OS_URL). A trap removes exactly these containers.
# 4. Per version, per bench: base, head, then base again (the control), saved
#    as Criterion baselines `base`, `head`, `control` in one CRITERION_HOME per
#    version under `<head target>/review-ab/<timestamp>/`.
# 5. bench-compare.py --control prints one table per version. Exits 1 when a
#    case is slower past the limit (default 5%) and twice the control spread.
set -euo pipefail

usage="usage: review-ab.sh [<base>] [--pg 16] [--bench postgres,cases,pipeline] [--filter <f>] [--limit 5] [--own-benches]"
base=origin/main
pg_versions=16
benches=postgres,cases,pipeline
filter=
limit=5
own_benches=0
positional=0
while [ $# -gt 0 ]; do
  case $1 in
    --pg) pg_versions=${2:?$usage}; shift 2 ;;
    --bench) benches=${2:?$usage}; shift 2 ;;
    --filter) filter=${2:?$usage}; shift 2 ;;
    --limit) limit=${2:?$usage}; shift 2 ;;
    --own-benches) own_benches=1; shift ;;
    -h | --help) echo "$usage"; exit 0 ;;
    -*) echo "$usage" >&2; exit 2 ;;
    *)
      [ "$positional" -eq 0 ] || { echo "$usage" >&2; exit 2; }
      base=$1; positional=1; shift ;;
  esac
done

scripts=$(cd "$(dirname "$0")" && pwd)
head_tree=$(git rev-parse --show-toplevel)
git_dir=$(realpath "$(git rev-parse --git-common-dir)")
base_sha=$(git rev-parse --verify "$base^{commit}")
home=$(realpath -m "${FLUSSO_REVIEW_HOME:-${XDG_CACHE_HOME:-$HOME/.cache}/flusso/review-ab}")
base_tree=$home/tree
base_target=$home/target
head_target=$(realpath -m "${CARGO_TARGET_DIR:-$head_tree/target}")
stamp=$home/stamp.json
run_id=$(date +%Y%m%d-%H%M%S)
out=$head_target/review-ab/$run_id

overlay=()
if [ "$own_benches" -eq 0 ]; then
  overlay=(
    libs/2-adapters/source-postgres/benches/postgres.rs
    libs/2-adapters/source-postgres/benches/cases.rs
    libs/2-engine/benches/pipeline.rs
  )
fi
stamp_files=()
for rel in "${overlay[@]}"; do
  stamp_files+=(--file "$rel")
done

toolchain() { (cd "$1" && rustc -vV) 2>/dev/null || echo none; }

# Check the base out into the warm tree and lay the head's bench sources over it.
checkout_base() {
  mkdir -p "$base_tree"
  GIT_INDEX_FILE=$home/index git --git-dir="$git_dir" --work-tree="$base_tree" read-tree --reset -u "$base_sha"
  local manifest=$base_tree/libs/2-adapters/source-postgres/Cargo.toml
  for rel in "${overlay[@]}"; do
    cp "$head_tree/$rel" "$base_tree/$rel"
  done
  if [ "${#overlay[@]}" -gt 0 ] && ! grep -q '^name = "cases"' "$manifest"; then
    printf '\n[[bench]]\nname = "cases"\nharness = false\n' >>"$manifest"
  fi
}

build() { # build <tree> <target dir>
  (cd "$1" && CARGO_TARGET_DIR=$2 BENCH_SET=docker BENCH_BUILD_ONLY=1 "$scripts/bench-in-process.sh" review >/dev/null)
}

echo "review-ab: base $base (${base_sha:0:12}) vs the working tree; Postgres $pg_versions; benches $benches"
status=0
verdict=$(python3 "$scripts/review-ab-stamp.py" check "$stamp" --commit "$base_sha" \
  --toolchain "$(toolchain "$base_tree")" --target "$base_target" --root "$head_tree" "${stamp_files[@]}") || status=$?
if [ "$status" -eq 0 ]; then
  echo "warm base build is current; not rebuilding"
else
  echo "warm base build is stale ($verdict); building the base in $base_tree"
  checkout_base
  if [ ! -d "$base_target" ] && [ -d "$head_target/release" ]; then
    mkdir -p "$base_target"
    cp -a --reflink=auto "$head_target/release" "$base_target/release"
  fi
  if ! build "$base_tree" "$base_target"; then
    echo "error: the base does not build with the head's bench sources; try --own-benches" >&2
    exit 1
  fi
  python3 "$scripts/review-ab-stamp.py" write "$stamp" --commit "$base_sha" \
    --toolchain "$(toolchain "$base_tree")" --target "$base_target" \
    --executables "$base_target/bench-executables/review.tsv" --root "$head_tree" "${stamp_files[@]}"
fi
echo "building the head in $head_tree"
build "$head_tree" "$head_target"
python3 "$scripts/bench-provenance.py" "$base_target" "$head_target" review review

containers=()
cleanup() {
  if [ "${#containers[@]}" -gt 0 ]; then
    docker rm -f "${containers[@]}" >/dev/null 2>&1 || true
  fi
}
trap cleanup EXIT
trap 'exit 130' INT TERM

host_port() { docker port "$1" "$2" | head -1 | sed 's/.*://'; }

start_postgres() { # start_postgres <version> → prints the container name
  local name=flusso-review-$$-pg$1
  containers+=("$name")
  docker run -d --name "$name" -e POSTGRES_PASSWORD=postgres -p 127.0.0.1::5432 "postgres:$1-alpine" >/dev/null
  local deadline=$((SECONDS + 120))
  until docker exec "$name" pg_isready -U postgres -h 127.0.0.1 -q 2>/dev/null; do
    ((SECONDS < deadline)) || { echo "error: Postgres $1 did not start" >&2; return 1; }
    sleep 0.5
  done
}

os_url=
if [[ ",$benches," == *,pipeline,* ]]; then
  os=flusso-review-$$-os
  containers+=("$os")
  docker run -d --name "$os" -p 127.0.0.1::9200 \
    -e discovery.type=single-node -e DISABLE_SECURITY_PLUGIN=true -e DISABLE_INSTALL_DEMO_CONFIG=true \
    -e action.destructive_requires_name=false -e OPENSEARCH_JAVA_OPTS="-Xms512m -Xmx512m" \
    opensearchproject/opensearch:2 >/dev/null
  os_url=http://127.0.0.1:$(host_port "$os" 9200)
  deadline=$((SECONDS + 180))
  until curl -sf "$os_url/_cluster/health" >/dev/null; do
    ((SECONDS < deadline)) || { echo "error: OpenSearch did not start" >&2; exit 1; }
    sleep 1
  done
fi

package_dir() {
  case $1 in
    postgres | cases) echo libs/2-adapters/source-postgres ;;
    pipeline) echo libs/2-engine ;;
  esac
}
executable() { # executable <target dir> <bench>
  awk -F'\t' -v b="$2" '$1 == b { print $2 }' "$1/bench-executables/review.tsv"
}

database=0
run_side() { # run_side <container> <port> <tree> <target dir> <bench> <baseline> <criterion home>
  local exe
  exe=$(executable "$4" "$5")
  if [ -z "$exe" ]; then
    echo "no \`$5\` bench in $3; skipped"
    return
  fi
  database=$((database + 1))
  docker exec "$1" psql -U postgres -qc "CREATE DATABASE r$database" >/dev/null
  if [ -n "$os_url" ]; then
    curl -sf -X DELETE "$os_url/_all" >/dev/null
  fi
  echo "── $5 · $6 · $(basename "$7")"
  (cd "$3/$(package_dir "$5")" &&
    BENCH_PG_URL=postgres://postgres:postgres@127.0.0.1:$2/r$database BENCH_OS_URL=$os_url CRITERION_HOME=$7 \
      "$exe" --bench --noplot --save-baseline "$6" ${filter:+"$filter"} >/dev/null)
  docker exec "$1" psql -U postgres -qc "DROP DATABASE r$database" >/dev/null
}

failed=0
reports=()
for version in ${pg_versions//,/ }; do
  start_postgres "$version"
  container=flusso-review-$$-pg$version
  port=$(host_port "$container" 5432)
  home_v=$out/pg$version
  for bench in ${benches//,/ }; do
    run_side "$container" "$port" "$base_tree" "$base_target" "$bench" base "$home_v"
    run_side "$container" "$port" "$head_tree" "$head_target" "$bench" head "$home_v"
    run_side "$container" "$port" "$base_tree" "$base_target" "$bench" control "$home_v"
  done
  docker rm -f "$container" >/dev/null
  reports+=("$version")
done

for version in "${reports[@]}"; do
  echo
  echo "## Postgres $version: ${base} (${base_sha:0:12}) vs the working tree"
  echo
  python3 "$scripts/bench-compare.py" "$out/pg$version" base head "$limit" --control control || failed=1
done
echo
echo "Criterion output: $out"
exit "$failed"
