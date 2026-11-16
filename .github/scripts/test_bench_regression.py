"""Tests for bench-regression.py and bench-runner-facts.py, over the real `main` history.

    python3 -m unittest discover -s .github/scripts -p 'test_*.py'

`fixtures/bench-history.json` is the `benchmarks` branch through run 78 (the 79th run, commit
26a81616): every series of both `data.js` files, one value per run. Each test cuts the history at a
run, writes the two `data.js` files the workflow reads, and runs the script exactly as `bench.yml`
does (`25 --rss-threshold 20`). `fixtures/frozen_main_bench_regression.py` is a frozen copy of the
script as on `main` before PR #331 (main's file plus two comment lines recording the commit and the
file's SHA-256, checked below): the detection rule must flag exactly what it flags. One difference is
deliberate: a point whose `extra` has no `images` key reads as having no image, as a point without
`extra` does, so stamping runner facts never restarts a series' window; main read it as `"null"`.
No point in the stored history has an `extra` without `images`, so the replay is unaffected.
"""
import hashlib
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
from unittest import mock

HERE = pathlib.Path(__file__).resolve().parent
SCRIPT = HERE / "bench-regression.py"
FROZEN_MAIN = HERE / "fixtures" / "frozen_main_bench_regression.py"
FACTS = HERE / "bench-runner-facts.py"
HISTORY = json.loads((HERE / "fixtures" / "bench-history.json").read_text(encoding="utf-8"))
IMAGES = {"opensearch": "opensearchproject/opensearch:2", "postgres": "postgres:16-alpine"}
NEWEST = len(HISTORY["runs"]) - 1
SCENARIOS = [name for suite in HISTORY["suites"].values() for name in suite["series"] if "/ci/" in name]
ISSUE = "Performance regression on main"


RUNNER = {"cpu": "Example CPU", "cores": 4, "calibration": {"cpu_sha256_ms": 100.0}}


def entries(suite: dict, upto: int, steps: dict, image_change: dict, facts_from: int | None = None) -> list[dict]:
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
            extra = {}
            if "/ci/" in name:
                images = dict(IMAGES)
                if name in image_change and run >= image_change[name]:
                    images["opensearch"] = "opensearchproject/opensearch:3"
                extra["images"] = images
            if facts_from is not None and run >= facts_from:
                extra["runner"] = RUNNER
            if extra:
                bench["extra"] = json.dumps(extra, sort_keys=True)
            benches.append(bench)
        commit = HISTORY["runs"][run]
        out.append({
            "commit": {"id": commit + "0" * 28, "url": f"https://github.com/alias2k/flusso/commit/{commit}"},
            "date": run,
            "tool": suite["tool"],
            "benches": benches,
        })
    return out


def data_files(upto: int, steps: dict | None = None, image_change: dict | None = None, mutate=None,
               facts_from: int | None = None) -> dict[str, str]:
    """The two `data.js` texts, as the `benchmarks` branch holds them, with `main` at run `upto`."""
    texts = {}
    for key, suite in HISTORY["suites"].items():
        points = entries(suite, upto, steps or {}, image_change or {}, facts_from)
        if mutate:
            points = mutate(key, points)
        texts[key] = "window.BENCHMARK_DATA = " + json.dumps({"entries": {suite["name"]: points}})
    return texts


BOT = "app/github-actions"


class Gh:
    """A fake `gh` on `PATH` that records its calls and answers `issue list` with `open_issues`.

    Each open issue is a number (opened by the workflow's bot, labelled `bug`, as the script opens it)
    or a `(number, author)` pair, labelled `bug` too. Like a search that matched loosely, the answer
    ignores the call's filters: the script must check them.
    """

    def __init__(self, tmp: pathlib.Path, open_issues: list):
        self.calls = tmp / "gh-calls.jsonl"
        issues = [i if isinstance(i, tuple) else (i, BOT) for i in open_issues]
        listed = json.dumps([{"number": n, "title": ISSUE, "author": {"login": who}, "labels": [{"name": "bug"}]}
                             for n, who in issues]
                            + [{"number": 998, "title": ISSUE + " (not this one)", "author": {"login": BOT},
                                "labels": [{"name": "bug"}]}])
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


