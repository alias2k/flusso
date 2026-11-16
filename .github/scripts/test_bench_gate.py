"""Tests for the PR bench gate on identical benches (issue #346).

    python3 -m unittest discover -s .github/scripts -p 'test_*.py'

A bench whose executable and declared run-time inputs are byte-identical on the base and the PR
cannot differ in speed: the gate gives it no verdict. These tests run `bench-in-process.sh` and
`bench-gate.sh` end to end, as `bench.yml` does, over a fake `cargo` and fake Criterion bench
executables in a throwaway base tree and PR tree. A fake executable's bytes name its side only
when the test says the two builds differ; its timings come from `FAKE_TIMES`, keyed by the side it
reads from its own path, so two identical binaries can still measure differently (runner noise).

`Golden` pins the gate's output for benches whose binaries differ: every scenario's stdout, exit
code, breaches file and job summary must match `fixtures/gate-golden/`, which `origin/main`'s
scripts produced before the change (re-bless with `BENCH_GOLDEN_BLESS=1` only after reviewing why
the output moved).
"""
import json
import os
import pathlib
import re
import shutil
import subprocess
import sys
import tempfile
import unittest

HERE = pathlib.Path(__file__).resolve().parent
GOLDEN = HERE / "fixtures" / "gate-golden"

# The in-process set, as bench-in-process.sh lists it: source file, package dir, bench target.
BENCHES = [
    ("libs/2-engine/benches/engine.rs", "libs/2-engine", "engine"),
    ("libs/2-adapters/source-postgres/benches/pgoutput.rs", "libs/2-adapters/source-postgres", "pgoutput"),
    ("libs/2-adapters/source-postgres/benches/resolve.rs", "libs/2-adapters/source-postgres", "resolve"),
    ("libs/2-adapters/sink-opensearch/benches/render.rs", "libs/2-adapters/sink-opensearch", "render"),
    ("libs/2-script/benches/scripted.rs", "libs/2-script", "scripted"),
]
PACKAGE_DIRS = {
    "flusso-engine": "libs/2-engine",
    "flusso-source-postgres": "libs/2-adapters/source-postgres",
    "flusso-sink-opensearch": "libs/2-adapters/sink-opensearch",
    "flusso-script": "libs/2-script",
}
IDS = {
    "engine": ["live_drain/sinks1/64", "live_drain/sinks2/64"],
    "pgoutput": ["pgoutput/decode"],
    "resolve": ["resolve/unread_update"],
    "render": ["render/bulk"],
    "scripted": ["scripted/flat"],
}

FAKE_CARGO = r"""#!/usr/bin/env python3
import json, os, pathlib, sys
args = sys.argv[1:]
bench = args[args.index("--bench") + 1]
package = args[args.index("-p") + 1]
target = pathlib.Path(os.environ["CARGO_TARGET_DIR"]).resolve()
side = "base" if target == pathlib.Path(os.environ["FAKE_BASE_TARGET"]).resolve() else "pr"
with open(os.environ["FAKE_LOG"], "a") as log:
    log.write(f"build {side} {bench}\n")
exe = target / "release" / "deps" / f"{bench}-fake"
exe.parent.mkdir(parents=True, exist_ok=True)
body = pathlib.Path(os.environ["FAKE_EXE"]).read_text().replace("@TARGET@", bench)
if bench in os.environ.get("FAKE_DIFFER", "").split(","):
    body += f"# built by the {side} side\n"
exe.write_text(body)
exe.chmod(0o755)
dirs = json.loads(os.environ["FAKE_PACKAGE_DIRS"])
manifest = pathlib.Path.cwd() / dirs[package] / "Cargo.toml"
print(json.dumps({"reason": "compiler-artifact", "target": {"name": bench, "kind": ["bench"]},
                  "executable": str(exe), "manifest_path": str(manifest)}))
"""

