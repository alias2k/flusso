#!/usr/bin/env bash
# Compare the working tree with a base on the Docker-backed component benches.
#
#   review-ab.sh [<base>] [--pg 14,16,18] [--bench postgres,cases,pipeline]
#                [--filter <criterion filter>] [--limit <percent>] [--rounds <n>]
#                [--own-benches] [--bench-cpus LIST --server-cpus LIST | --no-pin]
#   review-ab.sh --digest [<base>] [--pg …] [--filter <index regex>] [--own-review] [cpu flags]
#   review-ab.sh --plans  [<base>] [--pg …] [--filter <case regex>]  [--own-review] [cpu flags]
#   review-ab.sh --clean            # the warm cache (~/.cache/flusso/review-ab)
#   review-ab.sh --clean-results    # the results under <head target>/review-ab
#
# <base> defaults to origin/main, as last fetched: the command never fetches.
# Run from the head tree (the working tree, uncommitted changes included).
# docs/src/contribute/benchmarks.md is the reader's guide; the steps:
#
# 1. One run at a time: an exclusive flock on `$FLUSSO_REVIEW_HOME/lock`
#    (default ~/.cache/flusso/review-ab) for the whole run, since the warm tree,
#    its target dir and the stamp are shared. A second run fails at once.
#    The cache must carry its marker (`.flusso-review-ab`), be new or empty, or
#    be a pre-marker cache (a real stamp, nothing foreign beside it): no other
#    directory is written to or deleted. Containers a killed run left
#    (`flusso-review-<pid>-*`, labelled with this host and pid namespace, pid
#    gone) are removed, and old result dirs pruned to the last 5.
# 2. The base is checked out into the warm tree with a private index (no git
#    worktree is registered; only files that differ are rewritten, so cargo
#    rebuilds only what changed), reset to the base commit before each build,
#    and built into its own target dir. The head builds into its own
#    `$CARGO_TARGET_DIR` (or ./target). bench-provenance.py checks each side
#    ran its own build. Only the benches asked for are built.
# 3. The head's bench sources are laid over the base (same harness, each side's
#    own code), unless --own-benches, which refuses a base whose benches would
#    ignore the server this script starts. review-ab-stamp.py decides whether
#    the warm build is current; a current build is reused without cargo.
# 4. One named Postgres container per version, plus one OpenSearch for
#    `pipeline`. Each bench run gets a fresh database (REVIEW_AB_PG_URL) and an
#    emptied cluster (REVIEW_AB_OS_URL). A trap removes these containers.
# 5. CPU pinning: the bench process runs under `taskset -c` on one set of
#    cores, the containers get `--cpuset-cpus` on a disjoint set (default: core
#    0 left alone, the next 2·k split in two, k = min(4, (nproc − 1) / 2); flags
#    or REVIEW_AB_BENCH_CPUS / REVIEW_AB_SERVER_CPUS override, --no-pin or
#    REVIEW_AB_PIN=0 turn it off). Off, and said so, without `taskset` or with
#    fewer than 5 cores.
# 6. Per version, per bench, --rounds (default 3) rounds of base then head:
#    base, head, base, head, … Criterion baselines `base-<r>` / `head-<r>`.
# 7. bench-compare.py --repeats flags a case past the limit (default 5%) and
#    twice the control spread; each flagged case is re-run alone (`--exact`,
#    base then head) and judged by the PR gate's confirm rule. Exits 1 only on
#    a confirmed case. Results: `<head target>/review-ab/<timestamp>/`.
#
# --digest and --plans swap steps 6-9 for one run of the `review` bench
# target per side (REVIEW_AB_MODE, see benches/review.rs, which includes
# cases.rs for its roots), built, stamped and checked as above, laid over the
# base with cases.rs. Only a mode builds it, so the bench comparison never
# needs a base that has what it uses; a mode refuses a base without one of
# its `// review-ab requires:` lines, and --bench, --rounds, --limit and
# --own-benches (exit 2). --own-review lays nothing over the base: each side
# runs its own harness over its own cases.rs, so a change to what a form
# calls (the backfill's key pages, #386) is compared against the base's own
# way of doing it. The base must have the harness; the output format is the
# same on both sides.
#
# - --digest: both sides build the `cases` roots' documents (`build`,
#   `build_many` under 16 keys and 16+, a backfill); review-ab-digest.py
#   prints one row per index: a digest per side, same or differ.
# - --plans: Postgres starts with pg_stat_statements (track_planning) and
#   auto_explain (JSON, every statement); per plan_cache_mode (auto,
#   force_generic_plan) each side runs every case 2 x 10 times;
#   review-ab-plans.py reads the counts and the server log and flags plans
#   made on every call, a changed root scan and lost partition pruning.
#
# A mode exits 0 when nothing is flagged, 1 when something is, 2 on a usage
# or setup error.
set -euo pipefail

