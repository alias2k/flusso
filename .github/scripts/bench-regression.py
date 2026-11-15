#!/usr/bin/env python3
"""Call regressions from the benchmark history the way the glossary defines them.

    bench-regression.py <threshold-percent> [--rss-threshold <percent>]
                        [--window 5] [--spread 3] [--issue] <data.js>...

Reads github-action-benchmark `data.js` files (the `benchmarks` branch, one per
direction: `smaller/data.js` and `bigger/data.js`). For every
series, each point is compared to the **median of the previous `--window`
accepted points**. A point is "over" when it is worse than that median by more
than the threshold (`--rss-threshold` for `peak_rss_mib` series) *and* by more
than `--spread` (default 3) times the window's own median absolute deviation, so
a series that swings from run to run needs a move larger than its swings. A **regression**
is the newest point being over *and* the point before it being over against
its own window — two consecutive runs. An image-tag change (in a point's
`extra`) restarts the window.

Direction comes from the tool: `customSmallerIsBetter` is worse when higher,
`customBiggerIsBetter` when lower.

Prints the verdict. With `--issue`, opens (or updates) a single GitHub issue
titled "Performance regression on main" via `gh`, and closes it when nothing
regresses.

Around that rule, and without changing what it flags:

- **Too noisy to watch.** For each series, the smallest lasting step the rule
  catches within two runs 9 times out of 10, measured: a step of each size in
  `NOISE_GRID` (times the threshold) is injected into the series' history at
  each of its last `NOISE_CUTS` runs, and the rule itself is run on the step's
  second run. Every series whose step is above its threshold is printed and
  listed in the issue under the findings.
- **Malformed entries** inside a readable file (a value that is not a number, a
  bench without a name or not an object, a point that is not an object, a
  point whose `benches` is not a list, `"commit": null`) are skipped with a
  `warning:` line; a point's missing commit reads as unknown. A `true`/`false`
  value is read as main reads it (1/0).
- **An unreadable data file** (missing, truncated, not a JSON object, `entries`
  not an object, a suite that is not a list) fails the run with an `error:`
  line and exit 1, before `gh` is called.
- **A point's image** is read from its `extra.images`; an `extra` without
  `images` (a point carrying only runner facts) reads as no image, like a point
  without `extra`, so recording runner facts never restarts a series' window.

Otherwise the exit code is 0 — a regression is a signal, not a broken job.
"""
import argparse
import json
import re
import statistics
import subprocess
import sys

ISSUE_TITLE = "Performance regression on main"
NOISE_CUTS = 20
NOISE_GRID = (1.2, 1.5, 2, 3, 4, 6, 8)


def warn(message: str) -> None:
    print(f"warning: {message}", file=sys.stderr)


def load(path: str) -> dict:
    text = open(path, encoding="utf-8").read()
    start = text.index("{")
    data = json.loads(text[start:])
    if not isinstance(data, dict):
        raise ValueError("not a JSON object")
    return data


def is_number(value) -> bool:
    return isinstance(value, (int, float))


def clean(data: dict) -> dict:
    """The data with every malformed entry left out, each with a warning."""
    entries = data.get("entries", {})
    if not isinstance(entries, dict):
        raise ValueError("`entries` is not an object")
    suites = {}
    for suite, points in entries.items():
        if not isinstance(points, list):
            raise ValueError(f"suite {suite!r} is not a list of points")
        kept = []
        for point in points:
            if not isinstance(point, dict):
                warn(f"{suite}: a point is not an object, skipped")
                continue
            commit = point.get("commit", {})
            if not isinstance(commit, dict):
                warn(f"{suite}: a point has no commit object ({commit!r}), read as an unknown commit")
                commit = {}
            cid = str(commit.get("id", ""))[:12]
            benches = []
            raw = point.get("benches", [])
            if not isinstance(raw, list):
                warn(f"{suite} at {cid or 'an unknown commit'}: `benches` is not a list ({raw!r}), skipped")
                raw = []
            for bench in raw:
                if not isinstance(bench, dict) or not isinstance(bench.get("name"), str) or not is_number(bench.get("value")):
                    name = bench.get("name") if isinstance(bench, dict) else bench
                    value = bench.get("value") if isinstance(bench, dict) else None
                    warn(f"{suite} at {cid or 'an unknown commit'}: bench {name!r} with value {value!r} skipped")
                    continue
                benches.append(bench)
            kept.append({**point, "commit": commit, "benches": benches})
        suites[suite] = kept
    return {"entries": suites}


