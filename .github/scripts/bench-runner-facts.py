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

`bench-regression.py` treats two runs with the same CPU model and core count and
calibrations within 5% as the same kind of machine.
"""
import hashlib
import json
import os
import pathlib
import platform
import sys
import tempfile
import time


def cpu_model() -> str:
    try:
        for line in pathlib.Path("/proc/cpuinfo").read_text(encoding="utf-8").splitlines():
            if line.startswith("model name"):
                return line.split(":", 1)[1].strip()
    except OSError:
        pass
    return platform.processor() or platform.machine() or "unknown"


def best_ms(work, passes: int) -> float:
    best = float("inf")
    for _ in range(passes):
        start = time.perf_counter()
        work()
        best = min(best, time.perf_counter() - start)
    return round(best * 1000, 3)


def disk_write() -> None:
    with tempfile.NamedTemporaryFile(dir=".", prefix=".bench-calibration-") as handle:
        handle.write(bytes(64 * 1024 * 1024))
        handle.flush()
        os.fsync(handle.fileno())


def calibrate() -> dict:
    block = bytes(64 * 1024 * 1024)
    large = bytes(256 * 1024 * 1024)
    return {
        "cpu_sha256_ms": best_ms(lambda: hashlib.sha256(block).digest(), 5),
        "memory_copy_ms": best_ms(lambda: bytearray(large), 5),
        "disk_write_ms": best_ms(disk_write, 3),
    }


def stamp(point: dict, runner: dict) -> dict:
    extra = point.get("extra")
    if isinstance(extra, str):
        try:
            extra = json.loads(extra)
        except ValueError:
            extra = {}
    if not isinstance(extra, dict):
        extra = {}
    extra["runner"] = runner
    return {**point, "extra": json.dumps(extra, sort_keys=True)}


def main() -> int:
    if len(sys.argv) < 2:
        print(__doc__, file=sys.stderr)
        return 2
    runner = {"cpu": cpu_model(), "cores": os.cpu_count() or 0, "calibration": calibrate()}
    for name in sys.argv[1:]:
        path = pathlib.Path(name)
        points = json.loads(path.read_text(encoding="utf-8"))
        path.write_text(json.dumps([stamp(p, runner) for p in points], indent=2) + "\n", encoding="utf-8")
    print(f"stamped with {runner['cpu']} ({runner['cores']} cores), calibration {runner['calibration']}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
