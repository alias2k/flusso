#!/usr/bin/env python3
"""The Postgres matrix's leg summary and its one nightly issue (issue #348).

    postgres-matrix.py summary JUNIT --tag 18-alpine --outcome failure --out result.json
    postgres-matrix.py issue RESULTS_DIR --tags '["14-alpine","18-alpine"]' --run-url URL [--dry-run]

`summary` reads one leg's nextest JUnit (the `ci` profile, which also stores passing tests' output
for the binaries with a `test_postgres::pinned` test, from which the pinned starts are read): prints the leg's Markdown summary (counts, failing tests, tests that started a pinned image instead
of the leg's) and writes the leg's result as JSON. A missing or unreadable report is a failed leg
(it broke before or while the tests ran).

`issue` reads every leg's result JSON under RESULTS_DIR (one per artifact, any depth). A leg in
`--tags` with no result counts as failed. Any failed leg opens or updates one issue (fixed title,
label `area: ci-release`, opened by the workflow's bot) listing each failing version and its tests,
with a link to the run; every leg passing closes it. It acts on one issue only, the newest it opened:
a human's issue with the same title is left alone. `--dry-run` prints the `gh` calls instead of making
them. Only the nightly run calls this; a dispatched run reports through its checks alone.
"""
import argparse
import json
import pathlib
import re
import subprocess
import sys
import xml.etree.ElementTree as ElementTree

ISSUE_TITLE = "Postgres matrix fails on a supported major"
ISSUE_LABEL = "area: ci-release"
# The workflow token's author, as `gh` names it in a filter and in `author.login`.
BOT_AUTHOR = "app/github-actions"
BOT_LOGINS = {BOT_AUTHOR, "github-actions[bot]", "github-actions"}
PINNED = re.compile(r"test-postgres: starting postgres:(\S+) instead of the run's")


def read_junit(path: pathlib.Path) -> dict | None:
    """Counts, failing and flaky tests, and pinned starts; None when there is no readable report."""
    try:
        root = ElementTree.parse(path).getroot()
    except (OSError, ElementTree.ParseError):
        return None
    passed, failed, flaky, pinned = 0, [], [], []
    for case in root.iter("testcase"):
        name = f"{case.get('classname', '')} {case.get('name', '')}".strip()
        if case.find("failure") is not None or case.find("error") is not None:
            failed.append(name)
        elif case.find("skipped") is None:
            passed += 1
            if case.find("flakyFailure") is not None or case.find("flakyError") is not None:
                flaky.append(name)
        output = "".join(node.text or "" for node in case.iter() if node.tag in ("system-err", "system-out"))
        images = sorted(set(PINNED.findall(output)))
        if images:
            pinned.append({"test": name, "images": images})
    return {"passed": passed, "failed": failed, "flaky": flaky, "pinned": pinned}


def leg_result(junit: pathlib.Path, tag: str, outcome: str) -> dict:
    report = read_junit(junit)
    result = {"tag": tag, "outcome": outcome, "report": report is not None}
    result.update(report or {"passed": 0, "failed": [], "flaky": [], "pinned": []})
    result["ok"] = outcome == "success" and report is not None and not result["failed"]
    return result


def summary_markdown(result: dict) -> str:
    verdict = "passed" if result["ok"] else "failed"
    lines = [f"### Postgres `{result['tag']}`: {verdict}", ""]
    if not result["report"]:
        lines.append(f"No test report: the leg stopped before or while its tests ran (step outcome `{result['outcome']}`).")
        return "\n".join(lines) + "\n"
    lines.append(f"{result['passed']} passed, {len(result['failed'])} failed.")
    if result["failed"]:
        lines += ["", "Failed:", ""] + [f"- `{name}`" for name in result["failed"]]
    if result["flaky"]:
        lines += ["", "Passed on a retry:", ""] + [f"- `{name}`" for name in result["flaky"]]
    lines += ["", f"Pinned to their own image whatever the leg's (`test_postgres::pinned`), so not a `{result['tag']}` result:", ""]
    if result["pinned"]:
        lines += [f"- `{p['test']}` on " + ", ".join(f"`postgres:{i}`" for i in p["images"]) for p in result["pinned"]]
    else:
        lines.append("- none")
    return "\n".join(lines) + "\n"


