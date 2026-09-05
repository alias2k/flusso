#!/usr/bin/env python3
"""Call regressions from the benchmark history the way the glossary defines them.

    bench-regression.py <threshold-percent> [--rss-threshold <percent>]
                        [--window 5] [--issue] <data.js>...

Reads github-action-benchmark `data.js` files (the `benchmarks` branch, one per
direction: `smaller/data.js` and `bigger/data.js`). For every
series, each point is compared to the **median of the previous `--window`
accepted points**. A point is "over" when it is worse than that median by more
than the threshold (`--rss-threshold` for `peak_rss_mib` series). A **regression**
is the newest point being over *and* the point before it being over against
its own window — two consecutive runs. An image-tag change (in a point's
`extra`) restarts the window.

Direction comes from the tool: `customSmallerIsBetter` is worse when higher,
`customBiggerIsBetter` when lower.

Prints the verdict. With `--issue`, opens (or updates) a single GitHub issue
titled "Performance regression on main" via `gh`, and closes it when nothing
regresses. Exit code is always 0 — a regression is a signal, not a broken job.
"""
import argparse
import json
import re
import statistics
import subprocess
import sys

ISSUE_TITLE = "Performance regression on main"


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


def worse_by(value: float, baseline: float, smaller_is_better: bool) -> float:
    if baseline == 0:
        return 0.0
    change = (value - baseline) / baseline * 100
    return change if smaller_is_better else -change


def analyse(data: dict, threshold: float, rss_threshold: float, window: int) -> list[dict]:
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
                return pct > limit, pct, base

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


def upsert_issue(findings: list[dict]) -> None:
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
        "Two consecutive runs on `main` crossed the threshold against the rolling median of the previous five points.",
        "",
        "| suite | series | worse by | baseline | now | commit |",
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
    parser.add_argument("--window", type=int, default=5)
    parser.add_argument("--issue", action="store_true")
    args = parser.parse_args()
    rss = args.rss_threshold if args.rss_threshold is not None else args.threshold
    findings = []
    for path in args.data:
        findings += analyse(load(path), args.threshold, rss, args.window)
    if findings:
        print(f"{len(findings)} regression(s):")
        for f in findings:
            print(f"  {f['suite']} {f['series']}: {f['worse_by_percent']:+.1f}% vs median {f['baseline']:.3g} (now {f['value']:.3g}) at {f['commit']}")
    else:
        print("no regression: no series is over its threshold on two consecutive runs")
    if args.issue:
        upsert_issue(findings)
    return 0


if __name__ == "__main__":
    sys.exit(main())
