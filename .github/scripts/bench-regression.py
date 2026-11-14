#!/usr/bin/env python3
"""Call regressions from the benchmark history the way the glossary defines them.

    bench-regression.py <threshold-percent> [--rss-threshold <percent>]
                        [--window 20] [--spread 3] [--issue] <data.js>...

Reads github-action-benchmark `data.js` files (the `benchmarks` branch, one per
direction: `smaller/data.js` and `bigger/data.js`). A run is one commit across
every file. Values are compared as logs, signed so that up is worse
(`customSmallerIsBetter` as is, `customBiggerIsBetter` negated).

1. **Matching runs.** GitHub runs `main` on a few kinds of machine, and each
   kind moves the series its own way (one makes Postgres 20% faster and
   OpenSearch 20% slower). An earlier run *matches* a run when, across every
   series both share, their values differ by less than 5% (the median of the
   absolute differences). Only the last `LOOKBACK` runs are searched.
2. **Against its matches.** Each point is compared to the median of its series
   over its matching runs (the same images only). A point with fewer than
   `MIN_MATCHES` matching runs is not judged: nothing comparable ran before it.
3. **Held baseline.** While a series stays worse than its matches by more than
   `--spread` times its swing (below), the runs since it first did are left out
   of its matches, so a lasting step is measured against the level before it
   instead of being absorbed. The new level is accepted once fewer than
   `MIN_MATCHES` matching runs from before the step are left in the lookback.
4. **Regression.** Judged on the series' two newest judged points, against the
   `--window` judged points before them, on two counts:
   - **real**: each point is worse than its matches by more than `--spread`
     times the series' swing (the median change of that comparison from one
     judged point to the next, divided by sqrt 2: the median absolute deviation
     for steady noise, which a step inside the window does not inflate);
   - **large**: on average the two are worse than their matches by more than
     the threshold (`--rss-threshold` for `peak_rss_mib` series).

A series is judged once it has `MIN_PRIOR` judged points before the newest
two. An image-tag change (in a point's `extra`) restarts its comparisons. A
change that moves most series at once also moves the run away from its
matches, so it goes unjudged; the PR gate's A/B on one runner is what catches
that.

Prints the verdict. With `--issue`, opens (or updates) a single GitHub issue
titled "Performance regression on main" via `gh`, and closes it when nothing
regresses. Exit code is always 0 — a regression is a signal, not a broken job.
"""
import argparse
import json
import math
import statistics
import subprocess
import sys
from typing import NamedTuple

ISSUE_TITLE = "Performance regression on main"
RECENT = 2
SAME_RUNNER = math.log(1.05)
LOOKBACK = 40
MIN_MATCHES = 3
MIN_PRIOR = 5

Run = tuple[str, int]
Key = tuple[str, str]


class Point(NamedTuple):
    run: Run
    worse: float
    images: str
    value: float
    commit: dict


def load(path: str) -> dict:
    text = open(path, encoding="utf-8").read()
    start = text.index("{")
    return json.loads(text[start:])


def images(bench: dict) -> str:
    extra = bench.get("extra") or ""
    try:
        return json.dumps(json.loads(extra).get("images"), sort_keys=True)
    except (ValueError, AttributeError):
        return ""


def collect(datas: list[dict]) -> tuple[list[Run], dict[Key, tuple[float, list[Point]]]]:
    """Every run in time order, and every series keyed by (suite, name): its direction sign and its points."""
    runs: list[Run] = []
    series: dict[Key, tuple[float, list[Point]]] = {}
    for data in datas:
        for suite, points in data.get("entries", {}).items():
            sign = 1.0 if points and points[-1].get("tool") == "customSmallerIsBetter" else -1.0
            seen: dict[str, int] = {}
            for point in points:
                commit = point.get("commit", {})
                cid = commit.get("id", "")
                run = (cid, seen.get(cid, 0))
                seen[cid] = run[1] + 1
                if run not in runs:
                    runs.append(run)
                for bench in point.get("benches", []):
                    if bench["value"] <= 0:
                        continue
                    _, history = series.setdefault((suite, bench["name"]), (sign, []))
                    history.append(Point(run, sign * math.log(bench["value"]), images(bench), bench["value"], commit))
    return runs, series


def matching_runs(runs: list[Run], table: dict[Run, dict[Key, Point]]) -> dict[Run, list[Run]]:
    """For every run, the earlier runs (within `LOOKBACK`) that ran on the same kind of machine."""

    def distance(a: Run, b: Run) -> float:
        gaps = [abs(p.worse - table[b][key].worse) for key, p in table[a].items()
                if key in table[b] and table[b][key].images == p.images]
        return statistics.median(gaps) if gaps else math.inf

    return {run: [earlier for earlier in runs[max(0, i - LOOKBACK):i] if distance(run, earlier) < SAME_RUNNER]
            for i, run in enumerate(runs)}


def swing(compared: list[float]) -> float:
    """A series' noise: the median change of its comparison from one judged point to the next, over sqrt 2."""
    return statistics.median(abs(b - a) for a, b in zip(compared, compared[1:])) / math.sqrt(2)


