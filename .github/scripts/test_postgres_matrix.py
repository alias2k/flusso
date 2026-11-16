"""Tests for postgres-matrix.py: a leg's summary from nextest JUnit, and the one nightly issue.

    python3 -m unittest discover -s .github/scripts -p 'test_postgres_matrix.py'

The JUnit fixtures follow what nextest 0.9's `ci` profile writes with `store-success-output`: a
`testcase` per test with `system-out`/`system-err`, `failure` (plus `rerunFailure`s) for a test that
failed every try, `flakyFailure` for one that passed on a retry. `gh` is a fake on `PATH` that records
its calls.
"""
import json
import os
import pathlib
import stat
import subprocess
import sys
import tempfile
import unittest

HERE = pathlib.Path(__file__).resolve().parent
SCRIPT = HERE / "postgres-matrix.py"
TITLE = "Postgres matrix fails on a supported major"
RUN = "https://github.com/alias2k/flusso/actions/runs/1"
PINNED_LINE = ("test-postgres: starting postgres:{image} instead of the run's postgres:18-alpine "
               "(FLUSSO_TEST_PG_TAG ignored): why")


def case(classname: str, name: str, body: str = "", err: str = "") -> str:
    return (f'<testcase name="{name}" classname="{classname}" time="1.0">{body}'
            f"<system-out></system-out><system-err>{err}</system-err></testcase>")


def junit(*cases: str) -> str:
    return ('<?xml version="1.0" encoding="UTF-8"?><testsuites name="nextest-run">'
            f'<testsuite name="s">{"".join(cases)}</testsuite></testsuites>')


GREEN = junit(
    case("flusso-source-postgres::integration", "a_small_batch_reuses_its_plan"),
    case("flusso-engine::bytea_output", "a_bytea_reads_as_hex", err=PINNED_LINE.format(image="18-alpine")),
    case("flusso-source-postgres::integration", "jit_refused", err=PINNED_LINE.format(image="10-alpine")),
    case("flusso-engine::wal", "flaky_one", body='<flakyFailure message="x" type="test failure">t</flakyFailure>'),
)
RED = junit(
    case("flusso-source-postgres::integration", "a_small_batch_reuses_its_plan",
         body='<failure message="planned 1200 times" type="test failure">p</failure>'
              '<rerunFailure message="x" type="test failure">p</rerunFailure>'),
    case("flusso-engine::wal", "passes"),
)


class Gh:
    """A fake `gh` on `PATH` that records its calls and answers `issue list` with `open_issues`."""

    def __init__(self, tmp: pathlib.Path, open_issues: list[int]):
        self.calls = tmp / "gh-calls.jsonl"
        listed = json.dumps([{"number": n, "title": TITLE} for n in open_issues]
                            + [{"number": 999, "title": TITLE + " (not this one)"}])
        script = tmp / "gh"
        script.write_text(
            f"#!{sys.executable}\nimport json, sys\n"
            f"open({str(self.calls)!r}, 'a').write(json.dumps(sys.argv[1:]) + '\\n')\n"
            f"if sys.argv[1:3] == ['issue', 'list']:\n    print({listed!r})\n"
        )
        script.chmod(script.stat().st_mode | stat.S_IEXEC)
        self.path = f"{tmp}:{os.environ['PATH']}"

    def recorded(self) -> list[list[str]]:
        if not self.calls.exists():
            return []
        return [json.loads(line) for line in self.calls.read_text().splitlines()]


def script(*args: str, env: dict | None = None) -> subprocess.CompletedProcess:
    return subprocess.run([sys.executable, "-I", str(SCRIPT), *args], capture_output=True, text=True,
                          env=env or dict(os.environ))


def summarize(tmp: pathlib.Path, text: str | None, tag: str, outcome: str) -> tuple[str, dict]:
    report = tmp / f"junit-{tag}.xml"
    if text is not None:
        report.write_text(text, encoding="utf-8")
    out = tmp / "results" / f"{tag}.json"
    out.parent.mkdir(exist_ok=True)
    done = script("summary", str(report), "--tag", tag, "--outcome", outcome, "--out", str(out))
    assert done.returncode == 0, done.stderr
    return done.stdout, json.loads(out.read_text(encoding="utf-8"))


def maintain(tmp: pathlib.Path, open_issues: list[int], *extra: str,
             tags: str = '["14-alpine","18-alpine"]') -> tuple[subprocess.CompletedProcess, list[list[str]]]:
    gh = Gh(tmp, open_issues)
    env = dict(os.environ) | {"PATH": gh.path}
    done = script("issue", str(tmp / "results"), "--tags", tags, "--run-url", RUN, *extra, env=env)
    return done, gh.recorded()


