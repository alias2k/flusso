"""Tests for bench-regression.py, replayed over the real `main` history.

    python3 -m unittest discover -s .github/scripts -p 'test_*.py'

`fixtures/bench-history.json` is the `benchmarks` branch through run 77
(commit 7a22e753): every series of both `data.js` files, one value per run.
Each test cuts the history at a run, writes the two `data.js` files the
workflow reads, and runs the script exactly as `bench.yml` does
(`25 --rss-threshold 20`), so the tests hold whatever the script's internals.
"""
import json
import pathlib
import re
import subprocess
import sys
import tempfile
import unittest

HERE = pathlib.Path(__file__).resolve().parent
SCRIPT = HERE / "bench-regression.py"
HISTORY = json.loads((HERE / "fixtures" / "bench-history.json").read_text(encoding="utf-8"))
IMAGES = {"opensearch": "opensearchproject/opensearch:2", "postgres": "postgres:16-alpine"}


FLOORS = {"components/baseline/select_1", "components/baseline/pg_select_1"}
JUDGED = [name for suite in HISTORY["suites"].values() for name in suite["series"] if name not in FLOORS]
P99 = {"complex/ci/visible_latency_p99_ms", "reference/ci/visible_latency_p99_ms"}
SCENARIOS = [name for name in JUDGED if "/ci/" in name]


def data_js(key: str, suite: dict, upto: int, steps: dict, image_change: dict, mutate) -> str:
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
            bench = {"name": name, "value": value, "unit": ""}
            if "/ci/" in name:
                images = dict(IMAGES)
                if name in image_change and run >= image_change[name]:
                    images["opensearch"] = "opensearchproject/opensearch:3"
                bench["extra"] = json.dumps({"images": images})
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


def run_script(upto: int, steps: dict | None = None, image_change: dict | None = None, mutate=None) -> subprocess.CompletedProcess:
    """The script's run, as `bench.yml` makes it, with `main` at run `upto`."""
    with tempfile.TemporaryDirectory() as tmp:
        paths = []
        for key, suite in HISTORY["suites"].items():
            path = pathlib.Path(tmp) / f"{key}.js"
            path.write_text(data_js(key, suite, upto, steps or {}, image_change or {}, mutate), encoding="utf-8")
            paths.append(str(path))
        return subprocess.run(
            [sys.executable, str(SCRIPT), "25", "--rss-threshold", "20", *paths],
            check=True, capture_output=True, text=True,
        )


def flagged(upto: int, steps: dict | None = None, image_change: dict | None = None, mutate=None) -> set[str]:
    """The series the script calls regressed when `main` is at run `upto`."""
    out = run_script(upto, steps, image_change, mutate).stdout
    return set(re.findall(r"^  flusso \(\w+ is better\) (\S+):", out, re.M))


def too_noisy(upto: int, steps: dict | None = None, image_change: dict | None = None) -> set[str]:
    """The series the script reports as too noisy to catch a step of the threshold."""
    out = run_script(upto, steps, image_change).stdout
    return set(re.findall(r"^too noisy for the threshold: flusso \(\w+ is better\) (\S+) ", out, re.M))


class RunnerWideShifts(unittest.TestCase):
    """A slower runner moves nearly every series of one run together; that is no series' regression."""

    def test_the_slow_runner_at_run_77_flags_only_the_real_resolve_step(self):
        # Runs 76 and 77 landed on a runner ~25% slower: 16 series crossed 25% together.
        # The two resolve series carry the lasting step from PR #297 (see LastingSteps).
        self.assertEqual(flagged(77), {"components/resolve/root_table", "components/baseline/resolve_unrelated"})

    def test_the_slow_runners_at_run_15_flag_none_of_the_series_that_moved_with_them(self):
        moved_with_the_runner = {
            "components/batch_size/100", "components/bulk_index/1", "components/bulk_index/100",
            "complex/ci/visible_latency_p99_ms", "reference/ci/visible_latency_p50_ms", "reference/ci/flush_p99_ms",
        }
        self.assertEqual(flagged(15) & moved_with_the_runner, set())