FAKE_EXE = r"""#!/usr/bin/env python3
import json, os, pathlib, sys
target = "@TARGET@"
me = pathlib.Path(sys.argv[0]).resolve()
side = "base" if pathlib.Path(os.environ["FAKE_BASE_TARGET"]).resolve() in me.parents else "pr"
args = sys.argv[1:]
ids = json.loads(os.environ["FAKE_IDS"]).get(target, [])
if "--list" in args:
    if target in os.environ.get("FAKE_LIST_FAIL", "").split(","):
        sys.exit(1)
    print("Gnuplot not found, using plotters backend", file=sys.stderr)
    for case in ids:
        print(f"{case}: benchmark")
    sys.exit(0)
baseline = args[args.index("--save-baseline") + 1]
exact = args[args.index("--exact") + 1] if "--exact" in args else None
if exact is None or exact in ids:
    with open(os.environ["FAKE_LOG"], "a") as log:
        log.write(f"run {side} {target} {baseline} {exact or '-'}\n")
times = json.loads(os.environ.get("FAKE_TIMES", "{}"))
home = pathlib.Path(os.environ["CRITERION_HOME"])
for case in ids:
    if exact not in (None, case):
        continue
    value = times.get(side, {}).get(case, {}).get(baseline, 1_000_000)
    out = home / case / baseline
    out.mkdir(parents=True, exist_ok=True)
    (out / "estimates.json").write_text(json.dumps({"median": {"point_estimate": value}}))
    (out / "benchmark.json").write_text(json.dumps({"full_id": case}))
"""


