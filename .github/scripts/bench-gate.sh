#!/usr/bin/env bash
# The PR gate: compare baseline `base` against `pr`, then confirm every breach
# by re-measuring it before failing.
#
#   bench-gate.sh <base-tree> <base-target-dir> <max-slowdown-percent>
#
# Run from the PR tree once both sides saved their baselines into one Criterion
# directory (CRITERION_HOME, default `$CARGO_TARGET_DIR/criterion`):
# `bench-in-process.sh base` in <base-tree> with CARGO_TARGET_DIR=<base-target-dir>,
# and `bench-in-process.sh pr` here with this tree's own CARGO_TARGET_DIR
# (default `target`). The two target dirs must differ: one shared dir lets cargo
# run the base's binary for the PR. bench-provenance.py checks that first.
#
# BENCH_BASE_EMPTY=1 says the base has no in-process benches (bench-in-process.sh
# exited 78 there): nothing to gate against, so the gate warns and passes. In
# every other case a side with no data fails.
#
# A bench past the limit is re-measured on its own, interleaved: base then PR,
# one bench at a time, saved as `base-confirm` / `pr-confirm`. Each side re-runs
# from its own target dir; cargo finds both builds fresh, and the provenance
# check proves each re-run executed the same binary as its first pass. Then
# bench-compare.py --confirm decides: past the limit on the re-run, or on the
# mean of both passes, is confirmed. Re-runs stop once BENCH_CONFIRM_BUDGET_SECS
# (default 600, checked before each benchmark) is spent; a bench left
# unmeasured counts as confirmed, so a broken runner still fails.
set -euo pipefail
usage="usage: bench-gate.sh <base-tree> <base-target-dir> <max-slowdown-percent>"
base_tree=$(cd "${1:?$usage}" && pwd)
base_target=$(realpath -m "${2:?$usage}")
limit="${3:?$usage}"
budget="${BENCH_CONFIRM_BUDGET_SECS:-600}"
scripts=$(cd "$(dirname "$0")" && pwd)
export CARGO_TARGET_DIR CRITERION_HOME
CARGO_TARGET_DIR=$(realpath -m "${CARGO_TARGET_DIR:-target}")
CRITERION_HOME="${CRITERION_HOME:-$CARGO_TARGET_DIR/criterion}"

if [ "${BENCH_BASE_EMPTY:-}" = 1 ]; then
  exec python3 "$scripts/bench-compare.py" "$CRITERION_HOME" base pr "$limit" --allow-empty
fi

python3 "$scripts/bench-provenance.py" "$base_target" "$CARGO_TARGET_DIR" base pr

find "$CRITERION_HOME" -type d \( -name base-confirm -o -name pr-confirm \) -prune -exec rm -rf {} + 2>/dev/null || true
breaches=$(mktemp)
trap 'rm -f "$breaches"' EXIT

status=0
python3 "$scripts/bench-compare.py" "$CRITERION_HOME" base pr "$limit" --breaches "$breaches" || status=$?
case $status in
  0) exit 0 ;;
  1) ;;
  *) exit "$status" ;;
esac

echo "Re-measuring $(wc -l <"$breaches") bench(es) past ${limit}%, base then PR, one at a time."
start=$SECONDS
provenance=0
while IFS= read -r id <&3; do
  if ((SECONDS - start >= budget)); then
    echo "::warning::confirmation budget (${budget}s) spent; \`$id\` is not re-measured and counts as confirmed"
    continue
  fi
  echo "::group::re-measure $id (base)"
  (cd "$base_tree" && CARGO_TARGET_DIR="$base_target" "$scripts/bench-in-process.sh" base-confirm "$id") ||
    echo "::warning::re-measuring \`$id\` on the base failed"
  echo "::endgroup::"
  echo "::group::re-measure $id (pr)"
  "$scripts/bench-in-process.sh" pr-confirm "$id" ||
    echo "::warning::re-measuring \`$id\` on the PR failed"
  echo "::endgroup::"
  python3 "$scripts/bench-provenance.py" "$base_target" "$CARGO_TARGET_DIR" base-confirm pr-confirm --same-as base pr ||
    provenance=1
done 3<"$breaches"

status=0
python3 "$scripts/bench-compare.py" "$CRITERION_HOME" base pr "$limit" --confirm base-confirm pr-confirm || status=$?
if [ "$status" -eq 0 ] && [ "$provenance" -ne 0 ]; then
  status=1
fi
exit "$status"
