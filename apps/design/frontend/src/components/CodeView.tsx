import { useEffect, useMemo, useRef, useState } from "react";
import { AlignLeft, CircleAlert, TriangleAlert } from "lucide-react";
import { autocompletion, closeBrackets, closeBracketsKeymap, completionKeymap } from "@codemirror/autocomplete";
import { defaultKeymap, history, historyKeymap, indentWithTab } from "@codemirror/commands";
import { yaml } from "@codemirror/lang-yaml";
import {
  bracketMatching,
  foldGutter,
  foldKeymap,
  HighlightStyle,
  indentOnInput,
  indentUnit,
  syntaxHighlighting,
  syntaxTree,
} from "@codemirror/language";
import { lintGutter, lintKeymap, setDiagnostics } from "@codemirror/lint";
import { highlightSelectionMatches, searchKeymap } from "@codemirror/search";
import { Compartment, EditorState, type Range } from "@codemirror/state";
import {
  crosshairCursor,
  Decoration,
  type DecorationSet,
  drawSelection,
  dropCursor,
  EditorView,
  highlightActiveLine,
  highlightActiveLineGutter,
  highlightSpecialChars,
  keymap,
  lineNumbers,
  rectangularSelection,
  ViewPlugin,
  type ViewUpdate,
} from "@codemirror/view";
import { tags } from "@lezer/highlight";
import { Vim, vim } from "@replit/codemirror-vim";
import { useDefaultLayout } from "react-resizable-panels";
import { parseDocument, type YAMLError } from "yaml";
import type { DiagnosticDto } from "../api";
import { useT } from "../i18n";
import { anchorField, anchorParseError, type ParseErrorInfo } from "../model/anchors";
import { formatYaml } from "../model/format";
import { LABEL } from "../styles";
import { Button } from "@/components/ui/button";
import { Kbd, KbdGroup } from "@/components/ui/kbd";
import { ResizableHandle, ResizablePanel, ResizablePanelGroup } from "@/components/ui/resizable";
import { Hint } from "./Hint";
import { cn } from "@/lib/utils";

// `basicSetup`, unbundled — identical set, but with our own fold gutter: the
// stock marker is a raw `⌄` text glyph on its baseline, which reads as a typo
// next to the app's drawn chevrons. Ours is a border-drawn chevron (see the
// `.cm-fold-marker` theme rules) that rotates to point right when folded.
const editorSetup = [
  lineNumbers(),
  highlightActiveLineGutter(),
  highlightSpecialChars(),
  history(),
  foldGutter({
    // The same chevron glyph the rest of the app draws (lucide's ChevronDown
    // path), rotated as one piece when folded — hand-drawn CSS triangles can't
    // keep the two states optically identical.
    markerDOM: (open) => {
      const marker = document.createElement("span");
      marker.className = `cm-fold-marker${open ? "" : " cm-fold-marker-closed"}`;
      marker.innerHTML =
        '<svg viewBox="0 0 24 24" width="11" height="11" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="m6 9 6 6 6-6"/></svg>';
      return marker;
    },
  }),
  drawSelection(),
  dropCursor(),
  EditorState.allowMultipleSelections.of(true),
  indentOnInput(),
  bracketMatching(),
  closeBrackets(),
  autocompletion(),
  rectangularSelection(),
  crosshairCursor(),
  highlightActiveLine(),
  highlightSelectionMatches(),
  keymap.of([
    ...closeBracketsKeymap,
    ...defaultKeymap,
    ...searchKeymap,
    ...historyKeymap,
    ...foldKeymap,
    ...completionKeymap,
    ...lintKeymap,
  ]),
];

// ── the YAML problems the whole view keys off ────────────────────────────────

interface Problem {
  from: number;
  to: number;
  /// The rail row's locator: `line:col` for syntax problems, the field name
  /// for validation findings, `schema` for a conversion error.
  label: string;
  severity: "error" | "warning";
  message: string;
}

