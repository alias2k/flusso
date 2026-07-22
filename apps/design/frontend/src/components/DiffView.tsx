import { useRef, useState } from "react";
import { ChevronsUpDown } from "lucide-react";
import { useT } from "../i18n";
import {
  attachWordDiff,
  buildPairs,
  collapse,
  diffLines,
  rowVisible,
  unifyRows,
  type DiffMode,
  type MSeg,
  type Row,
} from "../model/diff";
import { cn } from "@/lib/utils";

/// A git-style diff of one file, as a pane. Line-level add/remove highlighting
/// with old/new line-number gutters, inline word diff for small edits, long
/// unchanged stretches collapsed into expandable gaps, and four layouts:
/// unified, split (old left / new right), or a single side (old / new).

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
    <div className={cn("flex w-full", add && "bg-diff-add-bg", del && "bg-diff-del-bg")}>
      <span
        className={cn(
          GUTTER,
          "w-11",
          add ? "text-transparent" : del ? "text-diff-del-num" : "text-muted-foreground/50",
        )}
      >
        {row.oldNo ?? ""}
      </span>
      <span
        className={cn(
          GUTTER,
          "w-11",
          del ? "text-transparent" : add ? "text-diff-add-num" : "text-muted-foreground/50",
        )}
      >
        {row.newNo ?? ""}
      </span>
      <span
        className={cn(
          "w-4 shrink-0 select-none text-center",
          add ? "text-diff-add-fg" : del ? "text-diff-del-fg" : "text-transparent",
        )}
      >
        {add ? "+" : del ? "-" : " "}
      </span>
      <span className={cn("grow whitespace-pre pr-3", row.type === "eq" ? "text-muted-foreground" : "text-foreground")}>
        <LineText row={row} strong={add ? "bg-diff-add-strong" : del ? "bg-diff-del-strong" : undefined} />
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
            <span key={k} className="rounded-xs bg-diff-del-strong text-diff-del-fg">
              {s.text}
            </span>
          ) : (
            <span key={k} className="rounded-xs bg-diff-add-strong text-diff-add-fg">
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
      className={cn("flex h-6 w-full items-center", changed && (side === "old" ? "bg-diff-del-bg" : "bg-diff-add-bg"))}
    >
      <span
        className={cn(
          GUTTER,
          "w-11",
          changed ? (side === "old" ? "text-diff-del-num" : "text-diff-add-num") : "text-muted-foreground/50",
        )}
      >
        {no ?? ""}
      </span>
      <span
        className={cn(
          "w-4 shrink-0 select-none text-center",
          changed ? (side === "old" ? "text-diff-del-fg" : "text-diff-add-fg") : "text-transparent",
        )}
      >
        {side === "old" ? "-" : "+"}
      </span>
      <span className={cn("grow whitespace-pre pr-3", changed ? "text-foreground" : "text-muted-foreground")}>
        <LineText
          row={row}
          strong={changed ? (side === "old" ? "bg-diff-del-strong" : "bg-diff-add-strong") : undefined}
        />
      </span>
    </div>
  );
}

/// The collapsed-gap expander. Rendered once (unified) or in both split columns
/// at the same position, so the two sides stay aligned; either click expands.
function GapBar({ count, onExpand }: { count: number; onExpand: () => void }) {
  const { t } = useT();
  return (
    <button
      type="button"
      onClick={onExpand}
      className="flex h-6 w-full cursor-pointer items-center gap-2 bg-accent/40 px-3 text-2xs text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
    >
      <ChevronsUpDown className="size-3 shrink-0" />
      {t("diff.unchanged", { n: count })}
    </button>
  );
}

/// One file's diff, filling its container as a pane (path + counts header, then a
/// scrollable body). Layout is chosen by `mode`.
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
  const [expanded, setExpanded] = useState<ReadonlySet<number>>(new Set());
  const expand = (id: number) => setExpanded((s) => new Set(s).add(id));
  // Split view: the two halves are separate scrollers (each 50%, its own
  // horizontal scrollbar pinned to the pane bottom); keep their vertical scroll
  // in lock-step so rows stay aligned. Equal-value guard breaks the feedback loop.
  const leftRef = useRef<HTMLDivElement>(null);
  const rightRef = useRef<HTMLDivElement>(null);
  const syncScroll = (from: HTMLDivElement, to: HTMLDivElement | null) => {
    if (to && to.scrollTop !== from.scrollTop) to.scrollTop = from.scrollTop;
  };
  const rows = diffLines(current, next);
  const pairs = buildPairs(rows);
  attachWordDiff(pairs);
  const adds = rows.filter((r) => r.type === "add").length;
  const dels = rows.filter((r) => r.type === "del").length;

  // Unified: small 1:1 edits merge into one inline row; everything else stays a
  // -/+ pair. (Only in "unified" mode — see singleSide for old/new.)
  const unifiedBody = () =>
    collapse(unifyRows(rows), (u) => u.change).map((block) => {
      if (block.kind === "gap" && !expanded.has(block.id))
        return <GapBar key={block.id} count={block.items.length} onExpand={() => expand(block.id)} />;
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
        return <GapBar key={block.id} count={block.items.length} onExpand={() => expand(block.id)} />;
      return block.items
        .filter((r) => rowVisible(r.type, mode))
        .map((r, k) => <DiffRow key={`${block.id}-${k}`} row={r} />);
    });

  // Split: two independently-scrolling columns fed the SAME block sequence, so
  // matching row heights keep old (left) and new (right) aligned.
  const splitColumn = (side: "old" | "new") =>
    collapse(pairs, (p) => p.changed).map((block) => {
      if (block.kind === "gap" && !expanded.has(block.id))
        return <GapBar key={block.id} count={block.items.length} onExpand={() => expand(block.id)} />;
      return block.items.map((p, k) => (
        <SideCell key={`${block.id}-${k}`} row={side === "old" ? p.left : p.right} side={side} />
      ));
    });

  return (
    <div className="diff-file flex h-full min-h-0 flex-col">
      <div className="flex shrink-0 items-center gap-2 border-b border-border bg-secondary px-3 py-2">
        <span className="truncate font-mono text-xs font-medium text-foreground">{path}</span>
        {current === "" && <span className="badge object">{t("diff.newFile")}</span>}
        <span className="ml-auto flex shrink-0 items-center gap-2 font-mono text-2xs tabular-nums">
          <span className="text-diff-add-num">+{adds}</span>
          <span className="text-diff-del-num">-{dels}</span>
        </span>
      </div>
      {/* Each scroller owns both axes, so its horizontal scrollbar is pinned to
          the pane's bottom edge (visible without scrolling to the end of the
          file). Split is two fixed 50% halves with synced vertical scroll. */}
      {mode === "split" ? (
        <div className="grid min-h-0 flex-1 grid-cols-2 divide-x divide-border overflow-hidden font-mono text-xs leading-relaxed">
          <div ref={leftRef} onScroll={(e) => syncScroll(e.currentTarget, rightRef.current)} className="overflow-auto">
            <div className="w-max min-w-full">{splitColumn("old")}</div>
          </div>
          <div ref={rightRef} onScroll={(e) => syncScroll(e.currentTarget, leftRef.current)} className="overflow-auto">
            <div className="w-max min-w-full">{splitColumn("new")}</div>
          </div>
        </div>
      ) : (
        <div className="min-h-0 flex-1 overflow-auto">
          <div className="w-max min-w-full font-mono text-xs leading-relaxed">
            {mode === "unified" ? unifiedBody() : singleSide()}
          </div>
        </div>
      )}
    </div>
  );
}
