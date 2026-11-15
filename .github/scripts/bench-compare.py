#!/usr/bin/env python3
"""Compare two Criterion baselines saved in one run and fail on a slowdown.

    bench-compare.py <criterion-dir> <baseline> <candidate> <max-slowdown-percent>
                     [--breaches <file>] [--confirm <re-baseline> <re-candidate>]
                     [--allow-empty] [--repeats <n>]

For every bench with both `<baseline>/estimates.json` and
`<candidate>/estimates.json`, compares medians; a bench on one side only is
listed but not gated. Exits 1 when any bench's candidate median is more than
`<max-slowdown-percent>` slower than its baseline. Prints a table either way.

--breaches <file>   write the Criterion id of each bench past the limit, one
                    per line (what bench-gate.sh re-measures).
--confirm A B       decide each breach on its re-measurement, baselines A vs
                    B. It is confirmed when the re-run change is past the
                    limit, or the mean of the first-pass and re-run changes
                    is; it clears only when both are within it. A breach with
                    no re-measurement counts as confirmed. Exits 1 only on a
                    confirmed breach.
--allow-empty       no bench measured on both sides is a warning and exit 0
                    (a PR base that has no in-process benches yet). Without
                    it, that is an error, exit 2: a side that measured nothing
                    must not pass the gate.

--repeats N         the review table (review-ab.sh). Each side ran N times,
                    saved as `<baseline>-1`…`<baseline>-N` and
                    `<candidate>-1`…: a side's value is its fastest median
                    (noise only adds time), and the "control spread" is how
                    far the same binary's runs differ, (max − min) / min,
                    the larger of the two sides. A change counts only when
                    it is past both the limit and twice that spread
                    (`flagged` / `faster`, else `noise`). With --breaches,
                    writes `<bench target>\t<id>` per flagged case (the
                    first directory under the root is the bench target).
                    With --confirm, a flagged case is decided on its re-run
                    by the gate's rule (`confirmed`), against its own
                    threshold: `slower` when it stands, `flagged, cleared
                    on re-run` when not. Exits 1 on a flagged case (without
                    --confirm) or a slower one (with it); 2 when nothing was
                    measured on both sides.

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


def confirmed(first: float, again: float, limit: float) -> bool:
    """The gate's rule: a breach stands when its re-run, or the mean of both passes, is past the limit."""
    return again > limit or (first + again) / 2 > limit


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


def repeated(bench_dir: pathlib.Path, side: str, n: int) -> list[float]:
    return [median(p) for i in range(1, n + 1) if (p := bench_dir / f"{side}-{i}" / "estimates.json").is_file()]


def spread(values: list[float]) -> float | None:
    return (max(values) - min(values)) / min(values) * 100 if len(values) > 1 and min(values) else None


def span(values: list[float]) -> str:
    low, high = min(values), max(values)
    return fmt(low) if low == high else f"{fmt(low)} … {fmt(high)}"


