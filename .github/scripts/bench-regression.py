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
  can catch: the threshold, or `--spread` times the window's median absolute
  deviation over its median when that is larger (the median of the series'
  last `NOISE_WINDOWS` windows). Series that need more than their threshold are
  printed, and listed in the issue under the findings.
- **Malformed entries** inside a readable file (a value that is not a number, a
  bench without a name, a point that is not an object, `"commit": null`) are
  skipped with a `warning:` line; a point's missing commit reads as unknown.
- **An unreadable data file** (missing, truncated, not a JSON object) fails
  the run with an `error:` line and exit 1, before `gh` is called.

Otherwise the exit code is 0 — a regression is a signal, not a broken job.
"""
import argparse
import json
import math
import re
import statistics
import subprocess
import sys

ISSUE_TITLE = "Performance regression on main"
NOISE_WINDOWS = 20


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
    return isinstance(value, (int, float)) and not isinstance(value, bool)


def clean(data: dict) -> dict:
    """The data with every malformed entry left out, each with a warning."""
    entries = data.get("entries", {})
    if not isinstance(entries, dict):
        warn("`entries` is not an object, skipped")
        return {"entries": {}}
    suites = {}
    for suite, points in entries.items():
        if not isinstance(points, list):
            warn(f"{suite}: its points are not a list, skipped")
            continue
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
            for bench in raw if isinstance(raw, list) else []:
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
        return json.dumps(json.loads(extra).get("images"), sort_keys=True)
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


def too_noisy(data: dict, threshold: float, rss_threshold: float, window: int, spread: float) -> list[dict]:
    """Series whose own spread hides a lasting step of their threshold, with the smallest step the rule catches."""
    noisy = []
    for suite, points in data.get("entries", {}).items():
        series: dict[str, list[tuple[float, str]]] = {}
        for point in points:
            for bench in point.get("benches", []):
                series.setdefault(bench["name"], []).append((bench["value"], images(bench)))
        for name, history in series.items():
            limit = rss_threshold if name.endswith("peak_rss_mib") else threshold
            needed = []
            for i in range(window, len(history)):
                prior = [v for v, im in history[i - window:i] if im == history[i][1]]
                if len(prior) < 2:
                    continue
                base = statistics.median(prior)
                if base:
                    swing = statistics.median(abs(v - base) for v in prior)
                    needed.append(spread * swing / abs(base) * 100)
            if needed and statistics.median(needed[-NOISE_WINDOWS:]) > limit:
                noisy.append({"suite": suite, "series": name, "limit": limit,
                              "needs_percent": math.ceil(statistics.median(needed[-NOISE_WINDOWS:]))})
    return noisy


def noisy_lines(noisy: list[dict]) -> list[str]:
    return [f"| {n['suite']} | `{n['series']}` | +{n['needs_percent']}% | {n['limit']:g}% |" for n in noisy]


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
        lines += ["", "Too noisy to watch: these series swing so much from run to run that a lasting step must be at least "
                  "this large before the rule can catch it, so their silence is not a sign they are fine.", "",
                  "| suite | series | smallest lasting step caught | threshold |", "| --- | --- | ---: | ---: |",
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
        print("too noisy to watch (the smallest lasting step the rule can catch on each):")
        for n in noisy:
            print(f"    {n['suite']} {n['series']}: needs a lasting step of +{n['needs_percent']}% (threshold {n['limit']:g}%)")
    if args.issue:
        upsert_issue(findings, noisy)
    return 0


if __name__ == "__main__":
    sys.exit(main())
