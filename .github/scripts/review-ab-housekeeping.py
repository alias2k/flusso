#!/usr/bin/env python3
"""Housekeeping for review-ab.sh: containers a killed run left, and old result dirs.

    review-ab-housekeeping.py stale  < names     # prints the stale container names
    review-ab-housekeeping.py prune <dir> <keep>

`stale` reads container names, one per line, and prints those named exactly
`flusso-review-<pid>-<suffix>` whose `<pid>` is no longer a running process: a
run killed with SIGKILL never reached its trap. Any other name is never
printed, whatever it looks like.

`prune` keeps the `<keep>` newest run dirs in `<dir>` (named
`YYYYMMDD-HHMMSS`, so the name orders them) and deletes the rest. Anything else
in `<dir>` is left alone.
"""
import os
import pathlib
import re
import shutil
import sys

CONTAINER = re.compile(r"flusso-review-(\d+)-[a-z0-9]+")
RUN_DIR = re.compile(r"\d{8}-\d{6}")


def alive(pid: int) -> bool:
    try:
        os.kill(pid, 0)
    except ProcessLookupError:
        return False
    except PermissionError:
        return True
    return True


def stale(names: list[str]) -> list[str]:
    out = []
    for name in names:
        match = CONTAINER.fullmatch(name.strip())
        if match and not alive(int(match.group(1))):
            out.append(match.group(0))
    return out


def prune(root: pathlib.Path, keep: int) -> None:
    runs = sorted(p for p in root.iterdir() if p.is_dir() and RUN_DIR.fullmatch(p.name))
    for old in runs[: max(len(runs) - keep, 0)]:
        shutil.rmtree(old)


def main() -> int:
    if sys.argv[1:2] == ["stale"]:
        print("\n".join(stale(sys.stdin.read().splitlines())))
        return 0
    if sys.argv[1:2] == ["prune"] and len(sys.argv) == 4:
        root = pathlib.Path(sys.argv[2])
        if root.is_dir():
            prune(root, int(sys.argv[3]))
        return 0
    print(__doc__, file=sys.stderr)
    return 2


if __name__ == "__main__":
    sys.exit(main())
