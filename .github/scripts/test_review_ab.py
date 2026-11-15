"""Tests for the review command's script logic: the warm-build stamp and the A/B/control table.

    python3 -m unittest discover -s .github/scripts -p 'test_*.py'

`review-ab-stamp.py` decides whether the warm base build can be reused; `bench-compare.py
--repeats` prints the table `review-ab.sh` shows. Both run as subprocesses, exactly as the shell
script calls them, over files written to a temporary directory.
"""
import fcntl
import hashlib
import json
import os
import pathlib
import shutil
import subprocess
import sys
import tempfile
import textwrap
import time
import unittest

HERE = pathlib.Path(__file__).resolve().parent
STAMP = HERE / "review-ab-stamp.py"
COMPARE = HERE / "bench-compare.py"
HOUSEKEEPING = HERE / "review-ab-housekeeping.py"
REVIEW = HERE / "review-ab.sh"


def run(*args: object) -> subprocess.CompletedProcess:
    return subprocess.run([sys.executable, *map(str, args)], capture_output=True, text=True, check=False)


class Stamp(unittest.TestCase):
    def setUp(self) -> None:
        tmp = tempfile.TemporaryDirectory()
        self.addCleanup(tmp.cleanup)
        self.dir = pathlib.Path(tmp.name)
        self.target = self.dir / "base-target"
        self.exe = self.target / "release" / "deps" / "cases-1234"
        self.exe.parent.mkdir(parents=True)
        self.exe.write_bytes(b"binary")
        self.head = self.dir / "head"
        (self.head / "benches").mkdir(parents=True)
        (self.head / "benches" / "cases.rs").write_text("fn main() {}\n")
        self.tsv = self.dir / "review.tsv"
        digest = hashlib.sha256(b"binary").hexdigest()
        self.tsv.write_text(f"cases\t{self.exe}\t{digest}\n")
        self.stamp = self.dir / "stamp.json"

    def call(
        self,
        action: str,
        commit: str = "a" * 40,
        toolchain: str = "rustc 1.95.0",
        files=("benches/cases.rs",),
        build_env: str = "RUSTFLAGS=",
        need=(),
    ):
        args = [STAMP, action, self.stamp, "--commit", commit, "--toolchain", toolchain, "--target", self.target]
        args += ["--build-env", build_env]
        for name in need:
            args += ["--need", name]
        if action == "write":
            args += ["--executables", self.tsv]
        if files:
            args += ["--root", self.head]
            for rel in files:
                args += ["--file", rel]
        return run(*args)

    def write(self) -> None:
        self.assertEqual(self.call("write").returncode, 0)

    def assert_stale(self, result: subprocess.CompletedProcess, reason: str) -> None:
        self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
        self.assertIn(reason, result.stdout)

    def test_a_build_with_nothing_changed_is_current(self) -> None:
        self.write()
        result = self.call("check")
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertEqual(result.stdout.strip(), "current")

    def test_no_stamp_is_stale(self) -> None:
        self.assert_stale(self.call("check"), "no warm build recorded")

    def test_an_unreadable_stamp_is_stale(self) -> None:
        self.stamp.write_text("{not json")
        self.assert_stale(self.call("check"), "no warm build recorded")

    def test_a_new_base_commit_is_stale(self) -> None:
        self.write()
        self.assert_stale(self.call("check", commit="b" * 40), "base moved aaaaaaaaaaaa → bbbbbbbbbbbb")

    def test_a_new_toolchain_is_stale(self) -> None:
        self.write()
        self.assert_stale(self.call("check", toolchain="rustc 1.96.0"), "toolchain changed")

    def test_new_build_settings_are_stale(self) -> None:
        self.write()
        self.assert_stale(self.call("check", build_env="RUSTFLAGS=-C target-cpu=native"), "build settings changed")

    def test_a_bench_that_was_not_built_is_stale(self) -> None:
        self.write()
        self.assert_stale(self.call("check", need=("cases", "postgres")), "bench not built: postgres")

    def test_a_bench_that_was_built_is_current(self) -> None:
        self.write()
        self.assertEqual(self.call("check", need=("cases",)).returncode, 0)

    def test_an_edited_bench_file_is_stale(self) -> None:
        self.write()
        (self.head / "benches" / "cases.rs").write_text("fn main() { slower() }\n")
        self.assert_stale(self.call("check"), "bench file changed: benches/cases.rs")

    def test_switching_to_the_bases_own_benches_is_stale(self) -> None:
        self.write()
        self.assert_stale(self.call("check", files=()), "bench file changed: benches/cases.rs")

    def test_a_missing_executable_is_stale(self) -> None:
        self.write()
        self.exe.unlink()
        self.assert_stale(self.call("check"), "executable missing: cases")

    def test_a_rebuilt_executable_is_stale(self) -> None:
        self.write()
        self.exe.write_bytes(b"other binary")
        self.assert_stale(self.call("check"), "executable changed: cases")

    def test_an_executable_outside_the_base_target_dir_is_stale(self) -> None:
        elsewhere = self.dir / "head-target" / "cases-1234"
        elsewhere.parent.mkdir()
        elsewhere.write_bytes(b"binary")
        self.tsv.write_text(f"cases\t{elsewhere}\t{hashlib.sha256(b'binary').hexdigest()}\n")
        self.write()
        self.assert_stale(self.call("check"), "executable outside the base target dir: cases")

    def test_a_stamp_with_no_executable_is_stale(self) -> None:
        self.tsv.write_text("")
        self.write()
        self.assert_stale(self.call("check"), "no executable recorded")


