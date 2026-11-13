// Values typed into the inspector that haven't reached the model yet: each box
// waits for a pause in typing, then has the backend read the text (`api.value`).
// A reply is applied to the field the text was typed for (its uid), never to
// whatever the box shows by then. Anything that reads or replaces the model as
// a whole settles them first:
//
// - `flushPendingValues` before Save, Code mode, undo and redo: every waiting
//   text is read now. It resolves once each has reached the model or been
//   refused (which leaves the previous value), or after `limitMs`, with the
//   labels of the ones still not in: a read that failed (kept to be retried) or
//   one with no answer yet. Save refuses to write while any is left.
// - `dropPendingValues` on Reset, which discards unsaved edits.
//
// A box leaving the screen (blur, another field selected) flushes its own.
// One target holds at most one waiting value: a newer one drops the older.

interface Pending {
  /// What the value is for (index, field uid, slot); one entry per target.
  target: string;
  /// The target as the user reads it (`users.total.options.boost`).
  label: string;
  flush: () => Promise<void>;
  drop: () => void;
}

const pending = new Map<object, Pending>();
const listeners = new Set<() => void>();

const notify = () => {
  for (const listener of listeners) listener();
};

/// Register (or replace) the waiting value of the box `owner`.
export function trackPendingValue(owner: object, entry: Pending): void {
  for (const [other, p] of pending) if (other !== owner && p.target === entry.target) p.drop();
  const added = !pending.has(owner);
  pending.set(owner, entry);
  if (added) notify();
}

/// The box `owner` has nothing waiting any more.
export function settlePendingValue(owner: object): void {
  if (pending.delete(owner)) notify();
}

/// How long Save waits for values still being read.
export const PENDING_LIMIT_MS = 4000;

/// Read every waiting value now. Resolves once each reached the model or was
/// refused, or after `limitMs`, with the labels of those still not in.
export async function flushPendingValues(limitMs = PENDING_LIMIT_MS): Promise<string[]> {
  let timer: ReturnType<typeof setTimeout> | undefined;
  const late = new Promise<void>((resolve) => {
    timer = setTimeout(resolve, limitMs);
  });
  await Promise.race([Promise.all([...pending.values()].map((p) => p.flush())), late]);
  clearTimeout(timer);
  return [...new Set([...pending.values()].map((p) => p.label))];
}

/// Forget every waiting value; each box shows the model's value again.
export function dropPendingValues(): void {
  for (const p of [...pending.values()]) p.drop();
}

/// Whether any value is waiting (for `useSyncExternalStore`).
export const hasPendingValues = (): boolean => pending.size > 0;

/// Call `listener` whenever a value starts or stops waiting.
export function subscribePendingValues(listener: () => void): () => void {
  listeners.add(listener);
  return () => listeners.delete(listener);
}
