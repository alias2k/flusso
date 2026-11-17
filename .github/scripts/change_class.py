#!/usr/bin/env python3
"""Which checks a pull request must run: the whole suite, or what documentation can break.

    change_class.py --event <github.event_name> --job <job> [--changes <file> | --unknown]
                    --base-ref <PR base branch> --default-branch <repository default branch>
                    [--root <repo>] [--output <$GITHUB_OUTPUT>] [--summary <$GITHUB_STEP_SUMMARY>]

The one home of the docs-only rule (issue #402). `.github/scripts/change-class.sh` (through
`.github/actions/change-class`) runs it in each gated job, over `git diff --name-status -z -M
HEAD^1 HEAD` of the pull request's merge commit: the whole PR as it would merge, not the last push.
It runs the **base's** copy of this file (`HEAD^1`), so a PR never judges itself by its own rules;
a base without one, or a copy that fails, means a full run.

- **docs-only**: a `pull_request` into the default branch whose every changed path (both sides of
  a rename) is documentation (`is_docs`). The job then runs only `DOC_TESTS` whose files changed,
  and clippy, doctests and doc links of each crate whose `README.md` changed and is compiled into
  it (`#![doc = include_str!("../README.md")]`).
- **full**: anything else: every other event, a PR on another base (its head carries that branch's
  commits, which a docs-only run would vouch for unbuilt), anything under `.github/`, and an
  unknown or unreadable diff.

Outputs (`$GITHUB_OUTPUT`): `docs_only`, `cargo` (a toolchain is needed), `nextest_packages`
(`-p a -p b`), `nextest_filter` (a nextest filterset, empty when no test reads the change),
`doc_packages` (space-separated). The job summary says what was skipped, so a skip is never read as
a pass of the full suite.
"""
import argparse
import pathlib
import re
import sys
from dataclasses import dataclass, field

# Paths no build, test or bench reads, except the docs job (mdBook, lychee, check-manual-links.sh,
# which always runs) and the tests in DOC_TESTS. Globs: `*` stays in one path segment, `**` crosses.
DOCS = (
    "docs/**",
    "*.md",
    "plugin/**.md",
    ".claude/commands/**.md",
    ".claude/skills/**.md",
    "apps/cli/ADOPTIONS.md",
)

# A README.md or CHANGELOG.md anywhere is documentation, unless it sits where tests, benches or
# sources may read it as data.
DOC_NAMES = ("README.md", "CHANGELOG.md")
DATA_DIRS = {"tests", "fixtures", "benches", "examples", "src", "fuzz", "scenarios", "frontend", "dist"}


@dataclass(frozen=True)
class DocTest:
    """A test that reads documentation: run when a path in `reads` changes."""

    what: str
    package: str
    filter: str
    reads: tuple[str, ...]
    on_removal: bool = False  # also run when any path is removed or renamed away


DOC_TESTS = (
    DocTest(
        "every repo path the agent docs name exists",
        "flusso-cli",
        "binary(agent_docs_paths)",
        ("plugin/**", ".claude/commands/**", ".claude/skills/**"),
        on_removal=True,
    ),
    DocTest(
        "the generated Reference tables are current",
        "flusso-cli",
        "test(committed_adapter_docs_are_current)",
        ("docs/src/reference/generated/**",),
    ),
    DocTest(
        "the upgrade page's script replacements run",
        "flusso-script",
        "binary(dates) & test(every_replacement_on_the_upgrade_page_runs)",
        ("docs/src/operate/upgrade-to-0-18.md",),
    ),
    DocTest(
        "the derive's trusted rules and map example are in the manual",
        "flusso-query-derive",
        "binary(boundary) & (test(=every_trusted_rule_is_in_the_manual)"
        " | test(=the_docs_map_examples_are_the_tested_ones))",
        (
            "docs/src/query/document-structs.md",
            "docs/src/query/maps.md",
            "plugin/skills/flusso-query/maps.md",
        ),
    ),
)