def save(root: pathlib.Path, bench: str, baseline: str, median_ns: float, full_id: str | None = None) -> None:
    out = root / bench / baseline
    out.mkdir(parents=True, exist_ok=True)
    (out / "estimates.json").write_text(json.dumps({"median": {"point_estimate": median_ns}}))
    (out / "benchmark.json").write_text(json.dumps({"full_id": full_id or bench}))


class RepeatsTable(unittest.TestCase):
    def setUp(self) -> None:
        tmp = tempfile.TemporaryDirectory()
        self.addCleanup(tmp.cleanup)
        self.root = pathlib.Path(tmp.name)

    def case(self, bench: str, base: list[float], head: list[float]) -> None:
        for side, runs in (("base", base), ("head", head)):
            for i, value in enumerate(runs, start=1):
                save(self.root, bench, f"{side}-{i}", value)

    def compare(self, repeats: int = 2, limit: str = "5") -> subprocess.CompletedProcess:
        return run(COMPARE, self.root, "base", "head", limit, "--repeats", repeats)

    def row(self, out: str, bench: str) -> list[str]:
        line = next(line for line in out.splitlines() if line.startswith(f"| `{bench}`"))
        return [cell.strip() for cell in line.strip("|").split("|")]

    def test_each_side_is_its_fastest_run_and_the_spread_its_own_runs(self) -> None:
        self.case("key/int", [1_000_000, 1_020_000], [1_010_000, 1_015_000])
        result = self.compare()
        self.assertIn("| case | base (fastest … slowest) | head (fastest … slowest) | change | control spread | verdict |", result.stdout)
        self.assertEqual(
            self.row(result.stdout, "key/int"),
            ["`key/int`", "1.000 ms … 1.020 ms", "1.010 ms … 1.015 ms", "+1.0%", "2.0%", "noise"],
        )
        self.assertEqual(result.returncode, 0, result.stdout)

    def test_one_slow_run_per_side_does_not_widen_the_spread(self) -> None:
        # With 3+ runs, each side's slowest is dropped before the spread is taken.
        self.case("key/int", [1_000_000, 1_020_000, 1_500_000], [1_005_000, 1_010_000, 1_900_000])
        result = self.compare(repeats=3)
        self.assertEqual(self.row(result.stdout, "key/int")[4], "2.0%")

    def test_with_two_runs_nothing_is_dropped(self) -> None:
        self.case("key/int", [1_000_000, 1_500_000], [1_000_000, 1_000_000])
        result = self.compare(repeats=2)
        self.assertEqual(self.row(result.stdout, "key/int")[4], "50.0%")

    def test_the_slowest_run_is_still_shown(self) -> None:
        self.case("key/int", [1_000_000, 1_020_000, 1_500_000], [1_000_000, 1_000_000, 1_000_000])
        result = self.compare(repeats=3)
        self.assertEqual(self.row(result.stdout, "key/int")[1], "1.000 ms … 1.500 ms")

    def test_a_burst_on_one_head_run_is_not_a_slowdown(self) -> None:
        # The A/A run that motivated repeats: one head window hit by a build next door.
        self.case("key/citext", [9_000_000, 9_100_000], [16_100_000, 9_050_000])
        result = self.compare()
        self.assertEqual(self.row(result.stdout, "key/citext")[3], "+0.6%")
        self.assertEqual(self.row(result.stdout, "key/citext")[5], "noise")
        self.assertEqual(result.returncode, 0, result.stdout)

    def test_a_slowdown_on_every_head_run_is_counted(self) -> None:
        self.case("key/uuid", [1_000_000, 1_010_000], [1_300_000, 1_310_000])
        result = self.compare()
        self.assertEqual(self.row(result.stdout, "key/uuid")[5], "flagged ⚠️")
        self.assertEqual(result.returncode, 1, result.stdout)

    def test_a_speedup_is_counted_and_passes(self) -> None:
        self.case("batch/1000", [2_000_000, 2_000_000], [1_000_000, 1_000_000])
        result = self.compare()
        self.assertEqual(self.row(result.stdout, "batch/1000")[5], "faster")
        self.assertEqual(result.returncode, 0, result.stdout)

    def test_a_change_inside_twice_the_spread_is_noise(self) -> None:
        # +12% is past the 5% limit, but the head's own runs differ by 8%: twice that is 16%.
        self.case("root/keyless", [1_000_000, 1_000_000], [1_120_000, 1_209_600])
        result = self.compare()
        self.assertEqual(self.row(result.stdout, "root/keyless")[3:], ["+12.0%", "8.0%", "noise"])
        self.assertEqual(result.returncode, 0, result.stdout)

    def test_a_single_run_is_judged_on_the_limit(self) -> None:
        self.case("hit_ratio/0", [1_000_000], [1_100_000])
        result = self.compare(repeats=1)
        self.assertEqual(self.row(result.stdout, "hit_ratio/0")[4:], ["—", "flagged ⚠️"])
        self.assertEqual(result.returncode, 1, result.stdout)

    def test_a_case_on_one_side_only_is_listed_not_compared(self) -> None:
        self.case("key/int", [1_000_000, 1_000_000], [1_000_000, 1_000_000])
        save(self.root, "key/citext", "head-1", 1_000_000)
        result = self.compare()
        self.assertIn("`key/citext` (only `head`)", result.stdout)
        self.assertEqual(result.returncode, 0, result.stdout)

    def test_nothing_on_both_sides_is_an_error(self) -> None:
        save(self.root, "key/citext", "head-1", 1_000_000)
        self.assertEqual(self.compare().returncode, 2)

    def test_flagged_cases_are_written_with_their_bench_target(self) -> None:
        save(self.root, "cases/key/int", "base-1", 1_000_000, full_id="key/int")
        save(self.root, "cases/key/int", "head-1", 1_300_000, full_id="key/int")
        save(self.root, "cases/key/uuid", "base-1", 1_000_000, full_id="key/uuid")
        save(self.root, "cases/key/uuid", "head-1", 1_000_000, full_id="key/uuid")
        breaches = self.root / "breaches"
        result = run(COMPARE, self.root, "base", "head", "5", "--repeats", 1, "--breaches", breaches)
        self.assertEqual(breaches.read_text(), "cases\tkey/int\n")
        self.assertIn("flagged", self.row(result.stdout, "key/int")[5])
        self.assertEqual(result.returncode, 1, result.stdout)

    def confirm(self, first: float, again: float, again_base: float = 1_000_000) -> subprocess.CompletedProcess:
        save(self.root, "key/int", "base-1", 1_000_000)
        save(self.root, "key/int", "head-1", first)
        save(self.root, "key/int", "base-confirm", again_base)
        save(self.root, "key/int", "head-confirm", again)
        return run(COMPARE, self.root, "base", "head", "5", "--repeats", 1, "--confirm", "base-confirm", "head-confirm")

    def test_a_flag_the_re_run_confirms_is_slower(self) -> None:
        result = self.confirm(1_300_000, 1_250_000)
        self.assertEqual(self.row(result.stdout, "key/int")[5], "slower ⚠️ (re-run +25.0%)")
        self.assertEqual(result.returncode, 1, result.stdout)

    def test_a_flag_the_re_run_clears_passes_and_is_shown_apart(self) -> None:
        result = self.confirm(1_080_000, 1_000_000)
        self.assertEqual(self.row(result.stdout, "key/int")[5], "flagged, cleared on re-run (+0.0%)")
        self.assertIn("1 case(s) flagged and cleared on re-run", result.stdout)
        self.assertEqual(result.returncode, 0, result.stdout)

    def test_the_gates_mean_rule_confirms_a_large_first_pass(self) -> None:
        # +30% first, +0% re-run: the mean, +15%, is past the limit (bench-gate.sh's rule).
        result = self.confirm(1_300_000, 1_000_000)
        self.assertEqual(self.row(result.stdout, "key/int")[5], "slower ⚠️ (re-run +0.0%)")
        self.assertEqual(result.returncode, 1, result.stdout)

    def test_a_flag_left_unmeasured_counts_as_slower(self) -> None:
        save(self.root, "key/int", "base-1", 1_000_000)
        save(self.root, "key/int", "head-1", 1_300_000)
        result = run(COMPARE, self.root, "base", "head", "5", "--repeats", 1, "--confirm", "base-confirm", "head-confirm")
        self.assertEqual(self.row(result.stdout, "key/int")[5], "slower ⚠️ (not re-measured)")
        self.assertEqual(result.returncode, 1, result.stdout)

    def test_without_repeats_the_gate_table_is_unchanged(self) -> None:
        save(self.root, "live_drain/sinks1/64", "base", 1_000_000)
        save(self.root, "live_drain/sinks1/64", "head", 1_200_000)
        result = run(COMPARE, self.root, "base", "head", "10")
        self.assertIn("| bench | base | head | change |", result.stdout)
        self.assertIn("| `live_drain/sinks1/64` | 1.000 ms | 1.200 ms | +20.0% ⚠️ |", result.stdout)
        self.assertEqual(result.returncode, 1, result.stdout)


