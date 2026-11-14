"""Tests for bench-regression.py and bench-runner-facts.py, replayed over the real `main` history.

    python3 -m unittest discover -s .github/scripts -p 'test_*.py'

`fixtures/bench-history.json` is the `benchmarks` branch through run 78 (the 79th run,
commit 26a81616): every series of both `data.js` files, one value per run. Each test cuts
the history at a run, writes the two `data.js` files the workflow reads, and runs the
script exactly as `bench.yml` does (`25 --rss-threshold 20`), so the tests hold whatever
the script's internals. The stored runs recorded no runner facts; `facts=True` stamps the
fixture's offline-inferred runner kinds as facts to exercise that path.
"""
import json
import os
import pathlib
import re
import stat
import subprocess
import sys
import tempfile
import unittest

HERE = pathlib.Path(__file__).resolve().parent
SCRIPT = HERE / "bench-regression.py"
HISTORY = json.loads((HERE / "fixtures" / "bench-history.json").read_text(encoding="utf-8"))
IMAGES = {"opensearch": "opensearchproject/opensearch:2", "postgres": "postgres:16-alpine"}
NEWEST = len(HISTORY["runs"]) - 1
ALL = [name for suite in HISTORY["suites"].values() for name in suite["series"]]
P99 = {"complex/ci/visible_latency_p99_ms", "reference/ci/visible_latency_p99_ms"}
SCENARIOS = [name for name in ALL if "/ci/" in name]
COMPONENTS = [name for name in ALL if name.startswith("components/")]
INPROCESS = [name for name in ALL if name.startswith("inprocess/")]


def data_js(key: str, suite: dict, upto: int, steps: dict, image_change: dict, mutate, facts: bool) -> str:
    entries = []
    for run in range(upto + 1):
        benches = []
        for name, values in suite["series"].items():
            value = values[run]
            if value is None:
                continue
            start, factor, *end = steps.get(name, (None, 1.0))
            if start is not None and run >= start and (not end or run <= end[0]):
                value = value * factor if suite["tool"] == "customSmallerIsBetter" else value / factor
            extra = {}
            if "/ci/" in name:
                images = dict(IMAGES)
                if name in image_change and run >= image_change[name][0]:
                    images.update(image_change[name][1])
                extra["images"] = images
            if facts:
                kind = HISTORY["kinds"][run]
                extra["runner"] = {"cpu": f"kind {kind}", "cores": 4,
                                   "calibration": {"cpu_sha256_ms": 100 + kind, "memory_copy_ms": 50, "disk_write_ms": 80}}
            bench = {"name": name, "value": value, "unit": ""}
            if extra:
                bench["extra"] = json.dumps(extra)
            benches.append(bench)
        commit = HISTORY["runs"][run]
        entries.append({
            "commit": {"id": commit + "0" * 28, "url": f"https://github.com/alias2k/flusso/commit/{commit}"},
            "date": run,
            "tool": suite["tool"],
            "benches": benches,
        })
    if mutate:
        entries = mutate(key, entries)
    return "window.BENCHMARK_DATA = " + json.dumps({"entries": {suite["name"]: entries}})


def run_script(upto: int, steps: dict | None = None, image_change: dict | None = None, mutate=None,
               facts: bool = False, args: tuple = (), env: dict | None = None,
               raw: dict | None = None) -> subprocess.CompletedProcess:
    """The script's run, as `bench.yml` makes it, with `main` at run `upto`."""
    with tempfile.TemporaryDirectory() as tmp:
        paths = []
        for key, suite in HISTORY["suites"].items():
            path = pathlib.Path(tmp) / f"{key}.js"
            text = (raw or {}).get(key) or data_js(key, suite, upto, steps or {}, image_change or {}, mutate, facts)
            path.write_text(text, encoding="utf-8")
            paths.append(str(path))
        return subprocess.run(
            [sys.executable, str(SCRIPT), "25", "--rss-threshold", "20", *args, *paths],
            capture_output=True, text=True, env={**os.environ, **(env or {})},
        )


def findings(out: str) -> dict[str, str]:
    """Each flagged series, with the basis it was judged against."""
    return {name: basis for name, _, basis in
            re.findall(r"^  flusso \(\w+ is better\) (\S+):(.* against (same runner|all runners))?", out, re.M)}


def flagged(upto: int, steps: dict | None = None, image_change: dict | None = None, mutate=None,
            facts: bool = False) -> set[str]:
    """The series the script calls regressed when `main` is at run `upto`."""
    done = run_script(upto, steps, image_change, mutate, facts)
    assert done.returncode == 0, done.stderr
    return set(findings(done.stdout))


