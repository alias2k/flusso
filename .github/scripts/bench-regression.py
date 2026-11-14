#!/usr/bin/env python3
"""Call regressions from the benchmark history the way the glossary defines them.

    bench-regression.py <threshold-percent> [--rss-threshold <percent>]
                        [--window 20] [--spread 3] [--issue] <data.js>...

Reads github-action-benchmark `data.js` files (the `benchmarks` branch, one per
direction: `smaller/data.js` and `bigger/data.js`). A run is one commit across
every file, ordered by the time its points were stored. Values are compared as
logs, signed so that up is worse. Each series belongs to a suite: `inprocess`,
`components`, or the scenarios.

1. **Matching runs.** GitHub runs `main` on a few kinds of machine, and each
   kind moves the series its own way. Two runs match, for a series, when:
   - both recorded runner facts (`bench-runner-facts.py`): the same CPU model
     and core count, and calibrations (CPU, memory, disk; no flusso code)
     within 5% (median over the calibrations);
   - otherwise: the series of the *other* suites within 5% (median over
     them), so a step confined to one suite can't move its own match.
2. **Same-runner comparison.** With at least `MIN_MATCHES` matching runs
   among the last `LOOKBACK`, and `MIN_NOISE` earlier same-runner
   comparisons, a point is compared to the median of its matching runs. It
   is *real* past `--spread` swings: the median change of that comparison
   from one point to the next, over sqrt 2 (the median absolute deviation
   for steady noise; a step doesn't inflate it).
3. **All-runners comparison.** Otherwise, with at least two earlier points,
   a point is compared to the median of its series' last `LOOKBACK` points
   on any runner, and is *real* when it is past every one of them. Points
   already past everything (the run it belongs to) are left out of that pool.
4. **Regression.** The two newest judged points are each real and each worse
   than their comparison by more than the threshold (`--rss-threshold` for
   `peak_rss_mib`).
5. **Hold.** When at least one of those two points is a same-runner one, the
   series is held from the start of its run of real points: later points are
   compared to points from before it (matching runs of any age, or the last
   `LOOKBACK` on any runner). Across all runners alone there is no level to
   hold to (a new kind of machine also moves past everything before), so
   such a regression is reported only while it lasts. The hold ends when the
   `RELEASE` newest judged points are all same-runner and each within half
   the threshold, on an image-tag change, when the stored history no longer
   reaches back before the step, or when the series is missing from the
   newest `GONE_AFTER` runs.
6. **Image-tag change.** A series restarts on a new image: only points on the
   new image count, so the first two runs on it are its baseline.

Prints each finding with the basis it was judged against and, for a hold, the
last judged run before the step and the first one past it; how the newest run
was judged; and the series too noisy to catch a step of the threshold. With
`--issue`, opens (or updates) a single GitHub issue titled "Performance
regression on main" via `gh`, and closes it when no series is flagged. A
malformed entry is skipped with a warning; an unreadable file fails the run
(exit 1) before the issue is touched. Otherwise the exit code is 0.
"""
import argparse
import json
import math
import statistics
import subprocess
import sys
from typing import NamedTuple

ISSUE_TITLE = "Performance regression on main"
SAME_RUNNER = math.log(1.05)
LOOKBACK = 40
MIN_MATCHES = 3
MIN_NOISE = 5
MIN_SHARED = 5
GONE_AFTER = 3
RELEASE = 3
SAME, ALL = "same runner", "all runners"

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
    machine: tuple | None
    calibration: dict
    values: dict


def warn(message: str) -> None:
    print(f"warning: {message}", file=sys.stderr)


def load(path: str) -> dict:
    text = open(path, encoding="utf-8").read()
    data = json.loads(text[text.index("{"):])
    if not isinstance(data, dict):
        raise ValueError("not a benchmark data object")
    return data


def suite_of(name: str) -> str:
    head = name.split("/", 1)[0]
    return head if head in ("inprocess", "components") else "scenarios"


def extra_of(bench: dict) -> dict:
    extra = bench.get("extra")
    if isinstance(extra, str):
        try:
            extra = json.loads(extra)
        except ValueError:
            return {}
    return extra if isinstance(extra, dict) else {}


def images(extra: dict) -> str:
    found = extra.get("images")
    return json.dumps(found, sort_keys=True) if found else ""


