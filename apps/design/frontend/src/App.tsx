import { useEffect, useMemo, useState } from "react";
import type {
  CatalogResponse,
  ColumnShape,
  DiagnosticDto,
  IndexSchema,
  PreviewResponse,
  Project,
} from "./api";
import { api } from "./api";
import { Canvas } from "./components/Canvas";
import { ConfigPanel } from "./components/ConfigPanel";
import { Inspector } from "./components/Inspector";
import { Preview } from "./components/Preview";
import { Icon } from "./components/Icon";
import { Select, Text } from "./components/widgets";
import { DesignProvider, type Selection } from "./state";

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
  const [config, setConfig] = useState<Project["config"] | null>(null);
  const [schemas, setSchemas] = useState<Record<string, IndexSchema>>({});
  const [active, setActive] = useState<string>("config"); // "config" or an index name
  const [selection, setSelection] = useState<Selection>(null);
  const [leftOpen, setLeftOpen] = useState(true);
  const [preview, setPreview] = useState<PreviewResponse | null>(null);
  const [diagnostics, setDiagnostics] = useState<DiagnosticDto[] | null>(null);
  const [drawer, setDrawer] = useState(false);
  const [status, setStatus] = useState("");
  const [error, setError] = useState("");

  useEffect(() => {
    api
      .project()
      .then((p) => {
        setProject(p);
        setConfig(p.config);
        const map: Record<string, IndexSchema> = {};
        for (const idx of p.indexes) if (idx.schema) map[idx.name] = idx.schema;
        setSchemas(map);
        setActive(p.indexes[0]?.name ?? "config");
      })
      .catch((e) => setError(String(e)));
    api.catalog().then(setCatalog).catch((e) => setError(String(e)));
  }, []);

  const columnsFor = useMemo(() => {
    const tables = catalog?.catalog.tables ?? [];
    return (table: string): ColumnShape[] => tables.find((t) => t.name === table)?.columns ?? [];
  }, [catalog]);

  const schema = active !== "config" ? schemas[active] : undefined;
  // The inspector earns its column only when something's selected — otherwise the
  // canvas gets the room.
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

  const apply = (fn: (s: IndexSchema) => IndexSchema) => {
    if (active === "config") return;
    setSchemas((all) => ({ ...all, [active]: fn(all[active] ?? emptySchema("")) }));
  };

  const openIndex = (name: string) => {
    setActive(name);
    setSelection({ kind: "root" });
    setDiagnostics(null);
  };

  const save = async () => {
    if (!config) return;
    try {
      const indexes = (config.index ?? [])
        .filter((e) => schemas[e.name])
        .map((e) => ({ schema_path: e.schema, schema: schemas[e.name] }));
      const res = await api.save(config, indexes);
      setStatus(`Saved ${res.written.length} file(s).`);
      setError("");
    } catch (e) {
      setError(String(e));
    }
  };

  const validate = async () => {
    if (!config) return;
    const indexes = Object.entries(schemas).map(([name, s]) => ({ name, schema: s }));
    const res = await api.validate(config, indexes);
    if (!res.db_reachable) {
      setStatus(`Database not reachable: ${res.error}`);
      setDiagnostics(null);
    } else {
      setDiagnostics(res.diagnostics);
      setStatus(res.diagnostics.length ? `${res.diagnostics.length} issue(s) found.` : "Schemas match the database.");
      setDrawer(true);
    }
  };

  const createIndex = (name: string, table: string) => {
    const pk = catalog?.catalog.tables.find((t) => t.name === table)?.primary_key[0];
    setConfig((c) => (c ? { ...c, index: [...(c.index ?? []), { name, schema: `${name}.schema.yml`, enabled: true }] } : c));
    setSchemas((all) => ({ ...all, [name]: emptySchema(table, pk) }));
    openIndex(name);
  };

  if (!project || !config) return <div className="loading">{error || "Loading project…"}</div>;

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
        {status && <span className="status">{status}</span>}
        <button onClick={() => setDrawer((d) => !d)}>{drawer ? "Hide" : "YAML"}</button>
        <button onClick={validate}>Validate</button>
        <button className="primary" onClick={save}>
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
          <DesignProvider value={{ catalog, schema, indexName: active, apply, selection, select: setSelection, columnsFor }}>
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
