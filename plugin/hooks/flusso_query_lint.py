#!/usr/bin/env python3
"""flusso plugin hook — flag flusso-query escape-hatch handles after a Rust edit.

Wired as a PostToolUse hook on Edit/Write/MultiEdit. When the edited `.rs` file
uses `#[derive(FlussoDocument)]` *and* also constructs a `Keyword`/`Text` handle
from a string literal path (`Keyword::at("…")` / `Text::<Root>::at("…")`), it
feeds the agent a fix-it note (exit 2 + stderr): in a derive file every schema
field already has a generated `Type::field()` handle, so a string-path handle
bypasses the compile-time mapping check that is the whole point of the crate —
the classic escape-hatch mistake.

High precision by design: a file with no `#[derive(FlussoDocument)]` (hand-written
handles, the crate's own tests, derive-less consumers) is never flagged, and the
string-path form is what the derive never emits. Style only — the typed fix
compiles identically; this never blocks editing, it just nudges a same-turn fix.
Anything else exits 0 silently.
"""

import json
import re
import sys

# `Keyword`/`Text` built from a string literal path (optional scope turbofish):
# `Keyword::at("…")`, `Text::<Root>::at("…")`. This is the escape hatch.
HANDLE_AT = re.compile(r'\b(?:Keyword|Text)\s*(?:::<[^>;{}]*>)?\s*::\s*at\s*\(\s*"')

# The file declares at least one `#[derive(... FlussoDocument ...)]`.
HAS_DERIVE = re.compile(r"#\[derive\([^)]*\bFlussoDocument\b")


def main():
    try:
        data = json.load(sys.stdin)
    except (json.JSONDecodeError, ValueError):
        sys.exit(0)  # not our concern

    path = (data.get("tool_input") or {}).get("file_path")
    if not path or not path.endswith(".rs"):
        sys.exit(0)

    try:
        with open(path, "r", encoding="utf-8") as f:
            src = f.read()
    except OSError:
        sys.exit(0)

    # Only flag in a derive-using file — there, a generated `Type::field()`
    # handle covers every field, so a string-path handle is the escape hatch.
    if not HAS_DERIVE.search(src):
        sys.exit(0)

    hits = [i for i, line in enumerate(src.splitlines(), 1) if HANDLE_AT.search(line)]
    if not hits:
        sys.exit(0)

    where = ", ".join(f"line {n}" for n in hits)
    sys.stderr.write(
        f"flusso-query: string-path handle in {path} ({where}).\n"
        "In a `#[derive(FlussoDocument)]` file every schema field already has a "
        "generated `Type::field()` handle. A `Keyword::at(\"…\")` / `Text::at(\"…\")` "
        "string path bypasses the compile-time mapping check (the point of the crate). "
        "Use the generated handle, e.g. `Type::field_name()`. If you truly need a path "
        "the typed surface can't express, say so — otherwise switch it.\n"
    )
    sys.exit(2)


if __name__ == "__main__":
    main()
