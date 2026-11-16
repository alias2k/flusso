"""Tests for review-ab's two review modes: the document digest and the plan check.

    python3 -m unittest discover -s .github/scripts -p 'test_*.py'

`review-ab-digest.py` and `review-ab-plans.py` run as subprocesses over files written to a
temporary directory, as `review-ab.sh` calls them; `review-ab.sh --digest` / `--plans` run end to
end over the fake cargo, docker, nproc and taskset of `test_review_ab.py`, with a fake `cases`
executable that writes what its review mode would.
"""
import json
import os
import pathlib
import subprocess
import sys
import tempfile
import unittest

from test_review_ab import FAKE_CARGO, FAKE_NPROC, FAKE_TASKSET, WITH_URL

HERE = pathlib.Path(__file__).resolve().parent
DIGEST = HERE / "review-ab-digest.py"
PLANS = HERE / "review-ab-plans.py"
REVIEW = HERE / "review-ab.sh"


def run(*args: object) -> subprocess.CompletedProcess:
    return subprocess.run([sys.executable, *map(str, args)], capture_output=True, text=True, check=False)


def doc(index: str, form: str, doc_id: str, body: dict | None) -> str:
    if body is None:
        return f"{index}\t{form}\t{doc_id}\tdelete\t"
    return f"{index}\t{form}\t{doc_id}\tupsert\t{json.dumps(body, sort_keys=True, separators=(',', ':'))}"


def documents(name: str = "k_int", **override: dict) -> str:
    lines = []
    for form in ("build", "build_many<16", "build_many>=16", "backfill"):
        for n in (1, 2, 3):
            body = override.get(f"{form}:{n}", {"id": n, "name": f"name {n}"})
            lines.append(doc(name, form, str(n), body))
    return "\n".join(lines) + "\n"


class Digest(unittest.TestCase):
    def setUp(self) -> None:
        tmp = tempfile.TemporaryDirectory()
        self.addCleanup(tmp.cleanup)
        self.dir = pathlib.Path(tmp.name)

    def compare(self, base: str, head: str) -> subprocess.CompletedProcess:
        (self.dir / "base.tsv").write_text(base)
        (self.dir / "head.tsv").write_text(head)
        return run(DIGEST, self.dir / "base.tsv", self.dir / "head.tsv")

    def test_the_same_documents_pass_with_one_row_per_index(self) -> None:
        both = documents() + documents("plain")
        result = self.compare(both, both)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        rows = [line for line in result.stdout.splitlines() if line.startswith("| `")]
        self.assertEqual(len(rows), 2)
        self.assertTrue(all(row.endswith("| same |") for row in rows), rows)

    def test_the_digest_does_not_depend_on_line_order(self) -> None:
        lines = documents().splitlines()
        result = self.compare("\n".join(lines) + "\n", "\n".join(reversed(lines)) + "\n")
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)

    def test_a_different_body_is_flagged_with_its_id_and_a_diff(self) -> None:
        result = self.compare(documents(), documents(**{"build_many>=16:2": {"id": 2, "name": "other"}}))
        self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
        self.assertIn("differ ⚠️", result.stdout)
        self.assertIn("- `2` (build_many>=16): both upsert, the bodies differ", result.stdout)
        self.assertIn('-  "name": "name 2"', result.stdout)
        self.assertIn('+  "name": "other"', result.stdout)

    def test_an_upsert_turned_tombstone_is_named(self) -> None:
        result = self.compare(documents(), documents(**{"backfill:3": None}))
        self.assertEqual(result.returncode, 1)
        self.assertIn("- `3` (backfill): base upsert, head delete", result.stdout)

    def test_forms_that_disagree_on_one_side_are_flagged_even_when_both_sides_agree(self) -> None:
        both = documents(**{"build:1": {"id": 1, "name": "stale"}})
        result = self.compare(both, both)
        self.assertEqual(result.returncode, 1, result.stdout)
        self.assertIn("forms disagree on base ⚠️", result.stdout)
        self.assertIn("forms disagree on head ⚠️", result.stdout)
        self.assertNotIn("differ ⚠️", result.stdout)

    def test_an_index_on_one_side_only_is_flagged(self) -> None:
        result = self.compare(documents() + documents("plain"), documents())
        self.assertEqual(result.returncode, 1)
        self.assertIn("| `plain` | 12 | 0 |", result.stdout)
        self.assertIn("one side only ⚠️", result.stdout)

    def test_nothing_built_is_an_error(self) -> None:
        result = self.compare("", "")
        self.assertEqual(result.returncode, 2)
        self.assertIn("neither side built a document", result.stderr)

    def test_a_malformed_line_is_an_error(self) -> None:
        result = self.compare("k_int\tbuild\t1\n", documents())
        self.assertEqual(result.returncode, 2)
        self.assertIn("expected 5 tab-separated fields", result.stderr)


