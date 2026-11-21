"""Tests for test-parts.py, the split of `clippy + tests` into parts and its guards (issue #444).

    python3 -m unittest discover -s .github/scripts -p 'test_*.py'

The `check` cases run the script over a fake `cargo` named by `$CARGO` that answers each
`nextest list -E <filter>` from a table, so no toolchain is needed. The `Repo` cases read this
checkout's `.config/nextest.toml` and `ci.yml`.
"""
import importlib.util
import json
import os
import pathlib
import stat
import subprocess
import sys
import tempfile
import textwrap
import unittest

HERE = pathlib.Path(__file__).resolve().parent
SCRIPT = HERE / "test-parts.py"
ROOT = HERE.parents[1]

spec = importlib.util.spec_from_file_location("test_parts", SCRIPT)
parts = importlib.util.module_from_spec(spec)
spec.loader.exec_module(parts)

NEXTEST_TOML = textwrap.dedent(
    """\
    [test-groups.opensearch]
    max-threads = 1

    [[profile.default.overrides]]
    filter = 'package(a) & binary(os_one) | package(a) & binary(os_two)'
    test-group = 'opensearch'

    [[profile.default.overrides]]
    filter = 'package(a) & binary(pg)'
    test-group = 'docker'

    [[profile.ci.overrides]]
    filter = 'package(a) & binary(os_one) | package(a) & binary(os_two)'
    test-group = 'opensearch'
    """
)

PARTS = (
    parts.Part("os-1", "OpenSearch 1", ("a::os_one",), group="opensearch"),
    parts.Part("os-2", "OpenSearch 2", ("a::os_two",), group="opensearch"),
    parts.Part("slow", "slow", ("a::slow",)),
    parts.Part("rest", "rest", rest=True),
)


def listing(tests: dict[str, list[str]], unmatched: dict[str, list[str]] | None = None) -> str:
    """A `nextest list --message-format json` answer selecting `tests` (binary -> names)."""
    suites = {}
    for binary, names in tests.items():
        suites.setdefault(binary, {"testcases": {}})
        for name in names:
            suites[binary]["testcases"][name] = {"kind": "test", "ignored": False, "filter-match": {"status": "matches"}}
    for binary, names in (unmatched or {}).items():
        suites.setdefault(binary, {"testcases": {}})
        for name in names:
            suites[binary]["testcases"][name] = {
                "kind": "test",
                "ignored": False,
                "filter-match": {"status": "mismatch", "reasons": ["expression"]},
            }
    suites["a::skipped"] = {"status": "skipped"}
    return json.dumps({"rust-build-meta": {}, "test-count": 0, "rust-suites": suites})


class Filters(unittest.TestCase):
    def test_a_named_part_runs_exactly_its_binaries(self) -> None:
        self.assertEqual(parts.part_filter(PARTS[0], PARTS, NEXTEST_TOML), "binary_id(=a::os_one)")

    def test_rest_leaves_out_every_named_binary_and_every_divided_group(self) -> None:
        rest = parts.part_filter(PARTS[3], PARTS, NEXTEST_TOML)
        self.assertEqual(
            rest,
            "not ((binary_id(=a::os_one)) | (binary_id(=a::os_two)) | (binary_id(=a::slow))"
            " | ((package(a) & binary(os_one) | package(a) & binary(os_two))))",
        )

    def test_a_group_no_override_names_is_an_error(self) -> None:
        with self.assertRaises(ValueError):
            parts.group_filter(NEXTEST_TOML, "boundary")

    def test_the_read_side_scope_is_anded_to_every_part(self) -> None:
        for entry in parts.matrix(PARTS, NEXTEST_TOML, read_side_only=True):
            self.assertTrue(entry["filter"].startswith(f"({parts.READ_SIDE_SCOPE}) & ("), entry)
        for entry in parts.matrix(PARTS, NEXTEST_TOML, read_side_only=False):
            self.assertNotIn(parts.READ_SIDE_SCOPE, entry["filter"])


