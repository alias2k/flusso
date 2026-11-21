#!/usr/bin/env python3
"""The parts `clippy + tests` runs the test suite in, and the guards that none is lost (issue #444).

    test-parts.py matrix [--read-side-only]
    test-parts.py check [--read-side-only] [--nextest-toml <file>] [--summary <file>] [--output <file>]
                        -- <nextest list arguments>
    test-parts.py verdict --needs <json> --expected <count> --junit <dir> [--summary <file>]

One home for the split. `ci.yml` builds the test binaries once (`cargo nextest archive`) and runs
each part below in its own job from that archive; the job `clippy + tests` reports the verdict.

- `matrix` prints the parts as `parts=<json>` for `$GITHUB_OUTPUT`: id, name and the nextest
  filterset each runs, under the read-side scope when `--read-side-only`.
- `check` lists the whole run and every part with `$CARGO nextest list` (default `cargo`) and
  fails unless each test the single job would run is in exactly one part. Prints `count=<n>`.
- `verdict` is the required check: green only when every job succeeded and the parts' JUnit
  reports hold exactly `--expected` tests. `--needs` is the aggregate job's `toJSON(needs)`; a
  docs-only run (the plan job's `docs_only` output) may skip the lint and test jobs, nothing else.

**Adding a test binary.** A binary in no test group runs in `rest` with nothing to do. A binary in
the `opensearch` test group (`.config/nextest.toml`) must also be named in one `opensearch-*`
part below, or `check` fails naming it: pick the part with the least time (each takes about a
third of the group's run time; JUnit `time` per binary says how much it adds).

Exit codes: 0 pass, 1 a test in no part or in two, a part naming no binary, or a red verdict;
2 nextest failed or its output can't be read.
"""
import argparse
import json
import os
import pathlib
import subprocess
import sys
import tomllib
import xml.etree.ElementTree as ET
from dataclasses import dataclass

ROOT = pathlib.Path(__file__).resolve().parents[2]
NEXTEST_TOML = ROOT / ".config" / "nextest.toml"
WHERE = ".github/scripts/test-parts.py (PARTS)"


@dataclass(frozen=True)
class Part:
    id: str
    name: str
    binaries: tuple[str, ...] = ()
    # The test group whose binaries this part takes by name: every test of the group must be in
    # one such part, or `check` fails.
    group: str | None = None
    # Everything no other part takes, and no test of a group the parts divide by name.
    rest: bool = False
    # Whether a test in it runs cargo (the derive's boundary and consumer_lints, the trybuild
    # `ui` harnesses): such a part fetches the locked crates first.
    cargo: bool = True


# The `opensearch` group runs one test at a time on each runner, so its binaries are split by
# measured time (JUnit `time`, `main` at 494be84c): about 12 minutes each.
PARTS = (
    Part(
        "opensearch-1",
        "OpenSearch e2e 1/3",
        (
            "flusso-cli::shutdown",
            "flusso-cli::fence_restart",
            "flusso-sink-opensearch::document_ids",
            "flusso-sink-opensearch::tls",
            "flusso-query-e2e::tls",
            "flusso-query-e2e::geo_points",
            "flusso-query-e2e::combined_search",
        ),
        group="opensearch",
        cargo=False,
    ),
    Part(
        "opensearch-2",
        "OpenSearch e2e 2/3",
        (
            "flusso-engine::pipeline",
            "flusso-sink-opensearch::seeding",
            "flusso-sink-opensearch::reindex",
        ),
        group="opensearch",
        cargo=False,
    ),
    Part(
        "opensearch-3",
        "OpenSearch e2e 3/3",
        (
            "flusso-cli::duplicate_names",
            "flusso-daemon::fence",
            "flusso-query-e2e::read_time",
            "flusso-cli::time_columns",
            "flusso-engine::lane_parking",
            "flusso-query-e2e::cursor_paging",
            "flusso-query-e2e::map_keys",
            "flusso-sink-opensearch::map_keys",
            "flusso-query-e2e::array_columns",
        ),
        group="opensearch",
        cargo=False,
    ),
    # Compiles thousands of cases with cargo: alone, so it has the runner's cores.
    Part("boundary", "derive boundary", ("flusso-query-derive::boundary",)),
    Part("rest", "unit, integration, Postgres e2e, ui", rest=True),
)

# PRs confined to the query release train (sdk/query, sdk/query-derive, dev/query-e2e) skip the
# Postgres/OpenSearch e2e suites those paths can't break: every tests/ binary of these three
# crates is container e2e, so excluding them by kind is exact.
READ_SIDE_SCOPE = (
    "all() - (kind(test) & (package(flusso-source-postgres) + package(flusso-engine)"
    " + package(flusso-sink-opensearch)))"
)


