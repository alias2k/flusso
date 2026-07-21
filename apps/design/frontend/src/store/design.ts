// The edited document and everything scoped to it. Only `doc` is under undo/redo.

import { create, useStore } from "zustand";
import { temporal } from "zundo";
import type { CatalogResponse, ConfigToml, DiagnosticDto, IndexSchema, PreviewResponse, Project } from "../api";
import type { Selection } from "../state";

export interface Doc {
  config: ConfigToml;
  schemas: Record<string, IndexSchema>;
}

export const emptySchema = (table: string, pk?: string): IndexSchema => ({
  version: 1,
  table,
  db_schema: "public",
  primary_key: pk,
  fields: [],
});

const HISTORY_LIMIT = 200;

const collapseKey = (index: string) => `flusso-design.collapsed.${index}`;

interface DesignState {
  project: Project | null;
  catalog: CatalogResponse | null;
  doc: Doc | null;
  saved: string; // JSON of the last loaded/saved doc
  active: string;
  selection: Selection;
  collapsed: Set<string>;
  preview: PreviewResponse | null;
  diagnostics: DiagnosticDto[] | null;

  setCatalog: (catalog: CatalogResponse | null) => void;
  setSaved: (saved: string) => void;
  setPreview: (preview: PreviewResponse | null) => void;
  setDiagnostics: (diagnostics: DiagnosticDto[] | null) => void;
  setActive: (active: string) => void;
  setSelection: (selection: Selection) => void;

  loadProject: (project: Project, resetActive: boolean) => void;
  apply: (fn: (s: IndexSchema) => IndexSchema) => void;
  setConfig: (next: ConfigToml) => void;
  openIndex: (name: string) => void;
  createIndex: (name: string, table: string) => void;
  dupIndex: (i: number) => void;
  revertChanges: () => void;

  loadCollapsed: (index: string) => void;
  toggleCollapsed: (id: string) => void;
}

export const useDesignStore = create<DesignState>()(
  temporal(
    (set, get) => ({
      project: null,
      catalog: null,
      doc: null,
      saved: "",
      active: "config",
      selection: null,
      collapsed: new Set<string>(),
      preview: null,
      diagnostics: null,

      setCatalog: (catalog) => set({ catalog }),
      setSaved: (saved) => set({ saved }),
      setPreview: (preview) => set({ preview }),
      setDiagnostics: (diagnostics) => set({ diagnostics }),
      setActive: (active) => set({ active }),
      setSelection: (selection) => set({ selection }),

      loadProject: (project, resetActive) => {
        const schemas: Record<string, IndexSchema> = {};
        for (const idx of project.indexes) if (idx.schema) schemas[idx.name] = idx.schema;
        const doc: Doc = { config: project.config, schemas };
        set({
          project,
          doc,
          saved: JSON.stringify(doc),
          ...(resetActive ? { active: project.indexes[0]?.name ?? "config" } : {}),
        });
        useDesignStore.temporal.getState().clear();
      },

      apply: (fn) => {
        const { active } = get();
        if (active === "config") return;
        set((s) =>
          s.doc
            ? {
                doc: {
                  ...s.doc,
                  schemas: { ...s.doc.schemas, [active]: fn(s.doc.schemas[active] ?? emptySchema("")) },
                },
              }
            : {},
        );
      },

      setConfig: (next) => {
        const { doc, active } = get();
        if (!doc) return;
        const oldIdx = doc.config.index ?? [];
        const newIdx = next.index ?? [];
        const renames = new Map<string, string>();
        for (let i = 0; i < Math.min(oldIdx.length, newIdx.length); i += 1) {
          if (oldIdx[i].name !== newIdx[i].name) renames.set(oldIdx[i].name, newIdx[i].name);
        }
        const removed = oldIdx
          .filter((o) => !newIdx.some((n) => n.name === o.name) && !renames.has(o.name))
          .map((o) => o.name);

        // Re-key renamed schemas and drop removed ones, else a renamed index's schema is lost on save.
        const schemas = { ...doc.schemas };
        for (const [oldName, newName] of renames) {
          if (oldName in schemas) {
            schemas[newName] = schemas[oldName];
            delete schemas[oldName];
          }
        }
        for (const name of removed) delete schemas[name];

        const active2 = renames.has(active) ? renames.get(active)! : removed.includes(active) ? "config" : active;
        set({ doc: { config: next, schemas }, active: active2 });
      },

      openIndex: (name) => set({ active: name, selection: { kind: "root" }, diagnostics: null }),

      createIndex: (name, table) => {
        const { doc, catalog } = get();
        if (!doc) return;
        const pk = catalog?.catalog.tables.find((t) => t.name === table)?.primary_key[0];
        set({
          doc: {
            config: {
              ...doc.config,
              index: [...(doc.config.index ?? []), { name, schema: `${name}.schema.yml`, enabled: true }],
            },
            schemas: { ...doc.schemas, [name]: emptySchema(table, pk) },
          },
          active: name,
          selection: { kind: "root" },
          diagnostics: null,
        });
      },

      dupIndex: (i) => {
        const { doc } = get();
        if (!doc) return;
        const entries = doc.config.index ?? [];
        const src = entries[i];
        if (!src) return;
        let name = `${src.name}_copy`;
        let n = 1;
        while (entries.some((e) => e.name === name)) name = `${src.name}_copy${++n}`;
        set({
          doc: {
            config: {
              ...doc.config,
              index: [
                ...entries.slice(0, i + 1),
                { name, schema: `${name}.schema.yml`, enabled: src.enabled },
                ...entries.slice(i + 1),
              ],
            },
            schemas: doc.schemas[src.name]
              ? { ...doc.schemas, [name]: structuredClone(doc.schemas[src.name]) }
              : doc.schemas,
          },
          active: name,
          selection: { kind: "root" },
          diagnostics: null,
        });
      },

      revertChanges: () => {
        const { saved } = get();
        if (!saved) return;
        set({ doc: JSON.parse(saved) as Doc, selection: null });
      },

      loadCollapsed: (index) => {
        if (index === "config") return;
        try {
          set({ collapsed: new Set(JSON.parse(localStorage.getItem(collapseKey(index)) ?? "[]") as string[]) });
        } catch {
          set({ collapsed: new Set() });
        }
      },

      toggleCollapsed: (id) => {
        const { active, collapsed } = get();
        const next = new Set(collapsed);
        if (next.has(id)) next.delete(id);
        else next.add(id);
        try {
          localStorage.setItem(collapseKey(active), JSON.stringify([...next]));
        } catch {
          /* storage disabled — collapse just won't persist */
        }
        set({ collapsed: next });
      },
    }),
    {
      // Track only `doc`; reference equality keeps no-op edits and selection/active changes off the stack.
      limit: HISTORY_LIMIT,
      partialize: (s) => ({ doc: s.doc }),
      equality: (a, b) => a.doc === b.doc,
    },
  ),
);

export const undo = () => useDesignStore.temporal.getState().undo();
export const redo = () => useDesignStore.temporal.getState().redo();

export const useCanUndo = () => useStore(useDesignStore.temporal, (s) => s.pastStates.length > 0);
export const useCanRedo = () => useStore(useDesignStore.temporal, (s) => s.futureStates.length > 0);
