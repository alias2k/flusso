// A name typed into a name box (an index, a sink, a field, a canvas title) is
// a draft until the box is left: then it is applied, or dropped when refused.
//
// - The draft is applied through `apply`, which renames by identity (an index
//   row's id, a sink's id, a field's uid) against the state as it is at that
//   moment (`model/renames.ts`), never from a snapshot this box rendered with.
//   So it is safe whenever it runs: on blur, on Enter, on Save, and when the
//   box unmounts while holding a name (selecting another field, say), which
//   applies it like leaving the box does.
// - The draft is published in the UI store with its own `settle` and `drop`:
//   Save settles it; Undo, Redo and Reset drop it. Their buttons keep focus
//   (`keepFocus`), so the draft is still there when they act.
// - Focus moving into the command palette (`data-keeps-name-draft`) leaves the
//   draft pending, for the palette's own actions to settle or drop. Closing
//   the palette puts the focus back in the box (`refocus`), so leaving the box
//   applies the name as usual.
// - Escape and Ctrl+Z (⌘Z) in the box drop it, and keep the focus there (an
//   Escape with nothing typed does what Escape does on the page); Ctrl+Shift+Z
//   (⌘⇧Z) brings back the name Ctrl+Z dropped.
//
// **A name applies when the press that left the box ends**, never inside it
// (`model/press.ts`). Leaving a box with the pointer blurs it at pointerdown;
// applying the name there would re-render the page mid-press, and a control
// that moved or was re-created under the pointer gets no click. So the blur
// waits: the name applies after the click has run its handlers (or when no
// click comes: a drag, a cancelled touch). Every other way out (Tab, Enter, an
// unmount, Save) applies it at once. Consequences, each deliberate:
//
// - The pressed control acts first, on the state with the old name, then the
//   name applies: two undo steps, the action's first. Selecting another field
//   or index, ticking a column, switching to Code mode all read the same either
//   way, since a rename goes by identity.
// - A control that acts on the renamed thing or reads its name settles the
//   draft first (`settleNameDraft`), so it sees the new name: Save, duplicate,
//   remove, the switch to Code mode.
// - A rename chip drops the typed name: the chip is what was pressed.
//
// Layout still never moves when a name applies (reasons float and let the
// pointer through, rename chips keep their places, a node header is one line);
// that matters for the keyboard and for a press that starts after the name
// applied.
//
// ```tsx
// const draft = useNameDraft({ kind: "field", row: field.field, refusal, apply });
// <input value={draft.text ?? field.field} onChange={(e) => draft.change(e.target.value)}
//        onBlur={draft.blur} onKeyDown={draft.keyDown} />
// ```

import {
  useCallback,
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
  type FocusEvent,
  type KeyboardEvent,
  type MouseEvent,
} from "react";
import { createPressTracker } from "../model/press";
import { useUiStore, type NameDraft } from "../store/ui";

/// The page's one press tracker, fed by document-level capture listeners.
const tracker = createPressTracker({
  set: (run, ms) => window.setTimeout(run, ms),
  clear: (handle) => window.clearTimeout(handle as number | undefined),
});
let trackerInstalled = false;

function installPressTracker() {
  if (trackerInstalled) return;
  trackerInstalled = true;
  document.addEventListener("pointerdown", (e) => tracker.dispatch({ type: "down", pointer: e.pointerId }), true);
  document.addEventListener("pointerup", (e) => tracker.dispatch({ type: "up", pointer: e.pointerId }), true);
  document.addEventListener("pointercancel", (e) => tracker.dispatch({ type: "cancel", pointer: e.pointerId }), true);
  document.addEventListener("click", () => tracker.dispatch({ type: "click" }), true);
  window.addEventListener("blur", () => tracker.dispatch({ type: "windowBlur" }));
}

/// Apply the name pending in a name box now, for a control whose action must
/// see it (Save, duplicate, remove, Code mode).
export function settleNameDraft() {
  useUiStore.getState().nameDraft?.settle();
}

