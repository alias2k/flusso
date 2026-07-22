// Shared Tailwind class-string tokens reused across the app chrome.

// The bare `nav`/`active` class names are e2e selector hooks — don't drop them.
export const NAV =
  "nav mb-0.5 block w-full cursor-pointer rounded-md border-0 bg-transparent px-2.5 py-2 text-left hover:bg-secondary";
export const NAV_ACTIVE = "active bg-secondary text-primary";
export const NAV_HEADING = "mx-1 mb-1 mt-3 text-2xs uppercase text-muted-foreground";

// A button's leading glyph sits in a fixed 1rem slot: swapping icon/dot/spinner
// never resizes the button, and nesting the svg stops the `has-[>svg]` padding
// rule from toggling mid-swap.
export const BTN_ICON = "inline-flex size-4 shrink-0 items-center justify-center [&>*]:me-0";

// The uppercase micro-label over a field / table column (inspector field names,
// config table headers). One string so the treatment can't drift per-panel.
export const LABEL = "text-3xs font-semibold uppercase tracking-caps text-muted-foreground";

// Password-manager opt-out + browser-assist suppression for non-login inputs
// (1Password/LastPass/Bitwarden/Dashlane overlays cover dense UI otherwise).
// Spread onto any raw `<input>` that doesn't go through the `Text` widget.
export const NO_PW_MANAGER = {
  autoComplete: "off",
  autoCorrect: "off",
  autoCapitalize: "off",
  spellCheck: false,
  "data-1p-ignore": "true",
  "data-lpignore": "true",
  "data-bwignore": "true",
  "data-form-type": "other",
} as const;
