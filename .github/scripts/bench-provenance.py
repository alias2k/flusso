#!/usr/bin/env python3
"""Prove the base and the PR baselines came from their own builds.

    bench-provenance.py <base-target> <pr-target> <base-baseline> <pr-baseline>
                        [--same-as <first-base> <first-pr>]

Reads the executables bench-in-process.sh recorded in
`<target>/bench-executables/<baseline>.tsv` and exits 1 unless:

- the two target dirs are different and neither holds the other;
- each side ran at least one bench, every one from its own target dir;
- both sides ran the same bench targets (one only the PR has, such as a bench
  added by the PR, is reported, not failed).

With --same-as, the run being checked is a re-measurement: every executable
must also have the same sha256 as that side's first pass, so a re-run measures
exactly the binary the first pass did.

Why paths and not bytes: two target dirs never share an artifact, so each side
runs what cargo compiled from its own tree (one shared target dir is what let
the PR side reuse the base's binary: cargo keys a workspace member by its path
relative to the workspace root, and judged the base build fresh for the PR
tree). Identical bytes are not evidence of that bug: a PR that changes only a
`#[cfg(test)]` module, a comment, or code no bench links compiles to the same
bench binary. The byte comparison is printed for the reader, not gated.
"""
import argparse
import pathlib
import sys


def load(target: pathlib.Path, baseline: str) -> dict[str, tuple[pathlib.Path, str]]:
    path = target / "bench-executables" / f"{baseline}.tsv"
    if not path.is_file():
        return {}
    runs = {}
    for line in path.read_text().splitlines():
        name, exe, sha = line.split("\t")
        runs[name] = (pathlib.Path(exe).resolve(), sha)
    return runs


def inside(path: pathlib.Path, root: pathlib.Path) -> bool:
    return path == root or root in path.parents


def main() -> int:
    parser = argparse.ArgumentParser(usage=__doc__)
    parser.add_argument("base_target", type=pathlib.Path)
    parser.add_argument("pr_target", type=pathlib.Path)
    parser.add_argument("base")
    parser.add_argument("pr")
    parser.add_argument("--same-as", nargs=2, metavar=("FIRST_BASE", "FIRST_PR"))
    args = parser.parse_args()
    base_target, pr_target = args.base_target.resolve(), args.pr_target.resolve()

    errors = []
    if inside(base_target, pr_target) or inside(pr_target, base_target):
        errors.append(f"the base and PR target dirs overlap ({base_target}, {pr_target}): one build can stand in for the other")
    sides = {"base": (base_target, load(base_target, args.base)), "pr": (pr_target, load(pr_target, args.pr))}
    for side, (target, runs) in sides.items():
        if not runs:
            errors.append(f"the {side} side recorded no bench executable")
        for name, (exe, _) in runs.items():
            if not inside(exe, target):
                errors.append(f"the {side} side ran `{name}` from {exe}, outside its target dir {target}")
    base_runs, pr_runs = sides["base"][1], sides["pr"][1]
    missing = sorted(set(base_runs) - set(pr_runs))
    if missing:
        errors.append(f"the PR side did not run {', '.join(missing)}, which the base did")

    if args.same_as:
        first = {"base": load(base_target, args.same_as[0]), "pr": load(pr_target, args.same_as[1])}
        for side, (_, runs) in sides.items():
            for name, (exe, sha) in runs.items():
                was = first[side].get(name)
                if was is None or was[1] != sha:
                    errors.append(f"the {side} re-run of `{name}` is not the binary its first pass ran ({exe})")

    if not args.same_as:
        print("| bench | base sha256 | PR sha256 | bytes |")
        print("| --- | --- | --- | --- |")
        for name in sorted(set(base_runs) | set(pr_runs)):
            b = base_runs.get(name, (None, "—"))[1]
            p = pr_runs.get(name, (None, "—"))[1]
            same = "—" if "—" in (b, p) else "identical" if b == p else "differ"
            print(f"| `{name}` | `{b[:12]}` | `{p[:12]}` | {same} |")
        only_pr = sorted(set(pr_runs) - set(base_runs))
        if only_pr:
            print(f"Only on the PR side (new): {', '.join(only_pr)}.")

    for error in errors:
        print(f"::error::bench provenance: {error}")
    if not errors:
        print("Bench provenance: each side ran its own build" + (", the same binaries as its first pass." if args.same_as else "."))
    return 1 if errors else 0


if __name__ == "__main__":
    sys.exit(main())