def by_name(binaries: tuple[str, ...]) -> str:
    return " | ".join(f"binary_id(={b})" for b in binaries)


def group_filter(nextest_toml: str, group: str) -> str:
    """The union of every override filter that puts tests in `group`, over every profile."""
    config = tomllib.loads(nextest_toml)
    filters = []
    for profile in config.get("profile", {}).values():
        for override in profile.get("overrides", []):
            if override.get("test-group") == group and override.get("filter") not in filters:
                filters.append(override["filter"])
    if not filters:
        raise ValueError(f"no override puts tests in the `{group}` test group")
    return " | ".join(f"({f})" for f in filters)


def part_filter(part: Part, parts: tuple[Part, ...], nextest_toml: str) -> str:
    if not part.rest:
        return by_name(part.binaries)
    taken = [f"({by_name(p.binaries)})" for p in parts if p.binaries]
    taken += [f"({group_filter(nextest_toml, g)})" for g in sorted({p.group for p in parts if p.group})]
    return f"not ({' | '.join(taken)})"


def scoped(filter: str, read_side_only: bool) -> str:
    return f"({READ_SIDE_SCOPE}) & ({filter})" if read_side_only else filter


def matrix(parts: tuple[Part, ...], nextest_toml: str, read_side_only: bool) -> list[dict]:
    return [
        {
            "id": p.id,
            "name": p.name,
            "filter": scoped(part_filter(p, parts, nextest_toml), read_side_only),
            "cargo": p.cargo,
        }
        for p in parts
    ]


def listed(raw: str) -> set[str]:
    """`binary-id test-name` of each test a `nextest list --message-format json` selects."""
    data = json.loads(raw)
    tests = set()
    for binary, suite in data["rust-suites"].items():
        for name, case in suite.get("testcases", {}).items():
            if case["filter-match"]["status"] == "matches":
                tests.add(f"{binary} {name}")
    return tests


def binary_of(test: str) -> str:
    return test.split(" ", 1)[0]


def problems(
    whole: set[str], each: dict[str, set[str]], parts: tuple[Part, ...], scoped_run: bool
) -> list[str]:
    """Why the parts don't run exactly `whole`, one line per finding; empty when they do."""
    found = []
    seen: dict[str, list[str]] = {}
    for part_id, tests in each.items():
        for test in tests:
            seen.setdefault(test, []).append(part_id)
    missing: dict[str, int] = {}
    for test in sorted(whole):
        if test not in seen:
            missing[binary_of(test)] = missing.get(binary_of(test), 0) + 1
    groups = sorted({p.group for p in parts if p.group})
    for binary, count in missing.items():
        found.append(
            f"`{binary}`: {count} test(s) in no part. Its binary is in a test group the parts divide by "
            f"name ({', '.join(groups)}): add `{binary}` to the `binaries` of one of that group's parts "
            f"in {WHERE}."
        )
    for test, ids in sorted(seen.items()):
        if len(ids) > 1:
            found.append(f"`{test}` is in {len(ids)} parts ({', '.join(sorted(ids))}): name its binary in one.")
        if test not in whole:
            found.append(f"`{test}` is in part {ids[0]} but not in the whole run.")
    if not scoped_run:
        binaries = {binary_of(t) for t in whole}
        for part in parts:
            for binary in part.binaries:
                if binary not in binaries:
                    found.append(f"part {part.id} names `{binary}`, which has no test: remove or rename it in {WHERE}.")
    return found


def nextest_list(args: list[str], filter: str | None) -> set[str]:
    cargo = os.environ.get("CARGO", "cargo")
    command = [cargo, "nextest", "list", "--profile", "ci", "--run-ignored", "all", "--message-format", "json"]
    command += args
    if filter is not None:
        command += ["-E", filter]
    result = subprocess.run(command, capture_output=True, text=True)
    if result.returncode != 0:
        sys.stderr.write(result.stderr)
        raise RuntimeError(f"`{' '.join(command)}` exited {result.returncode}")
    try:
        return listed(result.stdout)
    except (ValueError, KeyError, TypeError) as error:
        raise RuntimeError(f"cannot read the output of `{' '.join(command)}`: {error!r}") from error