class Housekeeping(unittest.TestCase):
    def test_only_review_containers_of_dead_runs_are_stale(self) -> None:
        dead = subprocess.Popen(["true"])
        dead.wait()
        names = [
            f"flusso-review-{dead.pid}-pg16",
            f"flusso-review-{dead.pid}-os",
            f"flusso-review-{os.getpid()}-pg16",
            "flusso-review-dev-pg16",
            "postgres",
            f"my-flusso-review-{dead.pid}-pg16",
        ]
        result = subprocess.run(
            [sys.executable, HOUSEKEEPING, "stale"], input="\n".join(names), capture_output=True, text=True, check=True
        )
        self.assertEqual(result.stdout.split(), [f"flusso-review-{dead.pid}-pg16", f"flusso-review-{dead.pid}-os"])

    def test_prune_keeps_the_newest_result_dirs(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp)
            for day in range(1, 8):
                (root / f"202611{day:02d}-120000").mkdir()
            (root / "notes.txt").write_text("not a run")
            subprocess.run([sys.executable, HOUSEKEEPING, "prune", root, "4"], check=True)
            self.assertEqual(
                sorted(p.name for p in root.iterdir()),
                ["20261104-120000", "20261105-120000", "20261106-120000", "20261107-120000", "notes.txt"],
            )


