#!/usr/bin/env python3
"""Which benches ran the same bytes on both sides, and so cannot differ in speed.

    bench_identity.py every <base-target> <pr-target> <base-baseline> <pr-baseline>

The one home of "identical" for the PR gate (bench-compare.py --identities) and review-ab.sh.
bench-in-process.sh records, per bench target, beside its executables record:

- `<target>/bench-executables/<baseline>.identity.tsv`: `<bench target>\t<identity>`, a sha256
  over the executable's sha256 and every run-time input bench-inputs.tsv declares for it;
- `<target>/bench-executables/<baseline>.ids.tsv`: `<bench target>\t<Criterion id>`, what the
  executable's `--list` printed (in-process set only).

A Criterion id is skipped only when its bench target has the same identity on both sides, and no
target whose identity differs (or is unrecorded) lists the same id. An id no target lists is never
skipped: what cannot be traced to an executable is compared.

`every` exits 0 when both sides recorded the same bench targets, all with the same identity, else 1.
"""
import pathlib
import sys
from dataclasses import dataclass


def _rows(target: pathlib.Path, baseline: str, kind: str) -> list[tuple[str, str]]:
    path = target / "bench-executables" / f"{baseline}.{kind}.tsv"
    if not path.is_file():
        return []
    return [tuple(line.split("\t", 1)) for line in path.read_text().splitlines() if "\t" in line]


def identities(target: pathlib.Path, baseline: str) -> dict[str, str]:
    return dict(_rows(target, baseline, "identity"))


def listed(target: pathlib.Path, baseline: str) -> dict[str, set[str]]:
    out: dict[str, set[str]] = {}
    for bench, case in _rows(target, baseline, "ids"):
        out.setdefault(bench, set()).add(case)
    return out


@dataclass(frozen=True)
class Same:
    """What the two sides share: the Criterion ids to skip, and whether every bench target is identical."""

    ids: frozenset[str]
    every: bool


def compare(base_target: pathlib.Path, pr_target: pathlib.Path, base: str, pr: str) -> Same:
    base_ident, pr_ident = identities(base_target, base), identities(pr_target, pr)
    base_ids, pr_ids = listed(base_target, base), listed(pr_target, pr)
    same = {bench for bench in base_ident.keys() & pr_ident.keys() if base_ident[bench] == pr_ident[bench]}
    skip, keep = set(), set()
    for ids in (base_ids, pr_ids):
        for bench, cases in ids.items():
            (skip if bench in same else keep).update(cases)
    every = bool(base_ident) and base_ident.keys() == pr_ident.keys() == same
    every_listed = every and all(base_ids.get(b) or pr_ids.get(b) for b in same)
    return Same(frozenset(skip - keep), every_listed)


def every_identical(base_target: pathlib.Path, pr_target: pathlib.Path, base: str, pr: str) -> bool:
    base_ident, pr_ident = identities(base_target, base), identities(pr_target, pr)
    return bool(base_ident) and base_ident == pr_ident


def main() -> int:
    if len(sys.argv) != 6 or sys.argv[1] != "every":
        print(__doc__, file=sys.stderr)
        return 2
    base_target, pr_target, base, pr = pathlib.Path(sys.argv[2]), pathlib.Path(sys.argv[3]), *sys.argv[4:6]
    return 0 if every_identical(base_target, pr_target, base, pr) else 1


if __name__ == "__main__":
    sys.exit(main())
