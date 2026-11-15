"""Tests for bench-regression.py and bench-runner-facts.py, over the real `main` history.

    python3 -m unittest discover -s .github/scripts -p 'test_*.py'

`fixtures/bench-history.json` is the `benchmarks` branch through run 78 (the 79th run, commit
26a81616): every series of both `data.js` files, one value per run. Each test cuts the history at a
run, writes the two `data.js` files the workflow reads, and runs the script exactly as `bench.yml`
does (`25 --rss-threshold 20`). `fixtures/frozen_main_bench_regression.py` is a frozen copy of the
script as on `main` before PR #331: the detection rule must flag exactly what it flags.
"""
import importlib.util
import json
import os
import pathlib
import re
import resource
import stat
import subprocess
import sys
import tempfile
import unittest

HERE = pathlib.Path(__file__).resolve().parent
SCRIPT = HERE / "bench-regression.py"
FROZEN_MAIN = HERE / "fixtures" / "frozen_main_bench_regression.py"
FACTS = HERE / "bench-runner-facts.py"
HISTORY = json.loads((HERE / "fixtures" / "bench-history.json").read_text(encoding="utf-8"))
IMAGES = {"opensearch": "opensearchproject/opensearch:2", "postgres": "postgres:16-alpine"}
NEWEST = len(HISTORY["runs"]) - 1
SCENARIOS = [name for suite in HISTORY["suites"].values() for name in suite["series"] if "/ci/" in name]
ISSUE = "Performance regression on main"


def entries(suite: dict, upto: int, steps: dict, image_change: dict) -> list[dict]:
    out = []
    for run in range(upto + 1):
        benches = []
        for name, values in suite["series"].items():
            value = values[run]
            if value is None:
                continue
            start, factor = steps.get(name, (None, 1.0))
            if start is not None and run >= start:
                value = value * factor if suite["tool"] == "customSmallerIsBetter" else value / factor
            bench = {"name": name, "value": value, "unit": ""}
            if "/ci/" in name:
                images = dict(IMAGES)
                if name in image_change and run >= image_change[name]:
                    images["opensearch"] = "opensearchproject/opensearch:3"
                bench["extra"] = json.dumps({"images": images})
            benches.append(bench)
        commit = HISTORY["runs"][run]
        out.append({
            "commit": {"id": commit + "0" * 28, "url": f"https://github.com/alias2k/flusso/commit/{commit}"},
            "date": run,
            "tool": suite["tool"],
            "benches": benches,
        })
    return out


def data_files(upto: int, steps: dict | None = None, image_change: dict | None = None, mutate=None) -> dict[str, str]:
    """The two `data.js` texts, as the `benchmarks` branch holds them, with `main` at run `upto`."""
    texts = {}
    for key, suite in HISTORY["suites"].items():
        points = entries(suite, upto, steps or {}, image_change or {})
        if mutate:
            points = mutate(key, points)
        texts[key] = "window.BENCHMARK_DATA = " + json.dumps({"entries": {suite["name"]: points}})
    return texts


class Gh:
    """A fake `gh` on `PATH` that records its calls and answers `issue list` with `open_issues`."""

    def __init__(self, tmp: pathlib.Path, open_issues: list[int]):
        self.calls = tmp / "gh-calls.jsonl"
        listed = json.dumps([{"number": n, "title": ISSUE} for n in open_issues])
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


def run(script: pathlib.Path, texts: dict[str, str], *args: str, gh_issues: list[int] | None = None,
        missing: bool = False) -> tuple[subprocess.CompletedProcess, list[list[str]]]:
    """Run `script` over the two files as `bench.yml` does; with `gh_issues`, under `--issue` and a fake `gh`."""
    with tempfile.TemporaryDirectory() as tmp:
        tmp = pathlib.Path(tmp)
        paths = []
        for key, text in texts.items():
            path = tmp / f"{key}.js"
            path.write_text(text, encoding="utf-8")
            paths.append(str(path))
        if missing:
            paths.append(str(tmp / "absent.js"))
        env = dict(os.environ)
        gh = None
        if gh_issues is not None:
            gh = Gh(tmp, gh_issues)
            env["PATH"] = gh.path
            args = (*args, "--issue")
        done = subprocess.run([sys.executable, str(script), "25", "--rss-threshold", "20", *args, *paths],
                              capture_output=True, text=True, env=env)
        return done, gh.recorded() if gh else []


