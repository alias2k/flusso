// Ephemeral UI state — chrome toggles, transient feedback, in-flight flags. Kept
// apart from the edited document (the design store) and out of undo/redo.

import { create } from "zustand";
import type { OpDiff } from "../api";

export interface Toast {
  kind: "ok" | "error" | "info";
  text: string;
  /// A button in the toast, for a choice the user must make deliberately.
  action?: { label: string; run: () => void };
}

/// What the Code buffer last agreed with the document on: the text it was
/// seeded with or last applied, that schema as bare JSON, and the Code-mode
/// text the store held for the index then (`texts`, undone with the document).
export interface Seed {
  index: string;
  text: string;
  schema: string;
  base?: string;
}

const initialTheme = (): "dark" | "light" => {
  try {
    return (localStorage.getItem("flusso-design.theme") as "dark" | "light") || "dark";
  } catch {
    return "dark";
  }
};

const initialVim = (): boolean => {
  try {
    return localStorage.getItem("flusso-design.vim") === "1";
  } catch {
    return false;
  }
};

const initialAutoFormat = (): boolean => {
  try {
    return localStorage.getItem("flusso-design.autoformat") === "1";
  } catch {
    return false;
  }
};

interface UiState {
  theme: "dark" | "light";
  leftOpen: boolean;
  drawer: boolean;
  error: string;
  toast: Toast | null;
  saving: boolean;
  validating: boolean;
  rawMode: boolean;
  rawText: string;
  seed: Seed | null;
  /// VIM keybindings in the raw-YAML editor (persisted preference).
  vimMode: boolean;
  /// Auto-format the Code buffer on focus loss (persisted preference, off by default).
  autoFormat: boolean;
  diffs: OpDiff[] | null;
  browseCatalog: boolean;

  toggleTheme: () => void;
  toggleLeft: () => void;
  setDrawer: (open: boolean) => void;
  toggleDrawer: () => void;
  setError: (error: string) => void;
  setToast: (toast: Toast | null) => void;
  setSaving: (saving: boolean) => void;
  setValidating: (validating: boolean) => void;
  setRawMode: (rawMode: boolean) => void;
  setRawText: (rawText: string) => void;
  setSeed: (seed: Seed | null) => void;
  toggleVim: () => void;
  toggleAutoFormat: () => void;
  setDiffs: (diffs: OpDiff[] | null) => void;
  setBrowseCatalog: (open: boolean) => void;
}

export const useUiStore = create<UiState>()((set) => ({
  theme: initialTheme(),
  leftOpen: true,
  drawer: false,
  error: "",
  toast: null,
  saving: false,
  validating: false,
  rawMode: false,
  rawText: "",
  seed: null,
  vimMode: initialVim(),
  autoFormat: initialAutoFormat(),
  diffs: null,
  browseCatalog: false,

  toggleTheme: () => set((s) => ({ theme: s.theme === "dark" ? "light" : "dark" })),
  toggleLeft: () => set((s) => ({ leftOpen: !s.leftOpen })),
  setDrawer: (drawer) => set({ drawer }),
  toggleDrawer: () => set((s) => ({ drawer: !s.drawer })),
  setError: (error) => set({ error }),
  setToast: (toast) => set({ toast }),
  setSaving: (saving) => set({ saving }),
  setValidating: (validating) => set({ validating }),
  setRawMode: (rawMode) => set({ rawMode }),
  setRawText: (rawText) => set({ rawText }),
  setSeed: (seed) => set({ seed }),
  toggleVim: () =>
    set((s) => {
      const vimMode = !s.vimMode;
      try {
        localStorage.setItem("flusso-design.vim", vimMode ? "1" : "0");
      } catch {
        /* storage disabled — the preference just won't persist */
      }
      return { vimMode };
    }),
  toggleAutoFormat: () =>
    set((s) => {
      const autoFormat = !s.autoFormat;
      try {
        localStorage.setItem("flusso-design.autoformat", autoFormat ? "1" : "0");
      } catch {
        /* storage disabled — the preference just won't persist */
      }
      return { autoFormat };
    }),
  setDiffs: (diffs) => set({ diffs }),
  setBrowseCatalog: (browseCatalog) => set({ browseCatalog }),
}));
