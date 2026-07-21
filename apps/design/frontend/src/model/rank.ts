// Query ranking for the command palette — a small Spotlight/Raycast-style
// matcher, not a generic fuzzy blob. The query is split into tokens and **every**
// token must match the record somewhere (AND), so "items users" keeps only
// records that are about `items` *and* `users` — the `items` field scoped to the
// `users` index, not the `users` index itself. Each token scores against the
// title (heavily), then the scope subtitle, then the keywords, through
// prefix → word-boundary → substring → subsequence tiers, so an exact/prefix
// title hit always outranks a buried match.

import type { SearchRecord } from "./search";

/// Split into lowercased word starts: separators *and* camelCase humps, so
/// `order_items` and `avatarUrl` both expose their inner words for prefix hits.
function words(text: string): string[] {
  return text
    .replace(/([a-z0-9])([A-Z])/g, "$1 $2")
    .toLowerCase()
    .split(/[^a-z0-9]+/)
    .filter(Boolean);
}

/// A subsequence match (all of `tok`'s chars appear in order), scored by how
/// contiguous the run is. 0 when `tok` isn't a subsequence of `hay`.
function subsequence(tok: string, hay: string): number {
  let ti = 0;
  let streak = 0;
  let bonus = 0;
  for (let hi = 0; hi < hay.length && ti < tok.length; hi += 1) {
    if (hay[hi] === tok[ti]) {
      ti += 1;
      streak += 1;
      bonus += streak;
    } else {
      streak = 0;
    }
  }
  if (ti < tok.length) return 0;
  return Math.min(0.6, 0.28 + bonus / (hay.length * 4));
}

/// Score one token against one haystack string, 0..1.
function tokenScore(tok: string, hayLower: string, hayWords: string[]): number {
  if (hayLower === tok) return 1;
  if (hayLower.startsWith(tok)) return 0.93 - Math.min(0.12, (hayLower.length - tok.length) * 0.004);
  if (hayWords.some((w) => w.startsWith(tok))) return 0.85;
  const idx = hayLower.indexOf(tok);
  if (idx >= 0) return 0.72 - Math.min(0.22, idx * 0.02);
  return subsequence(tok, hayLower);
}

/// Score a record against all query tokens; null if any token is unmatched.
function recordScore(rec: SearchRecord, tokens: string[]): number | null {
  const fields = [
    { lower: rec.title.toLowerCase(), words: words(rec.title), weight: 1, title: true },
    { lower: (rec.subtitle ?? "").toLowerCase(), words: words(rec.subtitle ?? ""), weight: 0.55, title: false },
    { lower: rec.keywords.toLowerCase(), words: words(rec.keywords), weight: 0.42, title: false },
  ];

  let total = 0;
  let titleHit = false;
  for (const tok of tokens) {
    let best = 0;
    let bestOnTitle = false;
    for (const f of fields) {
      if (!f.lower) continue;
      const raw = tokenScore(tok, f.lower, f.words);
      const weighted = raw * f.weight;
      if (weighted > best) {
        best = weighted;
        bestOnTitle = f.title && raw > 0;
      }
    }
    if (best <= 0) return null; // AND: an unmatched token drops the record
    total += best;
    if (bestOnTitle) titleHit = true;
  }

  return total / tokens.length + (titleHit ? 0.18 : 0);
}

/// Rank records for `query`, dropping non-matches. `onScreen` records (the
/// active index, or settings while the config panel is open) get a score boost.
export function rankRecords(
  records: SearchRecord[],
  query: string,
  onScreen: (r: SearchRecord) => boolean,
): SearchRecord[] {
  const tokens = query.toLowerCase().split(/\s+/).filter(Boolean);
  if (!tokens.length) return records;
  return records
    .map((r) => ({ r, s: recordScore(r, tokens) }))
    .filter((x): x is { r: SearchRecord; s: number } => x.s !== null)
    .map((x) => ({ r: x.r, s: x.s * (onScreen(x.r) ? 1.4 : 1) }))
    .sort((a, b) => b.s - a.s)
    .map((x) => x.r);
}