/// Focus moving into an element marked `data-keeps-name-draft` (the command
/// palette) leaves the draft pending.
export const keepsNameDraft = (to: EventTarget | null) =>
  to instanceof Element && to.closest("[data-keeps-name-draft]") !== null;

/// `onMouseDown` for a button that acts on a pending name (Undo, Redo,
/// Reset): pressing it keeps the focus, and so the draft, in the box.
export const keepFocus = (e: MouseEvent) => e.preventDefault();

/// The draft state of one name box: `text` is what is typed (`null` when
/// nothing is), `refused` why it can't be applied; `change`/`blur`/`keyDown`
/// go on the input. `refusal` says why a text can't be applied, `apply`
/// applies it by identity and says why it could not, if it could not.
export function useNameDraft({
  kind,
  row,
  refusal,
  apply,
}: {
  kind: NameDraft["kind"];
  row: string;
  refusal: (text: string) => string | null;
  apply: (text: string) => string | null;
}) {
  const [text, setText] = useState<string | null>(null);
  const [late, setLate] = useState<string | null>(null);
  const setNameDraft = useUiStore((s) => s.setNameDraft);
  const callbacks = useRef({ refusal, apply });
  useLayoutEffect(() => {
    callbacks.current = { refusal, apply };
  });
  const typed = useRef<string | null>(null);
  // The name Ctrl+Z dropped, for Ctrl+Shift+Z to bring back.
  const undone = useRef<string | null>(null);
  useEffect(installPressTracker, []);
  const box = useRef<HTMLInputElement | null>(null);
  const refocus = useCallback(() => {
    const input = box.current;
    if (!input?.isConnected) return false;
    input.focus();
    return true;
  }, []);
  const end = useCallback(() => {
    typed.current = null;
    setText(null);
    setLate(null);
    setNameDraft(null);
  }, [setNameDraft]);
  const settle = useCallback(() => {
    const value = typed.current;
    if (value === null) return;
    if (callbacks.current.refusal(value)) {
      end();
      return;
    }
    // Refused only now (another field took the name meanwhile): the box keeps
    // it, marked, and Save stays refused until it is changed or dropped.
    const now = callbacks.current.apply(value);
    if (now === null) end();
    else setLate(now);
  }, [end]);
  const drop = useCallback(() => {
    if (typed.current !== null) end();
  }, [end]);
  // Save, Enter and a palette action apply the name at once, and a wait for a
  // press to end must not apply it a second time.
  const settleNow = useCallback(() => tracker.now(settle), [settle]);
  useEffect(
    () => () => {
      tracker.forget(settle);
      const value = typed.current;
      if (value === null) return;
      typed.current = null;
      if (!callbacks.current.refusal(value)) callbacks.current.apply(value);
      setNameDraft(null);
    },
    [setNameDraft, settle],
  );
  const change = (value: string) => {
    box.current = document.activeElement instanceof HTMLInputElement ? document.activeElement : box.current;
    typed.current = value;
    undone.current = null;
    setText(value);
    setLate(null);
    setNameDraft({ kind, row, name: value, refused: refusal(value), settle: settleNow, drop, refocus });
  };
  return {
    text,
    refused: late ?? (text === null ? null : refusal(text)),
    change,
    blur: (e: FocusEvent) => {
      if (!keepsNameDraft(e.relatedTarget)) tracker.afterPress(settle);
    },
    keyDown: (e: KeyboardEvent<HTMLInputElement>) => {
      const undoKey = (e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "z";
      if (e.key === "Enter") e.currentTarget.blur();
      else if (e.key === "Escape" && typed.current !== null) {
        // Only the name: the page's Escape (closing the Inspector) waits for
        // an Escape with nothing typed.
        e.preventDefault();
        e.stopPropagation();
        drop();
      } else if (undoKey && !e.shiftKey && typed.current !== null) {
        e.preventDefault();
        const value = typed.current;
        drop();
        undone.current = value;
      } else if (undoKey && e.shiftKey && undone.current !== null) {
        e.preventDefault();
        change(undone.current);
      }
    },
    settle: settleNow,
    drop,
  };
}
