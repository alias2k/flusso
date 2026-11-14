#!/usr/bin/env python3
"""Stamp benchmark data points with the runner they ran on.

    bench-runner-facts.py <smaller.json> [<more.json>...]

Adds `runner: {cpu, cores}` to every point's `extra` (a JSON string, as
github-action-benchmark wants it, keeping what was there) and appends one
calibration point to the first file: `runner/sha256_64mib`, the best of five
SHA-256 passes over 64 MiB in milliseconds. No flusso code runs in it, so it
measures the machine alone; `bench-regression.py` matches runs on it.
"""
import hashlib
import json
import os
import pathlib
import platform
import sys
import time

CALIBRATION = "runner/sha256_64mib"


def cpu_model() -> str:
    try:
        for line in pathlib.Path("/proc/cpuinfo").read_text(encoding="utf-8").splitlines():
            if line.startswith("model name"):
                return line.split(":", 1)[1].strip()
    except OSError:
        pass
    return platform.processor() or platform.machine() or "unknown"


def calibrate() -> float:
    block = bytes(64 * 1024 * 1024)
    best = float("inf")
    for _ in range(5):
        start = time.perf_counter()
        hashlib.sha256(block).digest()
        best = min(best, time.perf_counter() - start)
    return best * 1000


def stamp(point: dict, runner: dict) -> dict:
    try:
        extra = json.loads(point.get("extra") or "{}")
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
    runner = {"cpu": cpu_model(), "cores": os.cpu_count() or 0}
    for i, name in enumerate(sys.argv[1:]):
        path = pathlib.Path(name)
        points = json.loads(path.read_text(encoding="utf-8"))
        if i == 0:
            points.append({"name": CALIBRATION, "unit": "ms", "value": calibrate()})
        path.write_text(json.dumps([stamp(p, runner) for p in points], indent=2) + "\n", encoding="utf-8")
    print(f"stamped with {runner['cpu']} ({runner['cores']} cores)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