def runner(extra: dict) -> tuple[tuple, dict] | None:
    facts = extra.get("runner")
    if not isinstance(facts, dict) or not facts.get("cpu"):
        return None
    calibration = {k: math.log(v) for k, v in (facts.get("calibration") or {}).items()
                   if isinstance(v, (int, float)) and not isinstance(v, bool) and v > 0}
    return (str(facts["cpu"]), facts.get("cores")), calibration


def collect(datas: list[dict]) -> tuple[list[Run], dict[Run, RunFacts], dict[Key, tuple[float, list[Point]]]]:
    """Every run in time order with its facts, and every series: its direction sign and points."""
    stamps: dict[Run, tuple[float, int]] = {}
    commits: dict[Run, dict] = {}
    machines: dict[Run, tuple[tuple, dict]] = {}
    raw: dict[Key, tuple[float, list[tuple]]] = {}
    for data in datas:
        entries = data.get("entries")
        for suite, points in (entries if isinstance(entries, dict) else {}).items():
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
                stamp = float(date) if isinstance(date, (int, float)) and not isinstance(date, bool) else math.inf
                if run not in stamps:
                    stamps[run] = (stamp, len(stamps))
                elif stamp < stamps[run][0]:
                    stamps[run] = (stamp, stamps[run][1])
                commits.setdefault(run, commit)
                for bench in point.get("benches") or []:
                    name = bench.get("name") if isinstance(bench, dict) else None
                    value = bench.get("value") if isinstance(bench, dict) else None
                    if not isinstance(name, str) or isinstance(value, bool) or not isinstance(value, (int, float)) or value <= 0:
                        warn(f"{suite} at {cid[:12]}: bench {name!r} has no usable value ({value!r}), skipped")
                        continue
                    extra = extra_of(bench)
                    facts = runner(extra)
                    if facts:
                        machines[run] = facts
                    _, history = raw.setdefault((suite, name), (sign, []))
                    history.append((run, sign * math.log(value), images(extra), float(value), commit))
    runs = sorted(stamps, key=lambda r: stamps[r])
    order = {run: i for i, run in enumerate(runs)}
    values: dict[Run, dict] = {run: {} for run in runs}
    series: dict[Key, tuple[float, list[Point]]] = {}
    for key, (sign, history) in raw.items():
        points = sorted((Point(*p) for p in history), key=lambda p: order[p.run])
        series[key] = (sign, points)
        for p in points:
            values[p.run][key[1]] = p.worse
    facts = {run: RunFacts(order[run], commits[run], *(machines.get(run) or (None, {})), values[run]) for run in runs}
    return runs, facts, series


class Matcher:
    """Whether two runs ran on the same kind of machine, as seen by one suite's series."""

    def __init__(self, facts: dict[Run, RunFacts]):
        self.facts = facts
        self.cache: dict[tuple, bool] = {}

    def same(self, a: Run, b: Run, suite: str) -> bool:
        key = (a, b, suite)
        if key not in self.cache:
            self.cache[key] = self._same(self.facts[a], self.facts[b], suite)
        return self.cache[key]

    @staticmethod
    def _same(a: RunFacts, b: RunFacts, suite: str) -> bool:
        if a.machine and b.machine:
            shared = [abs(a.calibration[k] - b.calibration[k]) for k in a.calibration if k in b.calibration]
            return a.machine == b.machine and bool(shared) and statistics.median(shared) < SAME_RUNNER
        shared = [abs(v - b.values[k]) for k, v in a.values.items() if k in b.values and suite_of(k) != suite]
        return len(shared) >= MIN_SHARED and statistics.median(shared) < SAME_RUNNER


def swing(noise: list[float]) -> float:
    """A series' noise: the median change of its comparison from one point to the next, over sqrt 2."""
    return statistics.median(abs(b - a) for a, b in zip(noise, noise[1:])) / math.sqrt(2)


class Judged(NamedTuple):
    point: Point
    excess: float
    basis: str
    real: bool


class State(NamedTuple):
    """Where a series stands after its newest point."""
    held: bool
    confirmed: bool
    after: Point | None
    by: Point | None
    judged: list[Judged]
    swing: float | None


