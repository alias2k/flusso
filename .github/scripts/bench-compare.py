#!/usr/bin/env python3
"""Compare two Criterion baselines saved in one run and fail on a slowdown.

    bench-compare.py <criterion-dir> <baseline> <candidate> <max-slowdown-percent>
                     [--breaches <file>] [--confirm <re-baseline> <re-candidate>]

For every bench with both `<baseline>/estimates.json` and
`<candidate>/estimates.json`, compares medians; a bench on one side only is
listed but not gated. Exits 1 when any bench's candidate median is more than
`<max-slowdown-percent>` slower than its baseline. Prints a table either way.

--breaches <file>   write the Criterion id of each bench past the limit, one
                    per line (what bench-gate.sh re-measures).
--confirm A B       a breach counts only if the bench is past the limit again
                    in the re-measured baselines A vs B; a breach with no
                    re-measurement counts as confirmed. Exits 1 only on a
                    confirmed breach.

With `GITHUB_STEP_SUMMARY` set, the final report (no breach, or the --confirm
run) is appended there as Markdown too.
"""
import argparse
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


def change(b: float, c: float) -> float:
    return (c - b) / b * 100 if b else 0.0


def bench_id(root: pathlib.Path, bench_dir: pathlib.Path, baseline: str) -> str:
    """The Criterion full id (what `--exact` matches); the directory path is the fallback."""
    meta = bench_dir / baseline / "benchmark.json"
    if meta.is_file():
        full_id = json.loads(meta.read_text()).get("full_id")
        if full_id:
            return full_id
    return "/".join(bench_dir.relative_to(root).parts)


def pair(bench_dir: pathlib.Path, base: str, cand: str):
    b, c = bench_dir / base / "estimates.json", bench_dir / cand / "estimates.json"
    if b.is_file() and c.is_file():
        return median(b), median(c)
    return None


def main() -> int:
    parser = argparse.ArgumentParser(usage=__doc__)
    parser.add_argument("root", type=pathlib.Path)
    parser.add_argument("base")
    parser.add_argument("cand")
    parser.add_argument("limit", type=float)
    parser.add_argument("--breaches", type=pathlib.Path)
    parser.add_argument("--confirm", nargs=2, metavar=("RE_BASE", "RE_CAND"))
    args = parser.parse_args()
    root, base, cand, limit = args.root, args.base, args.cand, args.limit

    bench_dirs = sorted(
        {p.parent.parent for side in (base, cand) for p in root.rglob(f"{side}/estimates.json")}
    )
    rows, one_sided = [], []
    for bench_dir in bench_dirs:
        measured = pair(bench_dir, base, cand)
        if measured is None:
            side = base if (bench_dir / base / "estimates.json").is_file() else cand
            one_sided.append((bench_id(root, bench_dir, side), side))
            continue
        b, c = measured
        rows.append((bench_id(root, bench_dir, cand), bench_dir, b, c, change(b, c)))
    if not rows:
        # A base that predates the benches leaves nothing to gate against; the
        # candidate's own bench step already proved it runs.
        print(f"::warning::nothing to compare: no bench has both `{base}` and `{cand}` under {root}")
        return 0

    breached = [r for r in rows if r[4] > limit]
    lines = [f"| bench | {base} | {cand} | change |", "| --- | ---: | ---: | ---: |"]
    for rid, _, b, c, ch in rows:
        flag = " ⚠️" if ch > limit else ""
        lines.append(f"| `{rid}` | {fmt(b)} | {fmt(c)} | {ch:+.1f}%{flag} |")
    report = ["### First pass", "", *lines, ""]
    if one_sided:
        report.append("Not compared (measured on one side only): " + ", ".join(f"`{i}` (only `{s}`)" for i, s in one_sided) + ".")
        report.append("")

    if args.breaches is not None:
        args.breaches.write_text("".join(f"{r[0]}\n" for r in breached))

    if args.confirm is None:
        failed = breached
        verdict = (
            f"**{len(breached)} bench(es) slower than {limit:.0f}%** — regression."
            if breached
            else f"No bench slower than {limit:.0f}%."
        )
    else:
        re_base, re_cand = args.confirm
        failed = []
        re_lines = [
            f"| bench | first pass | {re_base} | {re_cand} | re-run change | verdict |",
            "| --- | ---: | ---: | ---: | ---: | --- |",
        ]
        for rid, bench_dir, _, _, ch in breached:
            again = pair(bench_dir, re_base, re_cand)
            if again is None:
                failed.append(rid)
                re_lines.append(f"| `{rid}` | {ch:+.1f}% | — | — | — | not re-measured: counts as confirmed |")
                continue
            rb, rc = again
            re_ch = change(rb, rc)
            if re_ch > limit:
                failed.append(rid)
                outcome = "**confirmed** ⚠️"
            else:
                outcome = "cleared on re-run (noise)"
            re_lines.append(f"| `{rid}` | {ch:+.1f}% | {fmt(rb)} | {fmt(rc)} | {re_ch:+.1f}% | {outcome} |")
        report += ["### Re-run of the breaches", "", *re_lines, ""]
        if failed:
            verdict = f"**{len(failed)} bench(es) confirmed slower than {limit:.0f}%** (see the re-run table) — regression."
        elif breached:
            verdict = f"{len(breached)} bench(es) past {limit:.0f}% on the first pass, all cleared on the re-run."
        else:
            verdict = f"No bench slower than {limit:.0f}%."

    if args.breaches is not None and breached:
        verdict = f"{len(breached)} bench(es) past {limit:.0f}% on the first pass; re-measuring before deciding."
    text = "\n".join([*report, verdict])
    print(text)
    final = args.breaches is None or not breached
    summary = os.environ.get("GITHUB_STEP_SUMMARY")
    if summary and final:
        with open(summary, "a", encoding="utf-8") as fh:
            fh.write(f"## In-process benches: `{cand}` vs `{base}`\n\n{text}\n")
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