/// The buffer's syntax problems, off the one shared parse — the same problems
/// drive the editor squiggles, the rail list, and the status-bar state, so
/// they can never disagree.
function syntaxProblemsOf(doc: ReturnType<typeof parseDocument>, text: string): Problem[] {
  const toProblem = (e: YAMLError, severity: Problem["severity"]): Problem => {
    const from = Math.min(e.pos[0], text.length);
    const to = Math.min(Math.max(e.pos[1], from + 1), Math.max(text.length, from));
    return {
      from,
      to,
      label: `${e.linePos?.[0].line ?? 1}:${e.linePos?.[0].col ?? 1}`,
      severity,
      // prettyErrors messages carry a multi-line code frame and repeat the
      // position; the first line minus the "at line …" suffix is the summary
      // (the row already leads with line:col).
      message: (e.message.split("\n")[0] ?? e.message).replace(/ at line \d+, column \d+:?$/, "").replace(/:$/, ""),
    };
  };
  return [...doc.errors.map((e) => toProblem(e, "error")), ...doc.warnings.map((w) => toProblem(w, "warning"))].sort(
    (a, b) => a.from - b.from,
  );
}

// ── CodeMirror theming (flusso palette via CSS vars → follows light/dark) ────

const flussoTheme = EditorView.theme({
  "&": { height: "100%", fontSize: "0.75rem", backgroundColor: "var(--panel-2)", color: "var(--fg)" },
  "&.cm-focused": { outline: "none" },
  ".cm-scroller": { fontFamily: "ui-monospace, monospace", lineHeight: "1.25rem" },
  ".cm-content": { caretColor: "var(--accent)", padding: "0.625rem 0" },
  ".cm-cursor, .cm-dropCursor": { borderLeftColor: "var(--accent)" },
  ".cm-fat-cursor": { background: "var(--accent)", color: "var(--bg)" },
  "&:not(.cm-focused) .cm-fat-cursor": { background: "none", outline: "solid 1px var(--accent)" },
  // The focused rule must out-specify the base theme's own focused selector
  // (`&.cm-focused > .cm-scroller > .cm-selectionLayer .cm-selectionBackground`),
  // or the selection only takes the brand colour while unfocused.
  "&.cm-focused > .cm-scroller > .cm-selectionLayer .cm-selectionBackground, .cm-selectionBackground, .cm-content ::selection":
    {
      backgroundColor: "var(--selection)",
    },
  ".cm-activeLine": { backgroundColor: "color-mix(in srgb, var(--panel-3) 45%, transparent)" },
  ".cm-gutters": {
    backgroundColor: "var(--panel-2)",
    color: "var(--muted)",
    border: "none",
    borderRight: "0.0625rem solid var(--border)",
  },
  ".cm-activeLineGutter": { backgroundColor: "transparent", color: "var(--fg)" },
  ".cm-panels": { backgroundColor: "var(--panel)", color: "var(--fg)" },
  ".cm-panels.cm-panels-bottom": { borderTop: "0.0625rem solid var(--border)" },
  ".cm-panel input, .cm-panel button, .cm-panel label": { fontFamily: "inherit" },
  ".cm-searchMatch": { backgroundColor: "color-mix(in srgb, var(--warn) 25%, transparent)" },
  ".cm-searchMatch.cm-searchMatch-selected": { backgroundColor: "color-mix(in srgb, var(--warn) 45%, transparent)" },
  ".cm-foldPlaceholder": { background: "var(--panel-3)", border: "none", color: "var(--muted)" },
  ".cm-foldGutter .cm-gutterElement": {
    display: "flex",
    alignItems: "center",
    justifyContent: "center",
    cursor: "pointer",
  },
  // A border-drawn chevron (two edges of a rotated square): 45° points down
  // (foldable), −45° points right (folded) — the canvas chevron language.
  ".cm-fold-marker": {
    display: "flex",
    color: "var(--muted)",
    transition: "transform 0.12s ease, color 0.12s ease",
  },
  ".cm-foldGutter .cm-gutterElement:hover .cm-fold-marker": { color: "var(--fg)" },
  ".cm-fold-marker-closed": { transform: "rotate(-90deg)" },
  ".cm-tooltip": { background: "var(--panel-2)", border: "0.0625rem solid var(--border)", color: "var(--fg)" },
  ".cm-lintRange-error": { textDecoration: "underline wavy var(--error) 1px" },
  ".cm-lintRange-warning": { textDecoration: "underline wavy var(--warn) 1px" },
  // Replace the lint gutter's stock SVG blob with the app's dot language: a
  // small centred dot with a soft same-hue halo (like the dirty/flow dots).
  ".cm-gutter-lint": { width: "0.875rem" },
  ".cm-lint-marker": {
    content: "none",
    width: "0.375rem",
    height: "0.375rem",
    margin: "0.4375rem auto 0",
    borderRadius: "62rem",
  },
  ".cm-lint-marker-error": {
    content: "none",
    backgroundColor: "var(--error)",
    boxShadow: "0 0 0 0.1875rem color-mix(in srgb, var(--error) 22%, transparent)",
  },
  ".cm-lint-marker-warning": {
    content: "none",
    backgroundColor: "var(--warn)",
    boxShadow: "0 0 0 0.1875rem color-mix(in srgb, var(--warn) 22%, transparent)",
  },
});

