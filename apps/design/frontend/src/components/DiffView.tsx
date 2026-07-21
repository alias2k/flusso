import { useState } from "react";
import { ChevronDown, ChevronsUpDown } from "lucide-react";
import { useT } from "../i18n";
import { cn } from "@/lib/utils";

/// A git-style diff of one file. Line-level add/remove highlighting with old/new
/// line-number gutters, long unchanged stretches collapsed into expandable gaps,
/// and four layouts: unified (both sides), split (old left / new right), or a
/// single side (old / new) — so a save review reads like a code review.

interface Row {
  type: "eq" | "add" | "del";
  text: string;
  oldNo?: number;
  newNo?: number;
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

const GUTTER = "shrink-0 select-none px-2 text-right text-2xs tabular-nums";

/// One line in the unified view: two gutters (old + new), a +/- sign, the text.
function DiffRow({ row }: { row: Row }) {
  const add = row.type === "add";
  const del = row.type === "del";
  return (
    <div className={cn("flex w-full", add && "bg-primary/12", del && "bg-destructive/12")}>
      <span
        className={cn(GUTTER, "w-11", add ? "text-primary/70" : del ? "text-transparent" : "text-muted-foreground/50")}
      >
        {row.oldNo ?? ""}
      </span>
      <span
        className={cn(
          GUTTER,
          "w-11",
          del ? "text-destructive/70" : add ? "text-transparent" : "text-muted-foreground/50",
        )}
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
        {row.text || " "}
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
        {row.text || " "}
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
  const adds = rows.filter((r) => r.type === "add").length;
  const dels = rows.filter((r) => r.type === "del").length;

  const unifiedBody = () =>
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
    collapse(buildPairs(rows), (p) => p.changed).map((block) => {
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
            <div className="w-max min-w-full font-mono text-xs leading-relaxed">{unifiedBody()}</div>
          </div>
        ))}
    </div>
  );
}