def check(args: argparse.Namespace) -> int:
    nextest_toml = pathlib.Path(args.nextest_toml).read_text(encoding="utf-8")
    try:
        whole = nextest_list(args.nextest, READ_SIDE_SCOPE if args.read_side_only else None)
        each = {
            entry["id"]: nextest_list(args.nextest, entry["filter"])
            for entry in matrix(PARTS, nextest_toml, args.read_side_only)
        }
    except RuntimeError as error:
        print(f"::error::{error}", file=sys.stderr)
        return 2
    found = problems(whole, each, PARTS, args.read_side_only)
    table = ["| Part | Tests |", "| --- | ---: |"]
    table += [f"| {p.name} (`{p.id}`) | {len(each[p.id])} |" for p in PARTS]
    table += [f"| **whole run** | **{len(whole)}** |"]
    report = "\n".join(table) + "\n"
    print(report)
    if args.summary:
        with open(args.summary, "a", encoding="utf-8") as out:
            out.write("### Test parts\n\n" + report)
            for line in found:
                out.write(f"\n- {line}")
    if args.output:
        with open(args.output, "a", encoding="utf-8") as out:
            out.write(f"count={len(whole)}\n")
    for line in found:
        print(f"::error::{line}", file=sys.stderr)
    return 1 if found else 0


def junit_count(path: pathlib.Path) -> int:
    root = ET.parse(path).getroot()
    if root.tag == "testsuites" and "tests" in root.attrib:
        return int(root.attrib["tests"])
    return sum(1 for _ in root.iter("testcase"))


def verdict(needs: dict, expected: int | None, junit: dict[str, int]) -> list[str]:
    """Why `clippy + tests` is red, one line per reason; empty when it is green."""
    plan = needs.get("plan", {})
    if plan.get("result") != "success":
        return [f"the plan job's result is `{plan.get('result', 'missing')}`"]
    docs_only = plan.get("outputs", {}).get("docs_only") == "true"
    found = []
    for job, result in sorted((name, need.get("result")) for name, need in needs.items() if name != "plan"):
        if result == "success" or (docs_only and job != "build" and result == "skipped"):
            continue
        found.append(f"the `{job}` job's result is `{result}`")
    if docs_only or found:
        return found
    # A part with no test under the read-side scope may write no report; a part that ran tests
    # and wrote none shows in the sum.
    ran = sum(junit.values())
    if expected is None:
        found.append("the build job listed no test count")
    elif ran != expected:
        found.append(f"the parts ran {ran} tests, the whole run lists {expected}")
    return found


def verdict_main(args: argparse.Namespace) -> int:
    needs = json.loads(args.needs)
    junit = {}
    unreadable = []
    root = pathlib.Path(args.junit)
    for part in PARTS:
        report = root / f"nextest-junit-{part.id}" / "junit.xml"
        if report.is_file():
            try:
                junit[part.id] = junit_count(report)
            except (ET.ParseError, ValueError) as error:
                unreadable.append(f"the JUnit report of part {part.id} can't be read: {error}")
    expected = int(args.expected) if args.expected else None
    found = unreadable + verdict(needs, expected, junit)
    lines = ["| Job | Result |", "| --- | --- |"]
    lines += [f"| {name} | {need.get('result')} |" for name, need in sorted(needs.items())]
    if junit:
        lines += ["", "| Part | Tests run |", "| --- | ---: |"]
        lines += [f"| `{p}` | {n} |" for p, n in junit.items()]
        lines += [f"| **total** | **{sum(junit.values())}** (listed: {expected}) |"]
    if needs.get("plan", {}).get("outputs", {}).get("docs_only") == "true":
        lines += ["", "Docs-only change: the plan job's summary says what was skipped and what ran."]
    text = "\n".join(lines) + "\n"
    text += "".join(f"\n- **red:** {line}" for line in found)
    print(text)
    if args.summary:
        with open(args.summary, "a", encoding="utf-8") as out:
            out.write(text + "\n")
    for line in found:
        print(f"::error::{line}", file=sys.stderr)
    return 1 if found else 0


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    commands = parser.add_subparsers(dest="command", required=True)

    m = commands.add_parser("matrix")
    m.add_argument("--read-side-only", action="store_true")
    m.add_argument("--nextest-toml", default=str(NEXTEST_TOML))

    c = commands.add_parser("check")
    c.add_argument("--read-side-only", action="store_true")
    c.add_argument("--nextest-toml", default=str(NEXTEST_TOML))
    c.add_argument("--summary")
    c.add_argument("--output")
    c.add_argument("nextest", nargs="*", help="arguments for `cargo nextest list`, after `--`")

    v = commands.add_parser("verdict")
    v.add_argument("--needs", required=True)
    v.add_argument("--expected", default="")
    v.add_argument("--junit", required=True)
    v.add_argument("--summary")

    args = parser.parse_args(argv)
    if args.command == "matrix":
        nextest_toml = pathlib.Path(args.nextest_toml).read_text(encoding="utf-8")
        print("parts=" + json.dumps(matrix(PARTS, nextest_toml, args.read_side_only)))
        return 0
    if args.command == "check":
        return check(args)
    return verdict_main(args)


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