class Gate(unittest.TestCase):
    """bench-in-process.sh + bench-gate.sh over a fake cargo, a base tree and a PR tree."""

    def setUp(self) -> None:
        tmp = tempfile.TemporaryDirectory()
        self.addCleanup(tmp.cleanup)
        self.dir = pathlib.Path(tmp.name)
        self.scripts = self.dir / "scripts"
        shutil.copytree(HERE, self.scripts, ignore=shutil.ignore_patterns("test_*", "fixtures", "__pycache__"))
        self.base_tree, self.pr_tree = self.dir / "base", self.dir / "pr"
        self.base_target, self.pr_target = self.dir / "base-target", self.dir / "pr-target"
        self.criterion = self.dir / "criterion"
        self.summary = self.dir / "summary.md"
        self.log = self.dir / "log"
        self.log.write_text("")
        for tree in (self.base_tree, self.pr_tree):
            for file, _, _ in BENCHES:
                (tree / file).parent.mkdir(parents=True, exist_ok=True)
                (tree / file).write_text("fn main() {}\n")
        bin_dir = self.dir / "bin"
        bin_dir.mkdir()
        (bin_dir / "cargo").write_text(FAKE_CARGO)
        (bin_dir / "cargo").chmod(0o755)
        (self.dir / "exe.py").write_text(FAKE_EXE)
        self.env = {
            **{k: v for k, v in os.environ.items() if not k.startswith(("BENCH_", "GITHUB_", "CRITERION", "CARGO"))},
            "PATH": f"{bin_dir}:{os.environ['PATH']}",
            "CRITERION_HOME": str(self.criterion),
            "GITHUB_STEP_SUMMARY": str(self.summary),
            "FAKE_BASE_TARGET": str(self.base_target),
            "FAKE_LOG": str(self.log),
            "FAKE_EXE": str(self.dir / "exe.py"),
            "FAKE_IDS": json.dumps(IDS),
            "FAKE_PACKAGE_DIRS": json.dumps(PACKAGE_DIRS),
        }

    def declare(self, *lines: str) -> None:
        (self.scripts / "bench-inputs.tsv").write_text("".join(f"{line}\n" for line in lines))

    def bench(self, side: str, baseline: str, **env: str) -> subprocess.CompletedProcess:
        tree, target = (self.base_tree, self.base_target) if side == "base" else (self.pr_tree, self.pr_target)
        result = subprocess.run(
            ["bash", self.scripts / "bench-in-process.sh", baseline],
            cwd=tree,
            env={**self.env, "CARGO_TARGET_DIR": str(target), **env},
            capture_output=True,
            text=True,
            timeout=120,
        )
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        return result

    def both(self, **env: str) -> None:
        self.bench("base", "base", **env)
        self.bench("pr", "pr", **env)

    def gate(self, **env: str) -> subprocess.CompletedProcess:
        return subprocess.run(
            ["bash", self.scripts / "bench-gate.sh", self.base_tree, self.base_target, "10"],
            cwd=self.pr_tree,
            env={**self.env, "CARGO_TARGET_DIR": str(self.pr_target), **env},
            capture_output=True,
            text=True,
            timeout=120,
        )

    def times(self, pr: dict) -> None:
        """PR-side medians per id and baseline; everything else measures 1 ms."""
        self.env["FAKE_TIMES"] = json.dumps({"pr": pr})

    def runs(self, baseline: str | None = None) -> list[str]:
        lines = [line.split()[1:] for line in self.log.read_text().splitlines() if line.startswith("run ")]
        return [" ".join(line) for line in lines if baseline in (None, line[2])]

    def row(self, out: str, bench: str) -> list[str]:
        """The bench's last row: the re-run table's when it was re-measured, else the first pass's."""
        line = [line for line in out.splitlines() if line.startswith(f"| `{bench}`")][-1]
        return [cell.strip() for cell in line.strip("|").split("|")]

    def test_identical_binaries_with_a_noisy_breach_pass_and_are_listed_not_compared(self) -> None:
        # PR #341: the same engine binary measured +12.8%, then +8.1% on the re-run.
        self.times({"live_drain/sinks2/64": {"pr": 1_128_000, "pr-confirm": 1_081_000}})
        self.both()
        result = self.gate()
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertEqual(self.row(result.stdout, "live_drain/sinks2/64")[3], "identical binaries, not compared")
        self.assertIn("| `live_drain/sinks2/64` |", self.summary.read_text())
        self.assertIn("identical binaries, not compared", self.summary.read_text())
        self.assertEqual(self.runs("pr-confirm"), [])

    def test_identical_binaries_with_a_changed_run_time_input_are_compared_and_can_fail(self) -> None:
        fixture = "libs/2-engine/benches/fixtures/changes.bin"
        self.declare(f"engine\t{fixture}")
        for tree, data in ((self.base_tree, "old"), (self.pr_tree, "new")):
            (tree / fixture).parent.mkdir(parents=True, exist_ok=True)
            (tree / fixture).write_text(data)
        self.times({"live_drain/sinks2/64": {"pr": 1_300_000, "pr-confirm": 1_300_000}})
        self.both()
        result = self.gate()
        self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
        self.assertEqual(self.row(result.stdout, "live_drain/sinks2/64")[-1], "**confirmed** by the re-run ⚠️")
        self.assertEqual(self.runs("pr-confirm"), ["pr engine pr-confirm live_drain/sinks2/64"])

    def test_a_declared_environment_variable_is_part_of_the_identity(self) -> None:
        self.declare("engine\tenv:FLUSSO_BENCH_KNOB")
        self.times({"live_drain/sinks2/64": {"pr": 1_300_000, "pr-confirm": 1_300_000}})
        self.bench("base", "base", FLUSSO_BENCH_KNOB="1")
        self.bench("pr", "pr", FLUSSO_BENCH_KNOB="2")
        result = self.gate()
        self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
        self.assertEqual(self.row(result.stdout, "live_drain/sinks1/64")[3], "+0.0%")
        self.assertEqual(self.row(result.stdout, "scripted/flat")[3], "identical binaries, not compared")

    def test_differing_binaries_are_gated_as_before(self) -> None:
        self.env["FAKE_DIFFER"] = "engine,pgoutput,resolve,render,scripted"
        self.times({"live_drain/sinks2/64": {"pr": 1_128_000, "pr-confirm": 1_081_000}})
        self.both()
        result = self.gate()
        self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
        self.assertEqual(self.row(result.stdout, "live_drain/sinks2/64")[-1], "**confirmed** by the mean ⚠️")
        self.assertNotIn("identical binaries", result.stdout)

    def test_a_mix_skips_only_the_identical_bench(self) -> None:
        self.env["FAKE_DIFFER"] = "render"
        self.times({
            "live_drain/sinks2/64": {"pr": 1_128_000, "pr-confirm": 1_081_000},
            "render/bulk": {"pr": 1_300_000, "pr-confirm": 1_250_000},
        })
        self.both()
        result = self.gate()
        self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
        self.assertEqual(self.row(result.stdout, "live_drain/sinks2/64")[3], "identical binaries, not compared")
        self.assertEqual(self.row(result.stdout, "render/bulk")[-1], "**confirmed** by the re-run ⚠️")
        self.assertEqual(self.runs("pr-confirm"), ["pr render pr-confirm render/bulk"])

    def test_an_id_that_cannot_be_mapped_to_its_executable_is_compared(self) -> None:
        self.env["FAKE_LIST_FAIL"] = "engine"
        self.times({"live_drain/sinks2/64": {"pr": 1_300_000, "pr-confirm": 1_300_000}})
        self.both()
        result = self.gate()
        self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
        self.assertEqual(self.row(result.stdout, "live_drain/sinks2/64")[-1], "**confirmed** by the re-run ⚠️")

    def workflow(self) -> None:
        """bench.yml's in-process job: build both sides, then run each, skipping what is identical."""
        self.bench("base", "base", BENCH_BUILD_ONLY="1")
        self.bench("pr", "pr", BENCH_BUILD_ONLY="1")
        self.bench("base", "base", BENCH_SKIP_SAME_AS=str(self.pr_target / "bench-executables" / "pr.identity.tsv"))
        self.bench("pr", "pr", BENCH_SKIP_SAME_AS=str(self.base_target / "bench-executables" / "base.identity.tsv"))

    def test_with_every_bench_identical_nothing_runs_and_the_gate_passes(self) -> None:
        self.workflow()
        self.assertEqual(self.runs(), [])
        result = self.gate()
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertIn("No bench compared: every bench's binary and run-time inputs are identical on both sides.", result.stdout)
        self.assertEqual(self.row(result.stdout, "scripted/flat")[1:], ["—", "—", "identical binaries, not compared"])

    def test_only_the_benches_that_differ_run(self) -> None:
        self.env["FAKE_DIFFER"] = "render"
        self.workflow()
        self.assertEqual(self.runs(), ["base render base -", "pr render pr -"])
        result = self.gate()
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertEqual(self.row(result.stdout, "render/bulk")[3], "+0.0%")
        self.assertEqual(self.row(result.stdout, "live_drain/sinks1/64")[3], "identical binaries, not compared")

    def test_a_bench_whose_ids_cannot_be_listed_still_runs(self) -> None:
        self.env["FAKE_LIST_FAIL"] = "engine"
        self.workflow()
        self.assertEqual(self.runs(), ["base engine base -", "pr engine pr -"])

    def test_a_base_with_no_in_process_bench_still_warns_and_passes(self) -> None:
        for file, _, _ in BENCHES:
            (self.base_tree / file).unlink()
        result = subprocess.run(
            ["bash", self.scripts / "bench-in-process.sh", "base"],
            cwd=self.base_tree,
            env={**self.env, "CARGO_TARGET_DIR": str(self.base_target), "BENCH_BUILD_ONLY": "1"},
            capture_output=True,
            text=True,
        )
        self.assertEqual(result.returncode, 78, result.stdout + result.stderr)
        self.bench("pr", "pr", BENCH_SKIP_SAME_AS=str(self.base_target / "bench-executables" / "base.identity.tsv"))
        self.assertEqual(len(self.runs("pr")), 5)
        result = self.gate(BENCH_BASE_EMPTY="1")
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertEqual(
            result.stdout,
            f"::warning::nothing to compare: no bench has both `base` and `pr` under {self.criterion}\n",
        )