def against_matches(series: dict[Key, tuple[float, list[Point]]], runs: list[Run], window: int, spread: float) -> dict[Key, dict[Run, float]]:
    """Per series, each judged point's distance (log, up is worse) from the median of its matching runs.

    While a series stays worse than its matches by more than its swing, the runs since it
    first did are left out of its matches: a lasting step is measured against the level
    before it, not absorbed. Once fewer than `MIN_MATCHES` runs remain from before it, the
    new level is accepted.
    """
    table: dict[Run, dict[Key, Point]] = {run: {} for run in runs}
    for key, (_, history) in series.items():
        for p in history:
            table[p.run][key] = p
    matches = matching_runs(runs, table)
    order = {run: i for i, run in enumerate(runs)}
    out: dict[Key, dict[Run, float]] = {}
    for key, (_, history) in series.items():
        compared: dict[Run, float] = {}
        since: int | None = None
        for p in history:
            same = [m for m in matches[p.run] if key in table[m] and table[m][key].images == p.images]
            before = [m for m in same if since is None or order[m] < since]
            if len(before) < MIN_MATCHES:
                since, before = None, same
            if len(before) < MIN_MATCHES:
                continue
            distance = p.worse - statistics.median(table[m][key].worse for m in before)
            prior = list(compared.values())[-window:]
            beyond = len(prior) >= MIN_PRIOR and distance > spread * swing(prior)
            if not beyond:
                since = None
            elif since is None:
                since = order[p.run]
            compared[p.run] = distance
        out[key] = compared
    return out


class Verdict(NamedTuple):
    regressed: bool
    worse_by_percent: float
    point: Point


def judge(history: list[Point], compared: dict[Run, float], limit: float, window: int, spread: float) -> Verdict | None:
    """The verdict on a series' two newest judged points, or None while it has too few to judge."""
    judged = [p for p in history if p.run in compared]
    recent = judged[-RECENT:]
    prior = [compared[p.run] for p in judged[-RECENT - window:-RECENT]]
    if len(recent) < RECENT or len(prior) < MIN_PRIOR:
        return None
    excess = [compared[p.run] for p in recent]
    mean = statistics.fmean(excess)
    regressed = min(excess) > spread * swing(prior) and mean > math.log1p(limit / 100)
    return Verdict(regressed, (math.exp(mean) - 1) * 100, recent[-1])


def analyse(datas: list[dict], threshold: float, rss_threshold: float, window: int = 20, spread: float = 3.0) -> list[dict]:
    runs, series = collect(datas)
    compared = against_matches(series, runs, window, spread)
    findings = []
    for (suite, name), (sign, history) in series.items():
        limit = rss_threshold if name.endswith("peak_rss_mib") else threshold
        verdict = judge(history, compared[(suite, name)], limit, window, spread)
        if verdict and verdict.regressed:
            point = verdict.point
            findings.append({
                "suite": suite,
                "series": name,
                "worse_by_percent": round(verdict.worse_by_percent, 1),
                "baseline": math.exp(sign * (point.worse - compared[(suite, name)][point.run])),
                "value": point.value,
                "commit": point.commit.get("id", "")[:12],
                "url": point.commit.get("url", ""),
            })
    return findings


def upsert_issue(findings: list[dict]) -> None:
    listed = subprocess.run(
        ["gh", "issue", "list", "--state", "open", "--search", f'in:title "{ISSUE_TITLE}"', "--json", "number,title"],
        check=True, capture_output=True, text=True,
    )
    existing = [i for i in json.loads(listed.stdout or "[]") if i["title"] == ISSUE_TITLE]
    if not findings:
        for issue in existing:
            subprocess.run(["gh", "issue", "close", str(issue["number"]), "--comment", "No series is flagged any more: each is back within its threshold, or a step has outlived the lookback and its new level is now the baseline."], check=True)
        return
    lines = [
        "The series' two newest runs on `main` are worse than the same series on earlier runs of the same kind of machine: each by more than the series' own swing, both by more than the threshold on average.",
        "",
        "| suite | series | worse by | on matching runs | now | commit |",
        "| --- | --- | ---: | ---: | ---: | --- |",
    ]
    for f in findings:
        commit = f"[`{f['commit']}`]({f['url']})" if f["url"] else f["commit"]
        lines.append(f"| {f['suite']} | `{f['series']}` | {f['worse_by_percent']:+.1f}% | {f['baseline']:.3g} | {f['value']:.3g} | {commit} |")
    lines += ["", "History: https://alias2k.github.io/flusso/bench/", "", "This issue is maintained by `.github/scripts/bench-regression.py`; it closes itself once the series recover."]
    body = "\n".join(lines)
    if existing:
        subprocess.run(["gh", "issue", "edit", str(existing[0]["number"]), "--body", body], check=True)
    else:
        subprocess.run(["gh", "issue", "create", "--title", ISSUE_TITLE, "--label", "bug", "--body", body], check=True)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("threshold", type=float)
    parser.add_argument("data", nargs="+")
    parser.add_argument("--rss-threshold", type=float, default=None)
    parser.add_argument("--window", type=int, default=20, help="judged points a series' swing is measured over")
    parser.add_argument("--spread", type=float, default=3.0, help="how many swings a point must be past to count as real")
    parser.add_argument("--issue", action="store_true")
    args = parser.parse_args()
    rss = args.rss_threshold if args.rss_threshold is not None else args.threshold
    findings = analyse([load(path) for path in args.data], args.threshold, rss, args.window, args.spread)
    if findings:
        print(f"{len(findings)} regression(s):")
        for f in findings:
            print(f"  {f['suite']} {f['series']}: {f['worse_by_percent']:+.1f}% vs {f['baseline']:.3g} on matching runs (now {f['value']:.3g}) at {f['commit']}")
    else:
        print("no regression: no series is past its threshold and its own swing on two consecutive runs")
    if args.issue:
        upsert_issue(findings)
    return 0


if __name__ == "__main__":
    sys.exit(main())