def flagged(out: str) -> set[str]:
    return set(re.findall(r"^  flusso \(\w+ is better\) (\S+): ", out, re.M))


def noisy(out: str) -> dict[str, tuple[int, int]]:
    return {name: (int(step), int(limit)) for name, step, limit in re.findall(
        r"^    flusso \(\w+ is better\) (\S+): needs a lasting step of \+(\d+)% \(threshold (\d+)%\)", out, re.M)}


class MainsRuleIsUnchanged(unittest.TestCase):
    """The detection rule is main's, exactly: the same (series, run) pairs are flagged."""

    def assert_same(self, texts: dict[str, str], label: str) -> None:
        mine, _ = run(SCRIPT, texts)
        frozen, _ = run(FROZEN_MAIN, texts)
        self.assertEqual(frozen.returncode, 0, frozen.stderr)
        self.assertEqual(mine.returncode, 0, mine.stderr)
        self.assertEqual(flagged(mine.stdout), flagged(frozen.stdout), label)

    def test_every_prefix_of_the_real_history_flags_what_main_flags(self):
        for upto in range(3, NEWEST + 1):
            with self.subTest(run=upto):
                self.assert_same(data_files(upto), f"run {upto}")

    def test_injected_steps_and_an_image_change_flag_what_main_flags(self):
        cases = {
            "x1.5 on three series at 60": {"steps": {n: (60, 1.5) for n in (
                "components/build/1", "inprocess/decode/fixture", "reference/ci/drain_changes_per_s")}},
            "x2 on every scenario series at 70": {"steps": {n: (70, 2.0) for n in SCENARIOS}},
            "an image change at 50": {"image_change": {n: 50 for n in SCENARIOS}},
        }
        for label, case in cases.items():
            for upto in range(45, NEWEST + 1, 3):
                with self.subTest(case=label, run=upto):
                    self.assert_same(data_files(upto, **case), f"{label} at {upto}")

    def test_an_empty_file_is_read_as_main_reads_it(self):
        for gh_issues in ([], [328]):
            texts = data_files(NEWEST)
            texts["bigger"] = 'window.BENCHMARK_DATA = {"entries": {}}'
            mine, mine_calls = run(SCRIPT, texts, gh_issues=gh_issues)
            frozen, frozen_calls = run(FROZEN_MAIN, texts, gh_issues=gh_issues)
            with self.subTest(open_issues=gh_issues):
                self.assertEqual(mine.returncode, frozen.returncode)
                self.assertEqual(flagged(mine.stdout), flagged(frozen.stdout))
                self.assertEqual([c[:2] + c[2:3] for c in mine_calls], [c[:2] + c[2:3] for c in frozen_calls])


class TheIssue(unittest.TestCase):
    """One issue, found by its title, opened, kept current and closed as on main."""

    def test_findings_open_the_issue_with_its_title_and_label(self):
        done, calls = run(SCRIPT, data_files(NEWEST), gh_issues=[])
        self.assertEqual(done.returncode, 0, done.stderr)
        self.assertEqual(calls[0][:4], ["issue", "list", "--state", "open"])
        create = calls[-1]
        self.assertEqual(create[:6], ["issue", "create", "--title", ISSUE, "--label", "bug"])

    def test_findings_update_the_open_issue(self):
        _, calls = run(SCRIPT, data_files(NEWEST), gh_issues=[328])
        self.assertEqual(calls[-1][:3], ["issue", "edit", "328"])

    def test_no_finding_closes_the_open_issue(self):
        _, calls = run(SCRIPT, data_files(40), gh_issues=[328])
        self.assertEqual(calls[-1][:3], ["issue", "close", "328"])