def logged(plan: dict, query: str) -> str:
    """One auto_explain entry as Postgres writes it to stderr: a prefix line, then tab-indented JSON."""
    body = json.dumps({"Query Text": query, "Plan": plan}, indent=2)
    return "2026-11-16 06:47:43.266 UTC [99] LOG:  duration: 0.084 ms  plan:\n" + "".join(
        f"\t{line}\n" for line in body.splitlines()
    )


def marker(run_name: str, case: str) -> str:
    return logged({"Node Type": "Result"}, f"SELECT 'flusso-review-case {run_name} {case}'")


def index_scan(table: str) -> dict:
    return {"Node Type": "Index Scan", "Relation Name": table, "Index Name": f"{table}_pkey"}


def partitioned(left: int, removed: int, node: str = "Seq Scan") -> dict:
    append = {"Node Type": "Append", "Plans": [{"Node Type": node, "Relation Name": f"s_hash_{n}"} for n in range(left)]}
    if removed:
        append["Subplans Removed"] = removed
    return {"Node Type": "Result", "Plans": [append]}


class Plans(unittest.TestCase):
    def setUp(self) -> None:
        tmp = tempfile.TemporaryDirectory()
        self.addCleanup(tmp.cleanup)
        self.dir = pathlib.Path(tmp.name)
        self.log: list[str] = []

    def case(self, side: str, mode: str, name: str, plan: dict, per_call: tuple[int, int] = (10, 0), partitions: int = 0, statements: int = 1) -> None:
        relations = ",".join(f"s_hash_{n}" for n in range(4)) if partitions else name.split("/")[1]
        calls2, plans2 = per_call
        with (self.dir / f"plans-{side}-{mode}.tsv").open("a") as tsv:
            tsv.write(f"{name}\t{relations}\t{partitions}\t{statements}\t20\t{6 + plans2}\t{calls2}\t{plans2}\n")
        self.log.append(marker(f"{side}-{mode}", name))
        self.log.append(logged(plan, "SELECT … FROM root"))
        self.log.append(marker(f"{side}-{mode}", "-"))

    def both(self, name: str, base: dict, head: dict, **kwargs) -> None:
        for mode in ("auto", "force_generic_plan"):
            self.case("base", mode, name, base, **kwargs)
            self.case("head", mode, name, head, **kwargs)

    def check(self) -> subprocess.CompletedProcess:
        (self.dir / "server.log").write_text("".join(self.log))
        return run(PLANS, self.dir)

    def row(self, result: subprocess.CompletedProcess, case: str) -> str:
        return next(line for line in result.stdout.splitlines() if line.startswith(f"| `{case}`"))

    def test_the_same_plans_pass(self) -> None:
        self.both("key/int/1", index_scan("int"), index_scan("int"))
        result = self.check()
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertIn("### plan_cache_mode = auto", result.stdout)
        self.assertIn("### plan_cache_mode = force_generic_plan", result.stdout)
        self.assertIn("| `key/int/1` | 1 | 20 | 6 | 0.00 | Index Scan using int_pkey | — |  |", result.stdout)

    def test_a_plan_on_every_call_on_head_only_is_flagged(self) -> None:
        self.case("base", "auto", "key/int/1", index_scan("int"), per_call=(10, 0))
        self.case("head", "auto", "key/int/1", index_scan("int"), per_call=(10, 10))
        result = self.check()
        self.assertEqual(result.returncode, 1, result.stdout)
        self.assertIn("| 6 → 16 | 0.00 → 1.00 |", self.row(result, "key/int/1"))
        self.assertIn("plans every call ⚠️", self.row(result, "key/int/1"))

    def test_a_plan_on_every_call_on_both_sides_is_shown_not_flagged(self) -> None:
        self.both("key/int/1", index_scan("int"), index_scan("int"), per_call=(10, 10))
        result = self.check()
        self.assertEqual(result.returncode, 0, result.stdout)
        self.assertIn("| 1.00 |", self.row(result, "key/int/1"))

    def test_a_changed_root_scan_is_flagged(self) -> None:
        self.both("key/int/1", index_scan("int"), {"Node Type": "Seq Scan", "Relation Name": "int"})
        result = self.check()
        self.assertEqual(result.returncode, 1)
        self.assertIn("Index Scan using int_pkey → Seq Scan", self.row(result, "key/int/1"))
        self.assertIn("root scan changed ⚠️", self.row(result, "key/int/1"))

    def test_the_root_scan_is_the_first_node_on_a_root_relation(self) -> None:
        plan = {"Node Type": "Nested Loop", "Plans": [
            {"Node Type": "Seq Scan", "Relation Name": "int_items"},
            index_scan("int"),
        ]}
        self.both("key/int/1", plan, plan)
        self.assertIn("Index Scan using int_pkey", self.row(self.check(), "key/int/1"))

    def test_lost_pruning_is_flagged(self) -> None:
        self.both("root/hash/1", partitioned(1, 3), partitioned(4, 0), partitions=4)
        result = self.check()
        self.assertEqual(result.returncode, 1)
        self.assertIn("1 of 4, 3 removed → 4 of 4", self.row(result, "root/hash/1"))
        self.assertIn("pruning lost ⚠️", self.row(result, "root/hash/1"))

    def test_pruning_kept_passes(self) -> None:
        self.both("root/hash/1", partitioned(1, 3), partitioned(1, 3), partitions=4)
        self.assertEqual(self.check().returncode, 0)

    def test_a_plan_after_the_closing_marker_belongs_to_no_case(self) -> None:
        self.both("key/int/1", index_scan("int"), index_scan("int"))
        self.log.append(logged({"Node Type": "Seq Scan", "Relation Name": "int"}, "INSERT INTO int SELECT …"))
        result = self.check()
        self.assertEqual(result.returncode, 0, result.stdout)
        self.assertIn("Index Scan using int_pkey", self.row(result, "key/int/1"))

    def test_a_case_with_no_logged_plan_reads_unknown_and_passes(self) -> None:
        self.both("key/int/1", {"Node Type": "Result"}, {"Node Type": "Result"})
        result = self.check()
        self.assertEqual(result.returncode, 0)
        self.assertIn("| ? |", self.row(result, "key/int/1"))

    def test_counts_are_the_harness_sums(self) -> None:
        self.both("key/int/1", index_scan("int"), index_scan("int"), statements=3)
        self.assertIn("| `key/int/1` | 3 | 20 |", self.check().stdout)

    def test_a_case_on_one_side_only_is_flagged(self) -> None:
        self.case("base", "auto", "key/int/1", index_scan("int"))
        self.case("head", "auto", "key/int/10", index_scan("int"))
        result = self.check()
        self.assertEqual(result.returncode, 1)
        self.assertIn("one side only ⚠️", self.row(result, "key/int/1"))

    def test_no_case_is_an_error(self) -> None:
        result = run(PLANS, self.dir)
        self.assertEqual(result.returncode, 2)
        self.assertIn("no case measured", result.stderr)


