import { useEffect, useMemo, useRef, useState } from "react";
import { useDesignStore, useCanUndo, useCanRedo, undo, redo, emptySchema, type Doc } from "./store/design";
import { useUiStore } from "./store/ui";
import {
  ChevronRight,
  CircleAlert,
  CircleCheck,
  AlignJustify,
  Columns2,
  Eye,
  FileCode2,
  Minus,
  Moon,
  Plus,
  RotateCcw,
  Save,
  Search,
  Settings,
  Sun,
  Table2,
  TriangleAlert,
  X,
} from "lucide-react";
import type { ColumnShape, FileDiff, SaveSchemaInput, ValidateResponse } from "./api";
import { api } from "./api";
import { Canvas } from "./components/Canvas";
import { CatalogBrowser } from "./components/CatalogBrowser";
import { DiffView } from "./components/DiffView";
import { diffStats, type DiffMode } from "./model/diff";
import { CommandPalette } from "./components/CommandPalette";
import { ConfigPanel } from "./components/ConfigPanel";
import { Icon } from "./components/Icon";
import { Inspector } from "./components/Inspector";
import { Preview } from "./components/Preview";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { Dialog, DialogContent, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuRadioGroup,
  DropdownMenuRadioItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { Drawer, DrawerClose, DrawerContent, DrawerHeader, DrawerTitle } from "@/components/ui/drawer";
import { Kbd } from "@/components/ui/kbd";
import { Hint } from "./components/Hint";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { Textarea } from "@/components/ui/textarea";
import { cn } from "@/lib/utils";
import { Select, Text } from "./components/widgets";
import { LANGS, useT, type Translate } from "./i18n";
import { removeAt, removeNode } from "./model/edit";
import { countTypeMismatches, fixAllTypes, requiredDefaultIssues } from "./model/issues";
import { prunedForPreview } from "./model/prune";
import type { SearchRecord } from "./model/search";
import { DesignProvider } from "./state";
import { BTN_ICON, NAV, NAV_ACTIVE, NAV_HEADING } from "./styles";
import { TYPE_FAMILIES } from "./theme";

const errText = (e: unknown): string => (e instanceof Error ? e.message : String(e));

// `kindDesc`/`typeDesc` switch on literal `t("legend.*")` calls (not a dynamic
// lookup) so the i18n key-usage checker can see every key.
const KIND_ROWS = ["root", "object", "belongs_to", "has_one", "has_many", "many_to_many"];
const kindDesc = (t: Translate, kind: string): string => {
  switch (kind) {
    case "root":
      return t("legend.kindRoot");
    case "object":
      return t("legend.kindObject");
    case "belongs_to":
      return t("legend.kindBelongsTo");
    case "has_one":
      return t("legend.kindHasOne");
    case "has_many":
      return t("legend.kindHasMany");
    default:
      return t("legend.kindManyToMany");
  }
};
const typeDesc = (t: Translate, varKey: string): string => {
  switch (varKey) {
    case "string":
      return t("legend.typeString");
    case "number":
      return t("legend.typeNumber");
    case "temporal":
      return t("legend.typeDate");
    case "bool":
      return t("legend.typeBoolean");
    case "uuid":
      return t("legend.typeUuid");
    default:
      return t("legend.typeGeo");
  }
};

function LegendRow({ color, label, desc }: { color: string; label: string; desc: string }) {
  // Hovering anywhere on the full-width row opens the tooltip (controlled), but
  // it anchors to the content-width text trigger, so it sits by the label rather
  // than the row's far edge.
  const [open, setOpen] = useState(false);
  return (
    <div
      className={cn("legend-row rounded-md px-1.5 py-1", open && "bg-secondary")}
      onMouseEnter={() => setOpen(true)}
      onMouseLeave={() => setOpen(false)}
    >
      <Tooltip open={open}>
        <TooltipTrigger asChild>
          <div className="flex w-fit items-center gap-2 text-2xs text-muted-foreground select-none">
            <span className="inline-block size-2.5 shrink-0 rounded-full" style={{ background: color }} />
            {label}
          </div>
        </TooltipTrigger>
        <TooltipContent side="right" sideOffset={6} className="pointer-events-none max-w-52 leading-snug">
          {desc}
        </TooltipContent>
      </Tooltip>
    </div>
  );
}

export default function App() {
  const { t, lang, setLang } = useT();

  const project = useDesignStore((s) => s.project);
  const catalog = useDesignStore((s) => s.catalog);
  const doc = useDesignStore((s) => s.doc);
  const saved = useDesignStore((s) => s.saved);
  const active = useDesignStore((s) => s.active);
  const selection = useDesignStore((s) => s.selection);
  const collapsed = useDesignStore((s) => s.collapsed);
  const preview = useDesignStore((s) => s.preview);
  const diagnostics = useDesignStore((s) => s.diagnostics);
  const setCatalog = useDesignStore((s) => s.setCatalog);
  const setSaved = useDesignStore((s) => s.setSaved);
  const setActive = useDesignStore((s) => s.setActive);
  const setSelection = useDesignStore((s) => s.setSelection);
  const setPreview = useDesignStore((s) => s.setPreview);
  const setDiagnostics = useDesignStore((s) => s.setDiagnostics);
  const loadProject = useDesignStore((s) => s.loadProject);
  const apply = useDesignStore((s) => s.apply);
  const setConfig = useDesignStore((s) => s.setConfig);
  const openIndex = useDesignStore((s) => s.openIndex);
  const createIndex = useDesignStore((s) => s.createIndex);
  const dupIndex = useDesignStore((s) => s.dupIndex);
  const revertChanges = useDesignStore((s) => s.revertChanges);
  const loadCollapsed = useDesignStore((s) => s.loadCollapsed);
  const toggleCollapsed = useDesignStore((s) => s.toggleCollapsed);
  const canUndo = useCanUndo();
  const canRedo = useCanRedo();
  const [paletteOpen, setPaletteOpen] = useState(false);

  const theme = useUiStore((s) => s.theme);
  const leftOpen = useUiStore((s) => s.leftOpen);
  const drawer = useUiStore((s) => s.drawer);
  const error = useUiStore((s) => s.error);
  const toast = useUiStore((s) => s.toast);
  const saving = useUiStore((s) => s.saving);
  const validating = useUiStore((s) => s.validating);
  const rawMode = useUiStore((s) => s.rawMode);
  const rawText = useUiStore((s) => s.rawText);
  const diffs = useUiStore((s) => s.diffs);
  const browseCatalog = useUiStore((s) => s.browseCatalog);
  const toggleTheme = useUiStore((s) => s.toggleTheme);
  const toggleLeft = useUiStore((s) => s.toggleLeft);
  const setDrawer = useUiStore((s) => s.setDrawer);
  const toggleDrawer = useUiStore((s) => s.toggleDrawer);
  const setError = useUiStore((s) => s.setError);
  const setToast = useUiStore((s) => s.setToast);
  const setSaving = useUiStore((s) => s.setSaving);
  const setValidating = useUiStore((s) => s.setValidating);
  const setRawMode = useUiStore((s) => s.setRawMode);
  const setRawText = useUiStore((s) => s.setRawText);
  const setDiffs = useUiStore((s) => s.setDiffs);
  const setBrowseCatalog = useUiStore((s) => s.setBrowseCatalog);

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
  }, [toast, setToast]);

  useEffect(() => {
    api
      .project()
      .then((p) => loadProject(p, true))
      .catch((e) => setError(String(e)));
    api
      .catalog()
      .then(setCatalog)
      .catch((e) => setError(String(e)));
  }, [loadProject, setCatalog, setError]);

  // Test the *currently edited* connection (not the on-disk one).
  const refreshCatalog = () =>
    (doc ? api.testConnection(doc.config) : api.catalog())
      .then((c) => {
        setCatalog(c);
        setToast({
          kind: c.error ? "error" : "ok",
          text: c.error ? t("toast.dbNotReachable") : t("toast.dbConnected"),
        });
      })
      .catch((e) => setToast({ kind: "error", text: errText(e) }));

  // Re-read everything from disk (after a raw save), keeping the active index.
  const reloadProject = () =>
    api
      .project()
      .then((p) => loadProject(p, false))
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
    const live = schema ? requiredDefaultIssues(schema, catalog, active, t) : [];
    return [...(diagnostics ?? []), ...live];
  }, [schema, catalog, active, diagnostics, t]);

  // Count of fields whose chosen type is a sharp change from the source column,
  // and a one-click bulk fix. The "ignore" dismissal resets per active index.
  const typeMismatches = useMemo(() => (schema ? countTypeMismatches(schema, catalog) : 0), [schema, catalog]);
  const [ignoreTypeWarn, setIgnoreTypeWarn] = useState(false);
  useEffect(() => setIgnoreTypeWarn(false), [active]);

  // Debounced live preview of the active index.
  useEffect(() => {
    if (!schema || active === "config") {
      setPreview(null);
      return;
    }
    const handle = setTimeout(() => {
      api
        .preview(active, prunedForPreview(schema))
        .then(setPreview)
        .catch((e) => setError(String(e)));
    }, 250);
    return () => clearTimeout(handle);
  }, [active, schema, setPreview, setError]);

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

  useEffect(() => {
    loadCollapsed(active);
  }, [active, loadCollapsed]);

  // The index files a save considers, in the order the diff endpoint returns
  // them (config first, then these), so a FileDiff at position i+1 is entry i.
  const saveEntries = (): { name: string; schema_path: string; schema: SaveSchemaInput["schema"] }[] =>
    (doc?.config.index ?? [])
      .filter((e) => doc?.schemas[e.name])
      .map((e) => ({ name: e.name, schema_path: e.schema, schema: doc!.schemas[e.name] }));

  const saveIndexes = (): SaveSchemaInput[] =>
    saveEntries().map((e) => ({ schema_path: e.schema_path, schema: e.schema }));

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

  // Save the whole project but skip the files the user unchecked in the review
  // (`ignore` = their FileDiff paths). The diff list (config first, then
  // saveEntries in order) maps each ignored path back to the config or an index,
  // so the saved snapshot only marks the actually-written files clean.
  const performSave = async (ignore: string[]) => {
    if (!doc || !diffs) return;
    const ignored = new Set(ignore);
    const entries = saveEntries();
    const configPath = diffs[0]?.path;
    setSaving(true);
    try {
      const res = await api.save(doc.config, saveIndexes(), ignore);
      const prev = saved ? (JSON.parse(saved) as Doc) : doc;
      const nextSchemas = { ...prev.schemas };
      entries.forEach((e, i) => {
        const path = diffs[i + 1]?.path;
        if (!path || !ignored.has(path)) nextSchemas[e.name] = doc.schemas[e.name];
      });
      const nextConfig = configPath && ignored.has(configPath) ? prev.config : doc.config;
      setSaved(JSON.stringify({ config: nextConfig, schemas: nextSchemas }));
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
      await api.save(doc.config, [
        { schema_path: entry.schema, schema: doc.schemas[active] ?? emptySchema(""), raw: rawText },
      ]);
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

  // The runnable actions the command palette exposes (its entity records are
  // derived from the document inside the palette). Literal `t(…)` keys so the
  // i18n checker sees them.
  const runAction = t("search.runAction");
  const commands: SearchRecord[] = [
    {
      id: "cmd.validate",
      category: "action",
      title: t("topbar.validate"),
      keywords: "validate check database diagnostics",
      detail: { body: t("search.descValidate"), enter: runAction },
      run: () => void validate(),
    },
    {
      id: "cmd.save",
      category: "action",
      title: t("topbar.save"),
      keywords: "save write disk files",
      shortcut: "⌘S",
      detail: { body: t("search.descSave"), enter: runAction },
      run: () => void save(),
    },
    {
      id: "cmd.reset",
      category: "action",
      title: t("topbar.reset"),
      keywords: "reset discard revert unsaved changes",
      detail: { body: t("search.descReset"), enter: runAction },
      run: revertChanges,
    },
    {
      id: "cmd.deployment",
      category: "action",
      title: t("sidebar.deployment"),
      keywords: "deployment settings config connection sinks",
      detail: { body: t("search.descDeployment"), enter: runAction },
      run: () => setActive("config"),
    },
    {
      id: "cmd.tables",
      category: "action",
      title: t("topbar.tables"),
      keywords: "tables database catalog browse columns",
      detail: { body: t("search.descTables"), enter: runAction },
      run: () => setBrowseCatalog(true),
    },
    {
      id: "cmd.yaml",
      category: "action",
      title: t("topbar.yaml"),
      keywords: "yaml preview drawer mapping document",
      detail: { body: t("search.descYaml"), enter: runAction },
      run: toggleDrawer,
    },
    {
      id: "cmd.theme",
      category: "action",
      title: t("topbar.toggleTheme"),
      keywords: "theme dark light appearance",
      detail: { body: t("search.descTheme"), enter: runAction },
      run: toggleTheme,
    },
    {
      id: "cmd.sidebar",
      category: "action",
      title: leftOpen ? t("topbar.hideSidebar") : t("topbar.showSidebar"),
      keywords: "sidebar toggle panel indexes",
      detail: { body: t("search.descSidebar"), enter: runAction },
      run: toggleLeft,
    },
  ];

  // Keyboard shortcuts. Held in a ref (updated in an effect, not during render)
  // so the listener subscribes once but always sees the latest state; undo/
  // delete are suppressed while typing so native text editing keeps working.
  // Declared here, after the handlers it calls (save/undo/apply/…).
  const keyHandler = useRef<(e: KeyboardEvent) => void>(() => {
    /* replaced by handleKey in an effect below */
  });
  const handleKey = (e: KeyboardEvent) => {
    const el = document.activeElement as HTMLElement | null;
    const editing = !!el && (el.tagName === "INPUT" || el.tagName === "TEXTAREA" || el.isContentEditable);
    const mod = e.metaKey || e.ctrlKey;

    if (mod && e.key.toLowerCase() === "k") {
      e.preventDefault();
      setPaletteOpen((o) => !o);
      return;
    }
    if (mod && e.key.toLowerCase() === "s") {
      e.preventDefault();
      void save();
      return;
    }
    if (mod && e.key.toLowerCase() === "z") {
      if (editing) return;
      e.preventDefault();
      if (e.shiftKey) redo();
      else undo();
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
    keyHandler.current = handleKey;
  });
  useEffect(() => {
    const fn = (e: KeyboardEvent) => keyHandler.current(e);
    window.addEventListener("keydown", fn);
    return () => window.removeEventListener("keydown", fn);
  }, []);

  if (!project || !doc || !config)
    return <div className="p-10 text-muted-foreground">{error || "Loading project…"}</div>;

  return (
    <div className="flex h-screen flex-col">
      <header className="topbar relative flex items-center gap-3 border-b border-border bg-card px-4 py-2.5">
        <Hint label={leftOpen ? t("topbar.hideSidebar") : t("topbar.showSidebar")}>
          <Button
            variant="ghost"
            size="icon-sm"
            aria-label={leftOpen ? t("topbar.hideSidebar") : t("topbar.showSidebar")}
            onClick={toggleLeft}
          >
            <Icon name="menu" />
          </Button>
        </Hint>
        <span className="brand inline-flex items-center gap-2 bg-[linear-gradient(90deg,var(--accent),var(--accent-2))] bg-clip-text text-[0.9375rem] font-bold tracking-[0.0125rem] text-transparent">
          <span className="inline-flex text-primary">
            <Icon name="flow" size={18} />
          </span>
          flusso
        </span>
        <span className="path text-xs text-muted-foreground">{project.config_path}</span>
        <Hint label={t("topbar.retestDb")}>
          <button
            className={cn(
              "db-chip cursor-pointer rounded-full border border-border bg-secondary px-2 py-0.5 text-2xs",
              catalog && !catalog.error ? "ok border-primary text-primary" : "off border-warn text-warn",
            )}
            onClick={() => void refreshCatalog()}
          >
            {catalog && !catalog.error ? t("topbar.dbConnected") : t("topbar.dbOffline")}
          </button>
        </Hint>
        <span className="spacer flex-1" />

        <button
          type="button"
          onClick={() => setPaletteOpen(true)}
          className="absolute top-1/2 left-1/2 flex h-8 w-72 max-w-[32vw] -translate-x-1/2 -translate-y-1/2 cursor-pointer items-center gap-2.5 rounded-full border border-primary/25 px-3 pr-1.5 text-xs text-muted-foreground transition-colors hover:border-primary/50"
          style={{ background: "linear-gradient(90deg, var(--accent-soft), transparent 55%), var(--panel-2)" }}
        >
          <span
            className="size-2.5 shrink-0 rounded-full"
            style={{
              background: "conic-gradient(from 90deg, var(--accent), var(--accent-2), var(--accent))",
              boxShadow: "0 0 0 3px var(--accent-soft)",
            }}
          />
          <span className="truncate">{t("search.placeholder")}</span>
          <Kbd className="ml-auto">⌘K</Kbd>
        </button>

        {/* global: browse the whole database catalog */}
        <Hint label={t("search.descTables")}>
          <Button variant="ghost" size="sm" onClick={() => setBrowseCatalog(true)}>
            <Table2 /> {t("topbar.tables")}
          </Button>
        </Hint>

        {/* global: edit history */}
        <Hint label={t("topbar.undo")}>
          <Button variant="ghost" size="icon-sm" aria-label={t("topbar.undo")} disabled={!canUndo} onClick={undo}>
            <Icon name="undo" />
          </Button>
        </Hint>
        <Hint label={t("topbar.redo")}>
          <Button variant="ghost" size="icon-sm" aria-label={t("topbar.redo")} disabled={!canRedo} onClick={redo}>
            <Icon name="redo" />
          </Button>
        </Hint>

        {/* global: app settings (theme + language) */}
        <DropdownMenu>
          <Hint label={t("topbar.settings")}>
            <DropdownMenuTrigger asChild>
              <Button variant="ghost" size="icon-sm" aria-label={t("topbar.settings")}>
                <Settings />
              </Button>
            </DropdownMenuTrigger>
          </Hint>
          <DropdownMenuContent align="end">
            <DropdownMenuItem onClick={toggleTheme}>
              {theme === "dark" ? <Sun /> : <Moon />} {t("topbar.toggleTheme")}
            </DropdownMenuItem>
            <DropdownMenuSeparator />
            <DropdownMenuLabel>{t("topbar.language")}</DropdownMenuLabel>
            <DropdownMenuRadioGroup value={lang} onValueChange={setLang}>
              {Object.entries(LANGS).map(([value, label]) => (
                <DropdownMenuRadioItem key={value} value={value}>
                  {label}
                </DropdownMenuRadioItem>
              ))}
            </DropdownMenuRadioGroup>
          </DropdownMenuContent>
        </DropdownMenu>

        <div className="mx-1 h-5 w-px bg-border" />

        {/* deployment actions — the whole config */}
        <Hint label={t("search.descValidate")}>
          <Button variant="secondary" size="sm" onClick={() => void validate()} disabled={validating}>
            <span className={BTN_ICON}>{validating ? <span className="spinner" /> : <CircleCheck />}</span>
            {t("topbar.validate")}
          </Button>
        </Hint>
        <Hint label={t("topbar.resetHint")}>
          <Button variant="secondary" size="sm" onClick={revertChanges} disabled={!dirty || saving}>
            <span className={BTN_ICON}>
              <RotateCcw />
            </span>
            {t("topbar.reset")}
          </Button>
        </Hint>
        <Hint label={dirty ? t("topbar.unsaved") : t("topbar.upToDate")}>
          <Button size="sm" onClick={() => void save()} disabled={saving}>
            <span className={BTN_ICON}>
              {saving ? <span className="spinner" /> : dirty ? <span className="dirty-dot" /> : <Save />}
            </span>
            {t("topbar.save")}
          </Button>
        </Hint>
      </header>

      {error && <div className="banner error bg-destructive/10 px-4 py-2 text-xs text-destructive">{error}</div>}
      {catalog?.error && (
        <div className="banner warn bg-warn/10 px-4 py-2 text-xs text-warn">{t("topbar.offlineBanner")}</div>
      )}

      <div
        className="grid min-h-0 flex-1 transition-all duration-150"
        style={{
          gridTemplateColumns: `${leftOpen ? "13.125rem" : "0"} 1fr ${inspectorOpen ? "22.5rem" : "0"}`,
          gridTemplateRows: "auto minmax(0, 1fr)",
        }}
      >
        {/* Context bar: a strip over the work area only (columns 2–3, right of the
            sidebar) naming the index you're editing and carrying the tools that act
            on it — kept apart from the global bar so index- and deployment-scoped
            actions never blur together. Absent on the Deployment screen. */}
        {active !== "config" && schema && (
          <div
            className="col-start-2 col-span-2 row-start-1 flex items-center gap-2.5 border-b border-border px-4 py-1.5"
            style={{ background: "linear-gradient(90deg, var(--accent-soft), transparent 42%), var(--panel-2)" }}
          >
            <span className="badge root">root</span>
            <span className="text-sm font-medium text-foreground">{active}</span>
            <span className="font-mono text-2xs text-muted-foreground">
              {schema.db_schema && schema.db_schema !== "public" ? `${schema.db_schema}.` : ""}
              {schema.table}
              {schema.primary_key ? ` · ${t("node.pk")}: ${schema.primary_key}` : ""}
            </span>
            <span className="flex-1" />
            <Hint label={t("search.descYaml")}>
              <Button variant="secondary" size="sm" onClick={() => setDrawer(true)}>
                <Eye /> {t("topbar.yaml")}
              </Button>
            </Hint>
            <Hint label={rawMode ? t("topbar.visualHint") : t("topbar.rawHint")}>
              <Button variant="ghost" size="sm" onClick={() => (rawMode ? setRawMode(false) : openRaw())}>
                <FileCode2 /> {rawMode ? t("topbar.visual") : t("topbar.rawYaml")}
              </Button>
            </Hint>
          </div>
        )}

        {leftOpen && (
          <nav className="sidebar col-start-1 row-start-1 row-span-2 flex min-h-0 flex-col border-r border-border bg-card">
            <div className="min-h-0 flex-1 overflow-y-auto p-2">
              <button className={cn(NAV, active === "config" && NAV_ACTIVE)} onClick={() => setActive("config")}>
                ⚙ {t("sidebar.deployment")}
              </button>
              <div className={NAV_HEADING}>{t("sidebar.indexes")}</div>
              {(config.index ?? []).map((e) => (
                <button
                  key={e.name}
                  className={cn(NAV, active === e.name && NAV_ACTIVE)}
                  onClick={() => openIndex(e.name)}
                >
                  {indexDirty(e.name) && <span className="dirty-dot" />}
                  {e.name}
                  {!e.enabled && <span className="text-muted-foreground"> {t("sidebar.off")}</span>}
                </button>
              ))}
              <NewIndex tables={catalog?.catalog.tables.map((tbl) => tbl.name) ?? []} onCreate={createIndex} />
            </div>
            {/* Colour key — open by default, but collapsible so a long index list
                isn't crowded out. Pinned below the scrolling list. */}
            <details className="legend group shrink-0 border-t border-border py-2" open>
              <summary className="flex cursor-pointer list-none items-center gap-1.5 px-1.5 py-1 text-2xs font-semibold uppercase tracking-[0.06em] text-muted-foreground [&::-webkit-details-marker]:hidden">
                <ChevronRight className="size-3 transition-transform group-open:rotate-90" aria-hidden="true" />
                {t("sidebar.legend")}
              </summary>
              <div className="max-h-[45vh] overflow-y-auto ps-2">
                <div className={NAV_HEADING}>{t("sidebar.kinds")}</div>
                <div className="flex flex-col">
                  {KIND_ROWS.map((k) => (
                    <LegendRow key={k} color={`var(--k-${k})`} label={k} desc={kindDesc(t, k)} />
                  ))}
                </div>
                <div className={NAV_HEADING}>{t("sidebar.types")}</div>
                <div className="flex flex-col">
                  {TYPE_FAMILIES.map((f) => (
                    <LegendRow
                      key={f.varKey}
                      color={`var(--t-${f.varKey})`}
                      label={f.label}
                      desc={typeDesc(t, f.varKey)}
                    />
                  ))}
                </div>
              </div>
            </details>
          </nav>
        )}

        {active === "config" ? (
          <main className="col-start-2 row-start-2 min-h-0 overflow-y-auto p-4">
            <ConfigPanel config={config} onChange={setConfig} onDuplicate={dupIndex} />
          </main>
        ) : rawMode ? (
          <main className="raw-pane col-start-2 row-start-2 flex min-h-0 flex-col">
            <div className="banner warn bg-warn/10 px-4 py-2 text-xs text-warn">
              {t("raw.editingFor")} <strong>{active}</strong> — {t("raw.help")}
            </div>
            <Textarea
              className="raw-editor m-2.5 min-h-0 flex-1 resize-none font-mono text-xs leading-relaxed"
              value={rawText}
              onChange={(e) => setRawText(e.target.value)}
              spellCheck={false}
            />
            <div className="raw-actions flex gap-2 px-2.5 pb-2.5">
              <Button size="sm" onClick={() => void saveRaw()} disabled={saving}>
                {t("raw.save")}
              </Button>
              <Button variant="secondary" size="sm" onClick={() => setRawMode(false)}>
                {t("common.cancel")}
              </Button>
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
            <main className="canvas-wrap col-start-2 row-start-2 relative h-full min-h-0">
              <Canvas />
              {typeMismatches > 0 && !ignoreTypeWarn && (
                <div className="pointer-events-none absolute inset-x-0 top-3 z-20 flex justify-end px-4">
                  <div className="pointer-events-auto flex items-center gap-3 rounded-lg border border-warn/40 bg-warn/15 px-4 py-2 text-xs text-warn shadow-lg backdrop-blur-sm">
                    <span>{t("typeWarn.banner", { n: typeMismatches })}</span>
                    <Button
                      size="sm"
                      variant="secondary"
                      className="border-warn bg-warn font-semibold text-background hover:border-warn hover:bg-warn/90"
                      onClick={() => apply((s) => fixAllTypes(s, catalog))}
                    >
                      {t("typeWarn.fixAll")}
                    </Button>
                    <Button size="sm" variant="ghost" onClick={() => setIgnoreTypeWarn(true)}>
                      {t("typeWarn.ignore")}
                    </Button>
                  </div>
                </div>
              )}
            </main>
            {/* A modal right drawer: the dim backdrop closes it on click (plus Esc / ✕). */}
            <Drawer open={drawer} onOpenChange={setDrawer} direction="right">
              <DrawerContent className="data-[vaul-drawer-direction=right]:w-[min(46rem,92vw)] data-[vaul-drawer-direction=right]:sm:max-w-none">
                <DrawerHeader className="flex-row items-center gap-2 border-b border-border p-3">
                  <DrawerTitle className="text-sm font-semibold">
                    {t("preview.title")} <span className="font-normal text-muted-foreground">· {active}</span>
                  </DrawerTitle>
                  <span className="flex-1" />
                  <DrawerClose asChild>
                    <Button variant="ghost" size="icon-sm" aria-label={t("common.close")}>
                      <X />
                    </Button>
                  </DrawerClose>
                </DrawerHeader>
                <Preview
                  index={active}
                  preview={preview}
                  diagnostics={allDiagnostics.filter((d) => d.index === active)}
                  onSample={
                    doc && schema && active !== "config" ? () => api.sample(doc.config, active, schema) : undefined
                  }
                />
              </DrawerContent>
            </Drawer>
            {inspectorOpen && (
              <aside className="col-start-3 row-start-2 min-h-0 overflow-y-auto border-l border-border bg-card p-3.5">
                <Inspector />
              </aside>
            )}
          </DesignProvider>
        ) : null}
      </div>

      {diffs && (
        <DiffModal
          diffs={diffs}
          doc={doc}
          saving={saving}
          onConfirm={(ignore) => void performSave(ignore)}
          onCancel={() => setDiffs(null)}
        />
      )}
      {browseCatalog && catalog && <CatalogBrowser catalog={catalog} onClose={() => setBrowseCatalog(false)} />}

      <CommandPalette
        open={paletteOpen}
        onOpenChange={setPaletteOpen}
        doc={doc}
        catalog={catalog}
        active={active}
        commands={commands}
      />

      {toast && (
        <div className={`toast ${toast.kind}`} role="status">
          {toast.text}
        </div>
      )}
    </div>
  );
}

// Middle-elide a long path to first segment + basename ("dev/…/users.schema.yml");
// the full path is shown on hover via the row's title.
function shortenPath(path: string, max = 30): string {
  if (path.length <= max) return path;
  const parts = path.split("/");
  if (parts.length <= 2) return path;
  return `${parts[0]}/…/${parts[parts.length - 1]}`;
}

// The quick-validation state the save review runs while it's open.
type Check = { state: "loading" } | { state: "failed"; err: string } | { state: "done"; res: ValidateResponse };

// A compact status chip for the review header: what the quick validation found.
function ValidationChip({ check }: { check: Check }) {
  const { t } = useT();
  const chip = "inline-flex shrink-0 items-center gap-1.5 rounded-md border px-2.5 py-1 text-xs font-medium";
  if (check.state === "loading")
    return (
      <span className={cn(chip, "border-border bg-secondary text-muted-foreground")}>
        <span className="spinner" /> {t("diff.validating")}
      </span>
    );
  const offline = check.state === "failed" || !check.res.db_reachable;
  if (offline)
    return (
      <span className={cn(chip, "border-warn/40 bg-warn/10 text-warn")}>
        <TriangleAlert className="size-3.5" /> {t("diff.dbOffline")}
      </span>
    );
  const { error, diagnostics } = check.res;
  if (error || diagnostics.length > 0)
    return (
      <span className={cn(chip, "border-destructive/40 bg-destructive/10 text-destructive")}>
        <CircleAlert className="size-3.5" />{" "}
        {error ? t("diff.validateFailed") : t("diff.issues", { n: diagnostics.length })}
      </span>
    );
  return (
    <span className={cn(chip, "border-primary/40 bg-primary/10 text-primary")}>
      <CircleCheck className="size-3.5" /> {t("diff.allGood")}
    </span>
  );
}

// The detail panel shown under the header when validation surfaced something.
function ValidationDetail({ check }: { check: Check }) {
  const { t } = useT();
  if (check.state === "loading") return null;
  const err = check.state === "failed" ? check.err : check.res.db_reachable ? check.res.error : undefined;
  const diagnostics = check.state === "done" && check.res.db_reachable && !check.res.error ? check.res.diagnostics : [];
  if (!err && diagnostics.length === 0) return null;
  return (
    <div className="max-h-28 overflow-y-auto rounded-md border border-destructive/40 bg-destructive/10 p-2 text-xs">
      {err && (
        <div className="text-destructive">
          {t("diff.validateFailed")}: {err}
        </div>
      )}
      {diagnostics.map((d, i) => (
        <div key={i} className={cn("py-0.5", d.severity === "warning" ? "text-warn" : "text-destructive")}>
          <span className="font-mono text-muted-foreground">
            {d.index}.{d.field}
          </span>{" "}
          {d.message}
        </div>
      ))}
    </div>
  );
}

function DiffModal({
  diffs,
  doc,
  saving,
  onConfirm,
  onCancel,
}: {
  diffs: FileDiff[];
  doc: Doc;
  saving: boolean;
  onConfirm: (paths: string[]) => void;
  onCancel: () => void;
}) {
  const { t } = useT();
  const [mode, setMode] = useState<DiffMode>("split");
  const [selected, setSelected] = useState(0);
  const [check, setCheck] = useState<Check>({ state: "loading" });
  const changed = diffs.filter((d) => d.changed);
  const active = changed[selected] ?? changed[0];

  // Which files to actually write (default all); the user can uncheck any to
  // save a subset.
  const [include, setInclude] = useState<ReadonlySet<string>>(() => new Set(changed.map((d) => d.path)));
  const [query, setQuery] = useState("");
  const needle = query.trim().toLowerCase();
  const shown = changed.filter((d) => !needle || d.path.toLowerCase().includes(needle));
  const toggle = (path: string) =>
    setInclude((s) => {
      const next = new Set(s);
      if (next.has(path)) next.delete(path);
      else next.add(path);
      return next;
    });
  // The select-all box acts on the currently-shown (filtered) files.
  const shownIncluded = shown.filter((d) => include.has(d.path)).length;
  const allShown = shown.length > 0 && shownIncluded === shown.length;

  // Search-box keybindings: Esc clears the query (instead of closing the whole
  // dialog — Radix's default); Enter opens the top match; arrows move through the
  // filtered list. Position is tracked in `shown` and mapped back to `changed`.
  const onSearchKey = (e: React.KeyboardEvent<HTMLInputElement>) => {
    if (e.key === "Escape" && query) {
      e.preventDefault();
      e.stopPropagation();
      setQuery("");
      return;
    }
    if (shown.length === 0) return;
    const pos = shown.findIndex((d) => changed.indexOf(d) === selected);
    if (e.key === "Enter") {
      e.preventDefault();
      setSelected(changed.indexOf(shown[Math.max(0, pos)]));
    } else if (e.key === "ArrowDown") {
      e.preventDefault();
      setSelected(changed.indexOf(shown[Math.min(shown.length - 1, pos + 1)]));
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      setSelected(changed.indexOf(shown[Math.max(0, pos <= 0 ? 0 : pos - 1)]));
    }
  };
  const toggleAll = () =>
    setInclude((s) => {
      const next = new Set(s);
      for (const d of shown) {
        if (allShown) next.delete(d.path);
        else next.add(d.path);
      }
      return next;
    });

  // Run a quick validation against the database when the review opens, so a save
  // surfaces schema/DB problems (or an all-clear) before writing anything.
  useEffect(() => {
    let alive = true;
    setCheck({ state: "loading" });
    const indexes = Object.entries(doc.schemas).map(([name, schema]) => ({ name, schema }));
    api
      .validate(doc.config, indexes)
      .then((res) => alive && setCheck({ state: "done", res }))
      .catch((e: unknown) => alive && setCheck({ state: "failed", err: errText(e) }));
    return () => {
      alive = false;
    };
  }, [doc]);
  const modes: { id: DiffMode; label: string; Icon: typeof Columns2 }[] = [
    { id: "split", label: t("diff.viewSplit"), Icon: Columns2 },
    { id: "unified", label: t("diff.viewUnified"), Icon: AlignJustify },
    { id: "old", label: t("diff.viewOld"), Icon: Minus },
    { id: "new", label: t("diff.viewNew"), Icon: Plus },
  ];
  return (
    <Dialog open onOpenChange={(open) => !open && onCancel()}>
      <DialogContent className="flex h-[92vh] w-[96vw] max-w-none flex-col sm:max-w-none" aria-label={t("diff.aria")}>
        <DialogHeader className="flex-row items-center justify-between gap-3 pr-8">
          <div className="flex min-w-0 items-center gap-3">
            <DialogTitle className="shrink-0">{t("diff.reviewTitle")}</DialogTitle>
            <ValidationChip check={check} />
          </div>
          <div className="inline-flex shrink-0 rounded-md border border-border bg-secondary p-0.5 text-xs">
            {modes.map((m) => (
              <button
                key={m.id}
                type="button"
                onClick={() => setMode(m.id)}
                className={cn(
                  "flex cursor-pointer items-center gap-1.5 rounded-sm px-2.5 py-1 transition-colors",
                  mode === m.id
                    ? "bg-background font-medium text-foreground shadow-sm"
                    : "text-muted-foreground hover:text-foreground",
                )}
              >
                <m.Icon className="size-3.5" />
                {m.label}
              </button>
            ))}
          </div>
        </DialogHeader>
        <ValidationDetail check={check} />
        <div className="flex min-h-0 flex-1 overflow-hidden rounded-md border border-border">
          <div className="flex w-60 shrink-0 flex-col border-r border-border bg-secondary/40">
            {changed.length > 3 && (
              <div className="flex h-9 shrink-0 items-center gap-2 border-b border-border px-3 focus-within:border-b-primary">
                <Search className="size-3.5 shrink-0 text-muted-foreground" />
                <input
                  value={query}
                  onChange={(e) => setQuery(e.target.value)}
                  onKeyDown={onSearchKey}
                  placeholder={t("diff.filterFiles")}
                  className="min-w-0 flex-1 bg-transparent text-xs outline-none placeholder:text-muted-foreground"
                  autoComplete="off"
                  autoCorrect="off"
                  spellCheck={false}
                  data-1p-ignore="true"
                  data-lpignore="true"
                  data-form-type="other"
                />
                {query && (
                  <button
                    type="button"
                    onClick={() => setQuery("")}
                    aria-label={t("common.clear")}
                    className="shrink-0 cursor-pointer text-muted-foreground hover:text-foreground"
                  >
                    <X className="size-3.5" />
                  </button>
                )}
              </div>
            )}
            <label className="flex shrink-0 cursor-pointer items-center gap-2 border-b border-border px-3 py-2 text-2xs font-medium text-muted-foreground">
              <Checkbox
                className="size-4"
                checked={allShown ? true : shownIncluded === 0 ? false : "indeterminate"}
                onCheckedChange={toggleAll}
              />
              {t("diff.selected", { n: include.size, m: changed.length })}
            </label>
            <div className="min-h-0 flex-1 overflow-y-auto">
              {shown.map((d) => {
                const i = changed.indexOf(d);
                const s = diffStats(d.current, d.next);
                const short = shortenPath(d.path);
                const base = short.slice(short.lastIndexOf("/") + 1);
                const dir = short.slice(0, short.length - base.length);
                const on = include.has(d.path);
                return (
                  <div
                    key={d.path}
                    className={cn(
                      "flex items-center gap-2 border-b border-border/60 border-l-2 pr-2 pl-3 transition-colors",
                      i === selected ? "border-l-primary bg-background" : "border-l-transparent hover:bg-accent/50",
                    )}
                  >
                    <Checkbox
                      className="size-4 shrink-0"
                      checked={on}
                      onCheckedChange={() => toggle(d.path)}
                      aria-label={d.path}
                    />
                    <button
                      type="button"
                      title={d.path}
                      onClick={() => setSelected(i)}
                      className={cn(
                        "flex min-w-0 flex-1 cursor-pointer flex-col gap-0.5 py-2 text-left",
                        !on && "opacity-45",
                      )}
                    >
                      <span className="truncate font-mono text-xs">
                        <span className="text-muted-foreground">{dir}</span>
                        <span className="font-medium text-foreground">{base}</span>
                      </span>
                      <span className="flex items-center gap-2 font-mono text-2xs tabular-nums">
                        {d.current === "" && <span className="badge object">{t("diff.newFile")}</span>}
                        <span className="text-diff-add-num">+{s.adds}</span>
                        <span className="text-diff-del-num">-{s.dels}</span>
                      </span>
                    </button>
                  </div>
                );
              })}
              {shown.length === 0 && (
                <p className="px-3 py-4 text-center text-2xs text-muted-foreground">{t("diff.noFiles")}</p>
              )}
            </div>
          </div>
          <div className="min-h-0 flex-1">
            {active && (
              <DiffView key={active.path} path={active.path} current={active.current} next={active.next} mode={mode} />
            )}
          </div>
        </div>
        <DialogFooter>
          <Button variant="secondary" size="sm" onClick={onCancel}>
            {t("common.cancel")}
          </Button>
          <Button
            size="sm"
            onClick={() => onConfirm(changed.filter((d) => !include.has(d.path)).map((d) => d.path))}
            disabled={saving || include.size === 0}
          >
            {saving && <span className="spinner" />}
            {t("diff.write", { n: include.size })}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

function NewIndex({ tables, onCreate }: { tables: string[]; onCreate: (name: string, table: string) => void }) {
  const { t } = useT();
  const [open, setOpen] = useState(false);
  const [name, setName] = useState("");
  const [table, setTable] = useState(tables[0] ?? "");
  if (!open) {
    return (
      <button
        className={cn(NAV, "mt-1.5 border border-dashed border-border text-primary")}
        onClick={() => setOpen(true)}
      >
        + {t("sidebar.newIndex")}
      </button>
    );
  }
  return (
    <div className="mt-1.5 flex flex-col gap-1.5 rounded-lg border border-border p-2">
      <Text value={name} onChange={setName} placeholder={t("sidebar.indexName")} />
      {tables.length ? (
        <Select value={table} options={tables} onChange={setTable} />
      ) : (
        <Text value={table} onChange={setTable} placeholder={t("sidebar.rootTable")} />
      )}
      <div className="row flex flex-wrap gap-2">
        <Button
          size="sm"
          disabled={!name || !table}
          onClick={() => {
            onCreate(name, table);
            setOpen(false);
            setName("");
          }}
        >
          {t("sidebar.create")}
        </Button>
        <Button variant="secondary" size="sm" onClick={() => setOpen(false)}>
          {t("common.cancel")}
        </Button>
      </div>
    </div>
  );
}
