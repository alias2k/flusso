"""Tests for change_class.py, the docs-only rule (issue #402).

    python3 -m unittest discover -s .github/scripts -p 'test_*.py'

Two halves: the rule over hand-written change sets (every pattern, mixed changes, renames,
deletions, the empty and unreadable diff), and guards over this checkout that keep the rule true as
the repo moves: every documentation file a Rust source reads is covered by a `DOC_TESTS` entry, and
every entry names files, a package, a binary and tests that exist.
"""
import importlib.util
import os
import pathlib
import re
import subprocess
import sys
import tempfile
import unittest

HERE = pathlib.Path(__file__).resolve().parent
REPO = HERE.parent.parent

spec = importlib.util.spec_from_file_location("change_class", HERE / "change_class.py")
rule = importlib.util.module_from_spec(spec)
sys.modules["change_class"] = rule
spec.loader.exec_module(rule)

Change = rule.Change


def classify(*changes: Change, event: str = "pull_request", root: pathlib.Path = REPO):
    return rule.classify(event, list(changes), root)


def m(path: str) -> Change:
    return Change("M", path)


class Patterns(unittest.TestCase):
    DOCS = [
        "docs/src/SUMMARY.md",
        "docs/src/author/scripts/dates.md",
        "docs/book.toml",
        "docs/adr/0012-each-index-runs-on-its-own-task.md",
        "docs/STYLE.md",
        "docs/channels/main/banner.html",
        "docs/src/reference/generated/sink-opensearch.md",
        "README.md",
        "CLAUDE.md",
        "CONTEXT.md",
        "plugin/skills/flusso-query/SKILL.md",
        "plugin/ARCHITECTURE.md",
        ".claude/commands/implement.md",
        ".claude/skills/coordinator/SKILL.md",
        ".github/ISSUE_TEMPLATE/bug_report.md",
        ".github/PULL_REQUEST_TEMPLATE.md",
        "apps/cli/ADOPTIONS.md",
        "apps/cli/README.md",
        "apps/cli/CHANGELOG.md",
        "libs/README.md",
        "libs/2-engine/README.md",
        "deploy/helm/flusso/README.md",
        "dev/README.md",
    ]
    CODE = [
        "libs/2-engine/src/ingest.rs",
        "Cargo.toml",
        "Cargo.lock",
        ".github/workflows/ci.yml",
        ".github/scripts/change_class.py",
        ".github/actions/change-class/action.yml",
        ".claude/settings.json",
        "plugin/hooks/hooks.json",
        "plugin/.claude-plugin/plugin.json",
        "libs/1-config/tests/compat/README.md",
        "libs/2-script/tests/recipes/recipes.schema.yml",
        "sdk/query-derive/tests/ui_pass/docs_map_examples.rs",
        "sdk/query/src/notes.md",
        "apps/design/frontend/README.md",
        "apps/design/dist/index.html",
        "dev/flusso.toml",
        "justfile",
        "docs.md.rs",
        "documentation/x.md",
    ]

    def test_documentation(self) -> None:
        for path in self.DOCS:
            with self.subTest(path):
                self.assertTrue(rule.is_docs(path))

    def test_code_and_data(self) -> None:
        for path in self.CODE:
            with self.subTest(path):
                self.assertFalse(rule.is_docs(path))

    def test_glob_segments(self) -> None:
        self.assertTrue(rule.matches("a/b/c.md", "a/**.md"))
        self.assertFalse(rule.matches("a/b/c.md", "a/*.md"))
        self.assertTrue(rule.matches("a/c.md", "a/*.md"))
        self.assertFalse(rule.matches("ab/c.md", "a/**"))