def too_noisy(upto: int, steps: dict | None = None, facts: bool = False) -> set[str]:
    """The series the script reports as too noisy to catch a step of the threshold."""
    out = run_script(upto, steps, facts=facts).stdout
    return set(re.findall(r"^too noisy for the threshold: flusso \(\w+ is better\) (\S+) ", out, re.M))


def doubled(group, at: int = NEWEST - 1) -> dict:
    return {name: (at, 2.0) for name in group}


class RealHistory(unittest.TestCase):
    """Every firing on the real 79 runs; the PR argues each one."""

    def test_the_only_findings_on_the_real_history_are_the_argued_ones(self):
        expected = {
            # The first runs of the slow kind of machine, past everything the runs before had done.
            4: {"complex/ci/cpu_seconds", "complex/ci/drain_changes_per_s", "components/baseline/pg_select_1",
                "components/baseline/select_1", "components/build/0", "components/build/1",
                "components/resolve/related_table", "reference/ci/backfill_docs_per_s", "reference/ci/cpu_seconds",
                "reference/ci/drain_changes_per_s"},
            5: {"complex/ci/backfill_docs_per_s", "complex/ci/cpu_seconds", "complex/ci/drain_changes_per_s",
                "components/baseline/pg_select_1", "components/baseline/select_1", "components/build/0",
                "components/build/1", "components/build/10", "components/resolve/related_table",
                "reference/ci/backfill_docs_per_s", "reference/ci/cpu_seconds", "reference/ci/drain_changes_per_s"},
            # A new machine for the in-process benches: no flusso change between runs 6 and 7.
            8: {"inprocess/live_drain/sinks1/64", "inprocess/live_drain/sinks2/64", "inprocess/snapshot/rows"},
            9: {"inprocess/live_drain/sinks1/64", "inprocess/live_drain/sinks2/64", "inprocess/snapshot/rows"},
            # PR #188 (run 18) made resolve/root_table ~70% slower: too early in the history for a same-runner
            # comparison, so it shows on all runners whenever two runs in a row are past everything before.
            21: {"components/resolve/root_table"}, 26: {"components/resolve/root_table"},
        }
        # Peak RSS +26% and +30% on runs 50 and 51 (designer-only commits, which the benchmark build leaves
        # out), held until three judged runs in a row were back within 10%.
        for run in range(51, 60):
            expected.setdefault(run, set()).add("reference/ci/peak_rss_mib")
        # Two slow flush_p99 runs of the reference scenario (57, 58), held until three runs were back.
        for run in range(58, 61):
            expected.setdefault(run, set()).add("reference/ci/flush_p99_ms")
        for run in range(67, NEWEST + 1):
            expected.setdefault(run, set()).add("components/resolve/root_table")
        for run in range(68, NEWEST + 1):
            expected.setdefault(run, set()).add("components/baseline/resolve_unrelated")
        for run in range(3, NEWEST + 1):
            with self.subTest(run=run):
                self.assertEqual(flagged(run), expected.get(run, set()))

    def test_the_resolve_step_names_where_it_started(self):
        self.assertRegex(run_script(NEWEST).stdout,
                         r"components/resolve/root_table: .* step after fb3f94b28d42, by 0cde709f0e51")


class UnmatchedRuns(unittest.TestCase):
    """Every run is judged: with no earlier run of its kind, against all runners (B1, 6)."""

    def test_the_newest_run_matches_no_earlier_run_and_is_judged_against_all_runners(self):
        self.assertRegex(run_script(NEWEST).stdout,
                         rf"newest run {HISTORY['runs'][NEWEST]}: 2 series judged against the same runner, "
                         r"49 against all runners")

    def test_a_doubling_of_every_series_in_the_last_two_runs_is_caught(self):
        got = findings(run_script(NEWEST, doubled(ALL)).stdout)
        self.assertGreaterEqual(len(got), 44)
        self.assertIn("all runners", got.values())

    def test_a_doubling_of_one_series_in_the_last_two_runs_is_caught(self):
        for name in ("components/build/1", "inprocess/decode/fixture", "reference/ci/drain_changes_per_s"):
            with self.subTest(series=name):
                self.assertEqual(findings(run_script(NEWEST, doubled([name])).stdout).get(name), "all runners")

    def test_a_series_four_runs_old_is_judged(self):
        def young(key, entries):
            for entry in entries[:NEWEST - 3]:
                entry["benches"] = [b for b in entry["benches"] if b["name"] != "components/build/1"]
            return entries

        self.assertIn("components/build/1", flagged(NEWEST, doubled(["components/build/1"]), mutate=young))


