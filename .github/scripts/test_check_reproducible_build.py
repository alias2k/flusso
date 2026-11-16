"""Tests for check-reproducible-build.sh, the manual check that a tree builds to the same bytes.

    python3 -m unittest discover -s .github/scripts -p 'test_*.py'

The script runs over a fake `cargo` (named by `$CARGO`) that writes the executable a real
build would report, with bytes a test decides, and records every call: no toolchain, no
build. Each case runs from a directory outside the repository.
"""
import json
import os
import pathlib
import shutil
import stat
import subprocess
import tempfile
import textwrap
import unittest

HERE = pathlib.Path(__file__).resolve().parent
SCRIPT = HERE / "check-reproducible-build.sh"
REPO = HERE.parents[1]

FAKE_CARGO = textwrap.dedent(
    """\
    #!/usr/bin/env bash
    # A cargo that "builds" by writing one executable; FAKE_* decide its bytes.
    set -euo pipefail
    log="$FAKE_DIR/calls.jsonl"
    target="${CARGO_TARGET_DIR:-$PWD/target}"
    fresh=true
    [ -e "$target/.fake-built" ] && fresh=false
    printf '{"cmd":"%s","pwd":"%s","target":"%s","fresh":%s}\\n' "$1" "$PWD" "$target" "$fresh" >> "$log"
    case "$1" in
      clean) exit 0 ;;
      bench|build)
        if [ -n "${FAKE_FAIL:-}" ]; then echo "error[E0425]: cannot find value \\`boom\\`" >&2; exit 101; fi
        n=$(cat "$FAKE_DIR/count" 2>/dev/null || echo 0); n=$((n + 1)); echo "$n" > "$FAKE_DIR/count"
        mkdir -p "$target/release/deps"; touch "$target/.fake-built"
        exe="$target/release/deps/scripted-0123"
        if [ -n "${FAKE_VARY:-}" ]; then echo "seed $n" > "$exe"; else echo "same" > "$exe"; fi
        echo "{\\"reason\\":\\"compiler-artifact\\",\\"target\\":{\\"name\\":\\"scripted\\"},\\"executable\\":null}"
        echo "{\\"reason\\":\\"compiler-artifact\\",\\"target\\":{\\"name\\":\\"flusso\\"},\\"executable\\":\\"$exe\\"}"
        echo "{\\"reason\\":\\"compiler-artifact\\",\\"target\\":{\\"name\\":\\"scripted\\"},\\"executable\\":\\"$exe\\"}"
        ;;
      *) echo "unexpected cargo $*" >&2; exit 2 ;;
    esac
    """
)


@unittest.skipUnless(shutil.which("jq") and shutil.which("bash"), "needs jq and bash")
class ReproducibleBuild(unittest.TestCase):
    def run_script(self, *args: str, **env: str) -> subprocess.CompletedProcess:
        tmp = pathlib.Path(self.enterContext(tempfile.TemporaryDirectory()))
        cargo = tmp / "cargo"
        cargo.write_text(FAKE_CARGO)
        cargo.chmod(cargo.stat().st_mode | stat.S_IEXEC)
        result = subprocess.run(
            ["bash", str(SCRIPT), *args],
            cwd=tmp,
            env={
                **os.environ,
                "CARGO": str(cargo),
                "CARGO_TARGET_DIR": str(tmp / "target"),
                "FAKE_DIR": str(tmp),
                **env,
            },
            capture_output=True,
            text=True,
        )
        log = tmp / "calls.jsonl"
        result.calls = [json.loads(line) for line in log.read_text().splitlines()] if log.exists() else []
        result.tmp = tmp
        return result

    def builds(self, result: subprocess.CompletedProcess) -> list[dict]:
        return [call for call in result.calls if call["cmd"] in ("bench", "build")]

    def test_the_same_bytes_twice_pass_after_rebuilding_rhai_s_seed(self) -> None:
        result = self.run_script("scripted")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual([call["cmd"] for call in result.calls], ["bench", "clean", "bench"])
        self.assertTrue(all(call["pwd"] == str(REPO) for call in result.calls), "runs at the workspace root")

    def test_different_bytes_fail(self) -> None:
        result = self.run_script("scripted", FAKE_VARY="1")
        self.assertEqual(result.returncode, 1)
        self.assertIn("not reproducible", result.stderr)

    def test_a_failed_build_shows_cargo_s_error(self) -> None:
        result = self.run_script("cli", FAKE_FAIL="1")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("cannot find value `boom`", result.stderr)
        self.assertIn("build of cli failed", result.stderr)

    def test_full_builds_twice_from_nothing_in_the_same_directory(self) -> None:
        result = self.run_script("--full", "cli")
        self.assertEqual(result.returncode, 0, result.stderr)
        builds = self.builds(result)
        self.assertEqual(len(builds), 2)
        self.assertNotIn("clean", [call["cmd"] for call in result.calls])
        self.assertTrue(all(call["fresh"] for call in builds), "each build starts from an empty target dir")
        self.assertEqual(builds[0]["target"], builds[1]["target"])
        self.assertEqual(builds[0]["target"], str(result.tmp / "target" / "reproducible-check"))
        self.assertFalse((result.tmp / "target" / "reproducible-check").exists(), "removed afterwards")

    def test_full_catches_different_bytes_too(self) -> None:
        result = self.run_script("--full", "scripted", FAKE_VARY="1")
        self.assertEqual(result.returncode, 1)

    def test_an_unknown_target_is_refused(self) -> None:
        result = self.run_script("engine")
        self.assertEqual(result.returncode, 2)
        self.assertEqual(result.calls, [])


if __name__ == "__main__":
    unittest.main()