def review(
    root: pathlib.Path,
    base: str,
    cand: str,
    n: int,
    limit: float,
    breaches: pathlib.Path | None,
    confirm: list[str] | None,
) -> int:
    """The A/B table over repeated runs of each side, with flagged cases re-run alone."""
    bench_dirs = sorted(
        {p.parent.parent for i in range(1, n + 1) for side in (base, cand) for p in root.rglob(f"{side}-{i}/estimates.json")}
    )
    lines = [
        "| case | base (fastest … slowest) | head (fastest … slowest) | change | control spread | verdict |",
        "| --- | ---: | ---: | ---: | ---: | --- |",
    ]
    one_sided, flagged, slower, cleared = [], [], 0, 0
    for bench_dir in bench_dirs:
        b_runs, c_runs = repeated(bench_dir, base, n), repeated(bench_dir, cand, n)
        side = f"{base}-1" if b_runs else f"{cand}-1"
        rid = bench_id(root, bench_dir, side)
        if not b_runs or not c_runs:
            one_sided.append((rid, base if b_runs else cand))
            continue
        b, c = min(b_runs), min(c_runs)
        ch = change(b, c)
        spreads = [x for x in (spread(b_runs), spread(c_runs)) if x is not None]
        noise = max(spreads) if spreads else None
        floor = max(limit, 2 * noise) if noise is not None else limit
        if ch > floor:
            flagged.append(f"{bench_dir.relative_to(root).parts[0]}\t{rid}\n")
            if confirm is None:
                verdict, slower = "flagged ⚠️", slower + 1
            elif (again := pair(bench_dir, *confirm)) is None:
                verdict, slower = "slower ⚠️ (not re-measured)", slower + 1
            elif confirmed(ch, re_ch := change(*again), floor):
                verdict, slower = f"slower ⚠️ (re-run {re_ch:+.1f}%)", slower + 1
            else:
                verdict, cleared = f"flagged, cleared on re-run ({re_ch:+.1f}%)", cleared + 1
        elif ch < -floor:
            verdict = "faster"
        else:
            verdict = "noise"
        shown = f"{noise:.1f}%" if noise is not None else "—"
        lines.append(f"| `{rid}` | {span(b_runs)} | {span(c_runs)} | {ch:+.1f}% | {shown} | {verdict} |")
    if breaches is not None:
        breaches.write_text("".join(flagged))
    if len(lines) == 2:
        print(f"::error::nothing to compare: no bench has both `{base}-*` and `{cand}-*` under {root}")
        return 2
    print(f"{n} runs per side; change = fastest vs fastest; control spread = how far one binary's own runs differ.")
    print()
    print("\n".join(lines))
    if one_sided:
        print()
        print("Not compared (measured on one side only): " + ", ".join(f"`{i}` (only `{s}`)" for i, s in one_sided) + ".")
    print()
    rule = f"past {limit:.0f}% and twice the control spread"
    if confirm is None:
        print(f"**{slower} case(s) flagged** {rule}." if slower else f"No case flagged {rule}.")
    else:
        print(f"**{slower} case(s) slower** {rule}, confirmed by a re-run." if slower else f"No case slower {rule}.")
        if cleared:
            print(f"{cleared} case(s) flagged and cleared on re-run.")
    return 1 if slower else 0


def main() -> int:
    parser = argparse.ArgumentParser(usage=__doc__)
    parser.add_argument("root", type=pathlib.Path)
    parser.add_argument("base")
    parser.add_argument("cand")
    parser.add_argument("limit", type=float)
    parser.add_argument("--breaches", type=pathlib.Path)
    parser.add_argument("--confirm", nargs=2, metavar=("RE_BASE", "RE_CAND"))
    parser.add_argument("--allow-empty", action="store_true")
    parser.add_argument("--repeats", type=int)
    args = parser.parse_args()
    root, base, cand, limit = args.root, args.base, args.cand, args.limit
    if args.repeats is not None:
        return review(root, base, cand, args.repeats, limit, args.breaches, args.confirm)

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
        if args.allow_empty:
            print(f"::warning::nothing to compare: no bench has both `{base}` and `{cand}` under {root}")
            return 0
        print(f"::error::nothing to compare: no bench has both `{base}` and `{cand}` under {root}")
        return 2

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
            f"| bench | first pass | {re_base} | {re_cand} | re-run | mean | verdict |",
            "| --- | ---: | ---: | ---: | ---: | ---: | --- |",
        ]
        for rid, bench_dir, _, _, ch in breached:
            again = pair(bench_dir, re_base, re_cand)
            if again is None:
                failed.append(rid)
                re_lines.append(f"| `{rid}` | {ch:+.1f}% | — | — | — | — | not re-measured: counts as confirmed |")
                continue
            rb, rc = again
            re_ch = change(rb, rc)
            mean = (ch + re_ch) / 2
            if not confirmed(ch, re_ch, limit):
                outcome = "cleared (noise)"
            elif re_ch > limit:
                failed.append(rid)
                outcome = "**confirmed** by the re-run ⚠️"
            else:
                failed.append(rid)
                outcome = "**confirmed** by the mean ⚠️"
            re_lines.append(
                f"| `{rid}` | {ch:+.1f}% | {fmt(rb)} | {fmt(rc)} | {re_ch:+.1f}% | {mean:+.1f}% | {outcome} |"
            )
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
