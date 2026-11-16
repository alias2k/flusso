#!/usr/bin/env python3
"""Fail when the flusso binary would bake a random hash seed into every build.

    check-ahash-seed.py

Rhai hashes with `ahash`. With `ahash`'s `compile-time-rng` and without `runtime-rng`,
`ahash` takes its keys from `const-random`, which draws a new seed in every compile: no
two builds of the same tree have the same bytes, and the PR bench gate can't skip
`scripted` (issue #355). Rhai always turns `compile-time-rng` on; its default features
add `runtime-rng`, which wins, so the seed is drawn per process instead.

Reads the resolved feature set with `cargo tree --offline --locked -e features -i ahash`
for `flusso-cli` as installed and as the server image builds it
(`--no-default-features`): no build, seconds, but the crates must already be fetched,
so CI runs it after the build checks. `$CARGO` names the cargo to run (default `cargo`).

Colour and charset are forced (CI sets `CARGO_TERM_COLOR=always`, which would hide every
line from the parser), and a tree with no feature read is an error, never a pass.

Exits 0 when every build is fine (or links no `ahash`), 1 when one has the seed baked in,
2 when cargo fails or its output can't be read.
"""
import os
import re
import subprocess
import sys

BUILDS = [
    ("flusso-cli", ["-p", "flusso-cli"]),
    ("flusso-cli --no-default-features", ["-p", "flusso-cli", "--no-default-features"]),
]

FEATURE = re.compile(r'^[├└]── ahash feature "([^"]+)"')
NO_AHASH = "did not match any packages"


def features_in(tree: str) -> set[str]:
    """The `ahash` features a `cargo tree -e features -i ahash` output shows enabled."""
    return {match.group(1) for line in tree.splitlines() if (match := FEATURE.match(line))}


def problem(features: set[str]) -> str | None:
    """Why these `ahash` features make the build non-reproducible, or `None`."""
    if "compile-time-rng" in features and "runtime-rng" not in features:
        return (
            "ahash has `compile-time-rng` without `runtime-rng`: every build bakes in a new "
            "random seed, so no two builds have the same bytes (issue #355). "
            "Fix: keep Rhai's default features (no `default-features = false` on `rhai` in "
            "the root Cargo.toml), or enable `ahash/runtime-rng` another way; "
            "see docs/src/contribute/benchmarks.md."
        )
    return None


def tree(cargo: str, args: list[str]) -> subprocess.CompletedProcess:
    return subprocess.run(
        [cargo, "tree", "--offline", "--locked", "--color", "never", "--charset", "utf8",
         *args, "-e", "features", "-i", "ahash"],
        capture_output=True,
        text=True,
    )


def main() -> int:
    cargo = os.environ.get("CARGO", "cargo")
    failed = False
    for name, args in BUILDS:
        result = tree(cargo, args)
        if result.returncode != 0:
            if NO_AHASH in result.stderr:
                print(f"{name}: links no ahash")
                continue
            sys.stderr.write(result.stderr)
            print(f"{name}: cargo tree failed", file=sys.stderr)
            return 2
        features = features_in(result.stdout)
        if not features:
            sys.stderr.write(result.stdout)
            print(f"{name}: no ahash feature read from cargo tree's output (above)", file=sys.stderr)
            return 2
        message = problem(features)
        if message:
            print(f"{name}: {message}", file=sys.stderr)
            failed = True
        else:
            print(f"{name}: ahash features {', '.join(sorted(features))}")
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
