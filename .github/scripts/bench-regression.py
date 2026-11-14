#!/usr/bin/env python3
"""Call regressions from the benchmark history the way the glossary defines them.

    bench-regression.py <threshold-percent> [--rss-threshold <percent>]
                        [--window 20] [--spread 3] [--issue] <data.js>...

Reads github-action-benchmark `data.js` files (the `benchmarks` branch, one per
direction: `smaller/data.js` and `bigger/data.js`). A run is one commit across
every file, ordered by the time its points were stored. Values are compared as
logs, signed so that up is worse (`customSmallerIsBetter` as is,
`customBiggerIsBetter` negated).

1. **Matching runs.** GitHub runs `main` on a few kinds of machine, and each
   kind moves the series its own way. Two runs *match* when they ran on the
   same kind, told by evidence no flusso change can move: the same CPU model
   where both recorded one, and the `FLOORS` series (two `SELECT 1`
   round-trips and a SHA-256 calibration) within 5% on average.
2. **Comparison.** A point is compared to the median of its series over its
   matching runs among the last `LOOKBACK` runs. With fewer than
   `MIN_MATCHES` of them it is not judged.
3. **Regression.** Two judged points in a row are each worse than their
   matches by more than `--spread` times the series' swing, and on average by
   more than the threshold (`--rss-threshold` for `peak_rss_mib`). The swing is
   the median change of the comparison from one judged point to the next, over
   sqrt 2: the median absolute deviation for steady noise, which a step does
   not inflate. It needs `MIN_NOISE` judged points.
4. **Hold.** From the first of those points (or the start of the run of
   points past the swing that led to them), the series is held: every later
   point is compared to its matching runs from *before* that point, of any
   age, and points with fewer than `MIN_MATCHES` of those are skipped. The
   swing stops taking in points. The hold ends when two judged points in a
   row are each within half the threshold, on an image-tag change, or when
   the stored history no longer reaches back before it.
5. **Image-tag changes.** The first judged point on a new image measures the
   image's own step against the old image's matches and is not judged; the
   series is judged again from the next one, with that step taken out.

Series named in `FLOORS` are the runner's evidence and are not judged. A
series whose `--spread` swings exceed its threshold cannot catch a step of
the threshold and is printed as too noisy, with the smallest step it can
catch.

Prints the verdict, how many series the newest run could be judged on, and the
too-noisy series. With `--issue`, opens (or updates) a single GitHub issue
titled "Performance regression on main" via `gh`, and closes it when no series
is held. Malformed entries are skipped with a warning on stderr. Exit code is
always 0 — a regression is a signal, not a broken job.
"""
import argparse
import json
import math
import statistics
import subprocess
import sys
from typing import NamedTuple

ISSUE_TITLE = "Performance regression on main"
# The runner's evidence: two plain `SELECT 1` round-trips and a SHA-256 calibration
# (`bench-runner-facts.py`, recorded since #317). No flusso change can move them.
FLOORS = ("components/baseline/select_1", "components/baseline/pg_select_1", "runner/sha256_64mib")
SAME_RUNNER = math.log(1.05)
LOOKBACK = 40
MIN_MATCHES = 3
MIN_NOISE = 5

Run = tuple[str, int]
Key = tuple[str, str]


class Point(NamedTuple):
    run: Run
    worse: float
    images: str
    value: float
    commit: dict


class RunFacts(NamedTuple):
    order: int
    commit: dict
    cpu: str
    floors: dict


def warn(message: str) -> None:
    print(f"warning: {message}", file=sys.stderr)


def load(path: str) -> dict:
    text = open(path, encoding="utf-8").read()
    start = text.index("{")
    return json.loads(text[start:])


def extra_of(bench: dict) -> dict:
    try:
        extra = json.loads(bench.get("extra") or "{}")
    except (TypeError, ValueError):
        return {}
    return extra if isinstance(extra, dict) else {}


def images(extra: dict) -> str:
    found = extra.get("images")
    return json.dumps(found, sort_keys=True) if found else ""


def cpu(extra: dict) -> str:
    runner = extra.get("runner")
    return str(runner.get("cpu", "")) if isinstance(runner, dict) else ""