class SameRunner(unittest.TestCase):
    """Against the same runner a 30% step is caught, held for as long as the history goes, and let go once back."""

    def test_a_30_percent_step_at_run_45_is_flagged_on_every_run_after_it_is_called(self):
        for name in ("reference/ci/flush_p50_ms", "components/build/1", "inprocess/decode/fixture",
                     "complex/ci/backfill_docs_per_s"):
            for run in range(47, NEWEST + 1):
                with self.subTest(series=name, run=run):
                    self.assertIn(name, flagged(run, {name: (45, 1.3)}))

    def test_a_30_percent_step_is_caught_within_four_runs(self):
        for name in ("components/build/1", "reference/ci/drain_changes_per_s"):
            with self.subTest(series=name):
                self.assertTrue([run for run in (61, 62, 63, 64) if name in flagged(run, {name: (60, 1.3)})])

    def test_a_30_percent_peak_rss_step_is_caught_within_four_runs(self):
        # The reference scenario's peak RSS is listed too noisy for 20%; the complex one is not.
        name = "complex/ci/peak_rss_mib"
        self.assertTrue([run for run in (61, 62, 63, 64) if name in flagged(run, {name: (60, 1.3)})])

    def test_a_step_that_goes_away_is_let_go(self):
        steps = {"components/build/1": (50, 1.5, 60)}
        self.assertIn("components/build/1", flagged(60, steps))
        self.assertNotIn("components/build/1", flagged(66, steps))

    def test_one_outlier_does_not_carry_a_pair(self):
        # B4: at run 32 `live_drain/sinks1/64` had one spike and one run about 11% worse: not two runs past 25%.
        self.assertNotIn("inprocess/live_drain/sinks1/64", flagged(32))


class GroupSteps(unittest.TestCase):
    """A step in a group of series is caught and stays caught; with runner facts, up to every series (B3)."""

    GROUPS = {
        "every tenth series": ALL[::10],
        "the components suite, floors included": COMPONENTS,
        "the in-process suite": INPROCESS,
        "the scenarios": SCENARIOS,
    }

    def assert_caught(self, group, upto: int, steps: dict, facts: bool = False) -> None:
        noisy = too_noisy(NEWEST, facts=facts)
        self.assertLessEqual(len(noisy), 10, noisy)
        self.assertEqual(set(group) - P99 - noisy - flagged(upto, steps, facts=facts), set())

    def test_a_30_percent_step_on_every_component_and_both_floors_is_caught_on_the_postgres_benches(self):
        got = flagged(NEWEST, {name: (45, 1.3) for name in COMPONENTS})
        for name in ("components/build/0", "components/build/1", "components/build/10", "components/build/100",
                     "components/change/item_update", "components/resolve/related_table"):
            with self.subTest(series=name):
                self.assertIn(name, got)

    def test_a_50_percent_step_in_a_group_stays_caught_for_25_runs(self):
        # Caught by run 60 (runs 54 and 55 match no earlier run; `cpu_seconds` is whole seconds), still at 65 and 78.
        for label, group in self.GROUPS.items():
            steps = {name: (52, 1.5) for name in group}
            for run in (60, 65, NEWEST):
                with self.subTest(group=label, run=run):
                    self.assert_caught(group, run, steps)

    def test_with_runner_facts_a_50_percent_step_on_every_series_stays_caught(self):
        steps = {name: (52, 1.5) for name in ALL}
        for run in (60, 65, NEWEST):
            with self.subTest(run=run):
                self.assert_caught(ALL, run, steps, facts=True)


class ImageChanges(unittest.TestCase):
    """An image-tag change restarts a series on the new image's points (B2)."""

    POSTGRES = {"postgres": "postgres:16.1-alpine"}

    def test_an_image_change_that_changes_nothing_holds_nothing(self):
        plain = flagged(NEWEST)
        for at in range(40, 71, 2):
            with self.subTest(change_at=at):
                self.assertEqual(flagged(NEWEST, image_change={n: (at, self.POSTGRES) for n in SCENARIOS}), plain)

    def test_a_doubling_one_two_or_four_runs_after_an_image_change_is_reported_within_three_runs(self):
        for after in (1, 2, 4):
            change = {n: (60, self.POSTGRES) for n in SCENARIOS}
            steps = {n: (60 + after, 2.0) for n in SCENARIOS}
            with self.subTest(runs_after=after):
                seen = set().union(*(flagged(60 + after + k, steps, change) for k in (1, 2, 3)))
                self.assertGreaterEqual(len(seen & set(SCENARIOS)), 12)

    def test_a_step_landing_with_an_image_change_is_taken_for_the_images_level(self):
        # The documented blind spot: a new image's first run is its baseline.
        change = {n: (60, self.POSTGRES) for n in SCENARIOS}
        self.assertEqual(flagged(NEWEST, {n: (60, 1.6) for n in SCENARIOS}, change) & set(SCENARIOS), set())