// The same hues the read-only highlighter (highlight.tsx) and the canvas use:
// keys cyan, strings amber, numbers blue, bools teal, comments muted italic.
const flussoHighlight = syntaxHighlighting(
  HighlightStyle.define([
    { tag: tags.propertyName, color: "var(--accent-2)" },
    { tag: tags.string, color: "var(--t-string)" },
    { tag: tags.number, color: "var(--t-number)" },
    { tag: tags.bool, color: "var(--t-bool)" },
    { tag: tags.null, color: "var(--t-bool)" },
    { tag: tags.comment, color: "var(--muted)", fontStyle: "italic" },
    { tag: tags.punctuation, color: "var(--muted)" },
    { tag: tags.separator, color: "var(--muted)" },
    { tag: tags.meta, color: "var(--muted)" },
  ]),
);

// The YAML grammar tags every *plain* scalar as generic content (only quoted
// ones are strings), so numbers/bools/strings would all render foreground.
// Classify each non-key Literal by its text — the same rule as highlight.tsx's
// yamlValue — and mark it with the app-wide type-family classes.
const NUM = Decoration.mark({ class: "t-number" });
const BOOL = Decoration.mark({ class: "t-bool" });
const STR = Decoration.mark({ class: "t-string" });

function scalarDecorations(view: EditorView): DecorationSet {
  const out: Range<Decoration>[] = [];
  for (const { from, to } of view.visibleRanges) {
    syntaxTree(view.state).iterate({
      from,
      to,
      enter: (node) => {
        if (node.name !== "Literal" || node.matchContext(["Key"])) return;
        const text = view.state.sliceDoc(node.from, node.to);
        const deco = /^-?\d+(\.\d+)?$/.test(text)
          ? NUM
          : text === "true" || text === "false" || text === "null"
            ? BOOL
            : STR;
        out.push(deco.range(node.from, node.to));
      },
    });
  }
  return Decoration.set(out);
}

const scalarHighlight = ViewPlugin.fromClass(
  class {
    decorations: DecorationSet;
    constructor(view: EditorView) {
      this.decorations = scalarDecorations(view);
    }
    update(u: ViewUpdate) {
      if (u.docChanged || u.viewportChanged) this.decorations = scalarDecorations(u.view);
    }
  },
  { decorations: (v) => v.decorations },
);

// ── the editor pane ──────────────────────────────────────────────────────────

interface EditorApi {
  /// Move the caret to an offset, scroll it centred, and focus the editor.
  jumpTo: (offset: number) => void;
}