class Cpus(unittest.TestCase):
    def cpus(self, *args: object) -> subprocess.CompletedProcess:
        return run(HOUSEKEEPING, "cpus", *args)

    def test_defaults_leave_core_0_and_split_the_rest_into_two_disjoint_sets(self) -> None:
        self.assertEqual(self.cpus("--nproc", 24, "--taskset", 1).stdout.split("\n")[:2], ["1-4", "5-8"])

    def test_a_smaller_machine_gets_smaller_sets(self) -> None:
        self.assertEqual(self.cpus("--nproc", 8, "--taskset", 1).stdout.split("\n")[:2], ["1-3", "4-6"])

    def test_too_few_cores_turns_pinning_off(self) -> None:
        result = self.cpus("--nproc", 4, "--taskset", 1)
        self.assertEqual(result.returncode, 0)
        self.assertEqual(result.stdout.split("\n")[:2], ["", ""])
        self.assertIn("4 cores", result.stdout)

    def test_no_taskset_turns_pinning_off(self) -> None:
        result = self.cpus("--nproc", 24, "--taskset", 0)
        self.assertEqual(result.stdout.split("\n")[:2], ["", ""])
        self.assertIn("taskset", result.stdout)

    def test_off_on_request(self) -> None:
        result = self.cpus("--nproc", 24, "--taskset", 1, "--off")
        self.assertEqual(result.stdout.split("\n")[:2], ["", ""])

    def test_overrides_are_used_as_given(self) -> None:
        result = self.cpus("--nproc", 24, "--taskset", 1, "--bench", "2,3", "--servers", "10-13")
        self.assertEqual(result.stdout.split("\n")[:2], ["2,3", "10-13"])

    def test_overlapping_sets_are_refused(self) -> None:
        result = self.cpus("--nproc", 24, "--taskset", 1, "--bench", "1-4", "--servers", "4-6")
        self.assertEqual(result.returncode, 2)
        self.assertIn("overlap", result.stderr)

    def test_a_core_the_machine_does_not_have_is_refused(self) -> None:
        result = self.cpus("--nproc", 8, "--taskset", 1, "--bench", "1-2", "--servers", "7-9")
        self.assertEqual(result.returncode, 2)
        self.assertIn("9", result.stderr)


