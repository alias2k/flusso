// Values typed into the inspector that haven't reached the model yet: each box
// waits for a pause in typing, then has the backend read the text (`api.value`).
// Anything that reads or replaces the model as a whole settles them first:
//
// - `flushPendingValues` before Save, Code mode, undo and redo: every waiting
//   text is read now, and the promise resolves once each has reached the model
//   (or been refused, which leaves the previous value);
// - `dropPendingValues` on Reset, which discards unsaved edits.
//
// A box leaving the screen (blur, another field selected) flushes its own.

interface Pending {
  flush: () => Promise<void>;
  drop: () => void;
}

const pending = new Map<object, Pending>();

/// Register (or replace) the waiting value of the box `owner`.
export function trackPendingValue(owner: object, entry: Pending): void {
  pending.set(owner, entry);
}

/// The box `owner` has nothing waiting any more.
export function settlePendingValue(owner: object): void {
  pending.delete(owner);
}

/// Read every waiting value now; resolves once each reached the model or was refused.
export async function flushPendingValues(): Promise<void> {
  await Promise.all([...pending.values()].map((p) => p.flush()));
}

/// Forget every waiting value; each box shows the model's value again.
export function dropPendingValues(): void {
  for (const p of [...pending.values()]) p.drop();
}