class Reporting(unittest.TestCase):
    """What the script says about itself, and what it refuses to do."""

    def test_the_series_too_noisy_for_the_threshold_are_named(self):
        self.assertTrue(P99 <= too_noisy(NEWEST))

    def test_runs_are_ordered_by_time_when_one_file_lacks_a_run(self):
        def drop_run_10(key, entries):
            return [e for e in entries if not (key == "smaller" and e["date"] == 10)]

        self.assertIn(f"newest run {HISTORY['runs'][NEWEST]}", run_script(NEWEST, mutate=drop_run_10).stdout)
        self.assertEqual(flagged(NEWEST, mutate=drop_run_10), flagged(NEWEST))

    def test_malformed_entries_are_skipped_with_a_warning(self):
        def damage(key, entries):
            if key == "smaller":
                benches = entries[-1]["benches"]
                benches[0]["value"] = None
                benches[1]["value"] = "fast"
                del benches[2]["name"]
                entries[5]["commit"] = None
            return entries

        done = run_script(NEWEST, mutate=damage)
        self.assertEqual(done.returncode, 0)
        self.assertIn("warning:", done.stderr)
        self.assertIn("components/resolve/root_table", findings(done.stdout))

    def test_an_unreadable_file_fails_and_leaves_the_issue_alone(self):
        with tempfile.TemporaryDirectory() as tmp:
            calls = pathlib.Path(tmp) / "calls"
            gh = pathlib.Path(tmp) / "gh"
            gh.write_text(f'#!/bin/sh\necho "$@" >> {calls}\n'
                          'echo \'[{"number": 328, "title": "Performance regression on main"}]\'\n')
            gh.chmod(gh.stat().st_mode | stat.S_IEXEC)
            done = run_script(NEWEST, args=("--issue",), env={"PATH": f"{tmp}:{os.environ['PATH']}"},
                              raw={"smaller": 'window.BENCHMARK_DATA = {"entries": {"flusso (smal'})
            self.assertNotEqual(done.returncode, 0)
            self.assertIn("error:", done.stderr)
            self.assertFalse(calls.exists(), calls.read_text() if calls.exists() else "")

    def test_a_series_no_longer_recorded_ends_its_hold(self):
        def removed_after_the_step(key, entries):
            for entry in entries[NEWEST - 3:]:
                entry["benches"] = [b for b in entry["benches"] if b["name"] != "components/resolve/root_table"]
            return entries

        out = run_script(NEWEST, mutate=removed_after_the_step).stdout
        self.assertNotIn("components/resolve/root_table", findings(out))
        self.assertIn("no longer recorded, hold ended: flusso (smaller is better) components/resolve/root_table", out)


class RunnerFacts(unittest.TestCase):
    """`bench-runner-facts.py` stamps each data point with the runner it ran on."""

    def test_every_point_is_stamped_with_the_machine_and_its_calibrations(self):
        with tempfile.TemporaryDirectory() as tmp:
            smaller = pathlib.Path(tmp) / "smaller.json"
            bigger = pathlib.Path(tmp) / "bigger.json"
            smaller.write_text(json.dumps([{"name": "inprocess/a", "unit": "ns", "value": 1.0},
                                           {"name": "inprocess/b", "unit": "ns", "value": 1.0, "extra": {"note": 1}}]))
            bigger.write_text(json.dumps([{"name": "reference/ci/b", "unit": "docs/s", "value": 2.0,
                                           "extra": json.dumps({"images": IMAGES})}]))
            subprocess.run([sys.executable, str(HERE / "bench-runner-facts.py"), str(smaller), str(bigger)],
                           check=True, capture_output=True, text=True, cwd=tmp)
            small, big = json.loads(smaller.read_text()), json.loads(bigger.read_text())
        self.assertEqual([p["name"] for p in small], ["inprocess/a", "inprocess/b"])
        for point in small + big:
            runner = json.loads(point["extra"])["runner"]
            self.assertTrue(runner["cpu"])
            self.assertGreater(runner["cores"], 0)
            self.assertEqual(set(runner["calibration"]), {"cpu_sha256_ms", "memory_copy_ms", "disk_write_ms"})
            self.assertTrue(all(v > 0 for v in runner["calibration"].values()))
        self.assertEqual(json.loads(big[0]["extra"])["images"], IMAGES)
        self.assertEqual(json.loads(small[1]["extra"])["note"], 1)


if __name__ == "__main__":
    unittest.main()
