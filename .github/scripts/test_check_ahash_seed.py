"""Tests for check-ahash-seed.py, the guard that keeps the flusso binary reproducible.

    python3 -m unittest discover -s .github/scripts -p 'test_*.py'

The two fixtures are `cargo tree -p flusso-cli -e features -i ahash` as it printed on
`main` before issue #355 (`compile-time-rng` alone: a random seed baked into every build)
and after it (`runtime-rng` beside it: the seed is drawn per process). The end-to-end
cases run the guard over a fake `cargo` named by `$CARGO`, so no toolchain is needed.
"""
import importlib.util
import os
import pathlib
import stat
import subprocess
import sys
import tempfile
import textwrap
import unittest

HERE = pathlib.Path(__file__).resolve().parent
SCRIPT = HERE / "check-ahash-seed.py"
FIXTURES = HERE / "fixtures" / "ahash-seed"

spec = importlib.util.spec_from_file_location("check_ahash_seed", SCRIPT)
guard = importlib.util.module_from_spec(spec)
spec.loader.exec_module(guard)

BAD = (FIXTURES / "compile-time-rng.txt").read_text()
# `main`'s tree as CI printed it: CI sets CARGO_TERM_COLOR=always.
BAD_COLORED = (FIXTURES / "compile-time-rng-colored.txt").read_text()
GOOD = (FIXTURES / "runtime-rng.txt").read_text()


class Features(unittest.TestCase):
    def test_the_tree_from_main_has_only_the_compile_time_seed(self) -> None:
        self.assertEqual(guard.features_in(BAD), {"compile-time-rng", "const-random", "std"})

    def test_the_tree_after_the_fix_also_has_the_runtime_seed(self) -> None:
        self.assertEqual(
            guard.features_in(GOOD),
            {"compile-time-rng", "const-random", "getrandom", "runtime-rng", "std"},
        )

    def test_a_feature_that_only_enables_another_is_not_read_as_ahash_s_own(self) -> None:
        tree = textwrap.dedent(
            """\
            ahash v0.8.12
            ├── ahash feature "std"
            │   └── ahash feature "runtime-rng"
            └── ahash feature "compile-time-rng"
            """
        )
        self.assertEqual(guard.features_in(tree), {"std", "compile-time-rng"})


class Problem(unittest.TestCase):
    def test_a_compile_time_seed_alone_is_refused_with_what_to_do(self) -> None:
        message = guard.problem(guard.features_in(BAD))
        self.assertIsNotNone(message)
        self.assertIn("compile-time-rng", message)
        self.assertIn("keep Rhai's default features", message)
        self.assertIn("docs/src/contribute/benchmarks.md", message)

    def test_a_runtime_seed_beside_it_passes(self) -> None:
        self.assertIsNone(guard.problem(guard.features_in(GOOD)))

    def test_no_runtime_seed_is_refused_even_without_the_compile_time_one(self) -> None:
        message = guard.problem({"std"})
        self.assertIsNotNone(message)
        self.assertIn("runtime-rng", message)


class EndToEnd(unittest.TestCase):
    """The guard over a fake `cargo` that answers each `cargo tree` call with a fixture."""

    def run_guard(self, default: str, server: str | None = None, status: int = 0) -> subprocess.CompletedProcess:
        with tempfile.TemporaryDirectory() as tmp:
            tmp_path = pathlib.Path(tmp)
            (tmp_path / "default.txt").write_text(default)
            (tmp_path / "server.txt").write_text(server if server is not None else default)
            cargo = tmp_path / "cargo"
            cargo.write_text(
                textwrap.dedent(
                    f"""\
                    #!/usr/bin/env bash
                    echo "$*" >> {tmp_path}/calls
                    case " $* " in
                      *" --no-default-features "*) out={tmp_path}/server.txt ;;
                      *) out={tmp_path}/default.txt ;;
                    esac
                    if [ {status} -ne 0 ]; then cat "$out" >&2; exit {status}; fi
                    cat "$out"
                    """
                )
            )
            cargo.chmod(cargo.stat().st_mode | stat.S_IEXEC)
            result = subprocess.run(
                [sys.executable, str(SCRIPT)],
                env={**os.environ, "CARGO": str(cargo)},
                capture_output=True,
                text=True,
            )
            result.calls = (tmp_path / "calls").read_text().splitlines()
            return result

    def test_the_fixed_tree_passes_both_builds(self) -> None:
        result = self.run_guard(GOOD)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(len(result.calls), 2)
        self.assertTrue(all("--offline" in call and "-i ahash" in call for call in result.calls))
        self.assertTrue(all("--color never" in call for call in result.calls), "CI forces colour on")
        self.assertTrue(any("--no-default-features" in call for call in result.calls))

    def test_main_s_tree_fails(self) -> None:
        result = self.run_guard(BAD)
        self.assertEqual(result.returncode, 1)
        self.assertIn("keep Rhai's default features", result.stderr)

    def test_only_the_server_build_regressing_still_fails_and_is_named(self) -> None:
        result = self.run_guard(GOOD, server=BAD)
        self.assertEqual(result.returncode, 1)
        self.assertIn("--no-default-features", result.stderr)

    def test_a_tree_it_cannot_read_never_passes(self) -> None:
        result = self.run_guard(BAD_COLORED)
        self.assertEqual(result.returncode, 2)
        self.assertIn("no ahash feature read", result.stderr)

    def test_a_binary_without_ahash_never_passes(self) -> None:
        missing = "error: package ID specification `ahash` did not match any packages\n"
        result = self.run_guard(missing, status=101)
        self.assertEqual(result.returncode, 2)
        self.assertIn("update this guard", result.stderr)

    def test_any_other_cargo_failure_fails_and_shows_cargo_s_error(self) -> None:
        result = self.run_guard("error: failed to load manifest\n", status=101)
        self.assertEqual(result.returncode, 2)
        self.assertIn("failed to load manifest", result.stderr)


if __name__ == "__main__":
    unittest.main()