class UnreadableHistory(unittest.TestCase):
    """A data file that can't be read fails the run before `gh` is called (B5)."""

    CASES = {
        "truncated": 'window.BENCHMARK_DATA = {"entries": {"flusso (smal',
        "no object": "window.BENCHMARK_DATA = []",
        "not an object": 'window.BENCHMARK_DATA = {"a": 1}\n{',
        "empty": "",
    }

    def test_an_unreadable_file_fails_and_leaves_the_issue_alone(self):
        for label, text in self.CASES.items():
            texts = data_files(NEWEST)
            texts["smaller"] = text
            with self.subTest(case=label):
                done, calls = run(SCRIPT, texts, gh_issues=[328])
                self.assertNotEqual(done.returncode, 0)
                self.assertIn("error:", done.stderr)
                self.assertEqual(calls, [])

    def test_a_missing_file_fails_and_leaves_the_issue_alone(self):
        done, calls = run(SCRIPT, data_files(NEWEST), gh_issues=[328], missing=True)
        self.assertNotEqual(done.returncode, 0)
        self.assertIn("error:", done.stderr)
        self.assertEqual(calls, [])


class MalformedEntries(unittest.TestCase):
    """A malformed entry inside a readable file is skipped with a warning; the rest is judged as main would."""

    def test_malformed_entries_are_skipped_with_a_warning(self):
        damaged = {
            "components/baseline/os_flush_1": None, "components/baseline/pg_select_1": "fast",
            "components/baseline/resolve_unrelated": True,
        }

        def damage(key, points):
            if key == "smaller":
                for bench in points[-1]["benches"]:
                    if bench["name"] in damaged:
                        bench["value"] = damaged[bench["name"]]
                points[-1]["benches"].append({"value": 1.0, "unit": "ns"})
                points[-1]["benches"].append("not a bench")
                points[5]["commit"] = None
            return points

        def what_main_reads(key, points):
            if key == "smaller":
                points[-1]["benches"] = [b for b in points[-1]["benches"] if b["name"] not in damaged]
                points[5]["commit"] = {}
            return points

        done, _ = run(SCRIPT, data_files(NEWEST, mutate=damage))
        self.assertEqual(done.returncode, 0, done.stderr)
        self.assertEqual(len(re.findall(r"^warning:", done.stderr, re.M)), 6, done.stderr)
        frozen, _ = run(FROZEN_MAIN, data_files(NEWEST, mutate=what_main_reads))
        self.assertEqual(flagged(done.stdout), flagged(frozen.stdout))


class TooNoisyToWatch(unittest.TestCase):
    """Each series whose own spread hides a step of its threshold is named, with the step it needs."""

    def test_the_noisy_series_are_listed_with_the_step_they_need(self):
        done, _ = run(SCRIPT, data_files(NEWEST))
        listed = noisy(done.stdout)
        self.assertIn("reference/ci/visible_latency_p99_ms", listed)
        self.assertIn("complex/ci/visible_latency_p99_ms", listed)
        self.assertNotIn("complex/ci/visible_latency_p50_ms", listed)
        for name, (step, limit) in listed.items():
            with self.subTest(series=name):
                self.assertGreater(step, limit)

    def test_the_list_goes_into_the_issue_under_the_findings(self):
        _, calls = run(SCRIPT, data_files(NEWEST), gh_issues=[328])
        body = calls[-1][calls[-1].index("--body") + 1]
        self.assertLess(body.index("| suite |"), body.index("Too noisy to watch"))
        self.assertIn("`reference/ci/visible_latency_p99_ms`", body[body.index("Too noisy to watch"):])


def load_facts():
    spec = importlib.util.spec_from_file_location("bench_runner_facts", FACTS)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