def walk(history: list[Point], suite: str, facts: dict[Run, RunFacts], match: Matcher,
         limit: float, window: int, spread: float) -> State:
    """Replay one series through the rule, oldest point first."""
    large = math.log1p(limit / 100)
    usable: list[Point] = []
    judged: list[Judged] = []
    noise: list[tuple[int, float]] = []
    image: str | None = None
    streak: int | None = None
    held: int | None = None
    calm: Point | None = None
    after: Point | None = None
    by: Point | None = None
    outside: int | None = None
    for p in history:
        here = facts[p.run].order
        if p.images != image:
            if image is not None:
                usable, judged, held, streak, calm, outside = [], [], None, None, None, None
            image = p.images
        if held is not None and not any(facts[q.run].order < held for q in usable):
            held, streak = None, None
        end = held if held is not None else streak if streak is not None else here
        earlier = [q for q in usable if facts[q.run].order < end]
        if held is None:
            earlier = [q for q in earlier if facts[q.run].order >= here - LOOKBACK]
        same = [q.worse for q in earlier if match.same(q.run, p.run, suite)]
        same_excess = p.worse - statistics.median(same) if len(same) >= MIN_MATCHES else None
        recent = [e for _, e in noise][-window:]
        if same_excess is not None and len(recent) >= MIN_NOISE:
            entry = Judged(p, same_excess, SAME, same_excess > spread * swing(recent))
        elif len([q for q in earlier if outside is None or facts[q.run].order < outside]) >= 2:
            pool = [q.worse for q in earlier if outside is None or facts[q.run].order < outside][-LOOKBACK:]
            centre = statistics.median(pool)
            entry = Judged(p, p.worse - centre, ALL,
                           p.worse > max(pool))
        else:
            usable.append(p)
            continue
        if entry.basis == ALL and same_excess is not None and held is None:
            noise.append((here, same_excess))
        judged.append(entry)
        outside = (outside if outside is not None else here) if entry.basis == ALL and entry.real else None
        pair = judged[-2:]
        if held is None:
            if entry.basis == SAME:
                noise.append((here, entry.excess))
            if not entry.real:
                streak, calm = None, p if entry.basis == SAME else calm
            elif streak is None:
                streak = here
            if len(pair) == 2 and any(j.basis == SAME for j in pair) and all(j.real and j.excess > large for j in pair):
                held, after = streak if streak is not None else facts[pair[0].point.run].order, calm
                by = next(j.point for j in judged if facts[j.point.run].order >= held)
                noise = [(o, e) for o, e in noise if o < held]
        elif len(judged) >= RELEASE and all(j.basis == SAME and facts[j.point.run].order >= held and j.excess <= large / 2
                                            for j in judged[-RELEASE:]):
            held, streak = None, None
        usable.append(p)
    recent = [e for _, e in noise][-window:]
    pair = judged[-2:]
    confirmed = len(pair) == 2 and all(j.real and j.excess > large for j in pair)
    return State(held is not None, confirmed, after, by, judged, swing(recent) if len(recent) >= MIN_NOISE else None)


def short(point: Point | None) -> str:
    return str(point.commit.get("id", ""))[:12] if point else ""


def analyse(datas: list[dict], threshold: float, rss_threshold: float, window: int = 20, spread: float = 3.0) -> dict:
    runs, facts, series = collect(datas)
    match = Matcher(facts)
    newest = runs[-1] if runs else None
    findings, watch, gone = [], [], []
    bases = {SAME: 0, ALL: 0, None: 0}
    for (suite, name), (sign, history) in series.items():
        if not history:
            continue
        limit = rss_threshold if name.endswith("peak_rss_mib") else threshold
        state = walk(history, suite_of(name), facts, match, limit, window, spread)
        last_judged = state.judged[-1] if state.judged else None
        if history[-1].run == newest:
            bases[last_judged.basis if last_judged and last_judged.point.run == newest else None] += 1
        elif newest is not None and facts[history[-1].run].order < facts[newest].order - GONE_AFTER + 1:
            if state.held:
                gone.append({"suite": suite, "series": name})
            continue
        if state.swing is not None and spread * state.swing > math.log1p(limit / 100):
            watch.append({"suite": suite, "series": name, "smallest_step_percent": round((math.exp(spread * state.swing) - 1) * 100)})
        if (state.held or state.confirmed) and last_judged:
            pair = state.judged[-2:]
            point = last_judged.point
            findings.append({
                "suite": suite,
                "series": name,
                "worse_by_percent": round((math.exp(statistics.fmean(j.excess for j in pair)) - 1) * 100, 1),
                "baseline": math.exp(sign * (point.worse - last_judged.excess)),
                "value": point.value,
                "basis": last_judged.basis,
                "commit": short(point),
                "url": point.commit.get("url", ""),
                "held": state.held,
                "after": short(state.after) if state.held else "",
                "by": short(state.by) if state.held else short(state.judged[-2].point),
            })
    return {"findings": findings, "watch": watch, "gone": gone, "bases": bases,
            "newest": short(Point(newest, 0, "", 0, facts[newest].commit)) if newest else ""}