# The default Postgres is the one the tests and benches start: dev/test-postgres's
# DEFAULT_TAG (`<major>-alpine`), read from the tree this script sits in.
test_postgres=$(cd "$(dirname "$0")/../.." && pwd)/dev/test-postgres/src/lib.rs
pg_versions=$(sed -n 's/^pub const DEFAULT_TAG: &str = "\([0-9][0-9]*\)-alpine";$/\1/p' "$test_postgres" 2>/dev/null || true)
if [ -z "$pg_versions" ]; then
  echo "error: no \`pub const DEFAULT_TAG: &str = \"<major>-alpine\";\` in $test_postgres" >&2
  exit 2
fi
usage="usage: review-ab.sh [--digest | --plans [--own-review]] [<base>] [--pg $pg_versions] [--bench postgres,cases,pipeline] [--filter <f>] [--limit 5] [--rounds 3] [--own-benches] [--bench-cpus LIST --server-cpus LIST | --no-pin] | --clean | --clean-results"
mode=bench
bench_only_flags=()
base=origin/main
benches=postgres,cases,pipeline
filter=
limit=5
rounds=3
own_benches=0
own_review=0
clean=0
clean_results=0
keep=5
bench_cpus=${REVIEW_AB_BENCH_CPUS:-}
server_cpus=${REVIEW_AB_SERVER_CPUS:-}
pin=${REVIEW_AB_PIN:-1}
positional=0
while [ $# -gt 0 ]; do
  case $1 in
    --pg) pg_versions=${2:?$usage}; shift 2 ;;
    --bench) benches=${2:?$usage}; bench_only_flags+=(--bench); shift 2 ;;
    --filter) filter=${2:?$usage}; shift 2 ;;
    --limit) limit=${2:?$usage}; bench_only_flags+=(--limit); shift 2 ;;
    --rounds) rounds=${2:?$usage}; bench_only_flags+=(--rounds); shift 2 ;;
    --own-benches) own_benches=1; shift ;;
    --own-review) own_review=1; shift ;;
    --bench-cpus) bench_cpus=${2:?$usage}; shift 2 ;;
    --server-cpus) server_cpus=${2:?$usage}; shift 2 ;;
    --no-pin) pin=0; shift ;;
    --digest) mode=digest; shift ;;
    --plans) mode=plans; shift ;;
    --clean) clean=1; shift ;;
    --clean-results) clean_results=1; shift ;;
    -h | --help) echo "$usage"; exit 0 ;;
    -*) echo "$usage" >&2; exit 2 ;;
    *)
      [ "$positional" -eq 0 ] || { echo "$usage" >&2; exit 2; }
      base=$1; positional=1; shift ;;
  esac
done

# A mode reports a setup failure as 2; the bench comparison keeps its 1.
setup_exit=1
if [ "$mode" != bench ]; then
  setup_exit=2
  if [ "$own_benches" -eq 1 ]; then
    bench_only_flags+=(--own-benches)
  fi
  if [ "${#bench_only_flags[@]}" -gt 0 ]; then
    echo "error: --$mode runs the head's \`review\` harness once per side, measuring nothing; it does not take ${bench_only_flags[*]}" >&2
    exit 2
  fi
  benches=review
elif [ "$own_review" -eq 1 ]; then
  echo "error: --own-review picks the \`review\` harness of --digest or --plans; the bench comparison has none" >&2
  exit 2
fi

