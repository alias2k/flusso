#!/usr/bin/env python3
"""Compare the documents base and head built: `review-ab.sh --digest`'s table.

    review-ab-digest.py <base.tsv> <head.tsv>

Each file is what the `cases` bench writes in its `digest` mode, one line per
built document: `<index>\t<form>\t<id>\t<op>\t<body>`, the body canonical JSON
(sorted keys, `sink::to_json`). The forms are `build`, `build_many<16`,
`build_many>=16` and `backfill`.

Prints one row per index: the documents each side built, a digest of each
side's lines (sha256 of the sorted lines, first 12 hex), and a verdict:

- `same`: both sides wrote the same lines;
- `differ`: they did not; below the table, the first differing documents and
  a diff of one of them;
- `forms disagree`: on one side, two forms built different documents for the
  same id (`build` against `build_many`, say), even if base and head agree;
- `one side only`: the index is missing on a side;
- `no documents`: both sides declared the index (a `#index <index>` line)
  and neither built a document for it.

`#note <index> <text>` lines (what the harness could not build, such as
missing ids on a root keyed by more than its `id`) are printed below the
table, once.

Exit: 0 when every index is `same`, 1 otherwise, 2 on a usage error or an
unreadable file.
"""
import collections
import difflib
import hashlib
import json
import pathlib
import sys

SHOWN = 5


def load(path: pathlib.Path, notes: dict[str, set[str]]) -> dict[str, list[tuple[str, str, str, str]]]:
    by_index: dict[str, list[tuple[str, str, str, str]]] = collections.defaultdict(list)
    for number, line in enumerate(path.read_text().splitlines(), 1):
        if not line:
            continue
        if line.startswith("#index\t"):
            by_index[line.split("\t", 1)[1]]
            continue
        if line.startswith("#note\t"):
            _, index, text = (line.split("\t", 2) + [""])[:3]
            notes.setdefault(index, set()).add(text)
            continue
        parts = line.split("\t")
        if len(parts) != 5:
            raise ValueError(f"{path}:{number}: expected 5 tab-separated fields, got {len(parts)}")
        index, form, doc_id, op, body = parts
        by_index[index].append((form, doc_id, op, body))
    return by_index


def digest(lines: list[tuple[str, str, str, str]]) -> str:
    text = "\n".join("\t".join(line) for line in sorted(lines))
    return hashlib.sha256(text.encode()).hexdigest()[:12]


def disagreeing(lines: list[tuple[str, str, str, str]]) -> list[str]:
    """Ids two forms built differently on one side."""
    built: dict[str, set[tuple[str, str]]] = collections.defaultdict(set)
    for _, doc_id, op, body in lines:
        built[doc_id].add((op, body))
    return sorted(doc_id for doc_id, outcomes in built.items() if len(outcomes) > 1)


def pretty(op: str, body: str) -> list[str]:
    if op != "upsert":
        return [f"{op} {body}".rstrip()]
    try:
        return json.dumps(json.loads(body), indent=2, sort_keys=True).splitlines()
    except ValueError:
        return [body]


def explain(index: str, base: list, head: list) -> list[str]:
    keyed_base = {(form, doc_id): (op, body) for form, doc_id, op, body in base}
    keyed_head = {(form, doc_id): (op, body) for form, doc_id, op, body in head}
    keys = sorted(k for k in keyed_base.keys() | keyed_head.keys() if keyed_base.get(k) != keyed_head.get(k))
    out = [f"`{index}`: {len(keys)} documents differ; the first {min(SHOWN, len(keys))}:", ""]
    for form, doc_id in keys[:SHOWN]:
        a, b = keyed_base.get((form, doc_id)), keyed_head.get((form, doc_id))
        if a and b and a[0] == b[0]:
            out.append(f"- `{doc_id}` ({form}): both {a[0]}, the bodies differ")
        else:
            out.append(f"- `{doc_id}` ({form}): base {describe(a)}, head {describe(b)}")
    if keys:
        form, doc_id = keys[0]
        a, b = keyed_base.get(keys[0]), keyed_head.get(keys[0])
        diff = difflib.unified_diff(
            pretty(*a) if a else [],
            pretty(*b) if b else [],
            f"base {doc_id} ({form})",
            f"head {doc_id} ({form})",
            lineterm="",
        )
        out += ["", "```diff", *diff, "```"]
    return out


def describe(outcome: tuple[str, str] | None) -> str:
    return "nothing" if outcome is None else outcome[0]


def compare(base: dict, head: dict, notes: dict[str, set[str]] | None = None) -> tuple[list[str], bool]:
    rows = [
        "| index | documents, base | documents, head | base | head | verdict |",
        "| --- | ---: | ---: | --- | --- | --- |",
    ]
    details: list[str] = []
    flagged = False
    for index in sorted(base.keys() | head.keys()):
        b, h = base.get(index), head.get(index)
        if b is None or h is None:
            flagged = True
            rows.append(
                f"| `{index}` | {len(b or [])} | {len(h or [])} | {digest(b) if b else '—'} | {digest(h) if h else '—'} | one side only ⚠️ |"
            )
            continue
        verdicts = []
        if not b and not h:
            verdicts.append("no documents ⚠️")
        elif digest(b) != digest(h):
            verdicts.append("differ ⚠️")
            details += [""] + explain(index, b, h)
        for side, lines in (("base", b), ("head", h)):
            ids = disagreeing(lines)
            if ids:
                verdicts.append(f"forms disagree on {side} ⚠️")
                shown = ", ".join(f"`{i}`" for i in ids[:SHOWN])
                details += ["", f"`{index}`, {side}: the forms built {len(ids)} ids differently: {shown}"]
        flagged = flagged or bool(verdicts)
        rows.append(
            f"| `{index}` | {len(b)} | {len(h)} | {digest(b) if b else '—'} | {digest(h) if h else '—'} | {', '.join(verdicts) or 'same'} |"
        )
    for index in sorted(notes or {}):
        for text in sorted(notes[index]):
            details += ["", f"`{index}`: {text}."]
    return rows + details, flagged


def main() -> int:
    if len(sys.argv) != 3:
        print(__doc__, file=sys.stderr)
        return 2
    try:
        notes: dict[str, set[str]] = {}
        base, head = load(pathlib.Path(sys.argv[1]), notes), load(pathlib.Path(sys.argv[2]), notes)
    except (OSError, ValueError) as error:
        print(f"error: {error}", file=sys.stderr)
        return 2
    if not base and not head:
        print("error: neither side built a document (does --filter match an index?)", file=sys.stderr)
        return 2
    lines, flagged = compare(base, head, notes)
    print("\n".join(lines))
    return 1 if flagged else 0


if __name__ == "__main__":
    sys.exit(main())