class Summary(unittest.TestCase):
    def test_a_green_leg_names_its_pinned_tests_and_retries(self):
        with tempfile.TemporaryDirectory() as tmp:
            text, result = summarize(pathlib.Path(tmp), GREEN, "18-alpine", "success")
        self.assertTrue(result["ok"])
        self.assertEqual(result["passed"], 4)
        self.assertIn("Postgres `18-alpine`: passed", text)
        self.assertIn("`flusso-engine::bytea_output a_bytea_reads_as_hex` on `postgres:18-alpine`", text)
        self.assertIn("`flusso-source-postgres::integration jit_refused` on `postgres:10-alpine`", text)
        self.assertIn("Passed on a retry", text)
        self.assertIn("flaky_one", text)

    def test_a_failing_test_fails_the_leg_and_is_listed(self):
        with tempfile.TemporaryDirectory() as tmp:
            text, result = summarize(pathlib.Path(tmp), RED, "14-alpine", "failure")
        self.assertFalse(result["ok"])
        self.assertEqual(result["failed"], ["flusso-source-postgres::integration a_small_batch_reuses_its_plan"])
        self.assertIn("1 passed, 1 failed", text)
        self.assertIn("- none", text)

    def test_no_report_is_a_failed_leg(self):
        with tempfile.TemporaryDirectory() as tmp:
            text, result = summarize(pathlib.Path(tmp), None, "14-alpine", "failure")
            _, garbled = summarize(pathlib.Path(tmp), "<testsuites", "18-alpine", "failure")
        self.assertFalse(result["ok"])
        self.assertFalse(result["report"])
        self.assertIn("No test report", text)
        self.assertFalse(garbled["ok"])

    def test_a_step_that_failed_with_every_test_green_still_fails_the_leg(self):
        with tempfile.TemporaryDirectory() as tmp:
            _, result = summarize(pathlib.Path(tmp), GREEN, "18-alpine", "failure")
        self.assertFalse(result["ok"])


class Issue(unittest.TestCase):
    def test_a_failure_with_no_open_issue_creates_one(self):
        with tempfile.TemporaryDirectory() as tmp:
            tmp = pathlib.Path(tmp)
            summarize(tmp, GREEN, "14-alpine", "success")
            summarize(tmp, RED, "18-alpine", "failure")
            done, calls = maintain(tmp, [])
        self.assertEqual(done.returncode, 0, done.stderr)
        self.assertEqual(calls[-1][:6], ["issue", "create", "--title", TITLE, "--label", "area: ci-release"])
        body = calls[-1][-1]
        self.assertIn(RUN, body)
        self.assertIn("## Postgres `18-alpine`", body)
        self.assertIn("a_small_batch_reuses_its_plan", body)
        self.assertNotIn("## Postgres `14-alpine`", body)
        self.assertIn("Passed: `14-alpine`.", body)

    def test_a_failure_with_an_open_issue_updates_it(self):
        with tempfile.TemporaryDirectory() as tmp:
            tmp = pathlib.Path(tmp)
            summarize(tmp, RED, "14-alpine", "failure")
            summarize(tmp, RED, "18-alpine", "failure")
            _, calls = maintain(tmp, [41])
        self.assertEqual(calls[-1][:4], ["issue", "edit", "41", "--body"])
        self.assertEqual(len([c for c in calls if c[:2] == ["issue", "create"]]), 0)

    def test_every_leg_passing_closes_the_open_issue(self):
        with tempfile.TemporaryDirectory() as tmp:
            tmp = pathlib.Path(tmp)
            summarize(tmp, GREEN, "14-alpine", "success")
            summarize(tmp, GREEN, "18-alpine", "success")
            _, calls = maintain(tmp, [41])
        self.assertEqual(calls[-1][:3], ["issue", "close", "41"])
        self.assertIn(RUN, calls[-1][-1])

    def test_every_leg_passing_with_no_issue_does_nothing(self):
        with tempfile.TemporaryDirectory() as tmp:
            tmp = pathlib.Path(tmp)
            summarize(tmp, GREEN, "14-alpine", "success")
            summarize(tmp, GREEN, "18-alpine", "success")
            _, calls = maintain(tmp, [])
        self.assertEqual([c[:2] for c in calls], [["issue", "list"]])

    def test_a_leg_with_no_result_counts_as_failed(self):
        with tempfile.TemporaryDirectory() as tmp:
            tmp = pathlib.Path(tmp)
            summarize(tmp, GREEN, "14-alpine", "success")
            done, calls = maintain(tmp, [])
        self.assertIn("18-alpine: failed", done.stdout)
        self.assertIn("outcome `missing`", calls[-1][-1])

    def test_a_dry_run_lists_but_writes_nothing(self):
        with tempfile.TemporaryDirectory() as tmp:
            tmp = pathlib.Path(tmp)
            summarize(tmp, RED, "18-alpine", "failure")
            done, calls = maintain(tmp, [], "--dry-run")
        self.assertEqual([c[:2] for c in calls], [["issue", "list"]])
        self.assertIn("would run: gh issue create", done.stdout)


if __name__ == "__main__":
    unittest.main()
