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
//   draft pending, for the palette's own actions to settle or drop.
// - Escape and Ctrl+Z (⌘Z) in the box drop it.
//
// Applying a name must move nothing on the page: a press that left the box
// would otherwise end on another control, and its click would be lost. So the
// boxes show reasons in an overlay, rename chips sit in a row of fixed height
// and fixed order (`model/renameChips.ts`), and the canvas title box lies over
// the title, out of the header's flow.
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
import { useUiStore, type NameDraft } from "../store/ui";

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
  useEffect(
    () => () => {
      const value = typed.current;
      if (value === null) return;
      if (!callbacks.current.refusal(value)) callbacks.current.apply(value);
      setNameDraft(null);
    },
    [setNameDraft],
  );
  return {
    text,
    refused: late ?? (text === null ? null : refusal(text)),
    change: (value: string) => {
      typed.current = value;
      setText(value);
      setLate(null);
      setNameDraft({ kind, row, name: value, refused: refusal(value), settle, drop });
    },
    blur: (e: FocusEvent) => {
      if (!keepsNameDraft(e.relatedTarget)) settle();
    },
    keyDown: (e: KeyboardEvent<HTMLInputElement>) => {
      if (e.key === "Enter") e.currentTarget.blur();
      else if (e.key === "Escape") drop();
      else if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "z" && typed.current !== null) {
        e.preventDefault();
        drop();
      }
    },
    settle,
    drop,
  };
}
