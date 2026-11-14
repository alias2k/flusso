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


def data_js(suite: dict, upto: int, steps: dict, image_change: dict) -> str:
    entries = []
    for run in range(upto + 1):
        benches = []
        for name, values in suite["series"].items():
            value = values[run]
            if value is None:
                continue
            factor = steps.get(name, (None, 1.0))
            if factor[0] is not None and run >= factor[0]:
                value = value * factor[1] if suite["tool"] == "customSmallerIsBetter" else value / factor[1]
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
    return "window.BENCHMARK_DATA = " + json.dumps({"entries": {suite["name"]: entries}})


def flagged(upto: int, steps: dict | None = None, image_change: dict | None = None) -> set[str]:
    """The series the script calls regressed when `main` is at run `upto`."""
    steps = steps or {}
    image_change = image_change or {}
    with tempfile.TemporaryDirectory() as tmp:
        paths = []
        for key, suite in HISTORY["suites"].items():
            path = pathlib.Path(tmp) / f"{key}.js"
            path.write_text(data_js(suite, upto, steps, image_change), encoding="utf-8")
            paths.append(str(path))
        out = subprocess.run(
            [sys.executable, str(SCRIPT), "25", "--rss-threshold", "20", *paths],
            check=True, capture_output=True, text=True,
        ).stdout
    return set(re.findall(r"^  flusso \(\w+ is better\) (\S+):", out, re.M))


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

    def test_an_image_change_restarts_the_window(self):
        name = "reference/ci/flush_p50_ms"
        self.assertNotIn(name, flagged(62, {name: (60, 1.6)}, {name: 60}))
        self.assertIn(name, flagged(62, {name: (60, 1.6)}))


if __name__ == "__main__":
    unittest.main()
