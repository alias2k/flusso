"""Tests for the review command's script logic: the warm-build stamp and the A/B/control table.

    python3 -m unittest discover -s .github/scripts -p 'test_*.py'

`review-ab-stamp.py` decides whether the warm base build can be reused; `bench-compare.py
--repeats` prints the table `review-ab.sh` shows. Both run as subprocesses, exactly as the shell
script calls them, over files written to a temporary directory.
"""
import hashlib
import json
import pathlib
import subprocess
import sys
import tempfile
import unittest

HERE = pathlib.Path(__file__).resolve().parent
STAMP = HERE / "review-ab-stamp.py"
COMPARE = HERE / "bench-compare.py"


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

    def call(self, action: str, commit: str = "a" * 40, toolchain: str = "rustc 1.95.0", files=("benches/cases.rs",)):
        args = [STAMP, action, self.stamp, "--commit", commit, "--toolchain", toolchain, "--target", self.target]
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


def save(root: pathlib.Path, bench: str, baseline: str, median_ns: float) -> None:
    out = root / bench / baseline
    out.mkdir(parents=True, exist_ok=True)
    (out / "estimates.json").write_text(json.dumps({"median": {"point_estimate": median_ns}}))
    (out / "benchmark.json").write_text(json.dumps({"full_id": bench}))


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
        self.assertEqual(self.row(result.stdout, "key/uuid")[5], "slower ⚠️")
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
        self.assertEqual(self.row(result.stdout, "hit_ratio/0")[4:], ["—", "slower ⚠️"])
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

    def test_without_repeats_the_gate_table_is_unchanged(self) -> None:
        save(self.root, "live_drain/sinks1/64", "base", 1_000_000)
        save(self.root, "live_drain/sinks1/64", "head", 1_200_000)
        result = run(COMPARE, self.root, "base", "head", "10")
        self.assertIn("| bench | base | head | change |", result.stdout)
        self.assertIn("| `live_drain/sinks1/64` | 1.000 ms | 1.200 ms | +20.0% ⚠️ |", result.stdout)
        self.assertEqual(result.returncode, 1, result.stdout)


if __name__ == "__main__":
    unittest.main()
