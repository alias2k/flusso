#!/usr/bin/env python3
"""Compare how Postgres planned base's and head's document queries: `review-ab.sh --plans`'s table.

    review-ab-plans.py <dir>

`<dir>` holds, per `plan_cache_mode` `<pcm>`, `plans-base-<pcm>.tsv` and
`plans-head-<pcm>.tsv` (what the `cases` bench writes in its `plans` mode), and
`server.log`, the Postgres server log of the whole run (`auto_explain` with
`log_format = json`, `log_min_duration = 0`).

A TSV line is one case: `<case>\t<root relations>\t<partitions>\t<statements>\t
<calls>\t<plans>\t<calls, second half>\t<plans, second half>`, counted by
pg_stat_statements over every statement naming the root (summed, never per
entry: Postgres 18 merges `ARRAY[…]` lists of different lengths into one).

The harness opens each case with `SELECT 'flusso-review-case <side>-<pcm>
<case>'` and closes it with the case `-`; the case's last logged plan that
scans its root gives:

- the root scan: the first node scanning a root relation (`Index Scan using
  …`, `Seq Scan`, `Bitmap Heap Scan`); for a partitioned root, the node type;
- for a partitioned root, the partitions left in the plan and `Subplans
  Removed` (run-time pruning) of the root's own Append (one whose children
  scan the root's partitions; a partitioned child's Append doesn't count).

Prints one table per `plan_cache_mode`; a cell reads `base → head` when the
sides differ. Flags, head against base:

- `plans every call`: the second half of the calls made ≥ 0.5 plans per call
  on head and fewer on base (a plan that is never cached);
- `root scan changed`;
- `pruning lost`: more partitions left on head;
- `one side only`: a case measured on one side;
- `no plan`: no logged plan of the case scans its root on a side, so its scan
  and pruning can't be compared.

Exit: 0 when nothing is flagged, 1 otherwise, 2 on a usage error, no case,
or a log with no plan at all for a side and `plan_cache_mode` (a missing or
wrong `server.log`, `auto_explain` not loaded): that is a setup error, not a
finding.
"""
import json
import pathlib
import re
import sys

MARKER = re.compile(r"flusso-review-case ([^\s']+) ([^\s']+)")
MODES = ("auto", "force_generic_plan")
REPLANS = 0.5


def load_cases(path: pathlib.Path) -> dict[str, dict]:
    cases = {}
    for number, line in enumerate(path.read_text().splitlines(), 1):
        if not line:
            continue
        parts = line.split("\t")
        if len(parts) != 8:
            raise ValueError(f"{path}:{number}: expected 8 tab-separated fields, got {len(parts)}")
        case, relations, partitions, statements, calls, plans, calls2, plans2 = parts
        cases[case] = {
            "relations": set(relations.split(",")),
            "partitions": int(partitions),
            "statements": int(statements),
            "calls": int(calls),
            "plans": int(plans),
            "per_call": int(plans2) / int(calls2) if int(calls2) else 0.0,
        }
    return cases


def logged_plans(lines: list[str]):
    """Each `auto_explain` JSON plan in a Postgres stderr log, in order."""
    body: list[str] | None = None
    for line in lines + [""]:
        if body is not None and line.startswith("\t"):
            body.append(line[1:])
            continue
        if body:
            try:
                yield json.loads("\n".join(body))
            except ValueError:
                pass
        body = [] if "duration:" in line and line.rstrip().endswith("plan:") else None


def nodes(plan: dict):
    yield plan
    for child in plan.get("Plans", []):
        yield from nodes(child)


def scan(plan: dict, relations: set[str], partitioned: bool) -> dict | None:
    """The root scan and the partitions left of one logged plan, or None when it never scans the root."""
    every = list(nodes(plan.get("Plan", {})))
    scans = [n for n in every if n.get("Relation Name") in relations]
    if not scans:
        return None
    first = scans[0]
    kind = first.get("Node Type", "?")
    if not partitioned and "Index Name" in first:
        kind += f" using {first['Index Name']}"
    removed = sum(
        n.get("Subplans Removed", 0)
        for n in every
        if n.get("Node Type") in ("Append", "Merge Append")
        and any(child.get("Relation Name") in relations for child in n.get("Plans", []))
    )
    return {"scan": kind, "left": len({n["Relation Name"] for n in scans}), "removed": removed}