def run(script: pathlib.Path, texts: dict[str, str], *args: str, gh_issues: list | None = None,
        missing: bool = False, env: dict | None = None, power: bool = False) -> tuple[subprocess.CompletedProcess, list[list[str]]]:
    """Run `script` over the two files as `bench.yml` does; with `gh_issues`, under `--issue` and a fake `gh`.

    Unless `power` is asked for, this script runs with `--skip-detection-power`: the measurement changes no
    flag, and skipping it keeps the suite fast. `bench.yml` never passes it.
    """
    with tempfile.TemporaryDirectory() as tmp:
        tmp = pathlib.Path(tmp)
        paths = []
        for key, text in texts.items():
            path = tmp / f"{key}.js"
            path.write_text(text, encoding="utf-8")
            paths.append(str(path))
        if missing:
            paths.append(str(tmp / "absent.js"))
        env = {k: v for k, v in os.environ.items() if not k.startswith("GITHUB_")} | (env or {})
        gh = None
        if gh_issues is not None:
            gh = Gh(tmp, gh_issues)
            env["PATH"] = gh.path
            args = (*args, "--issue")
        if script == SCRIPT and not power:
            args = ("--skip-detection-power", *args)
        done = subprocess.run([sys.executable, str(script), "25", "--rss-threshold", "20", *args, *paths],
                              capture_output=True, text=True, env=env)
        return done, gh.recorded() if gh else []


def flagged(out: str) -> set[str]:
    return set(re.findall(r"^  flusso \(\w+ is better\) (\S+): ", out, re.M))


POWER_LINE = re.compile(
    r"^    flusso \(\w+ is better\) (\S+): threshold (\d+)%, "
    r"(?:a lasting step of about \+(\d+)% is caught within two runs 9 times out of 10|not reliably caught up to \+(\d+)%)$",
    re.M)


def power(out: str) -> dict[str, tuple[int, bool, int]]:
    """The detection-power table: each series' step, whether it is caught 9 times out of 10, its threshold."""
    return {name: (int(caught or tried), bool(caught), int(limit))
            for name, limit, caught, tried in POWER_LINE.findall(out)}


def power_order(out: str) -> list[str]:
    return [m[0] for m in POWER_LINE.findall(out)]


def load_script(path: pathlib.Path, name: str):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def flagged_in_process(module, texts: dict[str, str], read) -> set[str]:
    """What a script's `analyse` flags over the two files, each read with `read(module, path)`."""
    found = set()
    with tempfile.TemporaryDirectory() as tmp:
        for key, text in texts.items():
            path = pathlib.Path(tmp) / f"{key}.js"
            path.write_text(text, encoding="utf-8")
            found |= {f["series"] for f in module.analyse(read(module, str(path)), 25, 20, 5, 3.0)}
    return found


class MainsRuleIsUnchanged(unittest.TestCase):
    """The detection rule is main's, exactly: the same (series, run) pairs are flagged.

    The replays call each script's `analyse` in-process (this script reads through its `clean`); one case
    runs both scripts end to end as `bench.yml` does.
    """

    MINE = load_script(SCRIPT, "bench_regression")
    MAIN = load_script(FROZEN_MAIN, "frozen_main_bench_regression")

    def assert_same(self, texts: dict[str, str], label: str) -> int:
        mine = flagged_in_process(self.MINE, texts, lambda m, p: m.clean(m.load(p)))
        main = flagged_in_process(self.MAIN, texts, lambda m, p: m.load(p))
        self.assertEqual(mine, main, label)
        return len(main)

    def test_the_scripts_end_to_end_flag_the_same(self):
        for upto in (40, 68, NEWEST):
            mine, _ = run(SCRIPT, data_files(upto))
            frozen, _ = run(FROZEN_MAIN, data_files(upto))
            with self.subTest(run=upto):
                self.assertEqual(frozen.returncode, 0, frozen.stderr)
                self.assertEqual(mine.returncode, 0, mine.stderr)
                self.assertEqual(flagged(mine.stdout), flagged(frozen.stdout))

    def test_the_frozen_copy_is_mains_file(self):
        lines = FROZEN_MAIN.read_text(encoding="utf-8").splitlines(keepends=True)
        recorded = re.search(r"sha256 ([0-9a-f]{64})", lines[1]).group(1)
        self.assertEqual(hashlib.sha256("".join(lines[:1] + lines[3:]).encode()).hexdigest(), recorded)

    def test_every_prefix_of_the_real_history_flags_what_main_flags(self):
        flags = 0
        for upto in range(3, NEWEST + 1):
            with self.subTest(run=upto):
                flags += self.assert_same(data_files(upto), f"run {upto}")
        self.assertGreater(flags, 30)

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
    """One issue, opened, kept current and closed as on main."""

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


