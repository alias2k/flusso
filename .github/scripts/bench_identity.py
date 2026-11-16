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
skipped: what cannot be traced to an executable is compared. That covers a bench whose `--list`
failed on both sides: its ids are in no record, so they are compared like any id. When it failed on
one side only, the other side's record still lists the ids: they are skipped when the two identities
match (same executable, same inputs) and compared when they differ.

A bench target present on one side only (one a PR adds) is no reason to fail: its ids are reported
as measured on one side only, as they always were. One the PR removes fails bench-provenance.py,
before any comparison.

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
    """What the two sides share.

    - `ids`: the Criterion ids to skip (identical on both sides).
    - `every`: every bench target present on both sides is identical and listed its ids.
    - `must_compare`: ids both sides' executables list under a target whose identity differs:
      measured on one side only, they are an error, not a one-sided row.
    """

    ids: frozenset[str]
    every: bool
    must_compare: frozenset[str]


def compare(base_target: pathlib.Path, pr_target: pathlib.Path, base: str, pr: str) -> Same:
    base_ident, pr_ident = identities(base_target, base), identities(pr_target, pr)
    base_ids, pr_ids = listed(base_target, base), listed(pr_target, pr)
    same = {bench for bench in base_ident.keys() & pr_ident.keys() if base_ident[bench] == pr_ident[bench]}
    skip, keep = set(), set()
    for ids in (base_ids, pr_ids):
        for bench, cases in ids.items():
            (skip if bench in same else keep).update(cases)
    both = base_ident.keys() & pr_ident.keys()
    every = bool(both) and both == same and all(base_ids.get(b) or pr_ids.get(b) for b in same)
    must_compare = set()
    for bench in both - same:
        must_compare |= base_ids.get(bench, set()) & pr_ids.get(bench, set())
    return Same(frozenset(skip - keep), every, frozenset(must_compare))


def every_identical(base_target: pathlib.Path, pr_target: pathlib.Path, base: str, pr: str) -> bool:
    """Both sides recorded the same bench targets, each with the same identity.

    False when nothing recorded on either side: an empty selection proves nothing about any binary,
    so a caller never claims "every difference is noise" for it.
    """
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