# Patterns that read something at run time: a file, a directory, an environment variable or the
# arguments. `include_bytes!`/`include_str!` are not among them: those land in the binary.
RUN_TIME_READ = re.compile(
    r"std::fs\b|\bfs::|\bFile::|OpenOptions|read_to_string|read_dir|env::var|var_os|env::args"
    r"|CARGO_MANIFEST_DIR|Path::new|PathBuf"
)


def declared() -> dict[str, list[str]]:
    out: dict[str, list[str]] = {}
    for line in (HERE / "bench-inputs.tsv").read_text().splitlines():
        if line.strip() and not line.startswith("#"):
            target, what = line.split("\t")
            out.setdefault(target, []).append(what)
    return out


def in_process_benches() -> list[tuple[str, str]]:
    """(source file, bench target) of the gate's set, read from bench-in-process.sh itself."""
    script = (HERE / "bench-in-process.sh").read_text()
    gated = script.split("\nelse\n", 1)[1].split("\nfi\n", 1)[0]
    return [(m[1], m[2]) for m in re.finditer(r"^\s*bench (\S+) \S+ (\S+)", gated, re.M)]


class RunTimeInputs(unittest.TestCase):
    """Every run-time read in a gated bench's source is declared in bench-inputs.tsv.

    This guards the bench sources only: a read inside library code a bench calls, or behind a helper
    with another name, is not seen. Environment variables are the same on both sides of one job (the
    scripts set only CARGO_TARGET_DIR per side), so an undeclared one cannot make the sides differ.
    """

    def test_the_scan_sees_the_bench_sources(self) -> None:
        benches = in_process_benches()
        self.assertEqual([t for _, t in benches], [t for _, _, t in BENCHES])
        root = HERE.parent.parent
        for file, _ in benches:
            self.assertTrue((root / file).is_file(), file)

    def test_a_bench_that_reads_at_run_time_declares_its_inputs(self) -> None:
        root, inputs = HERE.parent.parent, declared()
        for file, target in in_process_benches():
            source = (root / file).read_text()
            reads = sorted({m[0] for m in RUN_TIME_READ.finditer(source)})
            if reads:
                self.assertIn(target, inputs, f"{file} reads at run time ({', '.join(reads)}): declare it in bench-inputs.tsv")

    def test_every_declared_input_belongs_to_a_gated_bench_and_exists(self) -> None:
        root, targets = HERE.parent.parent, {t for _, t in in_process_benches()}
        for target, inputs in declared().items():
            self.assertIn(target, targets)
            for what in inputs:
                if not what.startswith("env:"):
                    self.assertTrue((root / what).exists(), what)

    def test_the_scan_flags_a_file_read_and_not_an_include(self) -> None:
        self.assertTrue(RUN_TIME_READ.search('let data = std::fs::read("benches/fixtures/x.bin")?;'))
        self.assertTrue(RUN_TIME_READ.search('let n = std::env::var("ROWS");'))
        self.assertFalse(RUN_TIME_READ.search('const FIXTURE: &[u8] = include_bytes!("fixtures/pgoutput.bin");'))