def plans_by_case(log: list[str], tsvs: dict[str, dict[str, dict]]) -> dict[tuple[str, str], dict]:
    """The last plan of each `(run, case)` that scans the case's root."""
    out: dict[tuple[str, str], dict] = {}
    current: tuple[str, str] | None = None
    for plan in logged_plans(log):
        match = MARKER.search(plan.get("Query Text", ""))
        if match:
            current = (match.group(1), match.group(2))
            continue
        if current is None:
            continue
        case = tsvs.get(current[0], {}).get(current[1])
        if case is None:
            continue
        found = scan(plan, case["relations"], case["partitions"] > 0)
        if found:
            out[current] = found
    return out


def cell(base: str, head: str) -> str:
    return base if base == head else f"{base} → {head}"


def partitions(found: dict | None, total: int) -> str:
    if not total:
        return "—"
    if found is None:
        return "?"
    text = f"{found['left']} of {total}"
    return text + (f", {found['removed']} removed" if found["removed"] else "")


def table(mode: str, base: dict, head: dict, plans: dict) -> tuple[list[str], bool]:
    rows = [
        f"### plan_cache_mode = {mode}",
        "",
        "| case | statements | calls | plans | plans per call | root scan | partitions | flag |",
        "| --- | ---: | ---: | ---: | ---: | --- | --- | --- |",
    ]
    flagged = False
    for case in sorted(base.keys() | head.keys()):
        b, h = base.get(case), head.get(case)
        if b is None or h is None:
            flagged = True
            rows.append(f"| `{case}` | | | | | | | one side only ⚠️ |")
            continue
        pb, ph = plans.get((f"base-{mode}", case)), plans.get((f"head-{mode}", case))
        flags = []
        if h["per_call"] >= REPLANS > b["per_call"]:
            flags.append("plans every call")
        if pb and ph and pb["scan"] != ph["scan"]:
            flags.append("root scan changed")
        if pb and ph and h["partitions"] and ph["left"] > pb["left"]:
            flags.append("pruning lost")
        missing = [side for side, found in (("base", pb), ("head", ph)) if found is None]
        if missing:
            flags.append(f"no plan on {' and '.join(missing)}")
        flagged = flagged or bool(flags)
        scan_b = pb["scan"] if pb else "?"
        scan_h = ph["scan"] if ph else "?"
        rows.append(
            "| `{}` | {} | {} | {} | {} | {} | {} | {} |".format(
                case,
                cell(str(b["statements"]), str(h["statements"])),
                cell(str(b["calls"]), str(h["calls"])),
                cell(str(b["plans"]), str(h["plans"])),
                cell(f"{b['per_call']:.2f}", f"{h['per_call']:.2f}"),
                cell(scan_b, scan_h),
                cell(partitions(pb, b["partitions"]), partitions(ph, h["partitions"])),
                " ".join(f"{flag} ⚠️" for flag in flags),
            )
        )
    return rows, flagged


def main() -> int:
    if len(sys.argv) != 2:
        print(__doc__, file=sys.stderr)
        return 2
    root = pathlib.Path(sys.argv[1])
    tsvs = {}
    log_path = root / "server.log"
    try:
        for mode in MODES:
            for side in ("base", "head"):
                path = root / f"plans-{side}-{mode}.tsv"
                if path.is_file():
                    tsvs[f"{side}-{mode}"] = load_cases(path)
        log = log_path.read_text(errors="replace").splitlines() if log_path.is_file() else []
    except (OSError, ValueError) as error:
        print(f"error: {error}", file=sys.stderr)
        return 2
    if not any(tsvs.values()):
        print(f"error: no case measured in {root} (does --filter match a case?)", file=sys.stderr)
        return 2
    for mode in MODES:
        if not tsvs.get(f"base-{mode}") and not tsvs.get(f"head-{mode}"):
            print(f"error: no case measured under plan_cache_mode = {mode} in {root}", file=sys.stderr)
            return 2
    plans = plans_by_case(log, tsvs)
    for run_name, cases in sorted(tsvs.items()):
        if cases and not any((run_name, case) in plans for case in cases):
            side, mode = run_name.split("-", 1)
            where = f"{log_path}" if log else f"{log_path}, which is missing or empty"
            print(
                f"error: no auto_explain plan of the {side} side under plan_cache_mode = {mode} in {where}",
                file=sys.stderr,
            )
            return 2
    out, flagged = [], False
    for mode in MODES:
        base, head = tsvs.get(f"base-{mode}"), tsvs.get(f"head-{mode}")
        if base is None and head is None:
            continue
        rows, flag = table(mode, base or {}, head or {}, plans)
        out += rows + [""]
        flagged = flagged or flag
    print("\n".join(out).rstrip())
    return 1 if flagged else 0


if __name__ == "__main__":
    sys.exit(main())
