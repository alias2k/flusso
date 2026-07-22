// Ephemeral UI state — chrome toggles, transient feedback, in-flight flags. Kept
// apart from the edited document (the design store) and out of undo/redo.

import { create } from "zustand";
import type { FileDiff } from "../api";

export interface Toast {
  kind: "ok" | "error" | "info";
  text: string;
}

const initialTheme = (): "dark" | "light" => {
  try {
    return (localStorage.getItem("flusso-design.theme") as "dark" | "light") || "dark";
  } catch {
    return "dark";
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
  diffs: FileDiff[] | null;
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
  setDiffs: (diffs: FileDiff[] | null) => void;
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
  setDiffs: (diffs) => set({ diffs }),
  setBrowseCatalog: (browseCatalog) => set({ browseCatalog }),
}));
