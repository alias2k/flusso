import { useState } from "react";
import { ChevronDown, ChevronsUpDown } from "lucide-react";
import { useT } from "../i18n";
import { cn } from "@/lib/utils";

/// A git-style diff of one file. Line-level add/remove highlighting with old/new
/// line-number gutters, long unchanged stretches collapsed into expandable gaps,
/// and four layouts: unified (both sides), split (old left / new right), or a
/// single side (old / new) — so a save review reads like a code review.

// A stretch of a line, flagged as changed or not by the intra-line word diff.
interface Seg {
  text: string;
  changed: boolean;
}

// A stretch of the merged (inline) word diff: kept, removed, or added.
interface MSeg {
  text: string;
  kind: "eq" | "del" | "add";
}

interface Row {
  type: "eq" | "add" | "del";
  text: string;
  oldNo?: number;
  newNo?: number;
  // Attached for a paired add/del so the exact changed tokens are highlighted.
  seg?: Seg[];
}

/// Which layout to render, chosen from the review's view toggle.
export type DiffMode = "unified" | "split" | "old" | "new";

// Lines around a change kept as context; longer unchanged runs collapse.
const CONTEXT = 3;

function splitLines(text: string): string[] {
  if (text === "") return [];
  return text.replace(/\n$/, "").split("\n");
}

/// Line-level LCS diff. Files here are small (config/schema), so the O(n·m)
/// table is cheap and keeps the alignment optimal (fewest add/remove rows).
function diffLines(current: string, next: string): Row[] {
  const a = splitLines(current);
  const b = splitLines(next);
  const n = a.length;
  const m = b.length;
  const dp: number[][] = Array.from({ length: n + 1 }, () => new Array<number>(m + 1).fill(0));
  for (let i = n - 1; i >= 0; i--) {
    for (let j = m - 1; j >= 0; j--) {
      dp[i][j] = a[i] === b[j] ? dp[i + 1][j + 1] + 1 : Math.max(dp[i + 1][j], dp[i][j + 1]);
    }
  }
  const rows: Row[] = [];
  let i = 0;
  let j = 0;
  let oldNo = 1;
  let newNo = 1;
  while (i < n && j < m) {
    if (a[i] === b[j]) {
      rows.push({ type: "eq", text: a[i], oldNo, newNo });
      oldNo++;
      newNo++;
      i++;
      j++;
    } else if (dp[i + 1][j] >= dp[i][j + 1]) {
      rows.push({ type: "del", text: a[i], oldNo });
      oldNo++;
      i++;
    } else {
      rows.push({ type: "add", text: b[j], newNo });
      newNo++;
      j++;
    }
  }
  while (i < n) {
    rows.push({ type: "del", text: a[i], oldNo });
    oldNo++;
    i++;
  }
  while (j < m) {
    rows.push({ type: "add", text: b[j], newNo });
    newNo++;
    j++;
  }
  return rows;
}

// A split-view row: the old line on the left, the new line on the right. A
// change with unequal add/remove counts leaves one side empty.
interface Pair {
  left?: Row;
  right?: Row;
  changed: boolean;
  // The merged word diff of left↔right, for the unified inline row.
  merged?: MSeg[];
}

function buildPairs(rows: Row[]): Pair[] {
  const pairs: Pair[] = [];
  let k = 0;
  while (k < rows.length) {
    if (rows[k].type === "eq") {
      pairs.push({ left: rows[k], right: rows[k], changed: false });
      k++;
      continue;
    }
    const dels: Row[] = [];
    const adds: Row[] = [];
    while (k < rows.length && rows[k].type !== "eq") {
      if (rows[k].type === "del") dels.push(rows[k]);
      else adds.push(rows[k]);
      k++;
    }
    const max = Math.max(dels.length, adds.length);
    for (let x = 0; x < max; x++) {
      pairs.push({
        left: x < dels.length ? dels[x] : undefined,
        right: x < adds.length ? adds[x] : undefined,
        changed: true,
      });
    }
  }
  return pairs;
}

// Word-ish tokens: whitespace runs, identifier runs, punctuation runs — enough
// granularity to highlight the exact bit of a line that changed.
function tokenize(s: string): string[] {
  return s.match(/\s+|[A-Za-z0-9_]+|[^\sA-Za-z0-9_]+/g) ?? [];
}