class Problems(unittest.TestCase):
    WHOLE = {"a::os_one t", "a::os_two t", "a::slow t", "a lib_test", "a::pg t"}

    def test_a_partition_passes(self) -> None:
        each = {"os-1": {"a::os_one t"}, "os-2": {"a::os_two t"}, "slow": {"a::slow t"}, "rest": {"a lib_test", "a::pg t"}}
        self.assertEqual(parts.problems(self.WHOLE, each, PARTS, scoped_run=False), [])

    def test_a_test_in_no_part_says_where_to_add_its_binary(self) -> None:
        each = {"os-1": {"a::os_one t"}, "os-2": set(), "slow": {"a::slow t"}, "rest": {"a lib_test", "a::pg t"}}
        found = parts.problems(self.WHOLE, each, PARTS, scoped_run=False)
        self.assertEqual(len(found), 1, found)
        self.assertIn("`a::os_two`: 1 test(s) in no part", found[0])
        self.assertIn("opensearch", found[0])
        self.assertIn("test-parts.py (PARTS)", found[0])

    def test_a_test_in_two_parts_is_named(self) -> None:
        each = {
            "os-1": {"a::os_one t"},
            "os-2": {"a::os_two t", "a::slow t"},
            "slow": {"a::slow t"},
            "rest": {"a lib_test", "a::pg t"},
        }
        found = parts.problems(self.WHOLE, each, PARTS, scoped_run=False)
        self.assertEqual(found, ["`a::slow t` is in 2 parts (os-2, slow): name its binary in one."])

    def test_a_test_outside_the_whole_run_is_named(self) -> None:
        each = {"os-1": {"a::os_one t"}, "os-2": {"a::os_two t"}, "slow": {"a::slow t"}, "rest": {"a lib_test", "a::pg t", "b x"}}
        self.assertEqual(
            parts.problems(self.WHOLE, each, PARTS, scoped_run=False),
            ["`b x` is in part rest but not in the whole run."],
        )

    def test_a_named_binary_with_no_test_is_stale_on_a_full_run_only(self) -> None:
        whole = self.WHOLE - {"a::slow t"}
        each = {"os-1": {"a::os_one t"}, "os-2": {"a::os_two t"}, "slow": set(), "rest": {"a lib_test", "a::pg t"}}
        found = parts.problems(whole, each, PARTS, scoped_run=False)
        self.assertEqual(len(found), 1, found)
        self.assertIn("part slow names `a::slow`, which has no test", found[0])
        self.assertEqual(parts.problems(whole, each, PARTS, scoped_run=True), [])


class Listed(unittest.TestCase):
    def test_only_matching_tests_count_and_skipped_binaries_have_none(self) -> None:
        raw = listing({"a::pg": ["one", "two"], "a": ["unit"]}, unmatched={"a::pg": ["three"]})
        self.assertEqual(parts.listed(raw), {"a::pg one", "a::pg two", "a unit"})