FAKE_CARGO = r"""#!/usr/bin/env python3
import json, os, pathlib, shutil, sys
args = sys.argv[1:]
bench = args[args.index("--bench") + 1]
package = args[args.index("-p") + 1]
target = pathlib.Path(os.environ["CARGO_TARGET_DIR"])
side = "base" if str(target).startswith(os.environ["FAKE_BASE_HOME"]) else "head"
with open(os.environ["FAKE_LOG"], "a") as log:
    log.write(f"build {side} {bench}\n")
if os.environ.get("FAKE_CARGO_FAIL", "").startswith(side + ":"):
    kind = os.environ["FAKE_CARGO_FAIL"].split(":", 1)[1]
    what = f'(bench "{bench}")' if kind == "bench" else "(lib)"
    print(f"error: could not compile `{package}` {what} due to 1 previous error", file=sys.stderr)
    sys.exit(101)
exe = target / "release" / "deps" / f"{bench}-fake"
exe.parent.mkdir(parents=True, exist_ok=True)
source = pathlib.Path(os.environ["FAKE_BENCH"]).read_text()
exe.write_text(source.replace("@SIDE@", side).replace("@TARGET@", bench))
exe.chmod(0o755)
manifest = pathlib.Path.cwd() / ("libs/2-engine" if bench == "pipeline" else "libs/2-adapters/source-postgres") / "Cargo.toml"
print(json.dumps({"reason": "compiler-artifact", "target": {"name": bench, "kind": ["bench"]},
                  "executable": str(exe), "manifest_path": str(manifest)}))
"""

FAKE_BENCH = r"""#!/usr/bin/env python3
import json, os, pathlib, sys
side, target = "@SIDE@", "@TARGET@"
args = sys.argv[1:]
baseline = args[args.index("--save-baseline") + 1]
exact = args[args.index("--exact") + 1] if "--exact" in args else None
with open(os.environ["FAKE_LOG"], "a") as log:
    log.write(f"run {side} {target} {baseline} {exact or '-'} {' '.join(args)}\n")
print("Gnuplot not found, using plotters backend", file=sys.stderr)
print("Benchmarking key/int: Warming up for 1.0000 s", file=sys.stderr)
print("Warning: Unable to complete 20 samples in 4.0s. You may wish to increase target time", file=sys.stderr)
print("skipping `root/x`: a message the reviewer needs", file=sys.stderr)
times = json.loads(os.environ.get("FAKE_TIMES", "{}"))
ids = [i for i in os.environ["FAKE_CASES"].split() if exact in (None, i)]
home = pathlib.Path(os.environ["CRITERION_HOME"])
for case in ids:
    value = times.get(side, {}).get(case, {}).get(baseline, 1_000_000)
    out = home / case / baseline
    out.mkdir(parents=True, exist_ok=True)
    (out / "estimates.json").write_text(json.dumps({"median": {"point_estimate": value}}))
    (out / "benchmark.json").write_text(json.dumps({"full_id": case}))
"""