class Rule(unittest.TestCase):
    def test_one_manual_page_runs_no_test(self) -> None:
        plan = classify(m("docs/src/operate/troubleshooting.md"))
        self.assertTrue(plan.docs_only)
        self.assertEqual(plan.tests, [])
        self.assertEqual(plan.doc_packages, [])
        self.assertFalse(plan.cargo)

    def test_one_code_file_among_docs_runs_everything(self) -> None:
        plan = classify(m("docs/src/SUMMARY.md"), m("README.md"), m("libs/2-engine/src/ingest.rs"))
        self.assertFalse(plan.docs_only)
        self.assertTrue(plan.cargo)
        self.assertIn("libs/2-engine/src/ingest.rs", plan.reason)

    def test_the_empty_diff_runs_everything(self) -> None:
        self.assertFalse(classify().docs_only)

    def test_an_unknown_diff_runs_everything(self) -> None:
        self.assertFalse(rule.classify("pull_request", None, REPO).docs_only)

    def test_every_other_event_runs_everything(self) -> None:
        for event in ("push", "workflow_dispatch", "schedule", "pull_request_target", "merge_group"):
            with self.subTest(event):
                self.assertFalse(classify(m("docs/src/SUMMARY.md"), event=event).docs_only)

    def test_a_rename_inside_the_docs_runs_the_agent_docs_guard(self) -> None:
        plan = classify(Change("R", "docs/src/operate/new.md", "docs/src/operate/old.md"))
        self.assertTrue(plan.docs_only)
        self.assertEqual([t.filter for t in plan.tests], ["binary(agent_docs_paths)"])

    def test_a_deletion_runs_the_agent_docs_guard(self) -> None:
        plan = classify(Change("D", "docs/src/operate/old.md"))
        self.assertTrue(plan.docs_only)
        self.assertEqual([t.package for t in plan.tests], ["flusso-cli"])

    def test_a_copy_removes_nothing(self) -> None:
        plan = classify(Change("C", "docs/src/b.md", "docs/src/a.md"))
        self.assertTrue(plan.docs_only)
        self.assertEqual(plan.tests, [])

    def test_a_file_moved_from_docs_to_code_runs_everything(self) -> None:
        plan = classify(Change("R", "libs/2-script/tests/recipes/x.md", "docs/src/x.md"))
        self.assertFalse(plan.docs_only)

    def test_a_file_moved_from_code_to_docs_runs_everything(self) -> None:
        plan = classify(Change("R", "docs/src/x.rs", "libs/2-script/src/x.rs"))
        self.assertFalse(plan.docs_only)
        self.assertIn("libs/2-script/src/x.rs", plan.reason)

    def test_a_type_change_is_judged_by_its_path(self) -> None:
        self.assertTrue(classify(Change("T", "docs/src/x.md")).docs_only)

    def test_each_doc_test_runs_on_its_files(self) -> None:
        cases = {
            "plugin/skills/flusso-query/SKILL.md": ["binary(agent_docs_paths)"],
            ".claude/skills/coordinator/SKILL.md": ["binary(agent_docs_paths)"],
            "docs/src/reference/generated/sink-opensearch.md": ["test(committed_adapter_docs_are_current)"],
            "docs/src/operate/upgrade-to-0-18.md": [
                "binary(dates) & test(every_replacement_on_the_upgrade_page_runs)"
            ],
            "docs/src/query/document-structs.md": [rule.DOC_TESTS[3].filter],
            "docs/src/query/maps.md": [rule.DOC_TESTS[3].filter],
            "plugin/skills/flusso-query/maps.md": ["binary(agent_docs_paths)", rule.DOC_TESTS[3].filter],
            "CLAUDE.md": [],
            "docs/adr/0001-adapter-owned-configuration.md": [],
        }
        for path, filters in cases.items():
            with self.subTest(path):
                plan = classify(m(path))
                self.assertTrue(plan.docs_only)
                self.assertEqual([t.filter for t in plan.tests], filters)

    def test_a_compiled_readme_runs_its_crates_doctests_and_doc_links(self) -> None:
        plan = classify(m("libs/2-engine/README.md"), m("libs/2-script/README.md"), m("docs/src/SUMMARY.md"))
        self.assertTrue(plan.docs_only)
        self.assertEqual(plan.doc_packages, ["flusso-engine", "flusso-script"])
        self.assertTrue(plan.cargo)

    def test_a_landing_readme_compiles_nothing(self) -> None:
        for path in ("sdk/query/README.md", "apps/cli/README.md", "libs/README.md", "README.md"):
            with self.subTest(path):
                plan = classify(m(path))
                self.assertTrue(plan.docs_only)
                self.assertEqual(plan.doc_packages, [])

    def test_a_removed_crate_readme_runs_everything(self) -> None:
        for change in (Change("D", "libs/2-engine/README.md"), Change("R", "libs/2-engine/ABOUT.md", "libs/2-engine/README.md")):
            with self.subTest(change):
                self.assertFalse(classify(change).docs_only)

    def test_a_readme_renamed_into_a_crate_is_compiled_there(self) -> None:
        plan = classify(Change("R", "libs/2-engine/README.md", "docs/src/engine.md"))
        self.assertTrue(plan.docs_only)
        self.assertEqual(plan.doc_packages, ["flusso-engine"])

    def test_the_nextest_selection_names_each_package_once(self) -> None:
        plan = classify(m("plugin/x.md"), m("docs/src/reference/generated/a.md"), m("docs/src/query/maps.md"))
        self.assertEqual(plan.nextest_packages, ["flusso-cli", "flusso-query-derive"])
        self.assertEqual(
            plan.nextest_filter,
            "(package(flusso-cli) & binary(agent_docs_paths))"
            " | (package(flusso-cli) & test(committed_adapter_docs_are_current))"
            f" | (package(flusso-query-derive) & {rule.DOC_TESTS[3].filter})",
        )


