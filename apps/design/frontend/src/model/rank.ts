// Query ranking for the command palette, built on MiniSearch (a small in-memory
// full-text index). The engine does the heavy lifting — BM25 relevance, prefix
// + fuzzy matching, multi-token AND — while we layer on the palette-specific
// bits: a tokenizer that splits camelCase/snake so `createdAt` and `order_items`
// expose their inner words, field boosting (title ≫ scope ≫ keywords), a
// frecency/on-screen weight multiplier, and title highlight positions.
//
// AND semantics keep scoped queries honest: "items users" needs both tokens, so
// it keeps the `items` field (title `items`, scope `users`) and drops the
// `users` index (no `items` anywhere).

import MiniSearch from "minisearch";
import type { SearchRecord } from "./search";

export interface Ranked {
  record: SearchRecord;
  /// Char indices in the title matched by the query (for highlighting).
  positions: number[];
}

/// Split into lowercased word starts: separators *and* camelCase humps, so
/// `avatarUrl` → [avatar, url] and `order_items` → [order, items]. Used for both
/// indexing and query tokenizing.
function tokenize(text: string): string[] {
  return text
    .replace(/([a-z0-9])([A-Z])/g, "$1 $2")
    .toLowerCase()
    .split(/[^a-z0-9]+/)
    .filter(Boolean);
}

/// Build the MiniSearch index over the current records.
export function createSearch(records: SearchRecord[]): MiniSearch<SearchRecord> {
  const ms = new MiniSearch<SearchRecord>({
    idField: "id",
    fields: ["title", "subtitle", "keywords"],
    tokenize,
    searchOptions: {
      boost: { title: 3, subtitle: 1.4, keywords: 0.8 },
      prefix: true,
      fuzzy: 0.2,
      combineWith: "AND",
    },
  });
  ms.addAll(records);
  return ms;
}

/// Char positions in `title` covered by any query token (substring hits) — a
/// best-effort highlight that lines up with what the user typed.
function titlePositions(query: string, title: string): number[] {
  const lower = title.toLowerCase();
  const pos = new Set<number>();
  for (const tok of tokenize(query)) {
    for (let from = lower.indexOf(tok); from >= 0; from = lower.indexOf(tok, from + tok.length)) {
      for (let k = 0; k < tok.length; k += 1) pos.add(from + k);
    }
  }
  return [...pos].sort((a, b) => a - b);
}

/// Rank records for `query`, dropping non-matches. `weight` scales each score
/// (on-screen boost × frecency), then results re-sort by the weighted score.
export function runSearch(
  ms: MiniSearch<SearchRecord>,
  query: string,
  byId: Map<string, SearchRecord>,
  weight: (r: SearchRecord) => number,
): Ranked[] {
  return ms
    .search(query)
    .map((res) => {
      const record = byId.get(String(res.id));
      return record
        ? { record, positions: titlePositions(query, record.title), score: res.score * weight(record) }
        : null;
    })
    .filter((x): x is { record: SearchRecord; positions: number[]; score: number } => x !== null)
    .sort((a, b) => b.score - a.score)
    .map(({ record, positions }) => ({ record, positions }));
}
