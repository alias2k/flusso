#!/usr/bin/env python3
"""Fail when a test or bench starts Postgres without going through `dev/test-postgres`.

    check-postgres-image.py [<repo-root>]

`dev/test-postgres` (crate `flusso-test-postgres`) is the one place that names the
Postgres image the tests and benches start (issue #342): its default tag, the
`FLUSSO_TEST_PG_TAG` override, and the named exceptions for a test that needs one
version. Every `.rs` file under `apps/`, `libs/`, `dev/` and `sdk/` outside that crate
is scanned with its comments blanked (line, block and nested block comments; string,
raw-string and char literals are read as such, so `"//"` is not a comment), and every
rule but the `GenericImage` arguments also reads string contents as blank.

The root rule is the import: only the home may reach `testcontainers_modules::postgres`,
whose `Postgres::default()` is `postgres:11-alpine`. So `testcontainers_modules` may
appear only as `testcontainers_modules::testcontainers…` or `testcontainers_modules as _`;
anything else (`::postgres`, a nested `{…}` or glob import, a renamed crate) is a finding.
Callers name the container type as `test_postgres::Container`. Beside it:

- `Postgres::default` (whitespace and comments between the parts included);
- `with_tag` / `with_name` in any call form: a hard-coded tag or image name;
- `GenericImage::new(…)` whose arguments mention `postgres` in any case or spelling, or
  whose image name is not a string literal; `GenericImage as …`.

Prints one line per finding and exits 1 when there is any, 0 otherwise.
"""
import pathlib
import re
import sys

ROOT = pathlib.Path(__file__).resolve().parents[2]
SCANNED = ("apps", "libs", "dev", "sdk")
HOME = pathlib.PurePosixPath("dev/test-postgres")
SKIPPED_DIRS = {"target", "node_modules", ".git"}

IMPORT = re.compile(r"\btestcontainers_modules\b")
ALLOWED_IMPORT = re.compile(r"\s*::\s*testcontainers\b|\s+as\s+_\b")
GENERIC_IMAGE_NEW = re.compile(r"\bGenericImage\s*::\s*new\s*\(")
STRING_FIRST = re.compile(r'\s*(?:"|r#*")')

RULES = (
    (re.compile(r"\bPostgres\s*::\s*default\b"),
     "starts the testcontainers Postgres image directly; use `test_postgres::image()`"),
    (re.compile(r"(?:\.\s*)?\bwith_(?:tag|name)\b"),
     "hard-codes an image tag or name; the Postgres image lives in `dev/test-postgres` "
     "(a named `test_postgres::Pinned` for a test that needs one version)"),
    (re.compile(r"\bGenericImage\s+as\b"),
     "renames GenericImage, which hides its arguments from this guard"),
)

RAW_STRING = re.compile(r'b?r(#*)"')
CHAR_LITERAL = re.compile(r"b?'(?:\\u\{[0-9a-fA-F]+\}|\\.|[^\\'\n])'")


def blank(chars: list[str], start: int, end: int) -> None:
    for i in range(start, end):
        if chars[i] != "\n":
            chars[i] = " "


def strip_comments(text: str, strings: bool = False) -> str:
    """Blank every comment (and, with `strings`, every literal's contents), keeping line numbers."""
    chars = list(text)
    i, n = 0, len(text)
    while i < n:
        if text.startswith("//", i):
            end = text.find("\n", i)
            end = n if end == -1 else end
            blank(chars, i, end)
            i = end
        elif text.startswith("/*", i):
            depth, j = 1, i + 2
            while j < n and depth:
                if text.startswith("/*", j):
                    depth, j = depth + 1, j + 2
                elif text.startswith("*/", j):
                    depth, j = depth - 1, j + 2
                else:
                    j += 1
            blank(chars, i, j)
            i = j
        elif (raw := RAW_STRING.match(text, i)) and (i == 0 or not (text[i - 1].isalnum() or text[i - 1] == "_")):
            close = text.find('"' + raw.group(1), raw.end())
            end = n if close == -1 else close
            if strings:
                blank(chars, raw.end(), end)
            i = n if close == -1 else close + 1 + len(raw.group(1))
        elif text[i] == '"':
            j = i + 1
            while j < n and text[j] != '"':
                j += 2 if text[j] == "\\" else 1
            if strings:
                blank(chars, i + 1, min(j, n))
            i = j + 1
        elif text[i] in "'b" and (char := CHAR_LITERAL.match(text, i)):
            if strings:
                blank(chars, i + 1, char.end() - 1)
            i = char.end()
        else:
            i += 1
    return "".join(chars)


def arguments(code: str, open_paren: int) -> str:
    """The text between the parenthesis before `open_paren` and its match."""
    depth, j = 1, open_paren
    while j < len(code) and depth:
        depth += {"(": 1, ")": -1}.get(code[j], 0)
        j += 1
    return code[open_paren:j - 1]


def findings_in(text: str) -> list[tuple[int, str]]:
    code = strip_comments(text)
    bare = strip_comments(text, strings=True)
    found = []

    def at(offset: int, reason: str) -> None:
        found.append((code.count("\n", 0, offset) + 1, reason))

    for match in IMPORT.finditer(bare):
        if not ALLOWED_IMPORT.match(bare, match.end()):
            at(match.start(), "reaches past `testcontainers_modules::testcontainers` (the Postgres module, a nested, "
                              "glob or renamed import); name the type as `test_postgres::Container`")
    for pattern, reason in RULES:
        for match in pattern.finditer(bare):
            at(match.start(), reason)
    for match in GENERIC_IMAGE_NEW.finditer(bare):
        args = arguments(code, match.end())
        if "postgres" in args.lower():
            at(match.start(), "starts Postgres as a generic image; use `test_postgres::image()`")
        elif not STRING_FIRST.match(args):
            at(match.start(), "names a generic image through a variable; write the image name as a literal")
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
