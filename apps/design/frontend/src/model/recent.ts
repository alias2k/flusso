// Recent searches: the last few *queries* you ran (distinct from frecency, which
// tracks the *results* you pick). Shown on the empty palette so you can re-run a
// search in one keystroke. A query is only remembered once it leads to a pick,
// so idle typing doesn't pollute the list.

const KEY = "flusso-design.search.recent";
const CAP = 6;

export function recentSearches(): string[] {
  try {
    return JSON.parse(localStorage.getItem(KEY) ?? "[]") as string[];
  } catch {
    return [];
  }
}

/// Push `query` to the front (deduped, case-insensitive), capped.
export function recordSearch(query: string): void {
  const q = query.trim();
  if (!q) return;
  const next = [q, ...recentSearches().filter((x) => x.toLowerCase() !== q.toLowerCase())].slice(0, CAP);
  try {
    localStorage.setItem(KEY, JSON.stringify(next));
  } catch {
    /* storage disabled — recents just won't persist */
  }
}

export function clearRecent(): void {
  try {
    localStorage.removeItem(KEY);
  } catch {
    /* storage disabled */
  }
}