class RunnerFacts(unittest.TestCase):
    """`bench-runner-facts.py` stamps each data point with its runner, and never fails the job."""

    def stamp(self, points: dict[str, list], cwd: pathlib.Path | None = None, limit_memory: bool = False,
              raw: dict[str, str] | None = None) -> tuple[subprocess.CompletedProcess, dict]:
        with tempfile.TemporaryDirectory() as tmp:
            tmp = pathlib.Path(tmp)
            paths = []
            for name, value in points.items():
                path = tmp / f"{name}.json"
                path.write_text((raw or {}).get(name) or json.dumps(value))
                paths.append(path)
            work = cwd or tmp

            def low_memory():
                resource.setrlimit(resource.RLIMIT_AS, (200 * 1024 * 1024, 200 * 1024 * 1024))

            done = subprocess.run([sys.executable, str(FACTS), *map(str, paths)], capture_output=True, text=True,
                                  cwd=work, preexec_fn=low_memory if limit_memory else None)
            written = {p.stem: p.read_text() for p in paths}
        return done, written

    def test_every_point_is_stamped_with_the_machine_and_its_calibrations(self):
        done, written = self.stamp({
            "smaller": [{"name": "inprocess/a", "unit": "ns", "value": 1.0},
                        {"name": "inprocess/b", "unit": "ns", "value": 1.0, "extra": {"note": 1}},
                        {"name": "inprocess/c", "unit": "ns", "value": 1.0, "extra": 7}],
            "bigger": [{"name": "reference/ci/b", "unit": "docs/s", "value": 2.0, "extra": json.dumps({"images": IMAGES})}],
        })
        self.assertEqual(done.returncode, 0, done.stderr)
        small, big = json.loads(written["smaller"]), json.loads(written["bigger"])
        self.assertEqual([p["name"] for p in small], ["inprocess/a", "inprocess/b", "inprocess/c"])
        for point in small + big:
            runner = json.loads(point["extra"])["runner"]
            self.assertTrue(runner["cpu"])
            self.assertGreater(runner["cores"], 0)
            self.assertEqual(set(runner["calibration"]), {"cpu_sha256_ms", "memory_copy_ms", "disk_write_ms"})
        self.assertEqual(json.loads(big[0]["extra"])["images"], IMAGES)
        self.assertEqual(json.loads(small[1]["extra"])["note"], 1)

    def test_a_read_only_directory_leaves_out_the_disk_calibration(self):
        with tempfile.TemporaryDirectory() as locked:
            locked = pathlib.Path(locked)
            locked.chmod(0o555)
            try:
                done, written = self.stamp({"smaller": [{"name": "inprocess/a", "unit": "ns", "value": 1.0}]}, cwd=locked)
            finally:
                locked.chmod(0o755)
        self.assertEqual(done.returncode, 0, done.stderr)
        self.assertIn("warning:", done.stderr)
        runner = json.loads(json.loads(written["smaller"])[0]["extra"])["runner"]
        self.assertNotIn("disk_write_ms", runner["calibration"])
        self.assertIn("cpu_sha256_ms", runner["calibration"])

    def test_too_little_memory_leaves_out_what_it_needs(self):
        done, written = self.stamp({"smaller": [{"name": "inprocess/a", "unit": "ns", "value": 1.0}]}, limit_memory=True)
        self.assertEqual(done.returncode, 0, done.stderr)
        self.assertIn("warning:", done.stderr)
        runner = json.loads(json.loads(written["smaller"])[0]["extra"])["runner"]
        self.assertNotIn("memory_copy_ms", runner["calibration"])
        self.assertTrue(runner["cpu"])

    def test_a_file_it_cannot_read_is_left_as_it_was(self):
        done, written = self.stamp({"smaller": [], "bigger": [{"name": "reference/ci/b", "unit": "", "value": 2.0}]},
                                   raw={"smaller": "[{not json"})
        self.assertEqual(done.returncode, 0, done.stderr)
        self.assertIn("warning:", done.stderr)
        self.assertEqual(written["smaller"], "[{not json")
        self.assertIn("runner", json.loads(json.loads(written["bigger"])[0]["extra"]))

    def test_a_cpuinfo_without_a_model_name_falls_back(self):
        facts = load_facts()
        with tempfile.TemporaryDirectory() as tmp:
            cpuinfo = pathlib.Path(tmp) / "cpuinfo"
            cpuinfo.write_text("processor\t: 0\nflags\t\t: fpu\n")
            self.assertTrue(facts.cpu_model(cpuinfo))
            self.assertTrue(facts.cpu_model(pathlib.Path(tmp) / "absent"))


if __name__ == "__main__":
    unittest.main()
