#!/usr/bin/env python3
"""Print a markdown A/B table of scenario runs (scenario-ab.yml): per metric,
every run's value for `main` and the branch, each side's median and spread
(min–max), and the ratio of medians. Stdlib only; reads `<runs>/<side>-<scenario>-r<n>[-suffix]/`
as `flusso-bench --out` wrote them. Prints nothing it can't read, never fails
on a number."""

import json
import os
import re
import statistics
import sys
from collections import defaultdict
from pathlib import Path

METRICS = [
    ("visible_latency_p50_ms", "visible latency p50 (ms)", "lower"),
    ("visible_latency_p99_ms", "visible latency p99 (ms)", "lower"),
    ("drain_changes_per_s", "burst changes/s", "higher"),
    ("burst_writes", "burst writes (bulk requests)", "lower"),
    ("burst_docs_per_write", "docs per write", "higher"),
    ("burst_refreshes", "burst refreshes (branch metric)", "lower"),
    ("backfill_docs_per_s", "backfill docs/s", "higher"),
    ("peak_rss_mib", "peak RSS (MiB)", "lower"),
    ("cpu_seconds", "CPU seconds", "lower"),
    ("flush_p50_ms", "flush p50 (ms)", "lower"),
    ("flush_p99_ms", "flush p99 (ms)", "lower"),
]

LABEL = re.compile(r"^(main|branch)-(\w+)-r(\d+)(.*)$")


def read_run(path: Path) -> dict:
    values = {}
    summaries = list(path.glob("*/summary.json"))
    if not summaries:
        return values
    summary_dir = summaries[0].parent
    for name in ("smaller.json", "bigger.json"):
        file = summary_dir / name
        if file.exists():
            for point in json.loads(file.read_text()):
                values[point["name"].rsplit("/", 1)[-1]] = point["value"]
    summary = json.loads(summaries[0].read_text())
    for phase in summary.get("phases", {}).get("attribution", []):
        if phase.get("phase") != "burst":
            continue
        writes = sum(f["flushes"] for f in phase.get("flushes", []))
        docs = sum(f["flushes"] * f["documents_per_flush"] for f in phase.get("flushes", []))
        if writes:
            values["burst_writes"] = writes
            values["burst_docs_per_write"] = docs / writes
        refreshes = phase.get("refreshes")
        if refreshes:
            values["burst_refreshes"] = sum(refreshes.values())
    return values


def fmt(value):
    if value is None:
        return "—"
    return f"{value:,.1f}" if abs(value) < 1000 else f"{value:,.0f}"


def main() -> None:
    runs_dir = Path(sys.argv[1])
    groups = defaultdict(lambda: defaultdict(list))
    for path in sorted(runs_dir.iterdir(), key=lambda p: p.name):
        match = LABEL.match(path.name)
        if not match:
            continue
        side, scenario, _, suffix = match.groups()
        groups[scenario + suffix][side].append((path.name, read_run(path)))
    print("# Scenario A/B\n")
    print(f"main `{os.environ.get('MAIN_SHA', '?')}` against branch `{os.environ.get('BRANCH_SHA', '?')}`\n")
    for group, sides in groups.items():
        print(f"## {group}\n")
        print("| metric | main runs | branch runs | main median (min–max) | branch median (min–max) | branch/main |")
        print("| --- | --- | --- | --- | --- | ---: |")
        for key, title, _ in METRICS:
            cells = []
            medians = []
            for side in ("main", "branch"):
                vals = [run[1].get(key) for run in sides.get(side, [])]
                present = [v for v in vals if v is not None]
                cells.append(" / ".join(fmt(v) for v in vals))
                if present:
                    median = statistics.median(present)
                    medians.append((median, f"{fmt(median)} ({fmt(min(present))}–{fmt(max(present))})"))
                else:
                    medians.append((None, "—"))
            if all(not c.replace("—", "").replace(" / ", "") for c in cells):
                continue
            ratio = (
                f"{medians[1][0] / medians[0][0]:.2f}"
                if medians[0][0] and medians[1][0] is not None
                else "—"
            )
            print(f"| {title} | {cells[0]} | {cells[1]} | {medians[0][1]} | {medians[1][1]} | {ratio} |")
        print()


if __name__ == "__main__":
    main()