def collect(datas: list[dict]) -> tuple[list[Run], dict[Run, RunFacts], dict[Key, tuple[float, list[Point]]]]:
    """Every run in time order with its runner facts, and every series: its direction sign and points."""
    stamps: dict[Run, tuple[float, int]] = {}
    commits: dict[Run, dict] = {}
    cpus: dict[Run, str] = {}
    raw: dict[Key, tuple[float, list[tuple[Run, float, str, float, dict]]]] = {}
    seen_order = 0
    for data in datas:
        entries = data.get("entries") if isinstance(data, dict) else None
        for suite, points in (entries or {}).items():
            if not isinstance(points, list):
                warn(f"{suite}: entries are not a list, skipped")
                continue
            tools = [p.get("tool") for p in points if isinstance(p, dict)]
            sign = 1.0 if tools and tools[-1] == "customSmallerIsBetter" else -1.0
            occurrences: dict[str, int] = {}
            for point in points:
                if not isinstance(point, dict):
                    warn(f"{suite}: a point is not an object, skipped")
                    continue
                commit = point.get("commit") if isinstance(point.get("commit"), dict) else {}
                cid = str(commit.get("id", ""))
                run = (cid, occurrences.get(cid, 0))
                occurrences[cid] = run[1] + 1
                date = point.get("date")
                stamp = float(date) if isinstance(date, (int, float)) else math.inf
                if run not in stamps or stamp < stamps[run][0]:
                    stamps[run] = (stamp, stamps.get(run, (0, seen_order))[1])
                    seen_order += 1
                commits.setdefault(run, commit)
                for bench in point.get("benches") or []:
                    name = bench.get("name") if isinstance(bench, dict) else None
                    value = bench.get("value") if isinstance(bench, dict) else None
                    if not isinstance(name, str) or isinstance(value, bool) or not isinstance(value, (int, float)) or value <= 0:
                        warn(f"{suite} at {cid[:12]}: bench {name!r} has no usable value ({value!r}), skipped")
                        continue
                    extra = extra_of(bench)
                    if cpu(extra):
                        cpus[run] = cpu(extra)
                    _, history = raw.setdefault((suite, name), (sign, []))
                    history.append((run, sign * math.log(value), images(extra), float(value), commit))
    runs = sorted(stamps, key=lambda r: stamps[r])
    order = {run: i for i, run in enumerate(runs)}
    series: dict[Key, tuple[float, list[Point]]] = {}
    floors: dict[Run, dict] = {run: {} for run in runs}
    for key, (sign, history) in raw.items():
        points = sorted((Point(*p) for p in history), key=lambda p: order[p.run])
        series[key] = (sign, points)
        if key[1] in FLOORS:
            for p in points:
                floors[p.run][key[1]] = p.worse
    facts = {run: RunFacts(order[run], commits[run], cpus.get(run, ""), floors[run]) for run in runs}
    return runs, facts, series


def same_runner(a: RunFacts, b: RunFacts) -> bool:
    """Two runs ran on the same kind of machine: same CPU model where both recorded one, `SELECT 1` within 5%."""
    if a.cpu and b.cpu and a.cpu != b.cpu:
        return False
    shared = [abs(a.floors[k] - b.floors[k]) for k in FLOORS if k in a.floors and k in b.floors]
    return bool(shared) and statistics.fmean(shared) < SAME_RUNNER


def swing(noise: list[float]) -> float:
    """A series' noise: the median change of its comparison from one judged point to the next, over sqrt 2."""
    return statistics.median(abs(b - a) for a, b in zip(noise, noise[1:])) / math.sqrt(2)


class Judged(NamedTuple):
    point: Point
    excess: float


class State(NamedTuple):
    """Where a series stands after its newest point."""
    held_since: Point | None
    judged: list[Judged]
    swing: float | None
    newest_judged: bool


def walk(history: list[Point], facts: dict[Run, RunFacts], limit: float, window: int, spread: float) -> State:
    """Replay one series through the rule, oldest point first."""
    offsets: dict[str, float] = {}
    usable: list[tuple[Point, float]] = []  # each point with its image's step taken out
    judged: list[Judged] = []
    noise: list[tuple[int, float]] = []
    streak: int | None = None
    held: int | None = None
    held_point: Point | None = None
    current_image: str | None = None
    newest_judged = False
    large = math.log1p(limit / 100)
    for p in history:
        here = facts[p.run]
        newest_judged = False
        if current_image is not None and p.images != current_image and p.images not in offsets:
            held, held_point, streak = None, None, None
        current_image = p.images
        if held is not None and not any(facts[q.run].order < held for q, _ in usable):
            held, held_point, streak = None, None, None
        if held is not None:
            pool = [v for q, v in usable if facts[q.run].order < held and same_runner(facts[q.run], here)]
        else:
            start = streak if streak is not None else here.order
            pool = [v for q, v in usable
                    if here.order - LOOKBACK <= facts[q.run].order < start and same_runner(facts[q.run], here)]
        if len(pool) < MIN_MATCHES:
            if p.images in offsets or not usable:
                offsets.setdefault(p.images, 0.0)
                usable.append((p, p.worse - offsets[p.images]))
            continue
        baseline = statistics.median(pool)
        if p.images not in offsets and usable:
            offsets[p.images] = p.worse - baseline
            usable += [(q, q.worse - offsets[q.images]) for q in history
                       if q.images == p.images and facts[q.run].order <= here.order]
            continue
        offsets.setdefault(p.images, 0.0)
        value = p.worse - offsets[p.images]
        excess = value - baseline
        recent_noise = [e for _, e in noise][-window:]
        bar = spread * swing(recent_noise) if len(recent_noise) >= MIN_NOISE else None
        beyond = bar is not None and excess > bar
        judged.append(Judged(p, excess))
        newest_judged = True
        if held is None:
            noise.append((here.order, excess))
            if not beyond:
                streak = None
            elif streak is None:
                streak = here.order
            pair = judged[-2:]
            if beyond and len(pair) == 2 and all(j.excess > bar for j in pair) and statistics.fmean(j.excess for j in pair) > large:
                held = streak if streak is not None else facts[pair[0].point.run].order
                held_point = next(j.point for j in judged if facts[j.point.run].order >= held)
                noise = [(o, e) for o, e in noise if o < held]
        else:
            pair = judged[-2:]
            if len(pair) == 2 and all(j.excess <= large / 2 and facts[j.point.run].order >= held for j in pair):
                held, held_point, streak = None, None, None
        usable.append((p, value))
    recent_noise = [e for _, e in noise][-window:]
    return State(held_point, judged, swing(recent_noise) if len(recent_noise) >= MIN_NOISE else None, newest_judged)


