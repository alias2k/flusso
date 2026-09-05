#!/usr/bin/env python3
"""Compare two Criterion baselines saved in one run and fail on a slowdown.

    bench-compare.py <criterion-dir> <baseline> <candidate> <max-slowdown-percent>

For every bench with both `<baseline>/estimates.json` and
`<candidate>/estimates.json`, compares medians. Exits 1 when any bench's
candidate median is more than `<max-slowdown-percent>` slower than its
baseline. Prints a table either way; with `GITHUB_STEP_SUMMARY` set, appends
it there as Markdown too.
"""
import json
import os
import pathlib
import sys


def median(path: pathlib.Path) -> float:
    return json.loads(path.read_text())["median"]["point_estimate"]


def fmt(ns: float) -> str:
    for unit, scale in (("s", 1e9), ("ms", 1e6), ("µs", 1e3)):
        if ns >= scale:
            return f"{ns / scale:.3f} {unit}"
    return f"{ns:.0f} ns"


def main() -> int:
    if len(sys.argv) != 5:
        print(__doc__, file=sys.stderr)
        return 2
    root, base, cand, limit = pathlib.Path(sys.argv[1]), sys.argv[2], sys.argv[3], float(sys.argv[4])
    rows = []
    for base_est in sorted(root.rglob(f"{base}/estimates.json")):
        bench_dir = base_est.parent.parent
        cand_est = bench_dir / cand / "estimates.json"
        if not cand_est.is_file():
            continue
        rel = "/".join(bench_dir.relative_to(root).parts)
        b, c = median(base_est), median(cand_est)
        change = (c - b) / b * 100 if b else 0.0
        rows.append((rel, b, c, change))
    if not rows:
        print(f"nothing to compare: no bench has both `{base}` and `{cand}` under {root}", file=sys.stderr)
        return 1
    regressed = [r for r in rows if r[3] > limit]
    lines = [
        f"| bench | {base} | {cand} | change |",
        "| --- | ---: | ---: | ---: |",
    ]
    for rel, b, c, change in rows:
        flag = " ⚠️" if change > limit else ""
        lines.append(f"| `{rel}` | {fmt(b)} | {fmt(c)} | {change:+.1f}%{flag} |")
    verdict = (
        f"**{len(regressed)} bench(es) slower than {limit:.0f}%** — regression."
        if regressed
        else f"No bench slower than {limit:.0f}%."
    )
    table = "\n".join(lines)
    print(table)
    print(verdict)
    summary = os.environ.get("GITHUB_STEP_SUMMARY")
    if summary:
        with open(summary, "a", encoding="utf-8") as fh:
            fh.write(f"## In-process benches: `{cand}` vs `{base}`\n\n{table}\n\n{verdict}\n")
    return 1 if regressed else 0


if __name__ == "__main__":
    sys.exit(main())