# What each gated job leaves out on a docs-only change, for its summary.
SKIPPED = {
    "test": "fmt, clippy, `cargo check --all-targets`, the server-only check, the hash-seed guard,"
    " the full nextest run (unit, integration, Postgres/OpenSearch e2e), workspace doctests and doc links",
    "fuzz": "the 60 s pgoutput fuzz run",
    "designer": "npm ci, format, lint, i18n, frontend tests, the SPA build and its drift check",
    "bench": "the in-process A/B against the base",
    "other-platforms": "the CLI check on this platform",
}


@dataclass(frozen=True)
class Change:
    status: str  # git's letter: A, M, D, R, C, T, …
    path: str
    old: str | None = None  # the source of a rename or copy


@dataclass
class Plan:
    docs_only: bool
    reason: str
    tests: list[DocTest] = field(default_factory=list)
    doc_packages: list[str] = field(default_factory=list)

    @property
    def nextest_packages(self) -> list[str]:
        return sorted({t.package for t in self.tests})

    @property
    def nextest_filter(self) -> str:
        return " | ".join(f"(package({t.package}) & {t.filter})" for t in self.tests)

    @property
    def cargo(self) -> bool:
        return not self.docs_only or bool(self.tests or self.doc_packages)


def full(reason: str) -> Plan:
    return Plan(docs_only=False, reason=reason)


def matches(path: str, pattern: str) -> bool:
    """`*` within one segment, `**` across segments (`docs/**` takes everything under docs/)."""
    regex = ""
    i = 0
    while i < len(pattern):
        if pattern.startswith("**", i):
            regex += ".*"
            i += 2
        elif pattern[i] == "*":
            regex += "[^/]*"
            i += 1
        else:
            regex += re.escape(pattern[i])
            i += 1
    return re.fullmatch(regex, path) is not None


def is_docs(path: str) -> bool:
    if path.startswith(".github/"):
        return False  # a change to CI runs CI, templates and READMEs included
    if any(matches(path, p) for p in DOCS):
        return True
    parts = path.split("/")
    return parts[-1] in DOC_NAMES and not DATA_DIRS.intersection(parts[:-1])


def parse_name_status(raw: bytes) -> list[Change]:
    """`git diff --name-status -z [-M]`: `S\\0path\\0`, or `R<score>\\0old\\0new\\0` for a rename or copy."""
    fields = raw.decode("utf-8").split("\0")
    if fields and fields[-1] == "":
        fields.pop()
    changes = []
    i = 0
    while i < len(fields):
        status = fields[i]
        if not re.fullmatch(r"[ACDMRTUXB][0-9]*", status):
            raise ValueError(f"not a status: {status!r}")
        letter = status[0]
        if letter in "RC":
            if i + 2 >= len(fields):
                raise ValueError(f"truncated {letter} entry")
            changes.append(Change(letter, fields[i + 2], fields[i + 1]))
            i += 3
        else:
            if i + 1 >= len(fields):
                raise ValueError(f"truncated {letter} entry")
            changes.append(Change(letter, fields[i + 1]))
            i += 2
    return changes


def compiled_readme(root: pathlib.Path, path: str) -> str | None:
    """The package a README.md is compiled into (its crate's `include_str!("../README.md")`), else None."""
    if pathlib.PurePosixPath(path).name != "README.md":
        return None
    crate = (root / path).parent
    lib = crate / "src" / "lib.rs"
    manifest = crate / "Cargo.toml"
    if not lib.is_file() or not manifest.is_file():
        return None
    if 'include_str!("../README.md")' not in lib.read_text(encoding="utf-8"):
        return None
    package = re.search(r'(?m)^\[package\][^\[]*?^name\s*=\s*"([^"]+)"', manifest.read_text(encoding="utf-8"))
    if package is None:
        raise ValueError(f"{manifest}: no package name")
    return package.group(1)