def images(bench: dict) -> str:
    extra = bench.get("extra") or ""
    try:
        found = json.loads(extra)
        if "images" not in found:
            return ""
        return json.dumps(found.get("images"), sort_keys=True)
    except (ValueError, AttributeError, TypeError):
        return ""


def worse_by(value: float, baseline: float, smaller_is_better: bool) -> float:
    if baseline == 0:
        return 0.0
    change = (value - baseline) / baseline * 100
    return change if smaller_is_better else -change


def analyse(data: dict, threshold: float, rss_threshold: float, window: int, spread: float = 3.0) -> list[dict]:
    findings = []
    for suite, points in data.get("entries", {}).items():
        smaller = points and points[-1].get("tool") == "customSmallerIsBetter"
        # series name → list of (value, images, commit) in time order
        series: dict[str, list[tuple[float, str, dict]]] = {}
        for point in points:
            for bench in point.get("benches", []):
                series.setdefault(bench["name"], []).append((bench["value"], images(bench), point.get("commit", {})))
        for name, history in series.items():
            if len(history) < 3:
                continue
            limit = rss_threshold if name.endswith("peak_rss_mib") else threshold

            def over(i: int) -> tuple[bool, float, float]:
                value, img, _ = history[i]
                prior = [v for v, im, _ in history[max(0, i - window):i] if im == img]
                if len(prior) < 2:
                    return False, 0.0, value
                base = statistics.median(prior)
                pct = worse_by(value, base, smaller)
                swing = statistics.median(abs(v - base) for v in prior)
                beyond_swings = abs(value - base) > spread * swing
                return pct > limit and beyond_swings, pct, base

            last_over, last_pct, base = over(len(history) - 1)
            prev_over, _, _ = over(len(history) - 2)
            if last_over and prev_over:
                findings.append({
                    "suite": suite,
                    "series": name,
                    "worse_by_percent": round(last_pct, 1),
                    "baseline": base,
                    "value": history[-1][0],
                    "commit": history[-1][2].get("id", "")[:12],
                    "url": history[-1][2].get("url", ""),
                })
    return findings


def reliable_step(suite: str, tool: str, history: list[tuple[dict, dict]], limit: float, window: int,
                  spread: float) -> tuple[float, bool] | None:
    """The smallest step in `NOISE_GRID` the rule catches within two runs 9 times out of 10, by running it.

    `history` is the series' `(bench, commit)` points in time order. Returns the step (worse by, in %) and
    whether it reached 9 out of 10; when no size did, the largest size tried. None with too little history.
    """
    smaller = tool == "customSmallerIsBetter"
    cuts = range(max(window, len(history) - 1 - NOISE_CUTS), len(history) - 1)
    if len(cuts) < NOISE_CUTS // 2:
        return None
    steps = [limit * m for m in NOISE_GRID if smaller or limit * m < 100]
    for step in steps:
        factor = 1 + step / 100 if smaller else 1 - step / 100
        caught = 0
        for cut in cuts:
            # The rule reads only the newest two points and the `window` before each, so that slice decides alone.
            start = max(0, cut - window)
            points = [{"tool": tool, "commit": commit,
                       "benches": [{**bench, "value": bench["value"] * factor} if i >= cut else bench]}
                      for i, (bench, commit) in enumerate(history[start:cut + 2], start)]
            caught += bool(analyse({"entries": {suite: points}}, limit, limit, window, spread))
        if caught >= 0.9 * len(cuts):
            return step, True
    return steps[-1], False


