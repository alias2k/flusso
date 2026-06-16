#!/usr/bin/env python3
"""flusso plugin hook — auto-validate after a schema/config edit.

Wired as a PostToolUse hook on Edit/Write/MultiEdit. When the edited file is a
`*.schema.yml`/`*.schema.yaml` or a `flusso.toml`, it finds the owning
`flusso.toml` and runs `flusso check --offline` against it, feeding any
validation error back to the agent (exit 2 + stderr) so it gets fixed in the
same turn. Anything else — unrelated edits, no flusso project, no runner — exits
0 silently and never interrupts editing.

Runner resolution (first that works):
  1. $FLUSSO_CHECK_CMD  — full command prefix, e.g. "flusso" or "cargo run -q --"
  2. `flusso` on PATH
  3. `cargo run --quiet --`  from the nearest Cargo workspace root (repo-dev mode)
"""

import json
import os
import shutil
import subprocess
import sys

TIMEOUT_SECS = 180

# Substrings that mean "the database isn't reachable / configured" rather than
# "the schema is wrong". When an online check fails for one of these, we fall
# back to offline structural validation instead of nagging on every edit.
DB_UNAVAILABLE_MARKERS = (
    "could not connect",
    "failed to connect",
    "connection refused",
    "connection reset",
    "connection closed",
    "timed out",
    "missingconnectionurl",
    "database_url",
    "no such host",
    "name or service not known",
    "name resolution",
    "password authentication failed",
    "role \"",
    "database \"",
    "does not exist",
)


def looks_like_db_unavailable(detail):
    low = detail.lower()
    return any(m in low for m in DB_UNAVAILABLE_MARKERS)


def out(msg=""):
    if msg:
        sys.stdout.write(msg + "\n")


def fail_to_agent(msg):
    # exit 2 → stderr is surfaced to the model for PostToolUse.
    sys.stderr.write(msg + "\n")
    sys.exit(2)


def is_relevant(path):
    name = os.path.basename(path)
    return (
        path.endswith(".schema.yml")
        or path.endswith(".schema.yaml")
        or name == "flusso.toml"
    )


def find_config(start_path):
    """The edited flusso.toml itself, else the nearest one walking up."""
    if os.path.basename(start_path) == "flusso.toml":
        return start_path if os.path.isfile(start_path) else None
    d = os.path.dirname(os.path.abspath(start_path))
    last = None
    while d and d != last:
        candidate = os.path.join(d, "flusso.toml")
        if os.path.isfile(candidate):
            return candidate
        last, d = d, os.path.dirname(d)
    return None


def find_cargo_root(start_dir):
    """Nearest dir up the tree whose Cargo.toml declares a [workspace]."""
    d = os.path.abspath(start_dir)
    last = None
    while d and d != last:
        cargo = os.path.join(d, "Cargo.toml")
        if os.path.isfile(cargo):
            try:
                with open(cargo, "r", encoding="utf-8") as f:
                    if "[workspace]" in f.read():
                        return d
            except OSError:
                pass
        last, d = d, os.path.dirname(d)
    return None


def resolve_runner(config_path):
    override = os.environ.get("FLUSSO_CHECK_CMD")
    if override:
        return override.split(), None
    if shutil.which("flusso"):
        return ["flusso"], None
    cargo_root = find_cargo_root(os.path.dirname(config_path))
    if cargo_root and shutil.which("cargo"):
        return ["cargo", "run", "--quiet", "--"], cargo_root
    return None, None


def run_check(runner, cwd, config, offline):
    """Run `flusso check`. Returns (returncode, detail) or None to skip
    (timeout / spawn failure — never a reason to block editing)."""
    cmd = runner + ["check", "--config", config]
    if offline:
        cmd.append("--offline")
    try:
        proc = subprocess.run(
            cmd, cwd=cwd, capture_output=True, text=True, timeout=TIMEOUT_SECS
        )
    except (subprocess.TimeoutExpired, OSError):
        return None
    return proc.returncode, (proc.stderr or proc.stdout or "").strip()


def main():
    try:
        data = json.load(sys.stdin)
    except (json.JSONDecodeError, ValueError):
        sys.exit(0)  # not our concern

    path = (data.get("tool_input") or {}).get("file_path")
    if not path or not is_relevant(path):
        sys.exit(0)

    config = find_config(path)
    if not config:
        sys.exit(0)  # edited file isn't part of a flusso project — nothing to validate

    runner, cwd = resolve_runner(config)
    if not runner:
        # No way to run flusso here; stay quiet rather than nag on every edit.
        sys.exit(0)

    rel = os.path.relpath(config)

    # Online check first — it also confirms declared types/nullability against
    # the live Postgres columns, which is the check a dev with a DB running
    # actually wants.
    online = run_check(runner, cwd, config, offline=False)
    if online is None:
        sys.exit(0)  # timeout / couldn't spawn — never block editing
    rc, detail = online
    if rc == 0:
        sys.exit(0)  # valid against the live DB — stay silent

    # The DB just isn't up (or isn't configured)? Fall back to offline
    # structural validation so we still catch schema errors without nagging
    # about connections on every edit.
    if looks_like_db_unavailable(detail):
        offline = run_check(runner, cwd, config, offline=True)
        if offline is None or offline[0] == 0:
            sys.exit(0)  # structure is fine; DB simply isn't reachable
        detail = offline[1]
        note = "(validated offline — the database was not reachable)"
    else:
        note = "(validated against the live database)"

    fail_to_agent(
        f"flusso check failed after this edit {note} — {rel} no longer "
        f"validates:\n\n{detail}\n\n"
        "Fix the reported schema/config error (see the flusso-schema skill), "
        "then it will re-validate on the next edit."
    )


if __name__ == "__main__":
    main()
