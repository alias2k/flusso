#!/usr/bin/env python3
"""Stamp benchmark data points with the runner they ran on.

    bench-runner-facts.py <data.json>...

Adds `runner` to every point's `extra` (a JSON string, as github-action-benchmark
wants it, keeping what was there):

- `cpu`: the CPU model, `cores`: the core count;
- `calibration`: the best of a few timed passes of fixed work that runs no flusso
  code and loads no flusso dependency, in milliseconds: `cpu_sha256_ms` (SHA-256
  over 64 MiB), `memory_copy_ms` (copying 256 MiB), `disk_write_ms` (writing and
  syncing 64 MiB in the working directory).

Nothing reads these yet: they are recorded so a later regression rule can tell
runner kinds apart (issue #317). This script never fails the benchmark job: a
calibration that can't run (a full or read-only disk, too little memory) is left
out, a file it can't read or write is left as it was (each file is written to a
temporary file beside it, then renamed over it), a CPU model it can't read
falls back to what `platform` knows, each with a `warning:` line, and it exits 0.
"""
import hashlib
import json
import os
import pathlib
import platform
import sys
import tempfile
import time

CPUINFO = pathlib.Path("/proc/cpuinfo")


def warn(message: str) -> None:
    print(f"warning: {message}", file=sys.stderr)


def cpu_model(cpuinfo: pathlib.Path = CPUINFO) -> str:
    try:
        for line in cpuinfo.read_text(encoding="utf-8").splitlines():
            if line.startswith("model name"):
                return line.split(":", 1)[1].strip()
    except (OSError, UnicodeDecodeError):
        pass
    return platform.processor() or platform.machine() or "unknown"


def best_ms(work, passes: int) -> float:
    best = float("inf")
    for _ in range(passes):
        start = time.perf_counter()
        work()
        best = min(best, time.perf_counter() - start)
    return round(best * 1000, 3)


def sha256_pass() -> None:
    hashlib.sha256(bytes(64 * 1024 * 1024)).digest()


def memory_copy() -> None:
    bytearray(bytes(256 * 1024 * 1024))


def disk_write() -> None:
    with tempfile.NamedTemporaryFile(dir=".", prefix=".bench-calibration-") as handle:
        handle.write(bytes(64 * 1024 * 1024))
        handle.flush()
        os.fsync(handle.fileno())


def calibrate() -> dict:
    measured = {}
    for name, work, passes in (("cpu_sha256_ms", sha256_pass, 5), ("memory_copy_ms", memory_copy, 5),
                               ("disk_write_ms", disk_write, 3)):
        try:
            measured[name] = best_ms(work, passes)
        except (OSError, MemoryError) as error:
            warn(f"calibration {name} could not run ({type(error).__name__}: {error}); left out")
    return measured


def stamp(point, runner: dict):
    if not isinstance(point, dict):
        return point
    extra = point.get("extra")
    if isinstance(extra, str):
        try:
            extra = json.loads(extra)
        except ValueError:
            extra = {"note": extra}
    if not isinstance(extra, dict):
        extra = {} if extra is None else {"note": extra}
    extra["runner"] = runner
    return {**point, "extra": json.dumps(extra, sort_keys=True)}


def stamp_file(path: pathlib.Path, runner: dict) -> None:
    try:
        points = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, ValueError) as error:
        warn(f"{path}: cannot read ({error}); left as it was")
        return
    if not isinstance(points, list):
        warn(f"{path}: not a list of data points; left as it was")
        return
    text = json.dumps([stamp(p, runner) for p in points], indent=2) + "\n"
    temp = None
    try:
        with tempfile.NamedTemporaryFile("w", encoding="utf-8", dir=path.parent, prefix=f".{path.name}.",
                                         delete=False) as handle:
            temp = pathlib.Path(handle.name)
            handle.write(text)
        os.replace(temp, path)
    except OSError as error:
        warn(f"{path}: cannot write ({error}); left as it was")
        if temp is not None:
            temp.unlink(missing_ok=True)


def main() -> int:
    if len(sys.argv) < 2:
        print(__doc__, file=sys.stderr)
        return 0
    try:
        runner = {"cpu": cpu_model(), "cores": os.cpu_count() or 0, "calibration": calibrate()}
        for name in sys.argv[1:]:
            stamp_file(pathlib.Path(name), runner)
        print(f"stamped with {runner['cpu']} ({runner['cores']} cores), calibration {runner['calibration']}")
    except Exception as error:  # noqa: BLE001 - recording runner facts must never fail the benchmark job
        warn(f"runner facts not recorded ({type(error).__name__}: {error})")
    return 0


if __name__ == "__main__":
    sys.exit(main())