class OnlyItsOwnIssue(unittest.TestCase):
    """The script touches one issue: the newest open one the workflow's bot opened with its title and label."""

    def test_the_list_asks_for_the_bots_labelled_open_issues(self):
        _, calls = run(SCRIPT, data_files(40), gh_issues=[])
        listing = calls[0]
        self.assertEqual(listing[:4], ["issue", "list", "--state", "open"])
        self.assertEqual(listing[listing.index("--label") + 1], "bug")
        self.assertEqual(listing[listing.index("--author") + 1], BOT)
        self.assertIn(f'in:title "{ISSUE}"', listing[listing.index("--search") + 1])
        self.assertIn("author", listing[listing.index("--json") + 1].split(","))

    def test_a_persons_issue_with_the_title_is_left_alone_on_a_flagged_run(self):
        done, calls = run(SCRIPT, data_files(NEWEST), gh_issues=[(999, "someone")])
        self.assertEqual(done.returncode, 0, done.stderr)
        self.assertEqual([c[:2] for c in calls], [["issue", "list"], ["issue", "create"]])

    def test_a_persons_issue_with_the_title_is_left_alone_on_a_clean_run(self):
        done, calls = run(SCRIPT, data_files(40), gh_issues=[(999, "someone")])
        self.assertEqual(done.returncode, 0, done.stderr)
        self.assertEqual([c[:2] for c in calls], [["issue", "list"]])

    def test_its_own_issue_is_updated_beside_a_persons(self):
        _, calls = run(SCRIPT, data_files(NEWEST), gh_issues=[(999, "someone"), 328])
        self.assertEqual([c[:3] for c in calls[1:]], [["issue", "edit", "328"]])

    def test_of_two_of_its_issues_only_the_newest_is_updated(self):
        done, calls = run(SCRIPT, data_files(NEWEST), gh_issues=[341, 357, (360, "someone")])
        self.assertEqual([c[:3] for c in calls[1:]], [["issue", "edit", "357"]])
        self.assertIn("left alone: #341", done.stdout)

    def test_of_two_of_its_issues_only_the_newest_is_closed(self):
        done, calls = run(SCRIPT, data_files(40), gh_issues=[341, 357])
        self.assertEqual([c[:3] for c in calls[1:]], [["issue", "close", "357"]])
        self.assertIn("left alone: #341", done.stdout)

    def test_a_closed_issue_is_not_reopened(self):
        """Only open issues are listed, so with its last issue closed a flagged run opens a new one."""
        _, calls = run(SCRIPT, data_files(NEWEST), gh_issues=[])
        self.assertEqual(calls[0][:4], ["issue", "list", "--state", "open"])
        self.assertEqual([c[:2] for c in calls], [["issue", "list"], ["issue", "create"]])