scripts=$(cd "$(dirname "$0")" && pwd)
head_tree=$(git rev-parse --show-toplevel)
git_dir=$(realpath "$(git rev-parse --git-common-dir)")
home=$(realpath -m "${FLUSSO_REVIEW_HOME:-${XDG_CACHE_HOME:-$HOME/.cache}/flusso/review-ab}")
base_tree=$home/tree
base_target=$home/target
head_target=$(realpath -m "${CARGO_TARGET_DIR:-$head_tree/target}")
results=$head_target/review-ab
stamp=$home/stamp.json

# The cache is ours only if it carries the marker, is new or empty, or is a
# cache from before the marker (a real stamp, nothing foreign beside it): a
# FLUSSO_REVIEW_HOME pointing anywhere else is never written to or deleted.
marker=$home/.flusso-review-ab
cache=$(python3 "$scripts/review-ab-housekeeping.py" cache-state "$home")
if [ "$cache" = foreign ]; then
  echo "error: $home is not a review-ab cache; FLUSSO_REVIEW_HOME must be new, empty, or an existing review-ab cache" >&2
  exit 2
fi
if [ "$clean_results" -eq 1 ]; then
  [ -e "$results" ] && echo "removing $results ($(du -sh "$results" 2>/dev/null | cut -f1))"
  rm -rf "$results"
fi
if [ "$clean" -eq 1 ]; then
  if [ "$cache" = new ] || [ "$cache" = empty ]; then
    echo "nothing to clean at $home"
    exit 0
  fi
  if [ "$home" = / ] || [ "$home" = "$(realpath -m "$HOME")" ]; then
    echo "error: refusing to delete $home" >&2
    exit 2
  fi
fi
[ "$clean" -eq 1 ] || [ "$clean_results" -eq 0 ] || exit 0
if [ "$clean" -eq 0 ]; then
  mkdir -p "$home"
  [ "$cache" = ours ] || : >"$marker"
fi
exec 9>"$home/lock"
if ! flock -n 9; then
  echo "error: another review-ab run holds $home/lock; wait for it to finish" >&2
  exit "$setup_exit"
fi

if [ "$clean" -eq 1 ]; then
  echo "removing $home ($(du -sh "$home" 2>/dev/null | cut -f1)), the warm base tree and its build"
  rm -rf "$home"
  exit 0
fi

base_sha=$(git rev-parse --verify "$base^{commit}")
base_date=$(git log -1 --format=%cd --date=format:'%Y-%m-%d %H:%M' "$base_sha")
run_id=$(date +%Y%m%d-%H%M%S)
out=$results/$run_id

declare -A bench_file=(
  [postgres]=libs/2-adapters/source-postgres/benches/postgres.rs
  [cases]=libs/2-adapters/source-postgres/benches/cases.rs
  [pipeline]=libs/2-engine/benches/pipeline.rs
)
declare -A package_dir=(
  [postgres]=libs/2-adapters/source-postgres
  [cases]=libs/2-adapters/source-postgres
  [pipeline]=libs/2-engine
  [review]=libs/2-adapters/source-postgres
)
# The review modes' harness: a target of its own, so the bench comparison
# never compiles it (nor needs a base with what it uses).
review_file=libs/2-adapters/source-postgres/benches/review.rs
if [ "$mode" != bench ]; then
  bench_file[review]=$review_file