def save(root: pathlib.Path, bench: str, baseline: str, median_ns: float) -> None:
    out = root / bench / baseline
    out.mkdir(parents=True, exist_ok=True)
    (out / "estimates.json").write_text(json.dumps({"median": {"point_estimate": median_ns}}))
    (out / "benchmark.json").write_text(json.dumps({"full_id": bench}))


# Differing binaries, every path of bench-compare.py as bench-gate.sh drives it: (name, first pass
# {id: (base, pr)}, re-run {id: (base, pr)}, extra arguments). `@breaches` is the breaches file.
SCENARIOS = [
    ("under", {"a/x": (1_000_000, 1_050_000)}, {}, []),
    ("at", {"a/x": (1_000_000, 1_100_000)}, {}, []),
    ("over", {"a/x": (1_000_000, 1_150_000), "a/y": (1_000_000, 990_000)}, {}, []),
    ("over-breaches", {"a/x": (1_000_000, 1_150_000), "a/y": (1_000_000, 990_000)}, {}, ["--breaches", "@breaches"]),
    ("under-breaches", {"a/x": (1_000_000, 1_020_000)}, {}, ["--breaches", "@breaches"]),
    ("cleared", {"a/x": (1_000_000, 1_150_000)}, {"a/x": (1_000_000, 1_020_000)}, ["--confirm", "base-confirm", "pr-confirm"]),
    ("by-re-run", {"a/x": (1_000_000, 1_150_000)}, {"a/x": (1_000_000, 1_200_000)}, ["--confirm", "base-confirm", "pr-confirm"]),
    ("by-mean", {"a/x": (1_000_000, 1_128_000)}, {"a/x": (1_000_000, 1_081_000)}, ["--confirm", "base-confirm", "pr-confirm"]),
    ("not-re-measured", {"a/x": (1_000_000, 1_150_000)}, {}, ["--confirm", "base-confirm", "pr-confirm"]),
    ("zero-base", {"a/x": (0, 1_000)}, {}, []),
    ("empty", {}, {}, []),
    ("empty-allowed", {}, {}, ["--allow-empty"]),
    ("one-sided", {"a/x": (1_000_000, 1_000_000), "a/new": (None, 1_000_000)}, {}, []),
]