FAKE_HARNESS = r"""#!/usr/bin/env python3
import os, pathlib, sys
side, target = "@SIDE@", "@TARGET@"
mode, out, run = os.environ["REVIEW_AB_MODE"], os.environ["REVIEW_AB_OUT"], os.environ["REVIEW_AB_RUN"]
with open(os.environ["FAKE_LOG"], "a") as log:
    log.write(f"harness {side} {mode} {run} filter={os.environ['REVIEW_AB_FILTER']} url={os.environ['REVIEW_AB_PG_URL']}\n")
if os.environ.get("FAKE_HARNESS_FAIL") == side:
    print("thread 'main' panicked", file=sys.stderr)
    sys.exit(101)
source = os.environ.get(f"FAKE_{mode.upper()}_{side.upper()}") or os.environ[f"FAKE_{mode.upper()}"]
text = pathlib.Path(source).read_text().replace("@RUN@", run)
pathlib.Path(out).write_text(text)
"""

FAKE_DOCKER = r"""#!/usr/bin/env bash
echo "docker $*" >>"$FAKE_LOG"
case $1 in
  port) echo 127.0.0.1:55432 ;;
  ps) printf '%b\n' "$FAKE_CONTAINERS" ;;
  logs) cat "$FAKE_SERVER_LOG" ;;
esac
exit 0
"""