def step_text(f: dict) -> str:
    return f"after {f['after']}, by {f['by']}" if f["after"] else f"at or before {f['by']}"


def report(result: dict) -> list[str]:
    b = result["bases"]
    lines = [f"newest run {result['newest']}: {b[SAME]} series judged against the same runner, "
             f"{b[ALL]} against all runners, {b[None]} not yet judgeable (too few points)"]
    for g in result["gone"]:
        lines.append(f"no longer recorded, hold ended: {g['suite']} {g['series']}")
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
            subprocess.run(["gh", "issue", "close", str(issue["number"]), "--comment",
                            "No series is flagged any more: each held series had three judged runs in a row back within half its "
                            "threshold of the level before its step, an image-tag change, or is no longer recorded."], check=True)
        return
    lines = [
        "Two judged runs in a row, each worse than its comparison by more than the threshold and by more than the "
        "series' own noise. *Held* series are compared to their level before the step and stay here until three judged "
        "runs in a row on the same runner are each back within half the threshold; the others were judged only "
        "across all runners and are listed while they last.",
        "",
        "| suite | series | worse by | compared to | now | judged against | held | last judged | step |",
        "| --- | --- | ---: | ---: | ---: | --- | --- | --- | --- |",
    ]
    for f in findings:
        commit = f"[`{f['commit']}`]({f['url']})" if f["url"] else f"`{f['commit']}`"
        lines.append(f"| {f['suite']} | `{f['series']}` | {f['worse_by_percent']:+.1f}% | {f['baseline']:.3g} | "
                     f"{f['value']:.3g} | {f['basis']} | {'yes' if f['held'] else 'no'} | {commit} | {step_text(f)} |")
    lines += ["", *report(result)]
    if result["watch"]:
        lines += ["", "Too noisy to catch a step of the threshold (smallest step each can catch): "
                  + ", ".join(f"`{w['series']}` +{w['smallest_step_percent']}%" for w in result["watch"]) + "."]
    lines += ["", "History: https://alias2k.github.io/flusso/bench/", "",
              "This issue is maintained by `.github/scripts/bench-regression.py`; it closes itself once the series recover."]
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
    parser.add_argument("--window", type=int, default=20, help="same-runner comparisons a series' swing is measured over")
    parser.add_argument("--spread", type=float, default=3.0, help="how many swings a point must be past to count as real")
    parser.add_argument("--issue", action="store_true")
    args = parser.parse_args()
    rss = args.rss_threshold if args.rss_threshold is not None else args.threshold
    datas = []
    for path in args.data:
        try:
            datas.append(load(path))
        except (OSError, ValueError) as error:
            print(f"error: {path}: cannot read ({error}); nothing judged, the issue is left as it is", file=sys.stderr)
            return 1
    result = analyse(datas, args.threshold, rss, args.window, args.spread)
    findings = result["findings"]
    if findings:
        print(f"{len(findings)} regression(s):")
        for f in findings:
            print(f"  {f['suite']} {f['series']}: {f['worse_by_percent']:+.1f}% vs {f['baseline']:.3g} "
                  f"(now {f['value']:.3g}) against {f['basis']}, {'held' if f['held'] else 'not held'}, "
                  f"last judged at {f['commit']}, step {step_text(f)}")
    else:
        print("no regression: no series is flagged")
    for line in report(result):
        print(line)
    for w in result["watch"]:
        print(f"too noisy for the threshold: {w['suite']} {w['series']} (smallest step it can catch: +{w['smallest_step_percent']}%)")
    if args.issue:
        upsert_issue(result)
    return 0


if __name__ == "__main__":
    sys.exit(main())
