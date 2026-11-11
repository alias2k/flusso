#!/usr/bin/env bash
# The PR gate: compare baseline `base` against `pr`, then confirm every breach
# by re-measuring it before failing.
#
#   bench-gate.sh <base-tree> <max-slowdown-percent>
#
# Run from the PR tree once both sides saved their baselines into the same
# Criterion directory: `bench-in-process.sh base` in <base-tree> and
# `bench-in-process.sh pr` here, with one CARGO_TARGET_DIR (default: this
# tree's `target`). Both trees stay checked out, so a re-run rebuilds nothing.
#
# A bench past the limit is re-measured on its own, interleaved: base then PR,
# one bench at a time, saved as `base-confirm` / `pr-confirm`. The gate fails
# only when a bench is past the limit again (bench-compare.py --confirm).
# Re-runs stop once BENCH_CONFIRM_BUDGET_SECS (default 600) is spent; a bench
# left unmeasured counts as confirmed, so a broken runner still fails.
set -euo pipefail
base_tree=$(cd "${1:?usage: bench-gate.sh <base-tree> <max-slowdown-percent>}" && pwd)
limit="${2:?usage: bench-gate.sh <base-tree> <max-slowdown-percent>}"
budget="${BENCH_CONFIRM_BUDGET_SECS:-600}"
scripts=$(cd "$(dirname "$0")" && pwd)
export CARGO_TARGET_DIR
CARGO_TARGET_DIR=$(realpath "${CARGO_TARGET_DIR:-target}")
criterion="$CARGO_TARGET_DIR/criterion"

find "$criterion" -type d \( -name base-confirm -o -name pr-confirm \) -prune -exec rm -rf {} + 2>/dev/null || true
breaches=$(mktemp)
trap 'rm -f "$breaches"' EXIT

status=0
python3 "$scripts/bench-compare.py" "$criterion" base pr "$limit" --breaches "$breaches" || status=$?
case $status in
  0) exit 0 ;;
  1) ;;
  *) exit "$status" ;;
esac

echo "Re-measuring $(wc -l <"$breaches") bench(es) past ${limit}%, base then PR, one at a time."
start=$SECONDS
while IFS= read -r id <&3; do
  if ((SECONDS - start >= budget)); then
    echo "::warning::confirmation budget (${budget}s) spent; \`$id\` is not re-measured and counts as confirmed"
    continue
  fi
  echo "::group::re-measure $id (base)"
  (cd "$base_tree" && "$scripts/bench-in-process.sh" base-confirm "$id") ||
    echo "::warning::re-measuring \`$id\` on the base failed"
  echo "::endgroup::"
  echo "::group::re-measure $id (pr)"
  "$scripts/bench-in-process.sh" pr-confirm "$id" ||
    echo "::warning::re-measuring \`$id\` on the PR failed"
  echo "::endgroup::"
done 3<"$breaches"

python3 "$scripts/bench-compare.py" "$criterion" base pr "$limit" --confirm base-confirm pr-confirm
