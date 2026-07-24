// Recent picks: the last few records you actually ran/opened from the palette
// (distinct from frecency, which tracks pick *counts* for ranking). Shown on the
// empty palette so you can repeat a recent action in one keystroke — it stores
// what you picked, not what you typed to find it.

const KEY = "flusso-design.search.recent";
const CAP = 3;

/// A remembered pick: the record's id (to re-run it) plus its title (to show
/// even before the records are rebuilt).
export interface RecentPick {
  id: string;
  title: string;
}

export function recentPicks(): RecentPick[] {
  try {
    const parsed = JSON.parse(localStorage.getItem(KEY) ?? "[]") as RecentPick[];
    return parsed.filter((p) => p && typeof p.id === "string" && typeof p.title === "string").slice(0, CAP);
  } catch {
    return [];
  }
}

/// Push a pick to the front (deduped by id), capped at the latest few.
export function recordPickRecent(pick: RecentPick): void {
  if (!pick.id) return;
  const next = [pick, ...recentPicks().filter((x) => x.id !== pick.id)].slice(0, CAP);
  try {
    localStorage.setItem(KEY, JSON.stringify(next));
  } catch {
    /* storage disabled — recents just won't persist */
  }
}

/// Drop one pick from the recents by id.
export function removeRecent(id: string): void {
  const next = recentPicks().filter((p) => p.id !== id);
  try {
    localStorage.setItem(KEY, JSON.stringify(next));
  } catch {
    /* storage disabled */
  }
}

export function clearRecent(): void {
  try {
    localStorage.removeItem(KEY);
  } catch {
    /* storage disabled */
  }
}
