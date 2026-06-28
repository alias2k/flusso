import { useEffect, useMemo, useState } from "react";
import type {
  CatalogResponse,
  ColumnShape,
  ConfigToml,
  DiagnosticDto,
  IndexSchema,
  PreviewResponse,
  Project,
} from "./api";
import { api } from "./api";
import { Canvas } from "./components/Canvas";
import { ConfigPanel } from "./components/ConfigPanel";
import { Icon } from "./components/Icon";
import { Inspector } from "./components/Inspector";
import { Preview } from "./components/Preview";
import { Select, Text } from "./components/widgets";
import { useHistory } from "./history";
import { DesignProvider, type Selection } from "./state";

/// The whole editable document: the deployment config + every index's schema.
/// Held as one value so undo/redo and dirty-tracking cover it uniformly.
interface Doc {
  config: ConfigToml;
  schemas: Record<string, IndexSchema>;
}

const errText = (e: unknown): string => (e instanceof Error ? e.message : String(e));

const emptySchema = (table: string, pk?: string): IndexSchema => ({
  version: 1,
  table,
  db_schema: "public",
  primary_key: pk,
  fields: [],
});

export default function App() {
  const [project, setProject] = useState<Project | null>(null);
  const [catalog, setCatalog] = useState<CatalogResponse | null>(null);
  const { present: doc, set: setDoc, undo, redo, reset, canUndo, canRedo } = useHistory<Doc | null>(null);
  const [saved, setSaved] = useState<string>(""); // JSON of the last loaded/saved doc
  const [active, setActive] = useState<string>("config");
  const [selection, setSelection] = useState<Selection>(null);
  const [leftOpen, setLeftOpen] = useState(true);
  const [preview, setPreview] = useState<PreviewResponse | null>(null);
  const [diagnostics, setDiagnostics] = useState<DiagnosticDto[] | null>(null);
  const [drawer, setDrawer] = useState(false);
  const [error, setError] = useState("");
  const [toast, setToast] = useState<{ kind: "ok" | "error" | "info"; text: string } | null>(null);
  const [saving, setSaving] = useState(false);
  const [validating, setValidating] = useState(false);

  // Auto-dismiss toasts.
  useEffect(() => {
    if (!toast) return;
    const t = setTimeout(() => setToast(null), 3500);
    return () => clearTimeout(t);
  }, [toast]);

  useEffect(() => {
    api
      .project()
      .then((p) => {
        setProject(p);
        const schemas: Record<string, IndexSchema> = {};
        for (const idx of p.indexes) if (idx.schema) schemas[idx.name] = idx.schema;
        const initial: Doc = { config: p.config, schemas };
        reset(initial);
        setSaved(JSON.stringify(initial));
        setActive(p.indexes[0]?.name ?? "config");
      })
      .catch((e) => setError(String(e)));
    api.catalog().then(setCatalog).catch((e) => setError(String(e)));
  }, [reset]);

  const columnsFor = useMemo(() => {
    const tables = catalog?.catalog.tables ?? [];
    return (table: string): ColumnShape[] => tables.find((t) => t.name === table)?.columns ?? [];
  }, [catalog]);

  const dirty = !!doc && JSON.stringify(doc) !== saved;
  const indexDirty = (name: string): boolean => {
    if (!doc || !saved) return false;
    const savedDoc = JSON.parse(saved) as Doc;
    return JSON.stringify(doc.schemas[name]) !== JSON.stringify(savedDoc.schemas[name]);
  };

  const config = doc?.config;
  const schema = doc && active !== "config" ? doc.schemas[active] : undefined;
  const inspectorOpen = active !== "config" && !!schema && selection !== null;

  // Debounced live preview of the active index.
  useEffect(() => {
    if (!schema || active === "config") {
      setPreview(null);
      return;
    }
    const handle = setTimeout(() => {
      api.preview(active, schema).then(setPreview).catch((e) => setError(String(e)));
    }, 250);
    return () => clearTimeout(handle);
  }, [active, schema]);

  // Undo/redo keybindings — but only when not inside a text field, so native
  // text undo keeps working while you're typing.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (!(e.metaKey || e.ctrlKey) || e.key.toLowerCase() !== "z") return;
      const el = document.activeElement as HTMLElement | null;
      const editing =
        el && (el.tagName === "INPUT" || el.tagName === "TEXTAREA" || el.isContentEditable);
      if (editing) return;
      e.preventDefault();
      e.shiftKey ? redo() : undo();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [undo, redo]);

  // Warn before leaving with unsaved changes.
  useEffect(() => {
    if (!dirty) return;
    const warn = (e: BeforeUnloadEvent) => {
      e.preventDefault();
      e.returnValue = "";
    };
    window.addEventListener("beforeunload", warn);
    return () => window.removeEventListener("beforeunload", warn);
  }, [dirty]);

  const apply = (fn: (s: IndexSchema) => IndexSchema) => {
    if (active === "config") return;
    setDoc((d) =>
      d ? { ...d, schemas: { ...d.schemas, [active]: fn(d.schemas[active] ?? emptySchema("")) } } : d,
    );
  };
  const setConfig = (next: ConfigToml) => setDoc((d) => (d ? { ...d, config: next } : d));

  const openIndex = (name: string) => {
    setActive(name);
    setSelection({ kind: "root" });
    setDiagnostics(null);
  };

  const save = async () => {
    if (!doc || saving) return;
    setSaving(true);
    try {
      const indexes = (doc.config.index ?? [])
        .filter((e) => doc.schemas[e.name])
        .map((e) => ({ schema_path: e.schema, schema: doc.schemas[e.name] }));
      const res = await api.save(doc.config, indexes);
      setSaved(JSON.stringify(doc));
      setToast({ kind: "ok", text: `Saved ${res.written.length} file(s)` });
    } catch (e) {
      setToast({ kind: "error", text: `Save failed: ${errText(e)}` });
    } finally {
      setSaving(false);
    }
  };

  const validate = async () => {
    if (!doc || validating) return;
    setValidating(true);
    try {
      const indexes = Object.entries(doc.schemas).map(([name, s]) => ({ name, schema: s }));
      const res = await api.validate(doc.config, indexes);
      if (!res.db_reachable) {
        setDiagnostics(null);
        setToast({ kind: "error", text: `Database not reachable: ${res.error ?? "unknown"}` });
      } else {
        setDiagnostics(res.diagnostics);
        if (res.diagnostics.length) {
          setToast({ kind: "error", text: `${res.diagnostics.length} issue(s) — see the highlighted fields` });
          setDrawer(true);
        } else {
          setToast({ kind: "ok", text: "Schemas match the database" });
        }
      }
    } catch (e) {
      setToast({ kind: "error", text: `Validate failed: ${errText(e)}` });
    } finally {
      setValidating(false);
    }
  };

  const createIndex = (name: string, table: string) => {
    const pk = catalog?.catalog.tables.find((t) => t.name === table)?.primary_key[0];
    setDoc((d) =>
      d
        ? {
            config: { ...d.config, index: [...(d.config.index ?? []), { name, schema: `${name}.schema.yml`, enabled: true }] },
            schemas: { ...d.schemas, [name]: emptySchema(table, pk) },
          }
        : d,
    );
    openIndex(name);
  };

  if (!project || !doc || !config) return <div className="loading">{error || "Loading project…"}</div>;

  return (
    <div className="app">
      <header className="topbar">
        <button className="icon" title={leftOpen ? "Hide sidebar" : "Show sidebar"} onClick={() => setLeftOpen((o) => !o)}>
          <Icon name="menu" />
        </button>
        <span className="brand">
          <span className="brand-mark">
            <Icon name="flow" size={18} />
          </span>
          flusso
        </span>
        <span className="path">{project.config_path}</span>
        <span className="spacer" />
        <button className="icon" title="Undo (⌘Z)" disabled={!canUndo} onClick={undo}>
          <Icon name="undo" />
        </button>
        <button className="icon" title="Redo (⇧⌘Z)" disabled={!canRedo} onClick={redo}>
          <Icon name="redo" />
        </button>
        <button onClick={() => setDrawer((d) => !d)}>{drawer ? "Hide" : "YAML"}</button>
        <button onClick={validate} disabled={validating}>
          {validating && <span className="spinner" />}
          Validate
        </button>
        <button className="primary" onClick={save} disabled={saving} title={dirty ? "Unsaved changes" : "Up to date"}>
          {saving ? <span className="spinner" /> : dirty && <span className="dirty-dot" />}
          Save
        </button>
      </header>

      {error && <div className="banner error">{error}</div>}
      {catalog?.error && <div className="banner warn">Database not reachable — offline authoring only.</div>}

      <div
        className="layout"
        style={{ gridTemplateColumns: `${leftOpen ? "210px" : "0"} 1fr ${inspectorOpen ? "360px" : "0"}` }}
      >
        {leftOpen && (
          <nav className="sidebar">
            <button className={active === "config" ? "nav active" : "nav"} onClick={() => setActive("config")}>
              ⚙ Deployment
            </button>
            <div className="nav-heading">Indexes</div>
            {(config.index ?? []).map((e) => (
              <button key={e.name} className={active === e.name ? "nav active" : "nav"} onClick={() => openIndex(e.name)}>
                {indexDirty(e.name) && <span className="dirty-dot" />}
                {e.name}
                {!e.enabled && <span className="muted"> (off)</span>}
              </button>
            ))}
            <NewIndex tables={catalog?.catalog.tables.map((t) => t.name) ?? []} onCreate={createIndex} />
          </nav>
        )}

        {active === "config" ? (
          <main className="editor">
            <ConfigPanel config={config} onChange={setConfig} />
          </main>
        ) : schema ? (
          <DesignProvider
            value={{
              catalog,
              schema,
              indexName: active,
              apply,
              selection,
              select: setSelection,
              columnsFor,
              diagnostics: (diagnostics ?? []).filter((d) => d.index === active),
            }}
          >
            <main className="canvas-wrap">
              <Canvas />
              {drawer && (
                <div className="drawer">
                  <Preview preview={preview} diagnostics={diagnostics} />
                </div>
              )}
            </main>
            {inspectorOpen && (
              <aside className="inspector-pane">
                <button className="icon collapse" title="Close" onClick={() => setSelection(null)}>
                  <Icon name="close" />
                </button>
                <Inspector />
              </aside>
            )}
          </DesignProvider>
        ) : null}
      </div>

      {toast && (
        <div className={`toast ${toast.kind}`} onClick={() => setToast(null)}>
          {toast.text}
        </div>
      )}
    </div>
  );
}

function NewIndex({ tables, onCreate }: { tables: string[]; onCreate: (name: string, table: string) => void }) {
  const [open, setOpen] = useState(false);
  const [name, setName] = useState("");
  const [table, setTable] = useState(tables[0] ?? "");
  if (!open) {
    return (
      <button className="nav new" onClick={() => setOpen(true)}>
        + New index
      </button>
    );
  }
  return (
    <div className="new-index">
      <Text value={name} onChange={setName} placeholder="index name" />
      {tables.length ? (
        <Select value={table} options={tables} onChange={setTable} />
      ) : (
        <Text value={table} onChange={setTable} placeholder="root table" />
      )}
      <div className="row">
        <button
          className="primary"
          disabled={!name || !table}
          onClick={() => {
            onCreate(name, table);
            setOpen(false);
            setName("");
          }}
        >
          Create
        </button>
        <button onClick={() => setOpen(false)}>Cancel</button>
      </div>
    </div>
  );
}