class Golden(unittest.TestCase):
    """bench-compare.py's output for differing binaries is what origin/main's printed (fixtures/gate-golden)."""

    def scenario(self, name: str, first: dict, again: dict, extra: list[str], identities: bool) -> dict:
        with tempfile.TemporaryDirectory() as tmp:
            dir = pathlib.Path(tmp)
            root, summary, breaches = dir / "criterion", dir / "summary.md", dir / "breaches"
            root.mkdir()
            for bench, (b, c) in first.items():
                if b is not None:
                    save(root, bench, "base", b)
                save(root, bench, "pr", c)
            for bench, (b, c) in again.items():
                save(root, bench, "base-confirm", b)
                save(root, bench, "pr-confirm", c)
            args = [str(breaches) if a == "@breaches" else a for a in extra]
            if identities:
                for target, side in ((dir / "base-target", "base"), (dir / "pr-target", "pr")):
                    record = target / "bench-executables"
                    record.mkdir(parents=True)
                    (record / f"{side}.identity.tsv").write_text(f"a\t{side}-identity\n")
                    (record / f"{side}.ids.tsv").write_text("".join(f"a\t{i}\n" for i in first))
                args += ["--identities", str(dir / "base-target"), str(dir / "pr-target")]
            result = subprocess.run(
                [sys.executable, HERE / "bench-compare.py", root, "base", "pr", "10", *args],
                env={**os.environ, "GITHUB_STEP_SUMMARY": str(summary)},
                capture_output=True,
                text=True,
            )
            out = {
                "exit": result.returncode,
                "stdout": result.stdout.replace(str(root), "<criterion>"),
                "breaches": breaches.read_text() if breaches.exists() else None,
                "summary": summary.read_text() if summary.exists() else None,
            }
            return out

    def test_differing_binaries_print_what_main_printed(self) -> None:
        bless = os.environ.get("BENCH_GOLDEN_BLESS") == "1"
        for name, first, again, extra in SCENARIOS:
            golden = GOLDEN / f"{name}.json"
            for identities in (False, True):
                with self.subTest(name, identities=identities):
                    got = self.scenario(name, first, again, extra, identities)
                    if bless and not identities:
                        GOLDEN.mkdir(parents=True, exist_ok=True)
                        golden.write_text(json.dumps(got, indent=2, ensure_ascii=False) + "\n")
                    self.assertEqual(got, json.loads(golden.read_text()))


if __name__ == "__main__":
    unittest.main()