interface Cursor {
  line: number;
  col: number;
}

/// CodeMirror 6 with the official YAML grammar, flusso theming, the full stock
/// keymap (history, ⌘F search panel, folding, Tab indents), optional VIM
/// keybindings (swapped live via a Compartment — must sit *before* the other
/// keymaps), ⇧⌥F → `onFormat`, and `:w` → `onSave`. Controlled: external
/// `value` changes (auto-format) are dispatched in; `problems` become lint
/// squiggles + gutter markers; the caret position streams out via `onCursor`.
function YamlEditor({
  value,
  onChange,
  vim: vimOn,
  problems,
  onFormat,
  onSave,
  onCursor,
  onBlur,
  apiRef,
  className,
}: {
  value: string;
  onChange: (v: string) => void;
  vim?: boolean;
  problems: Problem[];
  onFormat?: () => void;
  onSave?: () => void;
  onCursor?: (c: Cursor) => void;
  onBlur?: () => void;
  apiRef?: React.MutableRefObject<EditorApi | null>;
  className?: string;
}) {
  const { t } = useT();
  const host = useRef<HTMLDivElement>(null);
  const view = useRef<EditorView | null>(null);
  const vimSwitch = useRef(new Compartment());
  // Latest callbacks behind refs, so the view is created exactly once.
  const cb = useRef({ onChange, onFormat, onSave, onCursor, onBlur });
  useEffect(() => {
    cb.current = { onChange, onFormat, onSave, onCursor, onBlur };
  });

  useEffect(() => {
    if (!host.current) return;
    Vim.defineEx("write", "w", () => cb.current.onSave?.());
    const ev = new EditorView({
      doc: value,
      parent: host.current,
      extensions: [
        vimSwitch.current.of(vimOn ? vim() : []),
        editorSetup,
        yaml(),
        indentUnit.of("  "),
        lintGutter(),
        keymap.of([
          indentWithTab,
          {
            key: "Shift-Alt-f",
            run: () => {
              cb.current.onFormat?.();
              return true;
            },
          },
        ]),
        flussoTheme,
        flussoHighlight,
        scalarHighlight,
        EditorView.updateListener.of((u) => {
          if (u.docChanged) cb.current.onChange(u.state.doc.toString());
          if (u.selectionSet || u.docChanged) {
            const head = u.state.selection.main.head;
            const line = u.state.doc.lineAt(head);
            cb.current.onCursor?.({ line: line.number, col: head - line.from + 1 });
          }
          if (u.focusChanged && !u.view.hasFocus) cb.current.onBlur?.();
        }),
      ],
    });
    view.current = ev;
    if (apiRef) {
      apiRef.current = {
        jumpTo: (offset) => {
          const pos = Math.min(offset, ev.state.doc.length);
          ev.dispatch({ selection: { anchor: pos }, effects: EditorView.scrollIntoView(pos, { y: "center" }) });
          ev.focus();
        },
      };
    }
    return () => {
      ev.destroy();
      view.current = null;
      if (apiRef) apiRef.current = null;
    };
    // The view is created once per mount; `value`/`vimOn`/`problems` sync below.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  useEffect(() => {
    view.current?.dispatch({ effects: vimSwitch.current.reconfigure(vimOn ? vim() : []) });
  }, [vimOn]);

  // External value changes (auto-format) replace the doc; the guard keeps the
  // editor's own keystrokes (already in `value`) from looping.
  useEffect(() => {
    const ev = view.current;
    if (!ev) return;
    const current = ev.state.doc.toString();
    if (current !== value) ev.dispatch({ changes: { from: 0, to: current.length, insert: value } });
  }, [value]);

  // After the doc sync above, so positions always refer to the same text.
  useEffect(() => {
    const ev = view.current;
    if (!ev) return;
    ev.dispatch(setDiagnostics(ev.state, problems));
  }, [problems]);

  return <div ref={host} role="textbox" aria-label={t("raw.editorAria")} className={className} />;
}