class NameStatus(unittest.TestCase):
    def test_every_entry_kind(self) -> None:
        raw = b"M\0docs/a.md\0A\0b c.md\0D\0gone.md\0R087\0old.md\0new.md\0C100\0src.md\0copy.md\0T\0link.md\0"
        self.assertEqual(
            rule.parse_name_status(raw),
            [
                Change("M", "docs/a.md"),
                Change("A", "b c.md"),
                Change("D", "gone.md"),
                Change("R", "new.md", "old.md"),
                Change("C", "copy.md", "src.md"),
                Change("T", "link.md"),
            ],
        )

    def test_empty(self) -> None:
        self.assertEqual(rule.parse_name_status(b""), [])

    def test_malformed(self) -> None:
        for raw in (b"M\0", b"R100\0old.md\0", b"docs/a.md\0M\0", b"\0\0"):
            with self.subTest(raw):
                with self.assertRaises(ValueError):
                    rule.parse_name_status(raw)

    def test_real_git_output(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            git = lambda *a: subprocess.run(["git", "-C", tmp, *a], check=True, capture_output=True).stdout
            git("init", "-q")
            git("config", "user.email", "t@example.com")
            git("config", "user.name", "t")
            git("config", "commit.gpgsign", "false")
            root = pathlib.Path(tmp)
            (root / "docs").mkdir()
            (root / "docs" / "old page.md").write_text("one\ntwo\nthree\nfour\n")
            (root / "docs" / "gone.md").write_text("x\n")
            (root / "main.rs").write_text("fn main() {}\n")
            git("add", "-A")
            git("commit", "-qm", "base")
            git("mv", "docs/old page.md", "docs/new page.md")
            git("rm", "-q", "docs/gone.md")
            (root / "README.md").write_text("hi\n")
            git("add", "-A")
            git("commit", "-qm", "head")
            raw = git("diff", "--name-status", "-z", "-M", "HEAD^1", "HEAD")
            changes = rule.parse_name_status(raw)
            self.assertEqual(
                sorted(changes, key=lambda c: c.path),
                [Change("A", "README.md"), Change("D", "docs/gone.md"), Change("R", "docs/new page.md", "docs/old page.md")],
            )
            self.assertTrue(rule.classify("pull_request", changes, root).docs_only)


class Cli(unittest.TestCase):
    def run_cli(self, *args: str, changes: bytes | None = None) -> tuple[str, str]:
        with tempfile.TemporaryDirectory() as tmp:
            out, summary = pathlib.Path(tmp, "out"), pathlib.Path(tmp, "summary")
            extra = []
            if changes is not None:
                pathlib.Path(tmp, "changes").write_bytes(changes)
                extra = ["--changes", str(pathlib.Path(tmp, "changes"))]
            rule.main([*args, *extra, "--root", str(REPO), "--output", str(out), "--summary", str(summary)])
            return out.read_text(), summary.read_text()

    def test_docs_only_outputs_and_summary(self) -> None:
        out, summary = self.run_cli("--event", "pull_request", "--job", "test", changes=b"M\0plugin/x.md\0")
        self.assertIn("docs_only=true\n", out)
        self.assertIn("cargo=true\n", out)
        self.assertIn("nextest_packages=-p flusso-cli\n", out)
        self.assertIn("nextest_filter=(package(flusso-cli) & binary(agent_docs_paths))\n", out)
        self.assertIn("docs-only: skipped", summary)
        self.assertIn("full nextest run", summary)

    def test_each_job_says_what_it_skipped(self) -> None:
        for job, skipped in rule.SKIPPED.items():
            with self.subTest(job):
                _, summary = self.run_cli("--event", "pull_request", "--job", job, changes=b"M\0docs/a.md\0")
                self.assertIn(f"docs-only: skipped** {skipped}", summary)

    def test_full_outputs(self) -> None:
        out, summary = self.run_cli("--event", "pull_request", "--job", "fuzz", changes=b"M\0Cargo.toml\0")
        self.assertIn("docs_only=false\n", out)
        self.assertIn("cargo=true\n", out)
        self.assertIn("Full suite", summary)

    def test_an_unreadable_change_set_runs_everything(self) -> None:
        out, _ = self.run_cli("--event", "pull_request", "--job", "test", changes=b"garbage")
        self.assertIn("docs_only=false\n", out)
        out, _ = self.run_cli("--event", "pull_request", "--job", "test", "--unknown")
        self.assertIn("docs_only=false\n", out)
        out, _ = self.run_cli("--event", "pull_request", "--job", "test")
        self.assertIn("docs_only=false\n", out)


def tracked(*patterns: str) -> list[str]:
    return subprocess.run(
        ["git", "-C", str(REPO), "ls-files", "--", *patterns], check=True, capture_output=True, text=True
    ).stdout.splitlines()


def packages() -> dict[str, str]:
    """Package name → its directory, for every Cargo.toml in the checkout."""
    found = {}
    for manifest in tracked("*Cargo.toml"):
        text = (REPO / manifest).read_text()
        name = re.search(r'(?m)^\[package\][^\[]*?^name\s*=\s*"([^"]+)"', text)
        if name:
            found[name.group(1)] = str(pathlib.PurePosixPath(manifest).parent)
    return found


# String literals in Rust code that resolve to a documentation file but are no read of it.
NOT_READS = {
    ("apps/design/src/commit/journal.rs", "README.md"),  # a file name a save journal may hold
}


class Repo(unittest.TestCase):
    """The rule against this checkout: what reads docs today is what DOC_TESTS runs."""

    def crate_of(self, path: str, crates: dict[str, str]) -> str | None:
        best = None
        for name, directory in crates.items():
            if path.startswith(directory + "/") and (best is None or len(directory) > len(crates[best])):
                best = name
        return best

    def test_every_documentation_file_rust_reads_is_covered(self) -> None:
        crates = packages()
        uncovered = []
        for source in tracked("*.rs"):
            code = "\n".join(
                line for line in (REPO / source).read_text(errors="replace").splitlines()
                if not line.lstrip().startswith("//")
            )
            crate = self.crate_of(source, crates)
            for literal in re.findall(r'"([^"\n]*)"', code):
                if not (literal.endswith(".md") or re.search(r"(^|/)(docs|plugin|\.claude)(/|$)", literal)):
                    continue
                if (source, literal) in NOT_READS:
                    continue
                bases = [pathlib.PurePosixPath(source).parent, pathlib.PurePosixPath(crates.get(crate, ".")), pathlib.PurePosixPath(".")]
                for base in bases:
                    resolved = os.path.normpath(str(base / literal))
                    if resolved.startswith("..") or "/" not in resolved or not (REPO / resolved).exists():
                        continue
                    probe = resolved + "/x.md" if (REPO / resolved).is_dir() else resolved
                    if not rule.is_docs(probe):
                        break
                    if rule.compiled_readme(REPO, resolved) == crate:
                        break
                    if any(
                        t.package == crate and any(rule.matches(probe, r) for r in t.reads)
                        for t in rule.DOC_TESTS
                    ):
                        break
                    uncovered.append(f"{source}: {literal!r} reads {resolved}")
                    break
        self.assertEqual(uncovered, [], "add a DocTest to change_class.py for each, or list it in NOT_READS")

    def test_every_compiled_readme_is_found(self) -> None:
        for lib in tracked("*src/lib.rs"):
            if 'include_str!("../README.md")' in (REPO / lib).read_text():
                readme = str(pathlib.PurePosixPath(lib).parent.parent / "README.md")
                with self.subTest(readme):
                    self.assertTrue(rule.is_docs(readme))
                    self.assertIsNotNone(rule.compiled_readme(REPO, readme))

    def test_every_doc_test_names_what_exists(self) -> None:
        crates = packages()
        for test in rule.DOC_TESTS:
            with self.subTest(test.what):
                self.assertIn(test.package, crates)
                files = tracked(*(r.replace("**", "*") for r in test.reads))
                for read in test.reads:
                    self.assertTrue(any(rule.matches(f, read) for f in files), f"{read} matches no file")
                    self.assertTrue(rule.is_docs(read.replace("**", "x.md")), f"{read} is not documentation")
                directory = REPO / crates[test.package]
                for binary in re.findall(r"binary\(([\w-]+)\)", test.filter):
                    self.assertTrue((directory / "tests" / f"{binary}.rs").is_file(), binary)
                sources = "\n".join(p.read_text() for p in directory.rglob("*.rs") if "target" not in p.parts)
                for name in re.findall(r"test\(=?([\w:]+)\)", test.filter):
                    self.assertRegex(sources, rf"fn {name.split('::')[-1]}\(", name)

    def test_every_gated_job_has_a_summary(self) -> None:
        workflows = "\n".join((REPO / ".github/workflows" / f).read_text() for f in ("ci.yml", "bench.yml"))
        jobs = set(re.findall(r"job: ([\w-]+)", workflows))
        self.assertEqual(jobs, set(rule.SKIPPED))


if __name__ == "__main__":
    unittest.main()
