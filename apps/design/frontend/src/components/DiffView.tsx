import { useState } from "react";
import { ChevronDown, ChevronsUpDown } from "lucide-react";
import { useT } from "../i18n";
import { cn } from "@/lib/utils";

/// A unified, git-style diff of one file: line-level add/remove highlighting,
/// old/new line-number gutters, and long unchanged stretches collapsed into
/// expandable gaps — so a save review reads like a code review, not two dumps.

interface Row {
  type: "eq" | "add" | "del";
  text: string;
  oldNo?: number;
  newNo?: number;
}

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

type Segment = { kind: "rows"; rows: Row[] } | { kind: "gap"; id: number; rows: Row[] };

// Keep every changed row plus `CONTEXT` unchanged rows on each side; group the
// rest into collapsible gaps.
function segment(rows: Row[]): Segment[] {
  const keep = new Array<boolean>(rows.length).fill(false);
  rows.forEach((r, idx) => {
    if (r.type === "eq") return;
    for (let k = Math.max(0, idx - CONTEXT); k <= Math.min(rows.length - 1, idx + CONTEXT); k++) keep[k] = true;
  });
  const out: Segment[] = [];
  let idx = 0;
  while (idx < rows.length) {
    const start = idx;
    const visible = keep[idx];
    while (idx < rows.length && keep[idx] === visible) idx++;
    const slice = rows.slice(start, idx);
    out.push(visible ? { kind: "rows", rows: slice } : { kind: "gap", id: start, rows: slice });
  }
  return out;
}

const GUTTER = "w-11 shrink-0 select-none px-2 text-right text-2xs tabular-nums";

function DiffRow({ row }: { row: Row }) {
  const add = row.type === "add";
  const del = row.type === "del";
  return (
    <div className={cn("flex w-full", add && "bg-primary/12", del && "bg-destructive/12")}>
      <span className={cn(GUTTER, add ? "text-primary/70" : del ? "text-transparent" : "text-muted-foreground/50")}>
        {row.oldNo ?? ""}
      </span>
      <span className={cn(GUTTER, del ? "text-destructive/70" : add ? "text-transparent" : "text-muted-foreground/50")}>
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

/// Which side of the change to show: `unified` keeps both, `old` hides
/// additions (the file as it was), `new` hides removals (the file as it will
/// be) — the surviving rows keep their add/remove colour either way.
export type DiffMode = "unified" | "old" | "new";

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
  const rows = diffLines(current, next);
  const segments = segment(rows);
  const adds = rows.filter((r) => r.type === "add").length;
  const dels = rows.filter((r) => r.type === "del").length;
  const renderRows = (rs: Row[]) =>
    rs.filter((r) => rowVisible(r.type, mode)).map((r, k) => <DiffRow key={`${r.oldNo}-${r.newNo}-${k}`} row={r} />);

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
      {open && (
        <div className="overflow-x-auto">
          <div className="w-max min-w-full font-mono text-xs leading-relaxed">
            {segments.map((seg) => {
              if (seg.kind === "rows") return renderRows(seg.rows);
              if (expanded.has(seg.id)) return renderRows(seg.rows);
              return (
                <button
                  key={seg.id}
                  type="button"
                  onClick={() => setExpanded((s) => new Set(s).add(seg.id))}
                  className="flex w-full items-center gap-2 bg-accent/40 px-3 py-1 text-2xs text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
                >
                  <ChevronsUpDown className="size-3 shrink-0" />
                  {t("diff.unchanged", { n: seg.rows.length })}
                </button>
              );
            })}
          </div>
        </div>
      )}
    </div>
  );
}