def analyse(datas: list[dict], threshold: float, rss_threshold: float, window: int = 20, spread: float = 3.0) -> dict:
    runs, facts, series = collect(datas)
    newest = runs[-1] if runs else None
    findings, watch = [], []
    judged_newest = total = 0
    for (suite, name), (sign, history) in series.items():
        if name in FLOORS or not history:
            continue
        limit = rss_threshold if name.endswith("peak_rss_mib") else threshold
        state = walk(history, facts, limit, window, spread)
        if history[-1].run == newest:
            total += 1
            judged_newest += state.newest_judged
        if state.swing is not None and spread * state.swing > math.log1p(limit / 100):
            watch.append({"suite": suite, "series": name, "smallest_step_percent": round((math.exp(spread * state.swing) - 1) * 100)})
        if state.held_since is not None and state.judged:
            last = state.judged[-2:]
            mean = statistics.fmean(j.excess for j in last)
            point = last[-1].point
            findings.append({
                "suite": suite,
                "series": name,
                "worse_by_percent": round((math.exp(mean) - 1) * 100, 1),
                "baseline": math.exp(sign * (point.worse - last[-1].excess)),
                "value": point.value,
                "commit": str(point.commit.get("id", ""))[:12],
                "url": point.commit.get("url", ""),
                "since": str(state.held_since.commit.get("id", ""))[:12],
            })
    newest_commit = str(facts[newest].commit.get("id", ""))[:12] if newest else ""
    return {"findings": findings, "watch": watch, "newest": newest_commit, "judged_newest": judged_newest, "series_newest": total}


def report(result: dict) -> list[str]:
    lines = [f"newest run {result['newest']}: judged {result['judged_newest']} of {result['series_newest']} series"]
    if result["judged_newest"] < result["series_newest"]:
        lines.append("  a series not judged on it had too few earlier runs on the same kind of machine; its verdict is from the last run it was judged at")
    return lines


def upsert_issue(result: dict) -> None:
    findings = result["findings"]
    listed = subprocess.run(
        ["gh", "issue", "list", "--state", "open", "--search", f'in:title "{ISSUE_TITLE}"', "--json", "number,title"],
        check=True, capture_output=True, text=True,
    )
    existing = [i for i in json.loads(listed.stdout or "[]") if i["title"] == ISSUE_TITLE]
    if not findings:
        for issue in existing:
            subprocess.run(["gh", "issue", "close", str(issue["number"]), "--comment", "No series is flagged any more: each is back within half its threshold of the level before its step."], check=True)
        return
    lines = [
        "These series are slower than on earlier runs of the same kind of machine before their step: two judged runs in a row, each past the series' own swing, on average past the threshold. A series stays here until two judged runs in a row are back within its swing and within half the threshold.",
        "",
        "| suite | series | worse by | before the step | now | last judged | step since |",
        "| --- | --- | ---: | ---: | ---: | --- | --- |",
    ]
    for f in findings:
        commit = f"[`{f['commit']}`]({f['url']})" if f["url"] else f"`{f['commit']}`"
        lines.append(f"| {f['suite']} | `{f['series']}` | {f['worse_by_percent']:+.1f}% | {f['baseline']:.3g} | {f['value']:.3g} | {commit} | `{f['since']}` |")
    lines += ["", *report(result)]
    if result["watch"]:
        lines += ["", "Too noisy to catch a step of the threshold (smallest step each can catch): " + ", ".join(f"`{w['series']}` +{w['smallest_step_percent']}%" for w in result["watch"]) + "."]
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
    datas = []
    for path in args.data:
        try:
            datas.append(load(path))
        except (OSError, ValueError) as error:
            warn(f"{path}: cannot read ({error}), skipped")
    result = analyse(datas, args.threshold, rss, args.window, args.spread)
    findings = result["findings"]
    if findings:
        print(f"{len(findings)} regression(s):")
        for f in findings:
            print(f"  {f['suite']} {f['series']}: {f['worse_by_percent']:+.1f}% vs {f['baseline']:.3g} before the step (now {f['value']:.3g}) at {f['commit']}, step since {f['since']}")
    else:
        print("no regression: no series is past its threshold and its own swing on two judged runs in a row")
    for line in report(result):
        print(line)
    for w in result["watch"]:
        print(f"too noisy for the threshold: {w['suite']} {w['series']} (smallest step it can catch: +{w['smallest_step_percent']}%)")
    if args.issue:
        upsert_issue(result)
    return 0


if __name__ == "__main__":
    sys.exit(main())