// ── the companion rail ───────────────────────────────────────────────────────

function ShortcutRow({ label, keys }: { label: string; keys: string[] }) {
  return (
    <div className="flex items-center justify-between py-0.5 text-2xs text-muted-foreground">
      <span>{label}</span>
      <KbdGroup>
        {keys.map((k) => (
          <Kbd key={k}>{k}</Kbd>
        ))}
      </KbdGroup>
    </div>
  );
}

function Rail({ problems, onJump }: { problems: Problem[]; onJump: (p: Problem) => void }) {
  const { t } = useT();
  return (
    <aside className="flex h-full min-h-0 flex-col gap-4 overflow-y-auto bg-card p-3">
      <section>
        <div className={cn(LABEL, "mb-1.5 flex items-center gap-1.5")}>
          {t("code.problems")}
          {problems.length > 0 && (
            <span className="rounded-full bg-destructive/15 px-1.5 text-3xs font-semibold text-destructive tabular-nums">
              {problems.length}
            </span>
          )}
        </div>
        {problems.length === 0 && <p className="text-2xs text-primary">✓ {t("code.noProblems")}</p>}
        {problems.map((p, i) => {
          const err = p.severity === "error";
          const Icon = err ? CircleAlert : TriangleAlert;
          return (
            <button
              key={i}
              type="button"
              onClick={() => onJump(p)}
              className={cn(
                "mb-1.5 flex w-full cursor-pointer items-start gap-1.5 rounded-md border px-2 py-1.5 text-left font-mono text-2xs transition-colors",
                err
                  ? "border-destructive/30 bg-destructive/10 hover:bg-destructive/20"
                  : "border-warn/30 bg-warn/10 hover:bg-warn/20",
              )}
            >
              <Icon className={cn("mt-px size-3.5 shrink-0", err ? "text-destructive" : "text-warn")} />
              <span className="min-w-0">
                <span className={cn("font-semibold", err ? "text-destructive" : "text-warn")}>{p.label}</span>{" "}
                <span className="text-foreground/90">{p.message}</span>
              </span>
            </button>
          );
        })}
      </section>
      <section>
        <div className={cn(LABEL, "mb-1.5")}>{t("code.shortcuts")}</div>
        <ShortcutRow label={t("code.search")} keys={["⌘", "F"]} />
        <ShortcutRow label={t("raw.format")} keys={["⇧", "⌥", "F"]} />
        <ShortcutRow label={t("topbar.save")} keys={["⌘", "S"]} />
        <ShortcutRow label={t("code.fold")} keys={["⌥", "⌘", "["]} />
      </section>
      <p className="mt-auto text-2xs leading-snug text-muted-foreground">{t("raw.help")}</p>
    </aside>
  );
}

// ── the Code view: editor ⟷ rail split over a status bar ─────────────────────