class Verdict(unittest.TestCase):
    def needs(self, plan="success", docs_only="false", build="success", lint="success", tests="success") -> dict:
        return {
            "plan": {"result": plan, "outputs": {"docs_only": docs_only}},
            "build": {"result": build, "outputs": {}},
            "lint": {"result": lint, "outputs": {}},
            "tests": {"result": tests, "outputs": {}},
        }

    JUNIT = {p.id: 2 for p in parts.PARTS}
    EXPECTED = 2 * len(parts.PARTS)

    def test_every_job_green_and_every_test_reported_is_green(self) -> None:
        self.assertEqual(parts.verdict(self.needs(), self.EXPECTED, self.JUNIT), [])

    def test_a_failed_or_cancelled_part_is_red(self) -> None:
        for result in ("failure", "cancelled", "skipped"):
            with self.subTest(result):
                found = parts.verdict(self.needs(tests=result), self.EXPECTED, self.JUNIT)
                self.assertEqual(found, [f"the `tests` job's result is `{result}`"])

    def test_a_failed_plan_is_red_whatever_else(self) -> None:
        self.assertEqual(
            parts.verdict(self.needs(plan="failure"), self.EXPECTED, self.JUNIT),
            ["the plan job's result is `failure`"],
        )

    def test_a_docs_only_run_may_skip_lint_and_tests_but_not_build(self) -> None:
        skipped = self.needs(docs_only="true", lint="skipped", tests="skipped")
        self.assertEqual(parts.verdict(skipped, None, {}), [])
        self.assertEqual(
            parts.verdict({**skipped, "build": {"result": "skipped"}}, None, {}),
            ["the `build` job's result is `skipped`"],
        )
        self.assertEqual(
            parts.verdict(self.needs(docs_only="true", tests="failure"), None, {}),
            ["the `tests` job's result is `failure`"],
        )

    def test_fewer_tests_reported_than_listed_is_red(self) -> None:
        found = parts.verdict(self.needs(), self.EXPECTED + 1, self.JUNIT)
        self.assertEqual(found, [f"the parts ran {self.EXPECTED} tests, the whole run lists {self.EXPECTED + 1}"])

    def test_a_part_without_a_report_counts_none_of_its_tests(self) -> None:
        junit = dict(self.JUNIT)
        del junit["boundary"]
        self.assertEqual(parts.verdict(self.needs(), self.EXPECTED - 2, junit), [])
        found = parts.verdict(self.needs(), self.EXPECTED, junit)
        self.assertEqual(found, [f"the parts ran {self.EXPECTED - 2} tests, the whole run lists {self.EXPECTED}"])


class VerdictCli(unittest.TestCase):
    """`verdict` end to end over JUnit reports laid out as download-artifact writes them."""

    NEEDS = {
        "plan": {"result": "success", "outputs": {"docs_only": "false"}},
        "build": {"result": "success", "outputs": {"count": "10"}},
        "lint": {"result": "success", "outputs": {}},
        "doctests": {"result": "success", "outputs": {}},
        "tests": {"result": "success", "outputs": {}},
    }

    def run_verdict(self, reports: dict[str, str]) -> subprocess.CompletedProcess:
        with tempfile.TemporaryDirectory() as tmp:
            for part, text in reports.items():
                folder = pathlib.Path(tmp) / f"nextest-junit-{part}"
                folder.mkdir()
                (folder / "junit.xml").write_text(text)
            return subprocess.run(
                [sys.executable, str(SCRIPT), "verdict", "--needs", json.dumps(self.NEEDS), "--expected", "10", "--junit", tmp],
                capture_output=True,
                text=True,
            )

    @staticmethod
    def report(tests: int) -> str:
        return f'<?xml version="1.0"?><testsuites name="nextest-run" tests="{tests}"></testsuites>'

    def test_reports_holding_the_listed_count_are_green(self) -> None:
        reports = {p.id: self.report(2) for p in parts.PARTS}
        result = self.run_verdict(reports)
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_a_truncated_report_is_an_error_line_not_a_traceback(self) -> None:
        reports = {p.id: self.report(2) for p in parts.PARTS}
        reports["rest"] = self.report(2)[:40]
        result = self.run_verdict(reports)
        self.assertEqual(result.returncode, 1)
        self.assertNotIn("Traceback", result.stderr)
        self.assertIn("::error::the JUnit report of part rest can't be read", result.stderr)
        self.assertIn("::error::the parts ran 8 tests, the whole run lists 10", result.stderr)