def read_results(directory: pathlib.Path, tags: list[str]) -> list[dict]:
    """One result per expected tag, in `tags` order; a tag with no readable result is a failed leg."""
    found = {}
    for path in sorted(directory.rglob("*.json")):
        try:
            result = json.loads(path.read_text(encoding="utf-8"))
        except (OSError, ValueError):
            continue
        if isinstance(result, dict) and "tag" in result:
            found[result["tag"]] = result
    missing = {"outcome": "missing", "report": False, "passed": 0, "failed": [], "flaky": [], "pinned": [], "ok": False}
    return [found.get(tag, {"tag": tag, **missing}) for tag in tags] + [
        found[tag] for tag in sorted(found) if tag not in tags
    ]


def issue_body(results: list[dict], run_url: str) -> str:
    lines = [f"The nightly Postgres matrix failed: [the run]({run_url}).", ""]
    for result in results:
        if result["ok"]:
            continue
        lines.append(f"## Postgres `{result['tag']}`")
        lines.append("")
        if not result["report"]:
            lines.append(f"No test report: the leg stopped before or while its tests ran (outcome `{result['outcome']}`).")
        else:
            lines += [f"- `{name}`" for name in result["failed"]] or [
                f"Every test passed, but the leg ended `{result['outcome']}`."
            ]
        lines.append("")
    passing = [f"`{r['tag']}`" for r in results if r["ok"]]
    if passing:
        lines += [f"Passed: {', '.join(passing)}.", ""]
    lines += [
        "Reproduce one version locally: `FLUSSO_TEST_PG_TAG=<tag> cargo nextest run --profile ci --run-ignored all -p <package>`.",
        "",
        "This issue is maintained by `.github/scripts/postgres-matrix.py`; it closes itself when a later nightly passes.",
    ]
    return "\n".join(lines)


def gh(args: list[str], dry_run: bool, capture: bool = False) -> str:
    if dry_run and not capture:
        print("would run: " + " ".join(json.dumps(a) if " " in a or "\n" in a else a for a in ["gh", *args]))
        return ""
    done = subprocess.run(["gh", *args], check=True, capture_output=capture, text=True)
    return done.stdout if capture else ""


def own_issue(listed: str) -> tuple[int | None, list[int]]:
    """The newest open issue this script maintains, and any older ones it leaves alone.

    Its issue has the fixed title and was opened by the workflow's bot: a human's issue with the same
    title is never edited or closed.
    """
    numbers = sorted(
        (issue["number"] for issue in json.loads(listed or "[]")
         if issue.get("title") == ISSUE_TITLE and (issue.get("author") or {}).get("login") in BOT_LOGINS),
        reverse=True,
    )
    return (numbers[0] if numbers else None), numbers[1:]


def maintain_issue(results: list[dict], run_url: str, dry_run: bool) -> None:
    # `--author app/github-actions` matches only through `--search`; alone, gh's filter drops bot authors.
    listed = gh(["issue", "list", "--state", "open", "--label", ISSUE_LABEL, "--author", BOT_AUTHOR,
                 "--search", f'in:title "{ISSUE_TITLE}"', "--json", "number,title,author"],
                dry_run, capture=True)
    existing, older = own_issue(listed)
    if older:
        print(f"maintaining #{existing}; also open with its title and author, left alone: "
              + ", ".join(f"#{n}" for n in older))
    if all(result["ok"] for result in results):
        if existing is not None:
            gh(["issue", "close", str(existing), "--comment", f"Every version passed in [this run]({run_url})."], dry_run)
        return
    body = issue_body(results, run_url)
    if existing is not None:
        gh(["issue", "edit", str(existing), "--body", body], dry_run)
    else:
        gh(["issue", "create", "--title", ISSUE_TITLE, "--label", ISSUE_LABEL, "--body", body], dry_run)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    commands = parser.add_subparsers(dest="command", required=True)
    summary = commands.add_parser("summary")
    summary.add_argument("junit", type=pathlib.Path)
    summary.add_argument("--tag", required=True)
    summary.add_argument("--outcome", required=True)
    summary.add_argument("--out", type=pathlib.Path, required=True)
    issue = commands.add_parser("issue")
    issue.add_argument("results", type=pathlib.Path)
    issue.add_argument("--tags", required=True, help="the expected legs, as the JSON array the matrix ran")
    issue.add_argument("--run-url", required=True)
    issue.add_argument("--dry-run", action="store_true")
    args = parser.parse_args()
    if args.command == "summary":
        result = leg_result(args.junit, args.tag, args.outcome)
        args.out.write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")
        sys.stdout.write(summary_markdown(result))
        return 0
    results = read_results(args.results, json.loads(args.tags))
    for result in results:
        print(f"{result['tag']}: {'passed' if result['ok'] else 'failed'}")
    maintain_issue(results, args.run_url, args.dry_run)
    return 0


if __name__ == "__main__":
    sys.exit(main())
