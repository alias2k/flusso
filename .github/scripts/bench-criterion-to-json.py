#!/usr/bin/env python3
"""Convert Criterion's saved estimates into github-action-benchmark's custom format.

    bench-criterion-to-json.py <criterion-dir> <baseline> <prefix> <out.json>

Walks `<criterion-dir>` (normally `target/criterion`) for every
`<group>/<function>[/<param>]/<baseline>/estimates.json` and emits one
`customSmallerIsBetter` point per bench: the median in nanoseconds, named
`<prefix>/<group>/<function>[/<param>]`.
"""
import json
import pathlib
import sys


def main() -> int:
    if len(sys.argv) != 5:
        print(__doc__, file=sys.stderr)
        return 2
    root, baseline, prefix, out = (pathlib.Path(sys.argv[1]), *sys.argv[2:4], pathlib.Path(sys.argv[4]))
    points = []
    for estimates in sorted(root.rglob(f"{baseline}/estimates.json")):
        bench_dir = estimates.parent.parent
        rel = bench_dir.relative_to(root)
        if rel.parts and rel.parts[0] == "report":
            continue
        data = json.loads(estimates.read_text())
        median = data["median"]["point_estimate"]
        points.append({"name": f"{prefix}/{'/'.join(rel.parts)}", "unit": "ns", "value": median})
    if not points:
        print(f"no `{baseline}` estimates under {root}", file=sys.stderr)
        return 1
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(json.dumps(points, indent=2) + "\n")
    print(f"wrote {len(points)} points to {out}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