def classify(
    event: str,
    changes: list[Change] | None,
    root: pathlib.Path,
    base_ref: str | None,
    default_branch: str | None,
) -> Plan:
    if event != "pull_request":
        return full(f"a `{event}` run checks everything")
    if not base_ref or not default_branch:
        return full("the pull request's base or the default branch is unknown")
    if base_ref != default_branch:
        return full(f"the pull request's base is `{base_ref}`, not the default branch `{default_branch}`")
    if changes is None:
        return full("the change set could not be read")
    if not changes:
        return full("the diff is empty")
    present: list[str] = []  # paths the merge result holds
    gone: list[str] = []  # paths it no longer holds: deleted, or renamed away
    for change in changes:
        (gone if change.status == "D" else present).append(change.path)
        if change.old is not None:
            (gone if change.status == "R" else present).append(change.old)
    paths = present + gone
    for path in paths:
        if not is_docs(path):
            return full(f"`{path}` is not documentation")
    for path in gone:
        if pathlib.PurePosixPath(path).name == "README.md" and (root / path).parent.joinpath("Cargo.toml").is_file():
            return full(f"`{path}`, a crate's README, is removed")

    doc_packages = {package for path in present if (package := compiled_readme(root, path))}
    removed = bool(gone)

    tests = [
        t
        for t in DOC_TESTS
        if (t.on_removal and removed) or any(matches(p, r) for p in paths for r in t.reads)
    ]
    return Plan(True, "every changed file is documentation", tests, sorted(doc_packages))


def outputs(plan: Plan) -> str:
    lines = {
        "docs_only": str(plan.docs_only).lower(),
        "cargo": str(plan.cargo).lower(),
        "nextest_packages": " ".join(f"-p {p}" for p in plan.nextest_packages),
        "nextest_filter": plan.nextest_filter,
        "doc_packages": " ".join(plan.doc_packages),
    }
    return "".join(f"{k}={v}\n" for k, v in lines.items())


def summary(plan: Plan, job: str) -> str:
    if not plan.docs_only:
        return f"**Full suite**: {plan.reason}.\n"
    ran = [f"`{t.what}` ({t.package})" for t in plan.tests if job == "test"]
    if job == "test":
        ran += [f"clippy, doctests and doc links of `{p}` (its README changed)" for p in plan.doc_packages]
    text = f"**docs-only: skipped** {SKIPPED[job]}.\n"
    if job == "test":
        text += "\nRan: " + ("; ".join(ran) if ran else "nothing here (no test reads these files)") + ".\n"
    text += "\nThe manual job builds and link-checks the book on every run.\n"
    return text


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    parser.add_argument("--event", required=True)
    parser.add_argument("--job", required=True, choices=sorted(SKIPPED))
    source = parser.add_mutually_exclusive_group()
    source.add_argument("--changes", type=pathlib.Path, help="git diff --name-status -z -M output")
    source.add_argument("--unknown", action="store_true", help="the diff could not be taken")
    parser.add_argument("--base-ref", help="the pull request's base branch")
    parser.add_argument("--default-branch", help="the repository's default branch")
    parser.add_argument("--root", type=pathlib.Path, default=pathlib.Path("."))
    parser.add_argument("--output", type=pathlib.Path)
    parser.add_argument("--summary", type=pathlib.Path)
    args = parser.parse_args(argv)

    changes = None
    if args.changes is not None and not args.unknown:
        try:
            changes = parse_name_status(args.changes.read_bytes())
        except (OSError, ValueError, UnicodeDecodeError) as error:
            print(f"::warning::cannot read the change set ({error}); running everything", file=sys.stderr)
    plan = classify(args.event, changes, args.root, args.base_ref, args.default_branch)

    text = outputs(plan)
    if args.output:
        with args.output.open("a", encoding="utf-8") as out:
            out.write(text)
    else:
        sys.stdout.write(text)
    if args.summary:
        with args.summary.open("a", encoding="utf-8") as out:
            out.write(summary(plan, args.job))
    print(f"{'docs-only' if plan.docs_only else 'full'}: {plan.reason}", file=sys.stderr)
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