def too_noisy(data: dict, threshold: float, rss_threshold: float, window: int, spread: float) -> list[dict]:
    """Series whose reliably caught step is above their threshold, with that step."""
    noisy = []
    for suite, points in data.get("entries", {}).items():
        if not points:
            continue
        tool = points[-1].get("tool")
        series: dict[str, list[tuple[dict, dict]]] = {}
        for point in points:
            for bench in point.get("benches", []):
                series.setdefault(bench["name"], []).append((bench, point.get("commit", {})))
        for name, history in series.items():
            limit = rss_threshold if name.endswith("peak_rss_mib") else threshold
            found = reliable_step(suite, tool, history, limit, window, spread)
            if found and found[0] > limit:
                noisy.append({"suite": suite, "series": name, "limit": limit, "step": round(found[0]), "reliable": found[1]})
    return noisy


def noisy_text(n: dict) -> str:
    if n["reliable"]:
        return f"a lasting step of about +{n['step']}% is caught within two runs 9 times out of 10"
    return f"a lasting step of +{n['step']}% is caught within two runs fewer than 9 times out of 10"


def noisy_lines(noisy: list[dict]) -> list[str]:
    return [f"| {n['suite']} | `{n['series']}` | {('+' if n['reliable'] else 'more than +')}{n['step']}% | {n['limit']:g}% |"
            for n in noisy]


def upsert_issue(findings: list[dict], noisy: list[dict]) -> None:
    listed = subprocess.run(
        ["gh", "issue", "list", "--state", "open", "--search", f'in:title "{ISSUE_TITLE}"', "--json", "number,title"],
        check=True, capture_output=True, text=True,
    )
    existing = [i for i in json.loads(listed.stdout or "[]") if i["title"] == ISSUE_TITLE]
    if not findings:
        for issue in existing:
            subprocess.run(["gh", "issue", "close", str(issue["number"]), "--comment", "Every series is back within its threshold."], check=True)
        return
    lines = [
        "Two consecutive runs on `main` crossed the threshold, and the series' own run-to-run swing, against the rolling median of the previous five points.",
        "",
        "| suite | series | worse by | baseline | now | commit |",
        "| --- | --- | ---: | ---: | ---: | --- |",
    ]
    for f in findings:
        commit = f"[`{f['commit']}`]({f['url']})" if f["url"] else f["commit"]
        lines.append(f"| {f['suite']} | `{f['series']}` | {f['worse_by_percent']:+.1f}% | {f['baseline']:.3g} | {f['value']:.3g} | {commit} |")
    if noisy:
        lines += ["", f"Too noisy to watch: on these series the rule catches a lasting step within two runs 9 times out of 10 "
                  f"only once it is at least this large, measured by injecting steps into each series' last {NOISE_CUTS} runs. "
                  "Their silence is not a sign they are fine.", "",
                  "| suite | series | lasting step caught within two runs 9 times out of 10 | threshold |",
                  "| --- | --- | ---: | ---: |",
                  *noisy_lines(noisy)]
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
    parser.add_argument("--window", type=int, default=5)
    parser.add_argument("--spread", type=float, default=3.0)
    parser.add_argument("--issue", action="store_true")
    args = parser.parse_args()
    rss = args.rss_threshold if args.rss_threshold is not None else args.threshold
    datas = []
    for path in args.data:
        try:
            datas.append(clean(load(path)))
        except (OSError, ValueError) as error:
            print(f"error: {path}: cannot read the benchmark history ({error}); nothing judged, the issue is left as it is",
                  file=sys.stderr)
            return 1
    findings = []
    for data in datas:
        findings += analyse(data, args.threshold, rss, args.window, args.spread)
    noisy = [n for data in datas for n in too_noisy(data, args.threshold, rss, args.window, args.spread)]
    if findings:
        print(f"{len(findings)} regression(s):")
        for f in findings:
            print(f"  {f['suite']} {f['series']}: {f['worse_by_percent']:+.1f}% vs median {f['baseline']:.3g} (now {f['value']:.3g}) at {f['commit']}")
    else:
        print("no regression: no series is over its threshold on two consecutive runs")
    if noisy:
        print(f"too noisy to watch (steps injected at each of the last {NOISE_CUTS} runs, the rule run on the second):")
        for n in noisy:
            print(f"    {n['suite']} {n['series']}: {noisy_text(n)} (threshold {n['limit']:g}%)")
    if args.issue:
        upsert_issue(findings, noisy)
    return 0


if __name__ == "__main__":
    sys.exit(main())