class TwoLevelSeries(unittest.TestCase):
    """A series that flips between two levels is judged against both, not against whichever the window holds."""

    def test_reference_p99_latency_swinging_78_to_1254_ms_is_not_a_regression(self):
        for run in (21, 58, 76, 77):
            with self.subTest(run=run):
                self.assertNotIn("reference/ci/visible_latency_p99_ms", flagged(run))

    def test_reference_p50_latency_flipping_28_to_50_ms_is_not_a_regression(self):
        for run in (15, 77):
            with self.subTest(run=run):
                self.assertNotIn("reference/ci/visible_latency_p50_ms", flagged(run))


class LastingSteps(unittest.TestCase):
    """A real, lasting step stays flagged while the new level settles in; it is not absorbed into the baseline."""

    def test_resolve_after_pr_297_stays_flagged_on_both_series_it_moved(self):
        # PR #297 (run 66) made `resolve/root_table` ~50% and `baseline/resolve_unrelated` ~35% slower for good.
        for run in range(67, 78):
            with self.subTest(run=run):
                self.assertIn("components/resolve/root_table", flagged(run))
        for run in range(68, 78):
            with self.subTest(run=run):
                self.assertIn("components/baseline/resolve_unrelated", flagged(run))

    def test_a_step_stays_flagged_after_its_new_level_fills_the_window(self):
        # Fifteen runs on the new level: more than half of any baseline window would sit on it.
        name = "reference/ci/flush_p50_ms"
        self.assertIn(name, flagged(77, {name: (62, 1.4)}))


class CatchesARegression(unittest.TestCase):
    """A lasting regression just past the threshold is caught within a few `main` runs."""

    def test_a_30_percent_step_on_a_runner_sensitive_bench_is_caught_by_its_third_run(self):
        # `build/1` swings ±30% with the runner; the step lands at run 60.
        caught = [run for run in range(61, 63) if "components/build/1" in flagged(run, {"components/build/1": (60, 1.30)})]
        self.assertTrue(caught)

    def test_a_30_percent_drop_in_drain_throughput_is_caught_by_its_third_run(self):
        caught = [run for run in range(61, 63)
                  if "reference/ci/drain_changes_per_s" in flagged(run, {"reference/ci/drain_changes_per_s": (60, 1.30)})]
        self.assertTrue(caught)

    def test_a_25_percent_peak_rss_step_is_caught_by_its_third_run(self):
        caught = [run for run in range(61, 63)
                  if "reference/ci/peak_rss_mib" in flagged(run, {"reference/ci/peak_rss_mib": (60, 1.25)})]
        self.assertTrue(caught)

    def test_a_step_landing_with_an_image_change_is_read_as_the_images_own(self):
        name = "reference/ci/flush_p50_ms"
        self.assertNotIn(name, flagged(62, {name: (60, 1.6)}, {name: 60}))
        self.assertIn(name, flagged(62, {name: (60, 1.6)}))


class HeldSteps(unittest.TestCase):
    """A lasting step stays flagged for as long as the history goes, and lets go once the series is back."""

    def test_a_30_percent_step_at_run_45_is_flagged_on_every_run_after_it_is_called(self):
        for name in ("reference/ci/flush_p50_ms", "components/build/1", "inprocess/decode/fixture",
                     "complex/ci/backfill_docs_per_s"):
            for run in range(50, 78):
                with self.subTest(series=name, run=run):
                    self.assertIn(name, flagged(run, {name: (45, 1.3)}))

    def test_a_step_that_goes_away_is_let_go(self):
        name = "components/build/1"
        steps = {name: (50, 1.5, 60)}
        self.assertIn(name, flagged(60, steps))
        self.assertNotIn(name, flagged(66, steps))