class Modes(unittest.TestCase):
    """review-ab.sh --digest / --plans end to end, over fakes in a throwaway repository."""

    def setUp(self) -> None:
        tmp = tempfile.TemporaryDirectory()
        self.addCleanup(tmp.cleanup)
        self.dir = pathlib.Path(tmp.name)
        self.repo = self.dir / "repo"
        self.home = self.dir / "cache"
        self.log = self.dir / "log"
        self.log.write_text("")
        bin_dir = self.dir / "bin"
        bin_dir.mkdir()
        for name, body in (("cargo", FAKE_CARGO), ("docker", FAKE_DOCKER), ("nproc", FAKE_NPROC), ("taskset", FAKE_TASKSET)):
            (bin_dir / name).write_text(body)
            (bin_dir / name).chmod(0o755)
        (self.dir / "harness.py").write_text(FAKE_HARNESS)
        (self.dir / "digest.tsv").write_text(documents())
        (self.dir / "digest-other.tsv").write_text(documents(**{"build:1": {"id": 1, "name": "x"}}))
        (self.dir / "plans.tsv").write_text("key/int/1\tint\t0\t1\t20\t6\t10\t0\n")
        (self.dir / "plans-replanned.tsv").write_text("key/int/1\tint\t0\t1\t20\t16\t10\t10\n")
        log = ""
        for run_name in ("base-auto", "head-auto", "base-force_generic_plan", "head-force_generic_plan"):
            log += marker(run_name, "key/int/1") + logged(index_scan("int"), "SELECT … FROM int") + marker(run_name, "-")
        (self.dir / "server.log").write_text(log)
        self.env = {
            **os.environ,
            "PATH": f"{bin_dir}:{os.environ['PATH']}",
            "FLUSSO_REVIEW_HOME": str(self.home),
            "CARGO_TARGET_DIR": str(self.dir / "head-target"),
            "FAKE_BASE_HOME": str(self.home),
            "FAKE_LOG": str(self.log),
            "FAKE_BENCH": str(self.dir / "harness.py"),
            "FAKE_CONTAINERS": "",
            "FAKE_NPROC": "2",
            "FAKE_DIGEST": str(self.dir / "digest.tsv"),
            "FAKE_PLANS": str(self.dir / "plans.tsv"),
            "FAKE_SERVER_LOG": str(self.dir / "server.log"),
        }
        self.repo.mkdir()
        self.git("init", "-q", "-b", "main")
        self.git("config", "user.email", "t@example.com")
        self.git("config", "user.name", "t")
        self.git("config", "commit.gpgsign", "false")
        for rel in (
            "libs/2-adapters/source-postgres/benches/postgres.rs",
            "libs/2-adapters/source-postgres/benches/cases.rs",
            "libs/2-engine/benches/pipeline.rs",
        ):
            path = self.repo / rel
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(WITH_URL)
        (self.repo / "libs/2-adapters/source-postgres/Cargo.toml").write_text("[package]\n")
        self.git("add", "-A")
        self.git("commit", "-qm", "base")
        self.base = self.git("rev-parse", "HEAD")

    def git(self, *args: str) -> str:
        return subprocess.run(["git", *args], cwd=self.repo, check=True, capture_output=True, text=True).stdout.strip()

    def review(self, *args: str, **env: str) -> subprocess.CompletedProcess:
        return subprocess.run(
            ["bash", REVIEW, *args], cwd=self.repo, env={**self.env, **env}, capture_output=True, text=True, timeout=120
        )

    def log_lines(self, prefix: str) -> list[str]:
        return [line for line in self.log.read_text().splitlines() if line.startswith(prefix)]

    def test_digest_runs_the_cases_harness_on_base_then_head_and_passes_on_the_same_documents(self) -> None:
        result = self.review("--digest", self.base)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertEqual([line.split()[1:3] for line in self.log_lines("harness")], [["base", "digest"], ["head", "digest"]])
        self.assertIn("| `k_int` | 12 | 12 |", result.stdout)
        self.assertIn("--digest", result.stdout)

    def test_digest_builds_only_the_cases_bench(self) -> None:
        self.review("--digest", self.base, "--bench", "postgres,pipeline")
        self.assertEqual(sorted(self.log_lines("build ")), ["build base cases", "build head cases"])

    def test_digest_exits_1_when_the_documents_differ(self) -> None:
        result = self.review("--digest", self.base, FAKE_DIGEST_HEAD=str(self.dir / "digest-other.tsv"))
        self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
        self.assertIn("differ ⚠️", result.stdout)

    def test_each_side_gets_a_fresh_database_and_the_filter(self) -> None:
        self.review("--digest", self.base, "--filter", "^k_")
        harness = self.log_lines("harness")
        self.assertTrue(all("filter=^k_" in line for line in harness), harness)
        self.assertEqual([line.split("url=")[1].rsplit("/", 1)[1] for line in harness], ["r1", "r2"])

    def test_plans_starts_postgres_with_the_statistics_and_explain_libraries(self) -> None:
        result = self.review("--plans", self.base)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        run = next(line for line in self.log_lines("docker run"))
        for arg in (
            "shared_preload_libraries=pg_stat_statements,auto_explain",
            "pg_stat_statements.track_planning=on",
            "auto_explain.log_min_duration=0",
            "auto_explain.log_format=json",
        ):
            self.assertIn(f"-c {arg}", run)
        self.assertIn("flusso.review-ab.owner=", run)

    def test_plans_runs_each_side_under_both_plan_cache_modes(self) -> None:
        self.review("--plans", self.base)
        self.assertEqual(
            [line.split()[3] for line in self.log_lines("harness")],
            ["base-auto", "head-auto", "base-force_generic_plan", "head-force_generic_plan"],
        )
        alters = [line for line in self.log_lines("docker exec") if "ALTER DATABASE" in line]
        self.assertEqual(len(alters), 4)
        self.assertIn("SET plan_cache_mode = force_generic_plan", alters[-1])

    def test_plans_exits_1_on_a_flag(self) -> None:
        result = self.review("--plans", self.base, FAKE_PLANS_HEAD=str(self.dir / "plans-replanned.tsv"))
        self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
        self.assertIn("plans every call ⚠️", result.stdout)

    def test_a_mode_removes_its_container(self) -> None:
        self.review("--plans", self.base)
        removed = [line for line in self.log_lines("docker rm -f") if "flusso-review-" in line and line.endswith("-pg16")]
        self.assertTrue(removed, self.log.read_text())

    def test_a_failing_harness_is_a_setup_error(self) -> None:
        result = self.review("--digest", self.base, FAKE_HARNESS_FAIL="head")
        self.assertEqual(result.returncode, 2, result.stdout + result.stderr)
        self.assertIn("the head side's `cases` harness failed in --digest", result.stderr)
        self.assertTrue(self.log_lines("docker rm -f"), "the container is removed on failure")

    def test_a_docker_failure_is_a_setup_error(self) -> None:
        failing = self.dir / "bin" / "docker"
        failing.write_text(FAKE_DOCKER.replace("exit 0", '[[ "$*" != *"CREATE DATABASE"* ]] || exit 1\nexit 0'))
        result = self.review("--digest", self.base)
        self.assertEqual(result.returncode, 2, result.stdout + result.stderr)
        self.assertTrue(self.log_lines("docker rm -f"), "the container is removed on failure")

    def test_a_base_that_does_not_build_is_a_setup_error(self) -> None:
        result = self.review("--plans", self.base, FAKE_CARGO_FAIL="base:lib")
        self.assertEqual(result.returncode, 2, result.stdout + result.stderr)

    def test_own_benches_is_refused(self) -> None:
        result = self.review("--digest", self.base, "--own-benches")
        self.assertEqual(result.returncode, 2)
        self.assertIn("does not take --own-benches", result.stderr)
        self.assertEqual(self.log_lines("build "), [])

    def test_a_held_lock_is_a_setup_error(self) -> None:
        import fcntl

        self.home.mkdir()
        (self.home / ".flusso-review-ab").write_text("")
        with open(self.home / "lock", "w") as lock:
            fcntl.flock(lock, fcntl.LOCK_EX)
            result = self.review("--digest", self.base)
        self.assertEqual(result.returncode, 2, result.stdout + result.stderr)
        self.assertIn("another review-ab run holds", result.stderr)


if __name__ == "__main__":
    unittest.main()