/// Token-level LCS diff of two lines as one merged stream (eq/del/add), or null
/// when the lines share no tokens (a full rewrite — highlight the whole row
/// instead of a noisy every-token flag).
function tokenDiff(oldText: string, newText: string): MSeg[] | null {
  const a = tokenize(oldText);
  const b = tokenize(newText);
  const n = a.length;
  const m = b.length;
  const dp: number[][] = Array.from({ length: n + 1 }, () => new Array<number>(m + 1).fill(0));
  for (let i = n - 1; i >= 0; i--) {
    for (let j = m - 1; j >= 0; j--) {
      dp[i][j] = a[i] === b[j] ? dp[i + 1][j + 1] + 1 : Math.max(dp[i + 1][j], dp[i][j + 1]);
    }
  }
  if (dp[0][0] === 0) return null;
  const merged: MSeg[] = [];
  const push = (text: string, kind: MSeg["kind"]) => {
    const last = merged[merged.length - 1];
    if (last?.kind === kind) last.text += text;
    else merged.push({ text, kind });
  };
  let i = 0;
  let j = 0;
  while (i < n && j < m) {
    if (a[i] === b[j]) {
      push(a[i], "eq");
      i++;
      j++;
    } else if (dp[i + 1][j] >= dp[i][j + 1]) {
      push(a[i], "del");
      i++;
    } else {
      push(b[j], "add");
      j++;
    }
  }
  while (i < n) {
    push(a[i], "del");
    i++;
  }
  while (j < m) {
    push(b[j], "add");
    j++;
  }
  return merged;
}

// One side of a merged diff: old keeps eq+del, new keeps eq+add.
function sideSegs(merged: MSeg[], side: "old" | "new"): Seg[] {
  const out: Seg[] = [];
  const push = (text: string, changed: boolean) => {
    const last = out[out.length - 1];
    if (last?.changed === changed) last.text += text;
    else out.push({ text, changed });
  };
  for (const s of merged) {
    if (side === "old" && s.kind === "add") continue;
    if (side === "new" && s.kind === "del") continue;
    push(s.text, s.kind !== "eq");
  }
  return out;
}

// Small enough to merge into one inline row (git --word-diff style): at most a
// couple of changed runs, changing a minority of the line — otherwise it stays
// a -/+ pair, which reads clearer for large edits.
function inlineable(merged: MSeg[]): boolean {
  const changed = merged.filter((s) => s.kind !== "eq");
  if (changed.length === 0 || changed.length > 2) return false;
  const changedChars = changed.reduce((n, s) => n + s.text.length, 0);
  const total = merged.reduce((n, s) => n + s.text.length, 0);
  return changedChars * 3 < total;
}

// For each paired modification, attach the word diff: per-side segments (both
// views highlight the changed tokens) plus the merged stream (for the unified
// inline row). Mutates the shared Row objects.
function attachWordDiff(pairs: Pair[]): void {
  for (const p of pairs) {
    if (!p.changed || !p.left || !p.right) continue;
    const merged = tokenDiff(p.left.text, p.right.text);
    if (!merged) continue;
    p.merged = merged;
    p.left.seg = sideSegs(merged, "old");
    p.right.seg = sideSegs(merged, "new");
  }
}

interface Block<T> {
  kind: "rows" | "gap";
  id: number;
  items: T[];
}

// Keep every changed item plus `CONTEXT` unchanged items on each side; group the
// rest into collapsible gaps. Shared by the unified (rows) and split (pairs) views.
function collapse<T>(items: T[], isChange: (t: T) => boolean): Block<T>[] {
  const keep = new Array<boolean>(items.length).fill(false);
  items.forEach((t, idx) => {
    if (!isChange(t)) return;
    for (let k = Math.max(0, idx - CONTEXT); k <= Math.min(items.length - 1, idx + CONTEXT); k++) keep[k] = true;
  });
  const out: Block<T>[] = [];
  let idx = 0;
  while (idx < items.length) {
    const start = idx;
    const visible = keep[idx];
    while (idx < items.length && keep[idx] === visible) idx++;
    out.push({ kind: visible ? "rows" : "gap", id: start, items: items.slice(start, idx) });
  }
  return out;
}

// A unified-view render unit: either a plain row, or a small modification merged
// into a single inline row (git --word-diff style).
interface URow {
  change: boolean;
  row?: Row;
  merged?: MSeg[];
  oldNo?: number;
  newNo?: number;
}

function unifyPairs(pairs: Pair[]): URow[] {
  const out: URow[] = [];
  for (const p of pairs) {
    if (!p.changed) {
      if (p.left) out.push({ change: false, row: p.left });
    } else if (p.left && p.right && p.merged && inlineable(p.merged)) {
      out.push({ change: true, merged: p.merged, oldNo: p.left.oldNo, newNo: p.right.newNo });
    } else {
      if (p.left) out.push({ change: true, row: p.left });
      if (p.right) out.push({ change: true, row: p.right });
    }
  }
  return out;
}

