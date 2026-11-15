#!/usr/bin/env python3
"""Record a warm base build, and tell whether it is still current.

    review-ab-stamp.py write <stamp> --commit SHA --toolchain TEXT --build-env TEXT
                             --target DIR --executables TSV [--root DIR --file REL ...]
    review-ab-stamp.py check <stamp> --commit SHA --toolchain TEXT --build-env TEXT
                             --target DIR [--need BENCH ...] [--root DIR --file REL ...]

`write` stores, as JSON: the base commit, the toolchain (`rustc -vV`), the
build settings (`RUSTFLAGS`, `CARGO_PROFILE_*`, the build script's sha256: any
text the caller passes), the sha256 of each bench file laid over the base (`--file`, relative to `--root`,
the head tree), and the bench executables bench-in-process.sh recorded in TSV
(`<bench>\t<path>\t<sha256>` lines).

`check` exits 0 and prints `current` when the stamp matches all of these,
every bench named by `--need` was built, and every recorded executable still
exists inside `--target` with its sha256.
Otherwise it exits 1 and prints the first reason the build is stale.
"""
import argparse
import hashlib
import json
import pathlib
import sys


def sha256(path: pathlib.Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def overlay(root: pathlib.Path | None, files: list[str]) -> dict[str, str]:
    if not files:
        return {}
    if root is None:
        raise SystemExit("--file needs --root")
    return {rel: sha256(root / rel) for rel in sorted(files)}


def executables(tsv: pathlib.Path) -> dict[str, list[str]]:
    out = {}
    for line in tsv.read_text().splitlines():
        if line:
            name, exe, digest = line.split("\t")
            out[name] = [exe, digest]
    return out


def stale_reason(
    stamp: dict,
    commit: str,
    toolchain: str,
    build_env: str,
    target: pathlib.Path,
    files: dict[str, str],
    need: list[str],
) -> str | None:
    """Why the stamped build no longer stands for this base, or None when it does."""
    if stamp.get("commit") != commit:
        return f"base moved {str(stamp.get('commit'))[:12]} → {commit[:12]}"
    if stamp.get("toolchain") != toolchain:
        return "toolchain changed"
    if stamp.get("build_env") != build_env:
        return "build settings changed"
    recorded = stamp.get("files", {})
    for rel in sorted(set(recorded) | set(files)):
        if recorded.get(rel) != files.get(rel):
            return f"bench file changed: {rel}"
    exes = stamp.get("executables") or {}
    if not exes:
        return "no executable recorded"
    for name in need:
        if name not in exes:
            return f"bench not built: {name}"
    target = target.resolve()
    for name, (exe, digest) in sorted(exes.items()):
        path = pathlib.Path(exe).resolve()
        if target not in path.parents:
            return f"executable outside the base target dir: {name}"
        if not path.is_file():
            return f"executable missing: {name}"
        if sha256(path) != digest:
            return f"executable changed: {name}"
    return None


def main() -> int:
    parser = argparse.ArgumentParser(usage=__doc__)
    parser.add_argument("action", choices=["write", "check"])
    parser.add_argument("stamp", type=pathlib.Path)
    parser.add_argument("--commit", required=True)
    parser.add_argument("--toolchain", required=True)
    parser.add_argument("--build-env", required=True)
    parser.add_argument("--need", action="append", default=[])
    parser.add_argument("--target", type=pathlib.Path, required=True)
    parser.add_argument("--executables", type=pathlib.Path)
    parser.add_argument("--root", type=pathlib.Path)
    parser.add_argument("--file", action="append", default=[])
    args = parser.parse_args()
    files = overlay(args.root, args.file)

    if args.action == "write":
        if args.executables is None:
            parser.error("write needs --executables")
        stamp = {
            "commit": args.commit,
            "toolchain": args.toolchain,
            "build_env": args.build_env,
            "files": files,
            "executables": executables(args.executables),
        }
        args.stamp.parent.mkdir(parents=True, exist_ok=True)
        args.stamp.write_text(json.dumps(stamp, indent=2, sort_keys=True) + "\n")
        return 0

    try:
        stamp = json.loads(args.stamp.read_text())
    except (OSError, ValueError):
        print("no warm build recorded")
        return 1
    reason = stale_reason(stamp, args.commit, args.toolchain, args.build_env, args.target, files, args.need)
    print(reason or "current")
    return 1 if reason else 0


if __name__ == "__main__":
    sys.exit(main())