FAKE_DOCKER = r"""#!/usr/bin/env bash
echo "docker $*" >>"$FAKE_LOG"
case $1 in
  port) echo 127.0.0.1:55432 ;;
  ps) printf '%s\n' $FAKE_CONTAINERS ;;
esac
exit 0
"""

FAKE_NPROC = """#!/usr/bin/env bash
echo "$FAKE_NPROC"
"""

FAKE_TASKSET = """#!/usr/bin/env bash
echo "taskset $1 $2" >>"$FAKE_LOG"
shift 2
exec "$@"
"""

WITH_URL = "// reads REVIEW_AB_PG_URL\n"


class Script(unittest.TestCase):
    """review-ab.sh end to end, over fake cargo, docker and bench executables in a throwaway repository."""

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
        (self.dir / "bench.py").write_text(FAKE_BENCH)
        self.env = {
            **os.environ,
            "PATH": f"{bin_dir}:{os.environ['PATH']}",
            "FLUSSO_REVIEW_HOME": str(self.home),
            "CARGO_TARGET_DIR": str(self.dir / "head-target"),
            "FAKE_BASE_HOME": str(self.home),
            "FAKE_LOG": str(self.log),
            "FAKE_BENCH": str(self.dir / "bench.py"),
            "FAKE_CASES": "key/int key/uuid",
            "FAKE_CONTAINERS": "",
            "FAKE_NPROC": "2",
        }
        self.repo.mkdir()
        self.git("init", "-q", "-b", "main")
        self.git("config", "user.email", "t@example.com")
        self.git("config", "user.name", "t")
        self.git("config", "commit.gpgsign", "false")
        self.write_files({
            "libs/2-adapters/source-postgres/Cargo.toml": "[package]\n",
            "libs/2-adapters/source-postgres/benches/postgres.rs": WITH_URL,
            "libs/2-engine/benches/pipeline.rs": WITH_URL,
        })
        self.without_cases = self.commit("base without cases")
        self.write_files({"libs/2-adapters/source-postgres/benches/cases.rs": WITH_URL})
        self.with_cases = self.commit("add cases")

    def git(self, *args: str) -> str:
        return subprocess.run(["git", *args], cwd=self.repo, check=True, capture_output=True, text=True).stdout.strip()

    def write_files(self, files: dict) -> None:
        for rel, text in files.items():
            path = self.repo / rel
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(text)

    def commit(self, message: str) -> str:
        self.git("add", "-A")
        self.git("commit", "-qm", message)
        return self.git("rev-parse", "HEAD")

    def review(self, *args: str, **env: str) -> subprocess.CompletedProcess:
        return subprocess.run(
            ["bash", REVIEW, *args], cwd=self.repo, env={**self.env, **env}, capture_output=True, text=True, timeout=120
        )

    def log_lines(self, prefix: str) -> list[str]:
        return [line for line in self.log.read_text().splitlines() if line.startswith(prefix)]

    def runs(self) -> list[str]:
        return [" ".join(line.split()[1:5]) for line in self.log_lines("run ")]

    def test_identical_sides_pass_and_alternate_base_and_head_three_rounds(self) -> None:
        result = self.review(self.with_cases, "--bench", "cases")
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertEqual(
            [r.split()[2] for r in self.runs()], ["base-1", "head-1", "base-2", "head-2", "base-3", "head-3"]
        )
        self.assertEqual([r.split()[0] for r in self.runs()], ["base", "head"] * 3)

    def test_only_the_benches_asked_for_are_built(self) -> None:
        self.review(self.with_cases, "--bench", "cases")
        self.assertEqual(sorted(self.log_lines("build ")), ["build base cases", "build head cases"])

    def test_a_narrower_bench_selection_reuses_the_warm_build(self) -> None:
        self.assertEqual(self.review(self.with_cases, "--bench", "cases,postgres").returncode, 0)
        result = self.review(self.with_cases, "--bench", "cases")
        self.assertIn("warm base build is current", result.stdout)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)

    def test_a_wider_bench_selection_rebuilds_the_base(self) -> None:
        self.assertEqual(self.review(self.with_cases, "--bench", "cases").returncode, 0)
        result = self.review(self.with_cases, "--bench", "cases,postgres")
        self.assertIn("bench not built: postgres", result.stdout)

    def test_the_benches_and_the_servers_run_on_disjoint_cores(self) -> None:
        result = self.review(self.with_cases, "--bench", "cases", FAKE_NPROC="24")
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertIn("CPU pinning: benches on cores 1-4, Postgres and OpenSearch on 5-8", result.stdout)
        self.assertEqual(set(self.log_lines("taskset")), {"taskset -c 1-4"})
        self.assertEqual(len(self.log_lines("taskset")), len(self.log_lines("run ")))
        runs = [line for line in self.log_lines("docker run")]
        self.assertTrue(runs and all("--cpuset-cpus 5-8" in line for line in runs), runs)

    def test_cores_can_be_chosen_by_env(self) -> None:
        result = self.review(
            self.with_cases, "--bench", "cases", FAKE_NPROC="24", REVIEW_AB_BENCH_CPUS="2-3", REVIEW_AB_SERVER_CPUS="6-7"
        )
        self.assertIn("benches on cores 2-3, Postgres and OpenSearch on 6-7", result.stdout)
        self.assertEqual(set(self.log_lines("taskset")), {"taskset -c 2-3"})

    def test_cores_can_be_chosen_by_flag_and_pinning_turned_off(self) -> None:
        result = self.review(self.with_cases, "--bench", "cases", "--bench-cpus", "2", "--server-cpus", "3", FAKE_NPROC="24")
        self.assertIn("benches on cores 2, Postgres and OpenSearch on 3", result.stdout)
        self.log.write_text("")
        result = self.review(self.with_cases, "--bench", "cases", "--no-pin", FAKE_NPROC="24")
        self.assertIn("CPU pinning off", result.stdout)
        self.assertEqual(self.log_lines("taskset"), [])

    def test_a_small_machine_runs_unpinned_and_says_so(self) -> None:
        result = self.review(self.with_cases, "--bench", "cases", FAKE_NPROC="2")
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertIn("CPU pinning off", result.stdout)
        self.assertEqual(self.log_lines("taskset"), [])
        self.assertFalse(any("--cpuset-cpus" in line for line in self.log_lines("docker run")))

    def test_criterion_runs_quietly_without_plots(self) -> None:
        self.review(self.with_cases, "--bench", "cases")
        line = self.log_lines("run ")[0]
        for flag in ("--quiet", "--noplot", "--plotting-backend plotters"):
            self.assertIn(flag, line)

    def test_criterion_progress_is_dropped_and_bench_messages_kept(self) -> None:
        result = self.review(self.with_cases, "--bench", "cases")
        self.assertNotIn("Gnuplot not found", result.stderr)
        self.assertNotIn("Benchmarking key/int", result.stderr)
        self.assertNotIn("Unable to complete", result.stderr)
        self.assertIn("a message the reviewer needs", result.stderr)

    def test_a_flagged_case_is_re_run_alone_and_judged_on_the_re_run(self) -> None:
        times = {"head": {
            "key/int": {f"head-{r}": 1_300_000 for r in (1, 2, 3)} | {"head-confirm": 1_300_000},
            "key/uuid": {f"head-{r}": 1_080_000 for r in (1, 2, 3)} | {"head-confirm": 1_000_000},
        }}
        result = self.review(self.with_cases, "--bench", "cases", FAKE_TIMES=json.dumps(times))
        self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
        confirms = [r for r in self.runs() if "confirm" in r]
        self.assertEqual(
            confirms,
            [
                "base cases base-confirm key/int",
                "head cases head-confirm key/int",
                "base cases base-confirm key/uuid",
                "head cases head-confirm key/uuid",
            ],
        )
        self.assertIn("slower ⚠️", result.stdout)
        self.assertIn("flagged, cleared on re-run", result.stdout)

    def test_a_flag_cleared_on_re_run_exits_0(self) -> None:
        times = {"head": {"key/uuid": {f"head-{r}": 1_080_000 for r in (1, 2, 3)} | {"head-confirm": 1_000_000}}}
        result = self.review(self.with_cases, "--bench", "cases", FAKE_TIMES=json.dumps(times))
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertIn("flagged, cleared on re-run", result.stdout)

    def test_a_second_run_fails_at_once_while_the_first_holds_the_lock(self) -> None:
        self.home.mkdir(parents=True)
        with open(self.home / "lock", "w") as lock:
            fcntl.flock(lock, fcntl.LOCK_EX)
            start = time.monotonic()
            result = self.review(self.with_cases, "--bench", "cases")
            self.assertLess(time.monotonic() - start, 10)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("another review-ab run", result.stderr)
        self.assertEqual(self.log_lines("build "), [])

    def test_a_build_failure_in_a_bench_target_suggests_own_benches(self) -> None:
        result = self.review(self.with_cases, "--bench", "cases", FAKE_CARGO_FAIL="base:bench")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("--own-benches", result.stderr)

    def test_a_build_failure_elsewhere_does_not_blame_the_overlay(self) -> None:
        result = self.review(self.with_cases, "--bench", "cases", FAKE_CARGO_FAIL="base:lib")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("the base does not build", result.stderr)
        self.assertNotIn("--own-benches", result.stderr)

    def test_own_benches_resets_the_tree_and_leaves_out_an_overlaid_file(self) -> None:
        self.assertEqual(self.review(self.without_cases, "--bench", "cases,postgres").returncode, 0)
        self.assertTrue((self.home / "tree/libs/2-adapters/source-postgres/benches/cases.rs").is_file())
        self.log.write_text("")
        result = self.review(self.without_cases, "--bench", "cases,postgres", "--own-benches")
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertFalse((self.home / "tree/libs/2-adapters/source-postgres/benches/cases.rs").exists())
        self.assertNotIn("build base cases", self.log_lines("build "))

    def test_own_benches_refuses_a_base_whose_benches_ignore_the_server(self) -> None:
        self.write_files({"libs/2-adapters/source-postgres/benches/postgres.rs": "// testcontainers only\n"})
        old = self.commit("old postgres bench")
        self.write_files({"libs/2-adapters/source-postgres/benches/postgres.rs": WITH_URL})
        self.commit("head")
        result = self.review(old, "--bench", "postgres", "--own-benches")
        self.assertEqual(result.returncode, 2, result.stdout + result.stderr)
        self.assertIn("predates", result.stderr)
        self.assertEqual(self.log_lines("run "), [])

    def test_containers_of_a_killed_run_are_removed_and_nothing_else(self) -> None:
        dead = subprocess.Popen(["true"])
        dead.wait()
        others = f"flusso-review-{dead.pid}-pg16 postgres flusso-review-{os.getpid()}-os"
        self.review(self.with_cases, "--bench", "cases", FAKE_CONTAINERS=others)
        removed = [line for line in self.log_lines("docker rm") if f"-{dead.pid}-" in line or "postgres " in line + " "]
        self.assertEqual(removed, [f"docker rm -f flusso-review-{dead.pid}-pg16"])
        self.assertFalse(any(f"flusso-review-{os.getpid()}-os" in line for line in self.log_lines("docker rm")))

    def test_the_base_commit_and_its_date_are_printed(self) -> None:
        result = self.review(self.with_cases, "--bench", "cases")
        self.assertIn(self.with_cases[:12], result.stdout)
        self.assertRegex(result.stdout, r"committed \d{4}-\d{2}-\d{2}")
        self.assertIn("not fetched", result.stdout)

    def test_clean_removes_the_cache_and_the_results(self) -> None:
        self.review(self.with_cases, "--bench", "cases")
        self.assertTrue(self.home.is_dir())
        result = self.review("--clean")
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertFalse(self.home.exists())
        self.assertFalse((self.dir / "head-target" / "review-ab").exists())


if __name__ == "__main__":
    unittest.main()