const GUTTER = "shrink-0 select-none px-2 text-right text-2xs tabular-nums";

/// A line's text, with the word-diff's changed tokens given a stronger tint
/// (`strong`) over the row's base colour. Falls back to plain text when the row
/// carries no word diff (unpaired change or full rewrite).
function LineText({ row, strong }: { row: Row; strong?: string }) {
  if (!row.seg) return <>{row.text || " "}</>;
  return (
    <>
      {row.seg.map((s, k) =>
        s.changed && strong ? (
          <span key={k} className={cn("rounded-xs", strong)}>
            {s.text}
          </span>
        ) : (
          <span key={k}>{s.text}</span>
        ),
      )}
    </>
  );
}

/// One line in the unified view: two gutters (old + new), a +/- sign, the text.
function DiffRow({ row }: { row: Row }) {
  const add = row.type === "add";
  const del = row.type === "del";
  return (
    <div className={cn("flex w-full", add && "bg-primary/12", del && "bg-destructive/12")}>
      <span
        className={cn(
          GUTTER,
          "w-11",
          add ? "text-transparent" : del ? "text-destructive/70" : "text-muted-foreground/50",
        )}
      >
        {row.oldNo ?? ""}
      </span>
      <span
        className={cn(GUTTER, "w-11", del ? "text-transparent" : add ? "text-primary/70" : "text-muted-foreground/50")}
      >
        {row.newNo ?? ""}
      </span>
      <span
        className={cn(
          "w-4 shrink-0 select-none text-center",
          add ? "text-primary" : del ? "text-destructive" : "text-transparent",
        )}
      >
        {add ? "+" : del ? "-" : " "}
      </span>
      <span className={cn("grow whitespace-pre pr-3", row.type === "eq" ? "text-muted-foreground" : "text-foreground")}>
        <LineText row={row} strong={add ? "bg-primary/30" : del ? "bg-destructive/30" : undefined} />
      </span>
    </div>
  );
}

/// A small modification merged onto one line: kept text plain, removed tokens
/// red, added tokens green. Both line numbers shown, marked with `~`.
function InlineRow({ merged, oldNo, newNo }: { merged: MSeg[]; oldNo?: number; newNo?: number }) {
  return (
    <div className="flex w-full">
      <span className={cn(GUTTER, "w-11 text-muted-foreground/50")}>{oldNo ?? ""}</span>
      <span className={cn(GUTTER, "w-11 text-muted-foreground/50")}>{newNo ?? ""}</span>
      <span className="w-4 shrink-0 select-none text-center text-muted-foreground">~</span>
      <span className="grow whitespace-pre pr-3 text-foreground">
        {merged.map((s, k) =>
          s.kind === "eq" ? (
            <span key={k}>{s.text}</span>
          ) : s.kind === "del" ? (
            <span key={k} className="rounded-xs bg-destructive/25 text-destructive">
              {s.text}
            </span>
          ) : (
            <span key={k} className="rounded-xs bg-primary/25 text-primary">
              {s.text}
            </span>
          ),
        )}
      </span>
    </div>
  );
}

/// One side of a split row. A missing line renders as a muted placeholder so the
/// two columns stay row-aligned.
function SideCell({ row, side }: { row?: Row; side: "old" | "new" }) {
  if (!row) return <div className="h-6 bg-muted/20" />;
  const changed = side === "old" ? row.type === "del" : row.type === "add";
  const no = side === "old" ? row.oldNo : row.newNo;
  return (
    <div
      className={cn(
        "flex h-6 w-full items-center",
        changed && (side === "old" ? "bg-destructive/12" : "bg-primary/12"),
      )}
    >
      <span
        className={cn(
          GUTTER,
          "w-11",
          changed ? (side === "old" ? "text-destructive/70" : "text-primary/70") : "text-muted-foreground/50",
        )}
      >
        {no ?? ""}
      </span>
      <span
        className={cn(
          "w-4 shrink-0 select-none text-center",
          changed ? (side === "old" ? "text-destructive" : "text-primary") : "text-transparent",
        )}
      >
        {side === "old" ? "-" : "+"}
      </span>
      <span className={cn("grow whitespace-pre pr-3", changed ? "text-foreground" : "text-muted-foreground")}>
        <LineText row={row} strong={changed ? (side === "old" ? "bg-destructive/30" : "bg-primary/30") : undefined} />
      </span>
    </div>
  );
}