/// The Code mode body: a resizable editor/rail split over a status bar. Edits
/// sync into the in-memory document (via App's /api/parse loop), so there is no
/// save action here — the one global Save covers YAML and visual edits alike.
/// The rail lists everything wrong, clickable: YAML syntax problems (by
/// position), the parser's conversion error, and live database-validation
/// findings (by field name).
export function CodeView({
  value,
  onChange,
  fileName,
  dirty,
  vim: vimOn,
  autoFormat,
  onSave,
  parseError,
  diagnostics,
}: {
  value: string;
  onChange: (v: string) => void;
  /// The schema file this buffer belongs to (from the config's index entry).
  fileName: string;
  dirty: boolean;
  vim: boolean;
  /// Format on focus loss (the context bar's toggle; explicit Format always works).
  autoFormat: boolean;
  /// The project save (diff review) — wired to ⌘S's editor-side hint and VIM's `:w`.
  onSave: () => void;
  parseError: ParseErrorInfo | null;
  diagnostics: DiagnosticDto[];
}) {
  const { t } = useT();
  const [cursor, setCursor] = useState<Cursor>({ line: 1, col: 1 });
  const api = useRef<EditorApi | null>(null);
  // One parse feeds everything: the syntax problems AND the AST whose node
  // ranges anchor the name-only problems (validation findings, parse errors).
  const ydoc = useMemo(() => parseDocument(value), [value]);
  const syntaxProblems = useMemo(() => syntaxProblemsOf(ydoc, value), [ydoc, value]);
  // One unified problem list — syntax, conversion, and validation — drives the
  // squiggles, the rail, and the status bar together. Name-only problems
  // anchor on their AST node (see model/anchors.ts), so a duplicate name in
  // another block can't mislead the squiggle.
  const problems = useMemo(() => {
    const list = [...syntaxProblems];
    for (const d of diagnostics) {
      list.push({
        ...(anchorField(ydoc, d.field)?.span ?? { from: 0, to: 0 }),
        label: d.field,
        severity: d.severity === "warning" ? "warning" : "error",
        message: d.message,
      });
    }
    if (parseError) {
      list.push({
        ...anchorParseError(ydoc, value, parseError),
        label: parseError.field ?? "schema",
        severity: "error",
        message: parseError.message,
      });
    }
    return list;
  }, [ydoc, value, syntaxProblems, diagnostics, parseError]);
  const jump = (p: Problem) => api.current?.jumpTo(p.from);
  // Auto-format whenever the buffer is syntactically valid: on the editor
  // losing focus, and via the button / ⇧⌥F.
  const format = () => {
    if (syntaxProblems.length > 0) return;
    const res = formatYaml(value);
    if (res.ok && res.text !== value) onChange(res.text);
  };
  const ACTION = "h-6 gap-1 px-2 text-2xs";
  // The editor/rail split is remembered across sessions (localStorage-backed).
  const { defaultLayout, onLayoutChanged } = useDefaultLayout({ id: "flusso-design.code-rail" });
  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <ResizablePanelGroup
        orientation="horizontal"
        defaultLayout={defaultLayout}
        onLayoutChanged={onLayoutChanged}
        className="min-h-0 flex-1"
      >
        <ResizablePanel id="editor" defaultSize="76" minSize="40">
          <YamlEditor
            className="h-full"
            value={value}
            onChange={onChange}
            vim={vimOn}
            problems={problems}
            onFormat={format}
            onSave={onSave}
            onCursor={setCursor}
            onBlur={autoFormat ? format : undefined}
            apiRef={api}
          />
        </ResizablePanel>
        <ResizableHandle withHandle />
        <ResizablePanel id="rail" defaultSize="24" minSize="12" maxSize="40">
          <Rail problems={problems} onJump={jump} />
        </ResizablePanel>
      </ResizablePanelGroup>

      <div className="flex items-center gap-3 border-t border-border bg-secondary px-3 py-1 text-2xs">
        <span className="flex items-center gap-1.5 font-mono text-foreground">
          {fileName}
          {dirty && <span className="dirty-dot" />}
        </span>
        {problems.length > 0 ? (
          <button
            type="button"
            onClick={() => problems[0] && jump(problems[0])}
            className="cursor-pointer font-mono text-destructive hover:underline"
          >
            ✗ {t("code.problemsN", { n: problems.length })}
          </button>
        ) : (
          <span className="font-mono text-primary">✓ {t("code.validYaml")}</span>
        )}
        <span className="font-mono text-muted-foreground tabular-nums">
          {t("code.cursor", { l: cursor.line, c: cursor.col })}
        </span>
        <span className="flex-1" />
        <Hint label={`${t("raw.format")} · ⇧⌥F`}>
          <Button variant="ghost" size="sm" className={ACTION} disabled={syntaxProblems.length > 0} onClick={format}>
            <AlignLeft className="size-3" /> {t("raw.format")}
          </Button>
        </Hint>
      </div>
    </div>
  );
}
