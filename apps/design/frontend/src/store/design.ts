// The edited document and everything scoped to it. Only `doc` is under undo/redo.

import { create, useStore } from "zustand";
import { temporal } from "zundo";
import type { CatalogResponse, ConfigToml, DiagnosticDto, IndexSchema, PreviewResponse, Project } from "../api";
import { cloneJson, parseJson } from "../model/exact";
import { dropPendingValues, flushPendingValues } from "../model/pending";
import { adoptIds, stampIds, stripUids, uidSkeleton } from "../model/ids";
import { loadOverrides, type Positions, saveOverrides } from "../model/layout";
import { type Graph, pathId, projectGraph } from "../model/tree";
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

// A session-local, stable id for an index entry — correlates it across renames
// and path changes when building the save op set. Never written to disk.
const newId = () => crypto.randomUUID();

const collapseKey = (index: string) => `flusso-design.collapsed.${index}`;

const persistCollapsed = (index: string, ids: Set<string>) => {
  try {
    localStorage.setItem(collapseKey(index), JSON.stringify([...ids]));
  } catch {
    /* storage disabled — collapse just won't persist */
  }
};

const readCollapsed = (index: string): string[] => {
  try {
    return JSON.parse(localStorage.getItem(collapseKey(index)) ?? "[]") as string[];
  } catch {
    return [];
  }
};

// Translate legacy positional keys (`root.2.0`) to stable node ids and drop
// entries for nodes that no longer exist, for the collapse set and the manual
// positions alike; persists only what changed. An entry already under a live
// id wins over a translated legacy one. Runs once, on project load: that is the
// on-disk doc positional keys were saved against, and it must precede the
// canvas's first read of the overrides. Later in the session a removed node's
// entries are kept, so undoing the removal brings its view state back.
function reconcileViewState(index: string, graph: Graph) {
  const live = new Set(graph.nodes.map((n) => n.id));
  const byPath = new Map(graph.nodes.map((n) => [pathId(n.path), n.id]));
  const translate = (id: string) => (live.has(id) ? id : byPath.get(id));

  const stored = readCollapsed(index);
  const collapsed = [...new Set(stored.map(translate).filter((id): id is string => id !== undefined))];
  if (collapsed.length !== stored.length || collapsed.some((id, i) => id !== stored[i]))
    persistCollapsed(index, new Set(collapsed));

  const overrides = loadOverrides(index);
  const kept: Positions = {};
  for (const [id, pos] of Object.entries(overrides)) {
    const to = translate(id);
    if (to && (to === id || !(to in overrides))) kept[to] = pos;
  }
  if (JSON.stringify(kept) !== JSON.stringify(overrides)) saveOverrides(index, kept);
}

// Node uids are made in the browser, so to key view state across reloads the
// uids of the *saved* doc (what the next load reads from disk) are persisted per
// index as a name skeleton, and adopted onto the loaded file by name — so a file
// changed on disk in between (a pull, a hand edit) can't hand one node's uid to
// another that moved into its slot. Unsaved containers get fresh uids next
// session; their view state is dropped with them.
const uidsKey = (index: string) => `flusso-design.uids.${index}`;

const readUids = (index: string): Pick<IndexSchema, "fields"> | undefined => {
  try {
    const stored = JSON.parse(localStorage.getItem(uidsKey(index)) ?? "null") as Pick<IndexSchema, "fields"> | null;
    return Array.isArray(stored?.fields) ? stored : undefined;
  } catch {
    return undefined;
  }
};

const persistUids = (saved: string) => {
  try {
    const doc = parseJson(saved) as Doc;
    for (const [name, schema] of Object.entries(doc.schemas))
      localStorage.setItem(uidsKey(name), JSON.stringify(uidSkeleton(schema)));
  } catch {
    /* storage disabled — view state just won't survive a reload */
  }
};

