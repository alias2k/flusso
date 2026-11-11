import { useEffect, useRef } from "react";
import { closeBrackets, closeBracketsKeymap } from "@codemirror/autocomplete";
import { defaultKeymap, history, historyKeymap, indentWithTab } from "@codemirror/commands";
import { bracketMatching, indentUnit } from "@codemirror/language";
import { type Diagnostic, lintGutter, setDiagnostics } from "@codemirror/lint";
import { EditorState } from "@codemirror/state";
import { EditorView, highlightActiveLine, keymap, lineNumbers } from "@codemirror/view";
import type { ScriptErrorDto } from "../api";
import { flussoTheme } from "./editorTheme";

/// The source editor of a script field: a small CodeMirror with line numbers and
/// the script's compile errors underlined at their line and column. Controlled:
/// `value` in, `onChange` out; an outside change (undo, a YAML edit) is synced in.
export function ScriptEditor({
  value,
  errors,
  onChange,
  label,
}: {
  value: string;
  errors: ScriptErrorDto[];
  onChange: (source: string) => void;
  label: string;
}) {
  const host = useRef<HTMLDivElement>(null);
  const view = useRef<EditorView | null>(null);
  const change = useRef(onChange);
  useEffect(() => {
    change.current = onChange;
  });

  useEffect(() => {
    if (!host.current) return;
    const ev = new EditorView({
      parent: host.current,
      state: EditorState.create({
        doc: value,
        extensions: [
          lineNumbers(),
          history(),
          closeBrackets(),
          bracketMatching(),
          highlightActiveLine(),
          indentUnit.of("  "),
          lintGutter(),
          keymap.of([indentWithTab, ...closeBracketsKeymap, ...defaultKeymap, ...historyKeymap]),
          flussoTheme,
          EditorView.contentAttributes.of({ "aria-label": label }),
          EditorView.updateListener.of((u) => {
            if (u.docChanged) change.current(u.state.doc.toString());
          }),
        ],
      }),
    });
    view.current = ev;
    return () => {
      ev.destroy();
      view.current = null;
    };
    // Created once per mount; `value` and `errors` sync below.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  useEffect(() => {
    const ev = view.current;
    if (!ev || ev.state.doc.toString() === value) return;
    ev.dispatch({ changes: { from: 0, to: ev.state.doc.length, insert: value } });
  }, [value]);

  useEffect(() => {
    const ev = view.current;
    if (!ev) return;
    const doc = ev.state.doc;
    const diagnostics: Diagnostic[] = errors.map((e) => {
      const line = doc.line(Math.min(Math.max(e.line ?? 1, 1), doc.lines));
      const from = Math.min(line.from + Math.max((e.column ?? 1) - 1, 0), line.to);
      return { from, to: Math.max(from + 1, line.to), severity: "error", message: e.message };
    });
    ev.dispatch(setDiagnostics(ev.state, diagnostics));
  }, [errors, value]);

  return <div ref={host} className="script-editor min-h-24 overflow-hidden rounded-md border border-border" />;
}
