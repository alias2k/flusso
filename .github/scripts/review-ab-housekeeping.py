#!/usr/bin/env python3
"""Housekeeping for review-ab.sh: containers a killed run left, old result dirs, CPU pinning.

    review-ab-housekeeping.py stale  < names     # prints the stale container names
    review-ab-housekeeping.py prune <dir> <keep>
    review-ab-housekeeping.py cpus --nproc N --taskset 0|1 [--bench LIST] [--servers LIST] [--off]

`stale` reads container names, one per line, and prints those named exactly
`flusso-review-<pid>-<suffix>` whose `<pid>` is no longer a running process: a
run killed with SIGKILL never reached its trap. Any other name is never
printed, whatever it looks like.

`cpus` prints three lines: the cores the bench process runs on (a `taskset
-c` list), the cores the Postgres and OpenSearch containers get
(`--cpuset-cpus`), and a sentence saying so. By default core 0 is left alone
and the next 2·k cores are split in two, k = min(4, (N − 1) // 2). Pinning is
off, the first two lines empty, when asked (`--off`), when `taskset` is
missing, or with fewer than 5 cores. Lists given with `--bench` / `--servers`
are used as given, after checking the cores exist and the two sets don't
overlap (exit 2 otherwise).

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


def cores(spec: str) -> set[int]:
    out = set()
    for part in spec.split(","):
        low, _, high = part.partition("-")
        out.update(range(int(low), int(high or low) + 1))
    return out


def cpus(argv: list[str]) -> int:
    import argparse

    parser = argparse.ArgumentParser(prog="review-ab-housekeeping.py cpus")
    parser.add_argument("--nproc", type=int, required=True)
    parser.add_argument("--taskset", type=int, choices=(0, 1), required=True)
    parser.add_argument("--bench")
    parser.add_argument("--servers")
    parser.add_argument("--off", action="store_true")
    args = parser.parse_args(argv)
    off = None
    if args.off:
        off = "turned off"
    elif not args.taskset:
        off = "`taskset` is not installed"
    k = min(4, (args.nproc - 1) // 2)
    if off is None and not (args.bench and args.servers) and k < 2:
        off = f"only {args.nproc} cores"
    if off is not None:
        print(f"\n\nCPU pinning off: {off}.")
        return 0
    bench = args.bench or f"1-{k}"
    servers = args.servers or f"{k + 1}-{2 * k}"
    try:
        bench_set, server_set = cores(bench), cores(servers)
    except ValueError:
        print(f"error: not a core list: {bench!r} / {servers!r}", file=sys.stderr)
        return 2
    missing = sorted(c for c in bench_set | server_set if c >= args.nproc)
    if missing:
        print(f"error: this machine has cores 0-{args.nproc - 1}, not {', '.join(map(str, missing))}", file=sys.stderr)
        return 2
    if bench_set & server_set:
        print(f"error: the bench cores {bench} and the server cores {servers} overlap", file=sys.stderr)
        return 2
    print(f"{bench}\n{servers}\nCPU pinning: benches on cores {bench}, Postgres and OpenSearch on {servers}.")
    return 0


def main() -> int:
    if sys.argv[1:2] == ["cpus"]:
        return cpus(sys.argv[2:])
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
