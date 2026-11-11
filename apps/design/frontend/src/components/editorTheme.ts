// The CodeMirror theme every editor in the designer shares: the flusso palette
// via CSS variables, so it follows light/dark.

import { EditorView } from "@codemirror/view";

export const flussoTheme = EditorView.theme({
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