class UnreadableHistory(unittest.TestCase):
    """A data file that can't be read fails the run before `gh` is called (B5)."""

    CASES = {
        "truncated": 'window.BENCHMARK_DATA = {"entries": {"flusso (smal',
        "no object": "window.BENCHMARK_DATA = []",
        "not an object": 'window.BENCHMARK_DATA = {"a": 1}\n{',
        "empty": "",
        "entries not an object": 'window.BENCHMARK_DATA = {"entries": []}',
        "a suite that is not a list": 'window.BENCHMARK_DATA = {"entries": {"flusso (smaller is better)": null}}',
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
        damaged = {"components/baseline/os_flush_1": None, "components/baseline/pg_select_1": "fast"}

        def damage(key, points):
            if key == "smaller":
                for bench in points[-1]["benches"]:
                    if bench["name"] in damaged:
                        bench["value"] = damaged[bench["name"]]
                points[-1]["benches"].append({"value": 1.0, "unit": "ns"})
                points[-1]["benches"].append("not a bench")
                points[-1]["benches"].append({"name": "inprocess/a_flag", "value": True, "unit": ""})
                points[5]["commit"] = None
                points[6]["benches"] = None
            return points

        def what_main_reads(key, points):
            if key == "smaller":
                points[-1]["benches"] = [b for b in points[-1]["benches"] if b["name"] not in damaged]
                points[-1]["benches"].append({"name": "inprocess/a_flag", "value": True, "unit": ""})
                points[5]["commit"] = {}
                points[6]["benches"] = []
            return points

        done, _ = run(SCRIPT, data_files(NEWEST, mutate=damage))
        self.assertEqual(done.returncode, 0, done.stderr)
        self.assertEqual(len(re.findall(r"^warning:", done.stderr, re.M)), 6, done.stderr)
        self.assertIn("benches", done.stderr)
        self.assertNotIn("a_flag", done.stderr)
        frozen, _ = run(FROZEN_MAIN, data_files(NEWEST, mutate=what_main_reads))
        self.assertEqual(flagged(done.stdout), flagged(frozen.stdout))


class StampedHistory(unittest.TestCase):
    """Runner facts in `extra` never restart a series' window: a step is caught as without them."""

    def test_a_step_right_after_the_first_stamped_run_is_flagged_as_without_facts(self):
        for name in ("inprocess/decode/fixture", "components/build/1", "reference/ci/flush_p50_ms"):
            steps = {name: (60, 3.0)}
            plain_runs, stamped_runs = set(), set()
            for upto in range(60, 66):
                if name in flagged(run(SCRIPT, data_files(upto, steps))[0].stdout):
                    plain_runs.add(upto)
                if name in flagged(run(SCRIPT, data_files(upto, steps, facts_from=60))[0].stdout):
                    stamped_runs.add(upto)
            with self.subTest(series=name):
                self.assertTrue(plain_runs)
                self.assertEqual(stamped_runs, plain_runs)


class DetectionPower(unittest.TestCase):
    """How large a lasting step the rule catches on each series, measured, in the summary; a count in the issue."""

    def test_every_series_gets_its_measured_step_weakest_first(self):
        done, _ = run(SCRIPT, data_files(NEWEST), power=True)
        table = power(done.stdout)
        self.assertEqual(len(table), sum(len(s["series"]) for s in HISTORY["suites"].values()))
        self.assertFalse(table["reference/ci/visible_latency_p99_ms"][1])
        self.assertTrue(table["complex/ci/visible_latency_p50_ms"][1])
        for name, (step, _, limit) in table.items():
            with self.subTest(series=name):
                self.assertGreater(step, limit)
        weakness = [(0, 0) if not table[n][1] else (1, -table[n][0] / table[n][2]) for n in power_order(done.stdout)]
        self.assertEqual(weakness, sorted(weakness))

    def test_the_summary_header_gives_the_count_and_the_median(self):
        done, _ = run(SCRIPT, data_files(NEWEST), power=True)
        self.assertRegex(done.stdout, r"detection power, weakest first: on \d+ of 51 series a lasting step of 1\.5 times "
                                      r"the threshold is caught within two runs 9 times out of 10; the median series "
                                      r"needs about \+\d+%")

    def test_series_no_longer_recorded_or_too_short_are_left_out_and_counted(self):
        def thin(key, points):
            if key == "smaller":
                for point in points[-50:]:
                    point["benches"] = [b for b in point["benches"] if b["name"] != "components/build/1"]
                for point in points[:-10]:
                    point["benches"] = [b for b in point["benches"] if b["name"] != "inprocess/render/wide"]
            return points

        done, _ = run(SCRIPT, data_files(NEWEST, mutate=thin), power=True)
        table = power(done.stdout)
        self.assertNotIn("components/build/1", table)
        self.assertNotIn("inprocess/render/wide", table)
        self.assertEqual(len(table), 49)
        self.assertRegex(done.stdout, r"left out: 1 series with too few runs to measure \(fewer than \d+\), "
                                      r"1 series not recorded in the last 50 runs")

    def test_the_listed_step_is_caught_within_two_runs_and_half_of_it_at_most_half_the_time(self):
        # The script measures on the last 20 runs: a step from each run `c` there, judged at `c + 1`.
        table = power(run(SCRIPT, data_files(NEWEST), power=True)[0].stdout)
        cuts = range(NEWEST - 20, NEWEST)
        for name in ("components/build/1", "reference/ci/flush_p50_ms"):
            step, reliable, _ = table[name]
            self.assertTrue(reliable, name)
            for size, check in ((step, lambda n: self.assertGreaterEqual(n, 18)),
                                (step / 2, lambda n: self.assertLessEqual(n, 10))):
                caught = sum(name in flagged(run(SCRIPT, data_files(c + 1, {name: (c, 1 + size / 100)}))[0].stdout)
                             for c in cuts)
                with self.subTest(series=name, step=size):
                    check(caught)

    def test_the_issue_gets_two_sentences_and_a_link_to_the_run(self):
        github = {"GITHUB_SERVER_URL": "https://github.com", "GITHUB_REPOSITORY": "alias2k/flusso", "GITHUB_RUN_ID": "42"}
        for env, link in ((github, "https://github.com/alias2k/flusso/actions/runs/42"), ({}, None)):
            done, calls = run(SCRIPT, data_files(NEWEST), gh_issues=[328], env=env, power=True)
            body = calls[-1][calls[-1].index("--body") + 1]
            table = power(done.stdout)
            at_one_and_a_half = sum(1 for step, reliable, limit in table.values() if reliable and step <= 1.5 * limit + 0.5)
            with self.subTest(linked=bool(link)):
                paragraph = body[body.index("Detection power"):].split("\n\n")[0]
                self.assertLess(body.index("| suite |"), body.index("Detection power"))
                self.assertIn(f"on {at_one_and_a_half} of 51 series the rule catches a lasting step of 1.5 times its "
                              "threshold within two runs 9 times out of 10", paragraph)
                self.assertGreater(at_one_and_a_half, 0)
                self.assertRegex(paragraph, r"median series needs a lasting step of about \+\d+%")
                self.assertLessEqual(paragraph.count(". "), 1)
                self.assertNotIn("`reference/ci/visible_latency_p99_ms`", body)
                if link:
                    self.assertIn(link, paragraph)
                else:
                    self.assertIn("summary of the run", paragraph)


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
        self.assertIn("stamped 2 of 2 files", done.stdout)
        small, big = json.loads(written["smaller"]), json.loads(written["bigger"])
        self.assertEqual([p["name"] for p in small], ["inprocess/a", "inprocess/b", "inprocess/c"])
        for point in small + big:
            runner = json.loads(point["extra"])["runner"]
            self.assertTrue(runner["cpu"])
            self.assertGreater(runner["cores"], 0)
            self.assertEqual(set(runner["calibration"]), {"cpu_sha256_ms", "memory_copy_ms", "disk_write_ms"})
        self.assertEqual(json.loads(big[0]["extra"])["images"], IMAGES)
        self.assertEqual(json.loads(small[1]["extra"])["note"], 1)

    @unittest.skipIf(os.geteuid() == 0, "root writes into a read-only directory")
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

    def test_a_write_that_fails_half_way_leaves_the_file_as_it_was(self):
        facts = load_facts()
        with tempfile.TemporaryDirectory() as tmp:
            path = pathlib.Path(tmp) / "smaller.json"
            original = json.dumps([{"name": "inprocess/a", "unit": "ns", "value": 1.0}])
            path.write_text(original)
            with mock.patch.object(facts.os, "replace", side_effect=OSError("disk full")):
                facts.stamp_file(path, RUNNER)
            self.assertEqual(path.read_text(), original)
            self.assertEqual(sorted(p.name for p in pathlib.Path(tmp).iterdir()), ["smaller.json"])

    @unittest.skipIf(os.geteuid() == 0, "root writes into a read-only directory")
    def test_a_file_in_a_read_only_directory_is_left_as_it_was(self):
        with tempfile.TemporaryDirectory() as tmp:
            folder = pathlib.Path(tmp) / "out"
            folder.mkdir()
            path = folder / "smaller.json"
            original = json.dumps([{"name": "inprocess/a", "unit": "ns", "value": 1.0}])
            path.write_text(original)
            folder.chmod(0o555)
            try:
                done = subprocess.run([sys.executable, str(FACTS), str(path)], capture_output=True, text=True, cwd=tmp)
            finally:
                folder.chmod(0o755)
            self.assertEqual(done.returncode, 0, done.stderr)
            self.assertIn("warning:", done.stderr)
            self.assertIn("stamped 0 of 1 files", done.stdout)
            self.assertEqual(path.read_text(), original)

    def test_a_cpuinfo_without_a_model_name_falls_back(self):
        facts = load_facts()
        with tempfile.TemporaryDirectory() as tmp:
            cpuinfo = pathlib.Path(tmp) / "cpuinfo"
            cpuinfo.write_text("processor\t: 0\nflags\t\t: fpu\n")
            self.assertTrue(facts.cpu_model(cpuinfo))
            self.assertTrue(facts.cpu_model(pathlib.Path(tmp) / "absent"))


if __name__ == "__main__":
    unittest.main()