const stampAll = (schemas: Record<string, IndexSchema>): Record<string, IndexSchema> =>
  Object.fromEntries(Object.entries(schemas).map(([name, schema]) => [name, adoptIds(readUids(name), schema, true)]));

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
  /// A pending "pan the canvas to this node" request (e.g. from global search),
  /// consumed and cleared by the canvas once the node is on screen. Addressed
  /// by path; the canvas resolves it to the node id.
  focus: { index: string; path: number[] } | null;

  setCatalog: (catalog: CatalogResponse | null) => void;
  setSaved: (saved: string) => void;
  setPreview: (preview: PreviewResponse | null) => void;
  setDiagnostics: (diagnostics: DiagnosticDto[] | null) => void;
  setActive: (active: string) => void;
  setSelection: (selection: Selection) => void;

  loadProject: (project: Project, resetActive: boolean) => void;
  apply: (fn: (s: IndexSchema) => IndexSchema) => void;
  /// `apply` on the index `index`, whichever is active now (an edit that lands
  /// after its index was left).
  applyTo: (index: string, fn: (s: IndexSchema) => IndexSchema) => void;
  setConfig: (next: ConfigToml) => void;
  openIndex: (name: string) => void;
  createIndex: (name: string, table: string, schemaPath: string) => void;
  dupIndex: (i: number) => void;
  revertChanges: () => void;

  loadCollapsed: (index: string) => void;
  toggleCollapsed: (id: string) => void;
  collapseAll: () => void;
  expandAll: () => void;

  requestFocus: (index: string, path: number[]) => void;
  clearFocus: () => void;
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
      focus: null,

      setCatalog: (catalog) => set({ catalog }),
      setSaved: (saved) => {
        persistUids(saved);
        set({ saved });
      },
      setPreview: (preview) => set({ preview }),
      setDiagnostics: (diagnostics) => set({ diagnostics }),
      setActive: (active) => set({ active }),
      setSelection: (selection) => set({ selection }),

      loadProject: (project, resetActive) => {
        const schemas: Record<string, IndexSchema> = {};
        for (const idx of project.indexes) if (idx.schema) schemas[idx.name] = idx.schema;
        // Stamp each entry with a stable id so the save op set can correlate it
        // across later renames / path moves.
        const config = {
          ...project.config,
          index: (project.config.index ?? []).map((e) => ({ ...e, id: newId() })),
        };
        const doc: Doc = { config, schemas: stampAll(schemas) };
        const saved = JSON.stringify(doc);
        persistUids(saved);
        for (const [name, schema] of Object.entries(doc.schemas)) reconcileViewState(name, projectGraph(schema));
        set({
          project,
          doc,
          saved,
          ...(resetActive ? { active: project.indexes[0]?.name ?? "config" } : {}),
        });
        useDesignStore.temporal.getState().clear();
      },

      apply: (fn) => get().applyTo(get().active, fn),

      applyTo: (index, fn) => {
        if (index === "config") return;
        set((s) => {
          if (!s.doc || !(index === s.active || index in s.doc.schemas)) return {};
          const current = s.doc.schemas[index];
          const next = stampIds(fn(current ?? emptySchema("")));
          return next === current ? {} : { doc: { ...s.doc, schemas: { ...s.doc.schemas, [index]: next } } };
        });
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

        // A renamed index whose file still tracks the default name follows the
        // rename (Create/Duplicate both derive `<name>.schema.yml`); a custom
        // schema path is left untouched.
        const index = newIdx.map((entry, i) => {
          const old = oldIdx[i];
          // Backfill an id for a row added in the Config panel, so every entry
          // carries one for op-set correlation.
          const withId = entry.id ? entry : { ...entry, id: newId() };
          return old && renames.get(old.name) === withId.name && withId.schema === `${old.name}.schema.yml`
            ? { ...withId, schema: `${withId.name}.schema.yml` }
            : withId;
        });

        const active2 = renames.has(active) ? renames.get(active)! : removed.includes(active) ? "config" : active;
        set({ doc: { config: { ...next, index }, schemas }, active: active2 });
      },

      openIndex: (name) => set({ active: name, selection: null, diagnostics: null }),

      createIndex: (name, table, schemaPath) => {
        const { doc, catalog } = get();
        if (!doc) return;
        const pk = catalog?.catalog.tables.find((t) => t.name === table)?.primary_key[0];
        const schema = schemaPath.trim() || `${name}.schema.yml`;
        set({
          doc: {
            config: {
              ...doc.config,
              index: [...(doc.config.index ?? []), { name, schema, enabled: true, id: newId() }],
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
                { name, schema: `${name}.schema.yml`, enabled: src.enabled, id: newId() },
                ...entries.slice(i + 1),
              ],
            },
            schemas: doc.schemas[src.name]
              ? { ...doc.schemas, [name]: stampIds(stripUids(cloneJson(doc.schemas[src.name]))) }
              : doc.schemas,
          },
          active: name,
          selection: { kind: "root" },
          diagnostics: null,
        });
      },

      revertChanges: () => {
        const { saved, active } = get();
        if (!saved) return;
        dropPendingValues();
        const doc = parseJson(saved) as Doc;
        // A reverted just-created index no longer exists; don't strand `active`
        // on it (that renders a blank canvas) — fall back to a real index/config.
        const active2 =
          active === "config" || doc.config.index?.some((e) => e.name === active)
            ? active
            : (doc.config.index?.[0]?.name ?? "config");
        set({ doc, selection: null, active: active2 });
      },

      loadCollapsed: (index) => {
        if (index === "config") return;
        set({ collapsed: new Set(readCollapsed(index)) });
      },

      toggleCollapsed: (id) => {
        const { active, collapsed } = get();
        const next = new Set(collapsed);
        if (next.has(id)) next.delete(id);
        else next.add(id);
        persistCollapsed(active, next);
        set({ collapsed: next });
      },

      collapseAll: () => {
        const { active, doc } = get();
        const schema = doc?.schemas[active];
        if (!schema) return;
        const next = new Set(projectGraph(schema).nodes.map((n) => n.id));
        persistCollapsed(active, next);
        set({ collapsed: next });
      },

      expandAll: () => {
        const { active } = get();
        const next = new Set<string>();
        persistCollapsed(active, next);
        set({ collapsed: next });
      },

      requestFocus: (index, path) => set({ focus: { index, path } }),
      clearFocus: () => set({ focus: null }),
    }),
    {
      // Track only `doc`; reference equality keeps no-op edits and selection/active changes off the stack.
      limit: HISTORY_LIMIT,
      partialize: (s) => ({ doc: s.doc }),
      equality: (a, b) => a.doc === b.doc,
    },
  ),
);

// A value still being read joins the history first, so undo takes it back.
export const undo = () => void flushPendingValues().then(() => useDesignStore.temporal.getState().undo());
export const redo = () => void flushPendingValues().then(() => useDesignStore.temporal.getState().redo());

export const useCanUndo = () => useStore(useDesignStore.temporal, (s) => s.pastStates.length > 0);
export const useCanRedo = () => useStore(useDesignStore.temporal, (s) => s.futureStates.length > 0);