fi
selected=()
for bench in ${benches//,/ }; do
  [ -n "${bench_file[$bench]:-}" ] || { echo "error: unknown bench \`$bench\`" >&2; exit 2; }
  selected+=("$bench")
done

in_base() { git cat-file -e "$base_sha:$1" 2>/dev/null; }

overlay=()
if [ "$own_benches" -eq 0 ] && [ "$own_review" -eq 0 ]; then
  overlay=("${bench_file[postgres]}" "${bench_file[cases]}" "${bench_file[pipeline]}")
fi
if [ "$own_review" -eq 1 ]; then
  if ! in_base "$review_file"; then
    echo "error: --own-review runs the base's own \`review\` harness, which base $base (${base_sha:0:12}) predates; drop --own-review" >&2
    exit 2
  fi
elif [ "$mode" != bench ]; then
  overlay+=("$review_file")
  # Each `// review-ab requires: <text>` line of the harness names what it
  # needs from the crate's source; a base without it can't build the harness.
  while IFS= read -r required; do
    if ! git grep -q -F -e "$required" "$base_sha" -- libs/2-adapters/source-postgres/src 2>/dev/null; then
      echo "error: --$mode needs \`$required\` in libs/2-adapters/source-postgres/src, which base $base (${base_sha:0:12}) predates; pick a newer base" >&2
      exit 2
    fi
  done < <(sed -n 's|^// review-ab requires: ||p' "$head_tree/$review_file")
fi

# A base bench that predates REVIEW_AB_PG_URL would start its own testcontainer
# (Postgres 11) instead of the server --pg names: the two sides would not be
# measured on the same Postgres.
need=()
for bench in "${selected[@]}"; do
  file=${bench_file[$bench]}
  if [ "$own_benches" -eq 1 ]; then
    in_base "$file" || continue
    if ! git cat-file -p "$base_sha:$file" | grep -q REVIEW_AB_PG_URL; then
      echo "error: the base's \`$bench\` bench predates REVIEW_AB_PG_URL: with --own-benches it would run on its own testcontainer (Postgres 11), not on --pg $pg_versions; drop --own-benches" >&2
      exit 2
    fi
  fi
  need+=(--need "$bench")
done

stamp_args=(--commit "$base_sha" --target "$base_target" --root "$head_tree")
for rel in "${overlay[@]}"; do
  stamp_args+=(--file "$rel")
done
build_env() {
  env | grep -E '^(RUSTFLAGS|CARGO_ENCODED_RUSTFLAGS|CARGO_BUILD_RUSTFLAGS|CARGO_PROFILE_[A-Z0-9_]+|CARGO_TARGET_[A-Z0-9_]+_(RUSTFLAGS|LINKER))=' | sort || true
  echo "bench-in-process.sh $(sha256sum <"$scripts/bench-in-process.sh" | cut -d' ' -f1)"
  # Cargo's own configuration: $CARGO_HOME's, then every .cargo/ from the base
  # tree up to /. An absent file contributes nothing.
  local dir=$base_tree config
  for config in "${CARGO_HOME:-$HOME/.cargo}"/config{,.toml}; do
    [ ! -f "$config" ] || echo "$config $(sha256sum <"$config" | cut -d' ' -f1)"
  done
  while :; do
    for config in "$dir"/.cargo/config{,.toml}; do
      [ ! -f "$config" ] || echo "$config $(sha256sum <"$config" | cut -d' ' -f1)"
    done
    [ "$dir" != / ] || break
    dir=$(dirname "$dir")
  done
}
toolchain() { (cd "$1" && rustc -vV) 2>/dev/null || echo none; }

# Reset the warm tree to the base commit (an earlier overlay's files included)
# and lay the head's bench sources over it.
checkout_base() {
  mkdir -p "$base_tree"
  GIT_INDEX_FILE=$home/index git --git-dir="$git_dir" --work-tree="$base_tree" read-tree --reset -u "$base_sha"
  for rel in "${bench_file[postgres]}" "${bench_file[cases]}" "${bench_file[pipeline]}" "$review_file"; do
    in_base "$rel" || rm -f "$base_tree/$rel"
  done
  for rel in "${overlay[@]}"; do
    cp "$head_tree/$rel" "$base_tree/$rel"
  done
  local manifest=$base_tree/libs/2-adapters/source-postgres/Cargo.toml
  if [ "${#overlay[@]}" -gt 0 ] && ! grep -q '^name = "cases"' "$manifest"; then
    printf '\n[[bench]]\nname = "cases"\nharness = false\n' >>"$manifest"
  fi
  if [ "$mode" != bench ] && ! grep -q '^name = "review"' "$manifest"; then
    printf '\n[[bench]]\nname = "review"\nharness = false\n' >>"$manifest"
  fi
}

build() { # build <tree> <target dir> <log>
  (cd "$1" && CARGO_TARGET_DIR=$2 BENCH_SET=docker BENCH_ONLY=$benches BENCH_BUILD_ONLY=1 \
    "$scripts/bench-in-process.sh" review 2>&1 >/dev/null | tee "$3" >&2)
}

echo "review-ab: base $base = ${base_sha:0:12}, committed $base_date (not fetched: \`git fetch\` first for the latest)"
if [ "$mode" = bench ]; then
  echo "vs the working tree; Postgres $pg_versions; benches ${selected[*]}; $rounds rounds"
else
  echo "vs the working tree; Postgres $pg_versions; --$mode with the \`review\` harness"
fi

cpu_args=(--nproc "$(nproc)" --taskset "$(command -v taskset >/dev/null && echo 1 || echo 0)")
[ "$pin" != 0 ] || cpu_args+=(--off)
[ -z "$bench_cpus" ] || cpu_args+=(--bench "$bench_cpus")
[ -z "$server_cpus" ] || cpu_args+=(--servers "$server_cpus")
cpu_plan=$(python3 "$scripts/review-ab-housekeeping.py" cpus "${cpu_args[@]}")
{ read -r bench_cpus; read -r server_cpus; read -r cpu_message; } <<<"$cpu_plan"
echo "$cpu_message"
bench_pin=() server_pin=()
if [ -n "$bench_cpus" ]; then
  bench_pin=(taskset -c "$bench_cpus")
  server_pin=(--cpuset-cpus "$server_cpus")
fi

owner="$(uname -n) $(readlink /proc/self/ns/pid 2>/dev/null || echo unknown)"
listing=$(docker ps -a --filter name=flusso-review- \
  --format '{{.Names}}\t{{.Label "flusso.review-ab.owner"}}' || true)
for name in $(printf '%s\n' "$listing" | python3 "$scripts/review-ab-housekeeping.py" stale --owner "$owner"); do
  echo "removing $name, left by a run that no longer exists"
  docker rm -f "$name" >/dev/null || true
done
python3 "$scripts/review-ab-housekeeping.py" prune "$results" $((keep - 1))
mkdir -p "$out"

status=0
verdict=$(python3 "$scripts/review-ab-stamp.py" check "$stamp" "${stamp_args[@]}" "${need[@]}" \
  --toolchain "$(toolchain "$base_tree")" --build-env "$(build_env)") || status=$?
if [ "$status" -eq 0 ]; then
  echo "warm base build is current; not rebuilding"
else
  echo "warm base build is stale ($verdict); building the base in $base_tree"
  checkout_base
  if [ ! -d "$base_target" ] && [ -d "$head_target/release" ]; then
    mkdir -p "$base_target"
    cp -a --reflink=auto "$head_target/release" "$base_target/release"
  fi
  status=0
  build "$base_tree" "$base_target" "$out/build-base.log" || status=$?
  if [ "$status" -eq 78 ]; then
    echo "error: the base has none of the benches asked for (${selected[*]})" >&2
    exit "$setup_exit"
  elif [ "$status" -ne 0 ]; then
    if [ "${#overlay[@]}" -gt 0 ] && grep -q '(bench "' "$out/build-base.log"; then
      echo "error: the base does not build with the head's bench sources; try --own-benches" >&2
    else
      echo "error: the base does not build (see above)" >&2
    fi
    exit "$setup_exit"
  fi
  python3 "$scripts/review-ab-stamp.py" write "$stamp" "${stamp_args[@]}" \
    --toolchain "$(toolchain "$base_tree")" --build-env "$(build_env)" \
    --executables "$base_target/bench-executables/review.tsv"
fi
echo "building the head in $head_tree"
status=0
build "$head_tree" "$head_target" "$out/build-head.log" || status=$?
if [ "$status" -ne 0 ]; then
  [ "$mode" != bench ] || exit "$status"
  echo "error: the head does not build (see above)" >&2
  exit 2
fi
# This run's benches only: a reused base build may hold more than was asked for.
for target in "$base_target" "$head_target"; do
  awk -F'\t' -v want=" ${selected[*]} " 'index(want, " " $1 " ")' \
    "$target/bench-executables/review.tsv" >"$target/bench-executables/review-run.tsv"
  { [ ! -f "$target/bench-executables/review.identity.tsv" ] || cat "$target/bench-executables/review.identity.tsv"; } |
    awk -F'\t' -v want=" ${selected[*]} " 'index(want, " " $1 " ")' >"$target/bench-executables/review-run.identity.tsv"
done
python3 "$scripts/bench-provenance.py" "$base_target" "$head_target" review-run review-run
aa=
if python3 "$scripts/bench_identity.py" every "$base_target" "$head_target" review-run review-run; then
  aa="The base and head bench binaries are byte-identical: every difference below is noise."
  echo "$aa"
fi

containers=()
cleanup() {
  if [ "${#containers[@]}" -gt 0 ]; then
    docker rm -f "${containers[@]}" >/dev/null 2>&1 || true
  fi
}
trap cleanup EXIT
trap 'exit 130' INT TERM

host_port() { docker port "$1" "$2" | head -1 | sed 's/.*://'; }

start_postgres() { # start_postgres <name> <version> [<server argument>…]
  containers+=("$1")
  docker run -d --name "$1" --label "flusso.review-ab.owner=$owner" "${server_pin[@]}" -e POSTGRES_PASSWORD=postgres -p 127.0.0.1::5432 \
    "postgres:$2-alpine" "${@:3}" >/dev/null
  local deadline=$((SECONDS + 120))
  until docker exec "$1" pg_isready -U postgres -h 127.0.0.1 -q 2>/dev/null; do
    ((SECONDS < deadline)) || { echo "error: Postgres $2 did not start" >&2; return 1; }
    sleep 0.5
  done
}

os_url=
if [[ " ${selected[*]} " == *" pipeline "* ]]; then
  os=flusso-review-$$-os
  containers+=("$os")
  docker run -d --name "$os" --label "flusso.review-ab.owner=$owner" "${server_pin[@]}" -p 127.0.0.1::9200 \
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

executable() { # executable <target dir> <bench>
  awk -F'\t' -v b="$2" '$1 == b { print $2 }' "$1/bench-executables/review-run.tsv"
}

database=0
run_side() { # run_side <side> <bench> <baseline> [<exact id>]
  local tree=$head_tree target=$head_target exe selector=()
  if [ "$1" = base ]; then
    tree=$base_tree target=$base_target
  fi
  exe=$(executable "$target" "$2")
  if [ -z "$exe" ]; then
    echo "no \`$2\` bench on the $1 side; skipped"
    return
  fi
  if [ -n "${4:-}" ]; then
    selector=(--exact "$4")
  elif [ -n "$filter" ]; then
    selector=("$filter")
  fi
  database=$((database + 1))
  docker exec "$container" psql -U postgres -qc "CREATE DATABASE r$database" >/dev/null
  if [ -n "$os_url" ]; then
    curl -sf -X DELETE "$os_url/_all" >/dev/null
  fi
  # Criterion prints its gnuplot probe and progress to stderr even with
  # --quiet; the bench's own messages (a skipped case) stay.
  echo "── $2 · $3${4:+ · $4} · pg$version"
  (cd "$tree/${package_dir[$2]}" &&
    REVIEW_AB_PG_URL=postgres://postgres:postgres@127.0.0.1:$port/r$database REVIEW_AB_OS_URL=$os_url \
      CRITERION_HOME=$out/pg$version/$2 \
      "${bench_pin[@]}" "$exe" --bench --quiet --noplot --plotting-backend plotters --save-baseline "$3" "${selector[@]}" 2>&1 >/dev/null |
      sed -E '/^(Gnuplot not found|Benchmarking |Warning: Unable to complete )/d' >&2)
  docker exec "$container" psql -U postgres -qc "DROP DATABASE r$database" >/dev/null
}

# review <side> <out file> [<plan_cache_mode>]: one run of the `cases`
# executable in $mode on a fresh database.
review() {
  local tree=$head_tree target=$head_target exe
  if [ "$1" = base ]; then
    tree=$base_tree target=$base_target
  fi
  exe=$(executable "$target" review)
  database=$((database + 1))
  docker exec "$container" psql -U postgres -qc "CREATE DATABASE r$database" >/dev/null
  if [ -n "${3:-}" ]; then
    docker exec "$container" psql -U postgres -qc "ALTER DATABASE r$database SET plan_cache_mode = $3" >/dev/null
  fi
  echo "── $mode · $1${3:+ · $3} · pg$version"
  if ! (cd "$tree/${package_dir[review]}" &&
    REVIEW_AB_MODE=$mode REVIEW_AB_PG_URL=postgres://postgres:postgres@127.0.0.1:$port/r$database \
      REVIEW_AB_OUT=$2 REVIEW_AB_FILTER=$filter REVIEW_AB_RUN=$1-${3:-} \
      "${bench_pin[@]}" "$exe" >&2); then
    echo "error: the $1 side's \`review\` harness failed in --$mode (see above)" >&2
    exit 2
  fi
  docker exec "$container" psql -U postgres -qc "DROP DATABASE r$database" >/dev/null
}

if [ "$mode" != bench ]; then
  # Anything failing from here on (a container, psql) is a setup error.
  set -E
  trap 'exit 2' ERR
  flagged_any=0
  for version in ${pg_versions//,/ }; do
    container=flusso-review-$$-pg$version
    server_args=()
    if [ "$mode" = plans ]; then
      server_args=(-c shared_preload_libraries=pg_stat_statements,auto_explain
        -c pg_stat_statements.track_planning=on -c auto_explain.log_min_duration=0
        -c auto_explain.log_format=json)
    fi
    start_postgres "$container" "$version" "${server_args[@]}"
    port=$(host_port "$container" 5432)
    dir=$out/pg$version
    mkdir -p "$dir"
    status=0
    if [ "$mode" = digest ]; then
      review base "$dir/digest-base.tsv"
      review head "$dir/digest-head.tsv"
      table=$(python3 "$scripts/review-ab-digest.py" "$dir/digest-base.tsv" "$dir/digest-head.tsv") || status=$?
    else
      for cache_mode in auto force_generic_plan; do
        review base "$dir/plans-base-$cache_mode.tsv" "$cache_mode"
        review head "$dir/plans-head-$cache_mode.tsv" "$cache_mode"
      done
      docker logs "$container" >"$dir/server.log" 2>&1
      table=$(python3 "$scripts/review-ab-plans.py" "$dir") || status=$?
    fi
    [ "$status" -le 1 ] || exit 2
    [ "$status" -eq 0 ] || flagged_any=1
    {
      echo
      echo "## Postgres $version: $base (${base_sha:0:12}) vs the working tree, --$mode"
      echo
      echo "$table"
    } >"$dir/table.md"
    docker rm -f "$container" >/dev/null
  done
  for version in ${pg_versions//,/ }; do
    cat "$out/pg$version/table.md"
  done
  echo
  echo "Output: $out"
  exit "$flagged_any"
fi

failed=0
for version in ${pg_versions//,/ }; do
  container=flusso-review-$$-pg$version
  start_postgres "$container" "$version"
  port=$(host_port "$container" 5432)
  for bench in "${selected[@]}"; do
    for ((round = 1; round <= rounds; round++)); do
      run_side base "$bench" "base-$round"
      run_side head "$bench" "head-$round"
    done
  done
  flagged=$out/pg$version/flagged.tsv
  status=0
  python3 "$scripts/bench-compare.py" "$out/pg$version" base head "$limit" --repeats "$rounds" \
    --breaches "$flagged" >/dev/null || status=$?
  [ "$status" -le 1 ] || { echo "error: nothing measured on both sides for Postgres $version" >&2; exit 1; }
  if [ -s "$flagged" ]; then
    echo "re-running $(wc -l <"$flagged") flagged case(s) alone, base then head"
  fi
  while IFS=$'\t' read -r bench id <&3; do
    run_side base "$bench" base-confirm "$id"
    run_side head "$bench" head-confirm "$id"
  done 3<"$flagged"
  {
    echo
    echo "## Postgres $version: $base (${base_sha:0:12}) vs the working tree"
    echo
    python3 "$scripts/bench-compare.py" "$out/pg$version" base head "$limit" --repeats "$rounds" \
      --confirm base-confirm head-confirm || echo "status $?"
    if [ -n "$aa" ]; then
      echo
      echo "$aa"
    fi
  } >"$out/pg$version/table.md"
  docker rm -f "$container" >/dev/null
done

for version in ${pg_versions//,/ }; do
  table=$out/pg$version/table.md
  if grep -q '^status ' "$table"; then
    failed=1
  fi
  grep -v '^status ' "$table"
done
echo
echo "Criterion output: $out"
exit "$failed"
