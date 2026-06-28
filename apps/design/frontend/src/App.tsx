import { useEffect, useMemo, useRef, useState } from "react";
import type {
  CatalogResponse,
  ColumnShape,
  ConfigToml,
  DiagnosticDto,
  FileDiff,
  IndexSchema,
  PreviewResponse,
  Project,
  SaveSchemaInput,
} from "./api";
import { api } from "./api";
import { Canvas } from "./components/Canvas";
import { CatalogBrowser } from "./components/CatalogBrowser";
import { ConfigPanel } from "./components/ConfigPanel";
import { Icon } from "./components/Icon";
import { Inspector } from "./components/Inspector";
import { Preview } from "./components/Preview";
import { Select, Text } from "./components/widgets";
import { useHistory } from "./history";
import { LANGS, useT } from "./i18n";
import { removeAt, removeNode } from "./model/edit";
import { requiredDefaultIssues } from "./model/issues";
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
  const { t, lang, setLang } = useT();
  const [project, setProject] = useState<Project | null>(null);
  const [catalog, setCatalog] = useState<CatalogResponse | null>(null);
  const { present: doc, set: setDoc, undo, redo, reset, canUndo, canRedo } = useHistory<Doc | null>(null);
  const [saved, setSaved] = useState<string>(""); // JSON of the last loaded/saved doc
  const [active, setActive] = useState<string>("config");
  const [selection, setSelection] = useState<Selection>(null);
  const [collapsed, setCollapsed] = useState<Set<string>>(new Set());
  const [leftOpen, setLeftOpen] = useState(true);
  const [preview, setPreview] = useState<PreviewResponse | null>(null);
  const [diagnostics, setDiagnostics] = useState<DiagnosticDto[] | null>(null);
  const [drawer, setDrawer] = useState(false);
  const [error, setError] = useState("");
  const [toast, setToast] = useState<{ kind: "ok" | "error" | "info"; text: string } | null>(null);
  const [saving, setSaving] = useState(false);
  const [validating, setValidating] = useState(false);
  const [rawMode, setRawMode] = useState(false);
  const [rawText, setRawText] = useState("");
  const [diffs, setDiffs] = useState<FileDiff[] | null>(null);
  const [browseCatalog, setBrowseCatalog] = useState(false);
  const [theme, setTheme] = useState<"dark" | "light">(
    () => (localStorage.getItem("flusso-design.theme") as "dark" | "light") || "dark",
  );

  useEffect(() => {
    document.documentElement.dataset.theme = theme;
    try {
      localStorage.setItem("flusso-design.theme", theme);
    } catch {
      /* storage disabled */
    }
  }, [theme]);

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

  // Test the *currently edited* connection (not the on-disk one).
  const refreshCatalog = () =>
    (doc ? api.testConnection(doc.config) : api.catalog())
      .then((c) => {
        setCatalog(c);
        setToast({ kind: c.error ? "error" : "ok", text: c.error ? t("toast.dbNotReachable") : t("toast.dbConnected") });
      })
      .catch((e) => setToast({ kind: "error", text: errText(e) }));

  // Re-read everything from disk (after a raw save), keeping the active index.
  const reloadProject = () =>
    api
      .project()
      .then((p) => {
        setProject(p);
        const schemas: Record<string, IndexSchema> = {};
        for (const idx of p.indexes) if (idx.schema) schemas[idx.name] = idx.schema;
        const fresh: Doc = { config: p.config, schemas };
        reset(fresh);
        setSaved(JSON.stringify(fresh));
      })
      .catch((e) => setToast({ kind: "error", text: errText(e) }));

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

  // Database diagnostics (from Validate) plus always-on, catalog-only schema
  // checks (e.g. required-over-nullable needs a default) — same channel, so
  // both highlight the fields and list in the preview.
  const allDiagnostics = useMemo(() => {
    const live = (schema ? requiredDefaultIssues(schema, catalog, active) : []) as DiagnosticDto[];
    return [...(diagnostics ?? []), ...live];
  }, [schema, catalog, active, diagnostics]);

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

  // Keyboard shortcuts. Held in a ref so the listener subscribes once but always
  // sees the latest state; undo/delete are suppressed while typing in a field so
  // native text editing keeps working.
  const keyHandler = useRef<(e: KeyboardEvent) => void>(() => {});
  keyHandler.current = (e: KeyboardEvent) => {
    const el = document.activeElement as HTMLElement | null;
    const editing = !!el && (el.tagName === "INPUT" || el.tagName === "TEXTAREA" || el.isContentEditable);
    const mod = e.metaKey || e.ctrlKey;

    if (mod && e.key.toLowerCase() === "s") {
      e.preventDefault();
      void save();
      return;
    }
    if (mod && e.key.toLowerCase() === "z") {
      if (editing) return;
      e.preventDefault();
      e.shiftKey ? redo() : undo();
      return;
    }
    if (e.key === "Escape") {
      setSelection(null);
      return;
    }
    if ((e.key === "Delete" || e.key === "Backspace") && !editing && selection) {
      if (selection.kind === "node" && selection.path.length > 0) {
        apply((s) => removeNode(s, selection.path));
        setSelection(null);
      } else if (selection.kind === "field") {
        apply((s) => removeAt(s, selection.path, selection.index));
        setSelection(null);
      }
    }
  };
  useEffect(() => {
    const fn = (e: KeyboardEvent) => keyHandler.current(e);
    window.addEventListener("keydown", fn);
    return () => window.removeEventListener("keydown", fn);
  }, []);

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
  // Editing the index list can rename or remove indexes. Schemas are keyed by
  // index name, so re-key renamed ones (matched by position) and drop removed
  // ones — otherwise a renamed index's schema is silently lost on save.
  const setConfig = (next: ConfigToml) => {
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

    setDoc((d) => {
      if (!d) return d;
      const schemas = { ...d.schemas };
      for (const [oldName, newName] of renames) {
        if (oldName in schemas) {
          schemas[newName] = schemas[oldName];
          delete schemas[oldName];
        }
      }
      for (const name of removed) delete schemas[name];
      return { config: next, schemas };
    });

    if (renames.has(active)) setActive(renames.get(active)!);
    else if (removed.includes(active)) setActive("config");
  };

  // Collapsed-node ids persist per index (like layout) so they survive reloads.
  const collapseKey = (index: string) => `flusso-design.collapsed.${index}`;
  useEffect(() => {
    if (active === "config") return;
    try {
      setCollapsed(new Set(JSON.parse(localStorage.getItem(collapseKey(active)) ?? "[]") as string[]));
    } catch {
      setCollapsed(new Set());
    }
  }, [active]);
  const toggleCollapsed = (id: string) => {
    setCollapsed((prev) => {
      const next = new Set(prev);
      next.has(id) ? next.delete(id) : next.add(id);
      try {
        localStorage.setItem(collapseKey(active), JSON.stringify([...next]));
      } catch {
        /* storage disabled — collapse just won't persist */
      }
      return next;
    });
  };

  const openIndex = (name: string) => {
    setActive(name);
    setSelection({ kind: "root" });
    setDiagnostics(null);
  };

  const saveIndexes = (): SaveSchemaInput[] =>
    (doc?.config.index ?? [])
      .filter((e) => doc?.schemas[e.name])
      .map((e) => ({ schema_path: e.schema, schema: doc!.schemas[e.name] }));

  // Save first shows a diff of what would change on disk; performSave writes it.
  const save = async () => {
    if (!doc || saving) return;
    setSaving(true);
    try {
      const result = await api.diff(doc.config, saveIndexes());
      if (result.some((d) => d.changed)) setDiffs(result);
      else setToast({ kind: "ok", text: t("toast.alreadyUpToDate") });
    } catch (e) {
      setToast({ kind: "error", text: t("toast.diffFailed", { err: errText(e) }) });
    } finally {
      setSaving(false);
    }
  };

  const performSave = async () => {
    if (!doc) return;
    setSaving(true);
    try {
      const res = await api.save(doc.config, saveIndexes());
      setSaved(JSON.stringify(doc));
      setDiffs(null);
      setToast({ kind: "ok", text: t("toast.saved", { n: res.written.length }) });
    } catch (e) {
      setToast({ kind: "error", text: t("toast.saveFailed", { err: errText(e) }) });
    } finally {
      setSaving(false);
    }
  };

  // Raw-YAML escape hatch: write the active index's file verbatim, then reload.
  const openRaw = () => {
    const onDisk = project?.indexes.find((i) => i.name === active)?.raw;
    setRawText(preview?.yaml ?? onDisk ?? "");
    setRawMode(true);
  };
  const saveRaw = async () => {
    if (!doc || active === "config" || saving) return;
    const entry = doc.config.index?.find((e) => e.name === active);
    if (!entry) return;
    setSaving(true);
    try {
      await api.save(doc.config, [{ schema_path: entry.schema, schema: doc.schemas[active] ?? emptySchema(""), raw: rawText }]);
      setRawMode(false);
      await reloadProject();
      setToast({ kind: "ok", text: t("toast.savedRaw") });
    } catch (e) {
      setToast({ kind: "error", text: t("toast.saveFailed", { err: errText(e) }) });
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
        setToast({ kind: "error", text: t("toast.dbNotReachableErr", { err: res.error ?? "unknown" }) });
      } else if (res.error) {
        // Reachable, but validation itself failed — not a connectivity problem.
        setDiagnostics(null);
        setToast({ kind: "error", text: t("toast.validateFailed", { err: res.error }) });
      } else {
        setDiagnostics(res.diagnostics);
        if (res.diagnostics.length) {
          setToast({ kind: "error", text: t("toast.issues", { n: res.diagnostics.length }) });
          setDrawer(true);
        } else {
          setToast({ kind: "ok", text: t("toast.schemasMatch") });
        }
      }
    } catch (e) {
      setToast({ kind: "error", text: t("toast.validateFailed", { err: errText(e) }) });
    } finally {
      setValidating(false);
    }
  };

  const dupIndex = (i: number) => {
    if (!doc) return;
    const entries = doc.config.index ?? [];
    const src = entries[i];
    if (!src) return;
    let name = `${src.name}_copy`;
    let n = 1;
    while (entries.some((e) => e.name === name)) name = `${src.name}_copy${++n}`;
    setDoc((d) =>
      d
        ? {
            config: {
              ...d.config,
              index: [
                ...entries.slice(0, i + 1),
                { name, schema: `${name}.schema.yml`, enabled: src.enabled },
                ...entries.slice(i + 1),
              ],
            },
            schemas: d.schemas[src.name]
              ? { ...d.schemas, [name]: structuredClone(d.schemas[src.name]) }
              : d.schemas,
          }
        : d,
    );
    openIndex(name);
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
        <button
          className="icon tip-below"
          data-tip={leftOpen ? t("topbar.hideSidebar") : t("topbar.showSidebar")}
          aria-label={leftOpen ? t("topbar.hideSidebar") : t("topbar.showSidebar")}
          onClick={() => setLeftOpen((o) => !o)}
        >
          <Icon name="menu" />
        </button>
        <span className="brand">
          <span className="brand-mark">
            <Icon name="flow" size={18} />
          </span>
          flusso
        </span>
        <span className="path">{project.config_path}</span>
        <button
          className={`db-chip tip-below ${catalog && !catalog.error ? "ok" : "off"}`}
          data-tip={t("topbar.retestDb")}
          onClick={refreshCatalog}
        >
          {catalog && !catalog.error ? t("topbar.dbConnected") : t("topbar.dbOffline")}
        </button>
        <span className="spacer" />
        <button onClick={() => setBrowseCatalog(true)} title={t("topbar.browseDb")}>
          {t("topbar.tables")}
        </button>
        {active !== "config" && (
          <button onClick={() => (rawMode ? setRawMode(false) : openRaw())}>{rawMode ? t("topbar.visual") : t("topbar.rawYaml")}</button>
        )}
        <button className="icon tip-below" data-tip={t("topbar.undo")} aria-label={t("topbar.undo")} disabled={!canUndo} onClick={undo}>
          <Icon name="undo" />
        </button>
        <button className="icon tip-below" data-tip={t("topbar.redo")} aria-label={t("topbar.redo")} disabled={!canRedo} onClick={redo}>
          <Icon name="redo" />
        </button>
        <button
          className="icon tip-below"
          aria-label={t("topbar.toggleThemeAria")}
          data-tip={t("topbar.toggleTheme")}
          onClick={() => setTheme((th) => (th === "dark" ? "light" : "dark"))}
        >
          <Icon name="theme" />
        </button>
        <select className="lang-select" aria-label={t("topbar.language")} value={lang} onChange={(e) => setLang(e.target.value)}>
          {Object.entries(LANGS).map(([code, name]) => (
            <option key={code} value={code}>
              {name}
            </option>
          ))}
        </select>
        <button onClick={() => setDrawer((d) => !d)}>{drawer ? t("topbar.hide") : t("topbar.yaml")}</button>
        <button onClick={validate} disabled={validating}>
          {validating && <span className="spinner" />}
          {t("topbar.validate")}
        </button>
        <button className="primary" onClick={save} disabled={saving} title={dirty ? t("topbar.unsaved") : t("topbar.upToDate")}>
          {saving ? <span className="spinner" /> : dirty && <span className="dirty-dot" />}
          {t("topbar.save")}
        </button>
      </header>

      {error && <div className="banner error">{error}</div>}
      {catalog?.error && <div className="banner warn">{t("topbar.offlineBanner")}</div>}

      <div
        className="layout"
        style={{ gridTemplateColumns: `${leftOpen ? "210px" : "0"} 1fr ${inspectorOpen ? "360px" : "0"}` }}
      >
        {leftOpen && (
          <nav className="sidebar">
            <button className={active === "config" ? "nav active" : "nav"} onClick={() => setActive("config")}>
              ⚙ {t("sidebar.deployment")}
            </button>
            <div className="nav-heading">{t("sidebar.indexes")}</div>
            {(config.index ?? []).map((e) => (
              <button key={e.name} className={active === e.name ? "nav active" : "nav"} onClick={() => openIndex(e.name)}>
                {indexDirty(e.name) && <span className="dirty-dot" />}
                {e.name}
                {!e.enabled && <span className="muted"> {t("sidebar.off")}</span>}
              </button>
            ))}
            <NewIndex tables={catalog?.catalog.tables.map((tbl) => tbl.name) ?? []} onCreate={createIndex} />
            <div className="legend">
              <div className="nav-heading">{t("sidebar.kinds")}</div>
              {["root", "object", "belongs_to", "has_one", "has_many", "many_to_many"].map((k) => (
                <div className="legend-row" key={k}>
                  <span className={`legend-dot ${k}`} />
                  {k}
                </div>
              ))}
            </div>
          </nav>
        )}

        {active === "config" ? (
          <main className="editor">
            <ConfigPanel config={config} onChange={setConfig} onDuplicate={dupIndex} />
          </main>
        ) : rawMode ? (
          <main className="raw-pane">
            <div className="banner warn">
              {t("raw.editingFor")} <strong>{active}</strong> — {t("raw.help")}
            </div>
            <textarea className="raw-editor" value={rawText} onChange={(e) => setRawText(e.target.value)} spellCheck={false} />
            <div className="raw-actions">
              <button className="primary" onClick={saveRaw} disabled={saving}>
                {t("raw.save")}
              </button>
              <button onClick={() => setRawMode(false)}>{t("common.cancel")}</button>
            </div>
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
              diagnostics: allDiagnostics.filter((d) => d.index === active),
              collapsed,
              toggleCollapsed,
            }}
          >
            <main className="canvas-wrap">
              <Canvas />
              {drawer && (
                <div className="drawer">
                  <Preview
                    preview={preview}
                    diagnostics={allDiagnostics.filter((d) => d.index === active)}
                    onSample={
                      doc && schema && active !== "config"
                        ? () => api.sample(doc.config, active, schema)
                        : undefined
                    }
                  />
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

      {diffs && <DiffModal diffs={diffs} saving={saving} onConfirm={performSave} onCancel={() => setDiffs(null)} />}
      {browseCatalog && catalog && <CatalogBrowser catalog={catalog} onClose={() => setBrowseCatalog(false)} />}

      {toast && (
        <div className={`toast ${toast.kind}`} role="status" onClick={() => setToast(null)}>
          {toast.text}
        </div>
      )}
    </div>
  );
}

function DiffModal({
  diffs,
  saving,
  onConfirm,
  onCancel,
}: {
  diffs: FileDiff[];
  saving: boolean;
  onConfirm: () => void;
  onCancel: () => void;
}) {
  const { t } = useT();
  const changed = diffs.filter((d) => d.changed);
  const confirmRef = useRef<HTMLButtonElement>(null);
  useEffect(() => {
    confirmRef.current?.focus();
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onCancel();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onCancel]);
  return (
    <div className="modal-backdrop" onClick={onCancel}>
      <div className="modal" role="dialog" aria-modal="true" aria-label={t("diff.aria")} onClick={(e) => e.stopPropagation()}>
        <h3>{t("diff.title", { n: changed.length })}</h3>
        <div className="diff-list">
          {changed.map((d) => (
            <div className="diff-file" key={d.path}>
              <div className="diff-path">{d.path}</div>
              <div className="diff-cols">
                <pre className="yaml current">{d.current || t("diff.newFile")}</pre>
                <pre className="yaml next">{d.next}</pre>
              </div>
            </div>
          ))}
        </div>
        <div className="modal-actions">
          <button ref={confirmRef} className="primary" onClick={onConfirm} disabled={saving}>
            {saving && <span className="spinner" />}
            {t("diff.write", { n: changed.length })}
          </button>
          <button onClick={onCancel}>{t("common.cancel")}</button>
        </div>
      </div>
    </div>
  );
}

function NewIndex({ tables, onCreate }: { tables: string[]; onCreate: (name: string, table: string) => void }) {
  const { t } = useT();
  const [open, setOpen] = useState(false);
  const [name, setName] = useState("");
  const [table, setTable] = useState(tables[0] ?? "");
  if (!open) {
    return (
      <button className="nav new" onClick={() => setOpen(true)}>
        + {t("sidebar.newIndex")}
      </button>
    );
  }
  return (
    <div className="new-index">
      <Text value={name} onChange={setName} placeholder={t("sidebar.indexName")} />
      {tables.length ? (
        <Select value={table} options={tables} onChange={setTable} />
      ) : (
        <Text value={table} onChange={setTable} placeholder={t("sidebar.rootTable")} />
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
          {t("sidebar.create")}
        </button>
        <button onClick={() => setOpen(false)}>{t("common.cancel")}</button>
      </div>
    </div>
  );
}