class GroupSteps(unittest.TestCase):
    """A regression that moves many series at once, up to every series, is caught and stays caught.

    The two `SELECT 1` floors are the runner's evidence: no flusso change can move them, so no
    case here moves them either.
    """

    GROUPS = {
        "every tenth series": JUDGED[::10],
        "the components suite": [n for n in JUDGED if n.startswith("components/")],
        "the in-process suite": [n for n in JUDGED if n.startswith("inprocess/")],
        "the scenarios": SCENARIOS,
        "every series": JUDGED,
    }

    def assert_caught(self, group: list[str], upto: int, steps: dict) -> None:
        noisy = too_noisy(upto, steps)
        self.assertLessEqual(len(noisy), 10, noisy)
        missed = set(group) - P99 - noisy - flagged(upto, steps)
        self.assertEqual(missed, set())

    def test_a_doubling_in_the_last_two_runs_is_caught(self):
        for label, group in self.GROUPS.items():
            with self.subTest(group=label):
                self.assert_caught(group, 77, {name: (76, 2.0) for name in group})

    def test_a_50_percent_step_stays_caught_for_25_runs(self):
        # Runs 54 and 55 ran on machines no earlier run matches, so they are not judged:
        # the first look after the step at 52 with two judged runs past it is 57.
        for label, group in self.GROUPS.items():
            steps = {name: (52, 1.5) for name in group}
            for run in (57, 65, 77):
                with self.subTest(group=label, run=run):
                    self.assert_caught(group, run, steps)


class ImageChanges(unittest.TestCase):
    """After an image-tag change a series is judged again from the image's third run."""

    def test_a_step_four_or_six_runs_after_an_image_change_is_caught(self):
        for back in (4, 6):
            change = {name: 78 - back for name in SCENARIOS}
            steps = {name: (76, 1.5) for name in SCENARIOS}
            with self.subTest(runs_back=back):
                missed = set(SCENARIOS) - P99 - too_noisy(77, steps, change) - flagged(77, steps, change)
                self.assertEqual(missed, set())


class Reporting(unittest.TestCase):
    """What the script says about itself."""

    def test_the_series_too_noisy_for_the_threshold_are_named(self):
        self.assertTrue(P99 <= too_noisy(77))

    def test_a_newest_run_on_an_unseen_runner_is_reported_as_not_judged(self):
        steps = {name: (77, 1.6) for name in FLOORS}
        out = run_script(77, steps).stdout
        self.assertRegex(out, r"newest run 7a22e7536658: judged 0 of \d+ series")
        self.assertIn("components/resolve/root_table", flagged(77, steps))

    def test_runs_are_ordered_by_time_when_one_file_lacks_a_run(self):
        def drop_run_10(key, entries):
            return [e for e in entries if not (key == "smaller" and e["date"] == 10)]

        out = run_script(77, mutate=drop_run_10).stdout
        self.assertIn("newest run 7a22e7536658", out)
        self.assertEqual(flagged(77, mutate=drop_run_10), flagged(77))

    def test_malformed_entries_are_skipped_with_a_warning(self):
        def damage(key, entries):
            if key == "smaller":
                benches = entries[-1]["benches"]
                benches[0]["value"] = None
                benches[1]["value"] = "fast"
                del benches[2]["name"]
                entries[5]["commit"] = None
            return entries

        done = run_script(77, mutate=damage)
        self.assertEqual(done.returncode, 0)
        self.assertIn("warning:", done.stderr)
        self.assertIn("components/resolve/root_table", flagged(77, mutate=damage))


class RunnerFacts(unittest.TestCase):
    """`bench-runner-facts.py` stamps each data point with the runner it ran on."""

    def test_every_point_is_stamped_and_a_calibration_point_is_added(self):
        with tempfile.TemporaryDirectory() as tmp:
            smaller = pathlib.Path(tmp) / "smaller.json"
            bigger = pathlib.Path(tmp) / "bigger.json"
            smaller.write_text(json.dumps([{"name": "inprocess/a", "unit": "ns", "value": 1.0}]))
            bigger.write_text(json.dumps([{"name": "reference/ci/b", "unit": "docs/s", "value": 2.0,
                                           "extra": json.dumps({"images": IMAGES})}]))
            subprocess.run([sys.executable, str(HERE / "bench-runner-facts.py"), str(smaller), str(bigger)],
                           check=True, capture_output=True, text=True)
            small, big = json.loads(smaller.read_text()), json.loads(bigger.read_text())
        self.assertEqual([p["name"] for p in small], ["inprocess/a", "runner/sha256_64mib"])
        self.assertGreater(small[-1]["value"], 0)
        for point in small + big:
            runner = json.loads(point["extra"])["runner"]
            self.assertTrue(runner["cpu"])
            self.assertGreater(runner["cores"], 0)
        self.assertEqual(json.loads(big[0]["extra"])["images"], IMAGES)


if __name__ == "__main__":
    unittest.main()