class Cli(unittest.TestCase):
    """The script end to end over a fake `cargo` that answers `nextest list` from a table."""

    def run_check(self, answers: dict[str, str], *args: str) -> tuple[subprocess.CompletedProcess, str]:
        with tempfile.TemporaryDirectory() as tmp:
            tmp = pathlib.Path(tmp)
            table = tmp / "answers.json"
            table.write_text(json.dumps(answers))
            cargo = tmp / "cargo"
            cargo.write_text(
                textwrap.dedent(
                    f"""\
                    #!{sys.executable}
                    import json, sys
                    args = sys.argv[1:]
                    assert args[:2] == ["nextest", "list"], args
                    assert "--run-ignored" in args and "--profile" in args, args
                    key = args[args.index("-E") + 1] if "-E" in args else ""
                    answers = json.load(open({str(table)!r}))
                    if key not in answers:
                        sys.stderr.write("no answer for " + repr(key))
                        sys.exit(3)
                    print(answers[key])
                    """
                )
            )
            cargo.chmod(cargo.stat().st_mode | stat.S_IEXEC)
            out = tmp / "output"
            return subprocess.run(
                [sys.executable, str(SCRIPT), "check", "--output", str(out), *args, "--", "--workspace"],
                capture_output=True,
                text=True,
                env={**os.environ, "CARGO": str(cargo)},
            ), out.read_text() if out.exists() else ""

    def answers(self, drop: str | None = None) -> dict[str, str]:
        whole: dict[str, list[str]] = {"x": ["unit"]}
        answers = {}
        for entry in parts.matrix(parts.PARTS, parts.NEXTEST_TOML.read_text(), read_side_only=False):
            part = next(p for p in parts.PARTS if p.id == entry["id"])
            tests = {b: ["t"] for b in part.binaries if b != drop} if part.binaries else {"x": ["unit"]}
            for binary in part.binaries:
                whole[binary] = ["t"]
            answers[entry["filter"]] = listing(tests)
        answers[""] = listing(whole)
        return answers

    def test_a_partition_passes_and_writes_the_count(self) -> None:
        result, output = self.run_check(self.answers())
        self.assertEqual(result.returncode, 0, result.stderr)
        expected = 1 + sum(len(p.binaries) for p in parts.PARTS)
        self.assertEqual(output, f"count={expected}\n")

    def test_a_test_in_no_part_fails(self) -> None:
        result, _ = self.run_check(self.answers(drop="flusso-cli::shutdown"))
        self.assertEqual(result.returncode, 1)
        self.assertIn("`flusso-cli::shutdown`: 1 test(s) in no part", result.stderr)

    def test_nextest_failing_is_exit_2(self) -> None:
        answers = self.answers()
        del answers[""]
        result, _ = self.run_check(answers)
        self.assertEqual(result.returncode, 2, result.stderr)


class Repo(unittest.TestCase):
    """The parts against this checkout's nextest config and workflow."""

    def test_every_part_id_is_unique_and_one_part_is_the_rest(self) -> None:
        ids = [p.id for p in parts.PARTS]
        self.assertEqual(len(ids), len(set(ids)))
        self.assertEqual([p.id for p in parts.PARTS if p.rest], ["rest"])

    def test_every_opensearch_binary_of_the_nextest_config_is_in_exactly_one_part(self) -> None:
        config = parts.NEXTEST_TOML.read_text()
        group = parts.group_filter(config, "opensearch")
        named = [b for p in parts.PARTS if p.group == "opensearch" for b in p.binaries]
        self.assertEqual(len(named), len(set(named)), "a binary is named in two parts")
        in_config = set()
        for term in group.replace("(", "").replace(")", "").split("|"):
            package = term.split("package")[1].split("&")[0].strip()
            binary = term.split("binary")[1].strip()
            in_config.add(f"{package}::{binary}")
        self.assertEqual(set(named), in_config)

    def test_the_workflow_reports_every_part_s_junit_under_the_name_verdict_reads(self) -> None:
        workflow = (ROOT / ".github" / "workflows" / "ci.yml").read_text()
        for needed in ("name: nextest-junit-${{ matrix.part.id }}", "name: clippy + tests", "test-parts.py verdict"):
            self.assertTrue(needed in workflow, f"ci.yml lacks {needed!r}")
        # A re-run part replaces its report, so the verdict reads the latest attempt's.
        upload = workflow.split("name: nextest-junit-${{ matrix.part.id }}", 1)[1].split("\n      - ", 1)[0]
        self.assertIn("overwrite: true", upload)

    def test_only_the_parts_that_run_cargo_fetch_the_crates(self) -> None:
        for part in parts.PARTS:
            self.assertEqual(part.cargo, part.group != "opensearch", part.id)


if __name__ == "__main__":
    unittest.main()
