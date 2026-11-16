#!/usr/bin/env python3
"""Fail when a test or bench starts Postgres without going through `dev/test-postgres`.

    check-postgres-image.py [<repo-root>]

`dev/test-postgres` (crate `flusso-test-postgres`) is the one place that names the
Postgres image the tests and benches start (issue #342): its default tag, the
`FLUSSO_TEST_PG_TAG` override, and the named exceptions for a test that needs one
version. Every `.rs` file under `apps/`, `libs/`, `dev/` and `sdk/` outside that crate
is scanned for:

- `Postgres::default` (a start of the `testcontainers-modules` image, whose own
  default tag is `11-alpine`: issue #342);
- `.with_tag(`: a hard-coded tag, which only the home may write;
- `GenericImage::new("postgres"`: the same image started by hand.

Whitespace and line breaks inside a match are ignored (`Postgres\\n    ::default`,
`.\\n with_tag (`), and lines that are only a `//` comment are skipped. Prints one
line per finding and exits 1 when there is any, 0 otherwise.
"""
import pathlib
import re
import sys

ROOT = pathlib.Path(__file__).resolve().parents[2]
SCANNED = ("apps", "libs", "dev", "sdk")
HOME = pathlib.PurePosixPath("dev/test-postgres")
SKIPPED_DIRS = {"target", "node_modules", ".git"}

RULES = (
    (re.compile(r"\bPostgres\s*::\s*default\b"),
     "starts the testcontainers Postgres image directly; use `test_postgres::image()`"),
    (re.compile(r"\.\s*with_tag\s*\("),
     "hard-codes an image tag; the Postgres tag lives in `dev/test-postgres` "
     "(a named `test_postgres::Pinned` for a test that needs one version)"),
    (re.compile(r"\bGenericImage\s*::\s*new\s*\(\s*\"postgres\""),
     "starts Postgres as a generic image; use `test_postgres::image()`"),
)


def strip_comment_lines(text: str) -> str:
    """Blank every line that is only a `//` comment, keeping line numbers."""
    return "\n".join("" if line.lstrip().startswith("//") else line for line in text.split("\n"))


def findings_in(text: str) -> list[tuple[int, str]]:
    code = strip_comment_lines(text)
    found = []
    for pattern, reason in RULES:
        for match in pattern.finditer(code):
            found.append((code.count("\n", 0, match.start()) + 1, reason))
    return sorted(found)


def sources(root: pathlib.Path):
    for top in SCANNED:
        base = root / top
        if not base.is_dir():
            continue
        for path in sorted(base.rglob("*.rs")):
            relative = path.relative_to(root)
            if SKIPPED_DIRS.intersection(relative.parts):
                continue
            if pathlib.PurePosixPath(relative.as_posix()).is_relative_to(HOME):
                continue
            yield relative, path


def scan(root: pathlib.Path) -> list[str]:
    report = []
    for relative, path in sources(root):
        for line, reason in findings_in(path.read_text(encoding="utf-8")):
            report.append(f"{relative.as_posix()}:{line}: {reason}")
    return report


def main() -> int:
    root = pathlib.Path(sys.argv[1]) if len(sys.argv) > 1 else ROOT
    report = scan(root)
    for line in report:
        print(line)
    if report:
        print(f"error: {len(report)} Postgres start(s) bypass dev/test-postgres (issue #342)", file=sys.stderr)
        return 1
    print("every Postgres start goes through dev/test-postgres")
    return 0


if __name__ == "__main__":
    sys.exit(main())
