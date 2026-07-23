// Frecency: rank search results by how *often* and how *recently* you pick them.
// A pick bumps a per-record counter + a timestamp in localStorage; the score is
// frequency weighted by a recency decay. Callers use it to order the empty
// palette and to break ties between equally-fuzzy matches.
//
// The clock is read here (not passed in) so the React-side callers stay pure.

const KEY = "flusso-design.search.frecency";
const CAP = 200; // keep the store bounded — evict the least-frecent beyond this

interface Entry {
  n: number;
  last: number;
}
type Store = Record<string, Entry>;

function read(): Store {
  try {
    return JSON.parse(localStorage.getItem(KEY) ?? "{}") as Store;
  } catch {
    return {};
  }
}

/// Recency multiplier for an age in ms — recent picks count for much more.
function recency(ageMs: number): number {
  const hours = ageMs / 3_600_000;
  if (hours < 1) return 4;
  if (hours < 24) return 2;
  if (hours < 24 * 7) return 1;
  return 0.4;
}

function scoresAt(store: Store, now: number): Record<string, number> {
  const out: Record<string, number> = {};
  for (const [id, e] of Object.entries(store)) out[id] = e.n * recency(now - e.last);
  return out;
}

/// Current frecency score per record id (frequency × recency decay).
export function frecencyScores(): Record<string, number> {
  return scoresAt(read(), Date.now());
}

/// Record that `id` was picked (bumps its count + recency), evicting the
/// least-frecent entries if the store grows past the cap.
export function recordPick(id: string): void {
  const now = Date.now();
  const store = read();
  const prev = store[id] ?? { n: 0, last: 0 };
  store[id] = { n: prev.n + 1, last: now };

  const ids = Object.keys(store);
  if (ids.length > CAP) {
    const scores = scoresAt(store, now);
    ids
      .sort((a, b) => (scores[a] ?? 0) - (scores[b] ?? 0))
      .slice(0, ids.length - CAP)
      .forEach((k) => delete store[k]);
  }

  try {
    localStorage.setItem(KEY, JSON.stringify(store));
  } catch {
    /* storage disabled — frecency just won't persist */
  }
}
