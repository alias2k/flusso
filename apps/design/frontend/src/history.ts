// A tiny undo/redo history over a single immutable value. Every `set` pushes the
// previous value onto the past and clears the redo stack; `undo`/`redo` walk the
// stacks. `reset` installs a new baseline with empty history (used on load).

import { useCallback, useState } from "react";

interface History<T> {
  past: T[];
  present: T;
  future: T[];
}

export interface UseHistory<T> {
  present: T;
  set: (next: T | ((prev: T) => T)) => void;
  undo: () => void;
  redo: () => void;
  reset: (value: T) => void;
  canUndo: boolean;
  canRedo: boolean;
}

const LIMIT = 200;

export function useHistory<T>(initial: T): UseHistory<T> {
  const [h, setH] = useState<History<T>>({ past: [], present: initial, future: [] });

  const set = useCallback((next: T | ((prev: T) => T)) => {
    setH((h) => {
      const value = typeof next === "function" ? (next as (p: T) => T)(h.present) : next;
      if (value === h.present) return h;
      const past = [...h.past, h.present];
      if (past.length > LIMIT) past.shift();
      return { past, present: value, future: [] };
    });
  }, []);

  const undo = useCallback(() => {
    setH((h) => {
      if (h.past.length === 0) return h;
      const present = h.past[h.past.length - 1];
      return { past: h.past.slice(0, -1), present, future: [h.present, ...h.future] };
    });
  }, []);

  const redo = useCallback(() => {
    setH((h) => {
      if (h.future.length === 0) return h;
      const [present, ...future] = h.future;
      return { past: [...h.past, h.present], present, future };
    });
  }, []);

  const reset = useCallback((value: T) => {
    setH({ past: [], present: value, future: [] });
  }, []);

  return {
    present: h.present,
    set,
    undo,
    redo,
    reset,
    canUndo: h.past.length > 0,
    canRedo: h.future.length > 0,
  };
}
