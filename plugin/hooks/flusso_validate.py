#!/usr/bin/env python3
"""flusso plugin hook — auto-validate after a schema/config edit.

Wired as a PostToolUse hook on Edit/Write/MultiEdit. When the edited file is a
`*.schema.yml`/`*.schema.yaml` or a `flusso.toml`, it finds the owning
`flusso.toml` and runs `flusso check --offline` against it, feeding any
validation error back to the agent (exit 2 + stderr) so it gets fixed in the
same turn. Anything else — unrelated edits, no flusso project, no runner — exits
0 silently and never interrupts editing.

Runner resolution: see "Runner resolution" in plugin/README.md. Any failure
message names the runner it used.
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


def builds_flusso(root):
    """True when the workspace at `root` contains the flusso CLI package."""
    try:
        with open(os.path.join(root, "apps", "cli", "Cargo.toml"), "r", encoding="utf-8") as f:
            return 'name = "flusso-cli"' in f.read()
    except OSError:
        return False


def path_flusso_label(exe):
    try:
        out = subprocess.run(
            [exe, "--version"], capture_output=True, text=True, timeout=10
        ).stdout.split()
    except (subprocess.TimeoutExpired, OSError):
        out = []
    return f"{exe} {out[-1]}" if out else exe


def resolve_runner(config_path):
    """Returns (command prefix, cwd, label) or (None, None, None)."""
    override = os.environ.get("FLUSSO_CHECK_CMD")
    if override:
        return override.split(), None, "$FLUSSO_CHECK_CMD"
    cargo_root = find_cargo_root(os.path.dirname(config_path))
    if cargo_root and builds_flusso(cargo_root) and shutil.which("cargo"):
        return ["cargo", "run", "--quiet", "--"], cargo_root, f"cargo run from {cargo_root}"
    exe = shutil.which("flusso")
    if exe:
        return [exe], None, exe
    return None, None, None


def run_check(runner, cwd, config, offline):
    """Run `flusso check`. Returns (returncode, detail, db_unavailable) or
    None to skip (timeout / spawn failure — never a reason to block editing)."""
    cmd = runner + ["check", "--config", config]
    if offline:
        cmd.append("--offline")
    try:
        proc = subprocess.run(
            cmd, cwd=cwd, capture_output=True, text=True, timeout=TIMEOUT_SECS
        )
    except (subprocess.TimeoutExpired, OSError):
        return None
    error = (proc.stderr or proc.stdout or "").strip()
    if proc.returncode == 0:
        return proc.returncode, error, False
    # A failing check prints its diagnostics on stdout and only the one-line
    # verdict on stderr: show both, diagnostics first. Whether the database
    # was unreachable is read from the verdict alone, not the printed mapping.
    parts = (proc.stdout.strip(), proc.stderr.strip())
    detail = "\n".join(part for part in parts if part)
    return proc.returncode, detail, looks_like_db_unavailable(error)


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

    runner, cwd, label = resolve_runner(config)
    if not runner:
        # No way to run flusso here; stay quiet rather than nag on every edit.
        sys.exit(0)

    config = os.path.abspath(config)
    rel = os.path.relpath(config)

    # Online check first — it also confirms declared types/nullability against
    # the live Postgres columns, which is the check a dev with a DB running
    # actually wants.
    online = run_check(runner, cwd, config, offline=False)
    if online is None:
        sys.exit(0)  # timeout / couldn't spawn — never block editing
    rc, detail, db_unavailable = online
    if rc == 0:
        sys.exit(0)  # valid against the live DB — stay silent

    # The DB just isn't up (or isn't configured)? Fall back to offline
    # structural validation so we still catch schema errors without nagging
    # about connections on every edit.
    if db_unavailable:
        offline = run_check(runner, cwd, config, offline=True)
        if offline is None or offline[0] == 0:
            sys.exit(0)  # structure is fine; DB simply isn't reachable
        detail = offline[1]
        note = "(validated offline — the database was not reachable)"
    else:
        note = "(validated against the live database)"

    if label == runner[0]:
        label = path_flusso_label(label)
    fail_to_agent(
        f"flusso check failed after this edit {note} — {rel} no longer "
        f"validates (runner: {label}):\n\n{detail}\n\n"
        "Fix the reported schema/config error (see the flusso-schema skill), "
        "then it will re-validate on the next edit."
    )


if __name__ == "__main__":
    main()