/// The collapsed-gap expander. Rendered once (unified) or in both split columns
/// at the same position, so the two sides stay aligned; either click expands.
function GapBar({ count, label, onExpand }: { count: number; label?: boolean; onExpand: () => void }) {
  const { t } = useT();
  return (
    <button
      type="button"
      onClick={onExpand}
      className="flex h-6 w-full items-center gap-2 bg-accent/40 px-3 text-2xs text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
    >
      <ChevronsUpDown className="size-3 shrink-0" />
      {label && t("diff.unchanged", { n: count })}
    </button>
  );
}

function rowVisible(type: Row["type"], mode: DiffMode): boolean {
  if (mode === "old") return type !== "add";
  if (mode === "new") return type !== "del";
  return true;
}

export function DiffView({
  path,
  current,
  next,
  mode,
}: {
  path: string;
  current: string;
  next: string;
  mode: DiffMode;
}) {
  const { t } = useT();
  const [open, setOpen] = useState(true);
  const [expanded, setExpanded] = useState<ReadonlySet<number>>(new Set());
  const expand = (id: number) => setExpanded((s) => new Set(s).add(id));
  const rows = diffLines(current, next);
  const pairs = buildPairs(rows);
  attachWordDiff(pairs);
  const adds = rows.filter((r) => r.type === "add").length;
  const dels = rows.filter((r) => r.type === "del").length;

  // Unified: small 1:1 edits merge into one inline row; everything else stays a
  // -/+ pair. (Only in "unified" mode — see singleSide for old/new.)
  const unifiedBody = () =>
    collapse(unifyPairs(pairs), (u) => u.change).map((block) => {
      if (block.kind === "gap" && !expanded.has(block.id))
        return <GapBar key={block.id} count={block.items.length} label onExpand={() => expand(block.id)} />;
      return block.items.map((u, k) =>
        u.merged ? (
          <InlineRow key={`${block.id}-${k}`} merged={u.merged} oldNo={u.oldNo} newNo={u.newNo} />
        ) : u.row ? (
          <DiffRow key={`${block.id}-${k}`} row={u.row} />
        ) : null,
      );
    });

  // Old / New: one side, no inline merge — each surviving row keeps its colour.
  const singleSide = () =>
    collapse(rows, (r) => r.type !== "eq").map((block) => {
      if (block.kind === "gap" && !expanded.has(block.id))
        return <GapBar key={block.id} count={block.items.length} label onExpand={() => expand(block.id)} />;
      return block.items
        .filter((r) => rowVisible(r.type, mode))
        .map((r, k) => <DiffRow key={`${block.id}-${k}`} row={r} />);
    });

  // Split: two independently-scrolling columns fed the SAME block sequence, so
  // matching row heights keep old (left) and new (right) aligned.
  const splitColumn = (side: "old" | "new") =>
    collapse(pairs, (p) => p.changed).map((block) => {
      if (block.kind === "gap" && !expanded.has(block.id))
        return (
          <GapBar key={block.id} count={block.items.length} label={side === "old"} onExpand={() => expand(block.id)} />
        );
      return block.items.map((p, k) => (
        <SideCell key={`${block.id}-${k}`} row={side === "old" ? p.left : p.right} side={side} />
      ));
    });

  return (
    <div className="diff-file mb-4 overflow-hidden rounded-lg border border-border shadow-sm last:mb-0">
      <button
        type="button"
        onClick={() => setOpen((o) => !o)}
        aria-expanded={open}
        className={cn(
          "flex w-full items-center gap-2 bg-secondary px-3 py-2 text-left transition-colors hover:bg-accent",
          open && "border-b border-border",
        )}
      >
        <ChevronDown
          className={cn("size-3.5 shrink-0 text-muted-foreground transition-transform", !open && "-rotate-90")}
        />
        <span className="truncate font-mono text-xs font-medium text-foreground">{path}</span>
        {current === "" && <span className="badge object">{t("diff.newFile")}</span>}
        <span className="ml-auto flex shrink-0 items-center gap-2 font-mono text-2xs tabular-nums">
          <span className="text-primary">+{adds}</span>
          <span className="text-destructive">-{dels}</span>
        </span>
      </button>
      {open &&
        (mode === "split" ? (
          <div className="grid grid-cols-2 divide-x divide-border font-mono text-xs leading-relaxed">
            <div className="overflow-x-auto">
              <div className="w-max min-w-full">{splitColumn("old")}</div>
            </div>
            <div className="overflow-x-auto">
              <div className="w-max min-w-full">{splitColumn("new")}</div>
            </div>
          </div>
        ) : (
          <div className="overflow-x-auto">
            <div className="w-max min-w-full font-mono text-xs leading-relaxed">
              {mode === "unified" ? unifiedBody() : singleSide()}
            </div>
          </div>
        ))}
    </div>
  );
}
