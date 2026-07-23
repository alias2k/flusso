// Pure line/word diff logic for the save-review diff. Kept JSX-free and separate
// from the components so it can be shared (e.g. the file list's stats) without
// tripping react-refresh's component-only-export rule.

// A stretch of a line, flagged as changed or not by the intra-line word diff.
export interface Seg {
  text: string;
  changed: boolean;
}

// A stretch of the merged (inline) word diff: kept, removed, or added.
export interface MSeg {
  text: string;
  kind: "eq" | "del" | "add";
}

export interface Row {
  type: "eq" | "add" | "del";
  text: string;
  oldNo?: number;
  newNo?: number;
  // Attached for a paired add/del so the exact changed tokens are highlighted.
  seg?: Seg[];
}

/// Which layout to render, chosen from the review's view toggle.
export type DiffMode = "unified" | "split" | "old" | "new";

// A split-view row: the old line on the left, the new line on the right. A
// change with unequal add/remove counts leaves one side empty.
export interface Pair {
  left?: Row;
  right?: Row;
  changed: boolean;
  // The merged word diff of left↔right, for the unified inline row.
  merged?: MSeg[];
}

export interface Block<T> {
  kind: "rows" | "gap";
  id: number;
  items: T[];
}

// A unified-view render unit: either a plain row, or a small modification merged
// into a single inline row (git --word-diff style).
export interface URow {
  change: boolean;
  row?: Row;
  merged?: MSeg[];
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
export function diffLines(current: string, next: string): Row[] {
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

export function buildPairs(rows: Row[]): Pair[] {
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

// Two lines are a modification (worth token-highlighting) only when they share a
// real majority — otherwise it's a block replacement, shown as solid rows.
function similar(merged: MSeg[]): boolean {
  let eq = 0;
  let total = 0;
  for (const s of merged) {
    total += s.text.length;
    if (s.kind === "eq") eq += s.text.length;
  }
  return total > 0 && eq * 2 >= total;
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

// For each paired modification of SIMILAR lines, attach the word diff: per-side
// segments (both views highlight the changed tokens) plus the merged stream (for
// the unified inline row). Dissimilar pairs (block replacements) get nothing, so
// they render as solid add/remove rows. Mutates the shared Row objects.
export function attachWordDiff(pairs: Pair[]): void {
  for (const p of pairs) {
    if (!p.changed || !p.left || !p.right) continue;
    const merged = tokenDiff(p.left.text, p.right.text);
    if (!merged || !similar(merged)) continue;
    p.merged = merged;
    p.left.seg = sideSegs(merged, "old");
    p.right.seg = sideSegs(merged, "new");
  }
}

// Keep every changed item plus `CONTEXT` unchanged items on each side; group the
// rest into collapsible gaps. Shared by the unified (rows) and split (pairs) views.
export function collapse<T>(items: T[], isChange: (t: T) => boolean): Block<T>[] {
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

// Walk rows in diff order (so a changed block stays all-removes-then-all-adds,
// like git). A run that is exactly one remove + one add and a small tweak merges
// into a single inline row; everything else keeps its -/+ rows.
export function unifyRows(rows: Row[]): URow[] {
  const out: URow[] = [];
  let k = 0;
  while (k < rows.length) {
    if (rows[k].type === "eq") {
      out.push({ change: false, row: rows[k] });
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
    const lone = dels.length === 1 && adds.length === 1 ? tokenDiff(dels[0].text, adds[0].text) : null;
    if (lone && inlineable(lone)) {
      out.push({ change: true, merged: lone, oldNo: dels[0].oldNo, newNo: adds[0].newNo });
      continue;
    }
    for (const d of dels) out.push({ change: true, row: d });
    for (const a of adds) out.push({ change: true, row: a });
  }
  return out;
}

export function rowVisible(type: Row["type"], mode: DiffMode): boolean {
  if (mode === "old") return type !== "add";
  if (mode === "new") return type !== "del";
  return true;
}

/// Add/remove line counts for a file — for the review's file list, without
/// rendering the whole diff.
export function diffStats(current: string, next: string): { adds: number; dels: number } {
  const rows = diffLines(current, next);
  return {
    adds: rows.filter((r) => r.type === "add").length,
    dels: rows.filter((r) => r.type === "del").length,
  };
}
