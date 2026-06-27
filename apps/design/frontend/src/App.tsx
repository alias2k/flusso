import { useEffect, useMemo, useState } from "react";
import type {
  CatalogResponse,
  DiagnosticDto,
  IndexSchema,
  PreviewResponse,
  Project,
  SoftDelete,
} from "./api";
import { api } from "./api";
import { CatalogCtx, NestedFields } from "./components/FieldEditor";
import { Filters } from "./components/Filters";
import { ConfigPanel } from "./components/ConfigPanel";
import { Preview } from "./components/Preview";
import { Field, Select, Text } from "./components/widgets";

const EMPTY_SCHEMA = (): IndexSchema => ({ version: 1, table: "", db_schema: "public", fields: [] });

export default function App() {
  const [project, setProject] = useState<Project | null>(null);
  const [catalog, setCatalog] = useState<CatalogResponse | null>(null);
  const [schemas, setSchemas] = useState<Record<string, IndexSchema>>({});
  const [config, setConfig] = useState<Project["config"] | null>(null);
  const [selected, setSelected] = useState<string | null>(null);
  const [preview, setPreview] = useState<PreviewResponse | null>(null);
  const [diagnostics, setDiagnostics] = useState<DiagnosticDto[] | null>(null);
  const [status, setStatus] = useState<string>("");
  const [error, setError] = useState<string>("");

  useEffect(() => {
    api
      .project()
      .then((p) => {
        setProject(p);
        setConfig(p.config);
        const map: Record<string, IndexSchema> = {};
        for (const idx of p.indexes) if (idx.schema) map[idx.name] = idx.schema;
        setSchemas(map);
        setSelected(p.indexes[0]?.name ?? "config");
      })
      .catch((e) => setError(String(e)));
    api.catalog().then(setCatalog).catch((e) => setError(String(e)));
  }, []);

  const ctx: CatalogCtx = useMemo(() => {
    const tables = catalog?.catalog.tables ?? [];
    return {
      tables,
      columnsFor: (t: string) => tables.find((x) => x.name === t)?.columns.map((c) => c.name) ?? [],
    };
  }, [catalog]);

  const schema = selected && selected !== "config" ? schemas[selected] ?? EMPTY_SCHEMA() : null;

  // Debounced live preview of the selected schema.
  useEffect(() => {
    if (!selected || selected === "config" || !schema) {
      setPreview(null);
      return;
    }
    const handle = setTimeout(() => {
      api.preview(selected, schema).then(setPreview).catch((e) => setError(String(e)));
    }, 250);
    return () => clearTimeout(handle);
  }, [selected, schema]);

  const updateSchema = (next: IndexSchema) => {
    if (!selected || selected === "config") return;
    setSchemas((s) => ({ ...s, [selected]: next }));
  };

  const save = async () => {
    if (!project || !config) return;
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
    }
  };

  if (!project || !config) {
    return <div className="loading">{error || "Loading project…"}</div>;
  }

  return (
    <div className="app">
      <header className="topbar">
        <span className="brand">flusso designer</span>
        <span className="path">{project.config_path}</span>
        <span className="spacer" />
        {status && <span className="status">{status}</span>}
        <button onClick={validate}>Validate against DB</button>
        <button className="primary" onClick={save}>
          Save
        </button>
      </header>

      {error && <div className="banner error">{error}</div>}
      {catalog?.error && (
        <div className="banner warn">Database not reachable — offline authoring only. ({catalog.error})</div>
      )}

      <div className="layout">
        <nav className="sidebar">
          <button className={selected === "config" ? "nav active" : "nav"} onClick={() => setSelected("config")}>
            ⚙ Deployment
          </button>
          <div className="nav-heading">Indexes</div>
          {(config.index ?? []).map((e) => (
            <button key={e.name} className={selected === e.name ? "nav active" : "nav"} onClick={() => setSelected(e.name)}>
              {e.name}
              {!e.enabled && <span className="muted"> (disabled)</span>}
            </button>
          ))}
        </nav>

        <main className="editor">
          {selected === "config" ? (
            <ConfigPanel config={config} onChange={setConfig} />
          ) : schema ? (
            <IndexEditor schema={schema} onChange={updateSchema} ctx={ctx} name={selected!} />
          ) : null}
        </main>

        <aside className="preview-pane">
          {selected === "config" ? (
            <div className="preview empty">Pick an index to preview its document.</div>
          ) : (
            <Preview preview={preview} diagnostics={diagnostics} />
          )}
        </aside>
      </div>
    </div>
  );
}

function IndexEditor({
  schema,
  onChange,
  ctx,
  name,
}: {
  schema: IndexSchema;
  onChange: (s: IndexSchema) => void;
  ctx: CatalogCtx;
  name: string;
}) {
  const tableNames = ctx.tables.map((t) => t.name);
  const cols = ctx.columnsFor(schema.table);

  return (
    <div className="index-editor">
      <h2>{name}</h2>
      <div className="row">
        <Field label="root table">
          <Text value={schema.table} onChange={(table) => onChange({ ...schema, table })} list={tableNames} />
        </Field>
        <Field label="schema">
          <Text value={schema.db_schema} onChange={(db_schema) => onChange({ ...schema, db_schema })} placeholder="public" />
        </Field>
        <Field label="primary_key">
          <Text value={schema.primary_key ?? ""} onChange={(pk) => onChange({ ...schema, primary_key: pk || undefined })} list={cols} />
        </Field>
      </div>

      <SoftDeleteEditor value={schema.soft_delete} onChange={(soft_delete) => onChange({ ...schema, soft_delete })} cols={cols} />

      <details>
        <summary>root filters</summary>
        <Filters value={schema.filters ?? []} onChange={(filters) => onChange({ ...schema, filters })} columns={cols} />
      </details>

      <h3>Fields</h3>
      <NestedFields fields={schema.fields} onChange={(fields) => onChange({ ...schema, fields })} ctx={ctx} table={schema.table} />
    </div>
  );
}

function SoftDeleteEditor({
  value,
  onChange,
  cols,
}: {
  value: SoftDelete | undefined;
  onChange: (v: SoftDelete | undefined) => void;
  cols: string[];
}) {
  const kind = value === undefined ? "none" : "field" in value ? "field" : "column";
  return (
    <div className="soft-delete">
      <Field label="soft delete">
        <Select
          value={kind}
          onChange={(k) => {
            if (k === "none") onChange(undefined);
            else if (k === "field") onChange({ field: "" });
            else onChange({ column: "" });
          }}
          options={["none", "field", "column"]}
        />
      </Field>
      {value && "field" in value && (
        <Text value={value.field} onChange={(field) => onChange({ ...value, field })} placeholder="document field" />
      )}
      {value && "column" in value && (
        <Text value={value.column} onChange={(column) => onChange({ ...value, column })} list={cols} placeholder="column" />
      )}
    </div>
  );
}
