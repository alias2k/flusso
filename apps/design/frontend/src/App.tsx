import { lazy, Suspense, useEffect, useMemo, useRef, useState } from "react";
import { useDesignStore, useCanUndo, useCanRedo, undo, redo, type Doc } from "./store/design";
import { useUiStore } from "./store/ui";
import {
  ChevronRight,
  CircleAlert,
  CircleCheck,
  AlignJustify,
  Columns2,
  Eye,
  Folder,
  Languages,
  Minus,
  Moon,
  Pencil,
  Plus,
  RotateCcw,
  Save,
  Search,
  Settings,
  Sun,
  Table2,
  TriangleAlert,
  Waypoints,
  X,
} from "lucide-react";
import type { ColumnShape, DiagnosticDto, FileOp, OpDiff, ValidateResponse } from "./api";
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
import { Switch } from "@/components/ui/switch";

// CodeMirror (+ the vim extension) is a chunk of its own — loaded only when
// Code mode actually opens, so the canvas app doesn't pay for it.
const CodeView = lazy(() => import("./components/CodeView").then((m) => ({ default: m.CodeView })));
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuRadioGroup,
  DropdownMenuRadioItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { Drawer, DrawerClose, DrawerContent, DrawerHeader, DrawerTitle } from "@/components/ui/drawer";
import { Kbd, KbdGroup } from "@/components/ui/kbd";
import { Label } from "@/components/ui/label";
import { Hint } from "./components/Hint";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { cn } from "@/lib/utils";
import { Combobox, Field, GlowDot, Text } from "./components/widgets";
import { LANGS, useT, type Translate } from "./i18n";
// Type-only: erased at compile time, so it doesn't pull the yaml package into
// the main chunk (the runtime anchor code stays in the lazy CodeView chunk).
import type { ParseErrorInfo } from "./model/anchors";
import { removeAt, removeFields, removeNode } from "./model/edit";
import { formatRoute, parseRoute, type Route } from "./router";
import { countTypeMismatches, fixAllTypes, requiredDefaultIssues } from "./model/issues";
import { prunedForPreview } from "./model/prune";
import type { SearchRecord } from "./model/search";
import { DesignProvider } from "./state";
import { BTN_ICON, NAV, NAV_ACTIVE, NAV_HEADING, NO_PW_MANAGER } from "./styles";
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
          {/* A real button so keyboard users can Tab to it — focus reveals the
              same description the row shows on hover. */}
          <button
            type="button"
            onFocus={() => setOpen(true)}
            onBlur={() => setOpen(false)}
            className="flex w-fit cursor-default items-center gap-2 rounded-sm text-2xs text-muted-foreground select-none focus-visible:ring-2 focus-visible:ring-ring/60 focus-visible:outline-none"
          >
            <span className="inline-block size-2.5 shrink-0 rounded-full" style={{ background: color }} />
            {label}
          </button>
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
  const [newIndexOpen, setNewIndexOpen] = useState(false);

  const theme = useUiStore((s) => s.theme);
  const leftOpen = useUiStore((s) => s.leftOpen);
  const drawer = useUiStore((s) => s.drawer);
  const error = useUiStore((s) => s.error);
  const toast = useUiStore((s) => s.toast);
  const saving = useUiStore((s) => s.saving);
  const validating = useUiStore((s) => s.validating);
  const rawMode = useUiStore((s) => s.rawMode);
  const rawText = useUiStore((s) => s.rawText);
  const vimMode = useUiStore((s) => s.vimMode);
  const toggleVim = useUiStore((s) => s.toggleVim);
  const autoFormat = useUiStore((s) => s.autoFormat);
  const toggleAutoFormat = useUiStore((s) => s.toggleAutoFormat);
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

  // ── routing: the hash mirrors what the main area shows (see router.ts) ────

  // URL → state. Behind a ref so the one popstate listener always sees the
  // latest handlers; validated against the loaded project so a stale deep link
  // can't select a nonexistent index.
  const applyRoute = (route: Route) => {
    if (route.view === "tables") {
      setBrowseCatalog(true);
      return;
    }
    setBrowseCatalog(false);
    if (route.view === "deployment") {
      setRawMode(false);
      setActive("config");
      return;
    }
    const known = useDesignStore.getState().doc?.config.index?.some((e) => e.name === route.name);
    if (!known) return;
    if (useDesignStore.getState().active !== route.name) openIndex(route.name);
    // Entering Code via URL: the buffer re-seeds itself (the rawFor effect)
    // once rawMode is on, so no openRaw here.
    setRawMode(route.code);
  };
  const applyRouteRef = useRef(applyRoute);
  useEffect(() => {
    applyRouteRef.current = applyRoute;
  });
  useEffect(() => {
    const onPop = () => {
      const route = parseRoute(window.location.pathname);
      if (route) applyRouteRef.current(route);
    };
    window.addEventListener("popstate", onPop);
    return () => window.removeEventListener("popstate", onPop);
  }, []);

  // State → URL. pushState doesn't fire popstate, so this can't loop; the very
  // first write replaces instead, keeping the entry the user landed on.
  const routePath = formatRoute(
    browseCatalog
      ? { view: "tables" }
      : active === "config"
        ? { view: "deployment" }
        : { view: "index", name: active, code: rawMode },
  );
  const routedOnce = useRef(false);
  useEffect(() => {
    if (!doc) return;
    if (window.location.pathname === routePath) return;
    window.history[routedOnce.current ? "pushState" : "replaceState"](null, "", routePath);
    routedOnce.current = true;
  }, [doc, routePath]);

  useEffect(() => {
    api
      .project()
      .then((p) => {
        loadProject(p, true);
        // A deep link wins over the default first index — applied after the
        // project loads so the index-name validation can see it.
        const route = parseRoute(window.location.pathname);
        if (route) applyRouteRef.current(route);
      })
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

  const columnsFor = useMemo(() => {
    const tables = catalog?.catalog.tables ?? [];
    return (table: string): ColumnShape[] => tables.find((t) => t.name === table)?.columns ?? [];
  }, [catalog]);

  // Parsed once per snapshot — `indexDirty` runs per sidebar row per render, so
  // it must not re-parse the whole saved doc each call.
  const savedDoc = useMemo(() => (saved ? (JSON.parse(saved) as Doc) : null), [saved]);
  const dirty = useMemo(() => !!doc && JSON.stringify(doc) !== saved, [doc, saved]);
  const indexDirty = (name: string): boolean => {
    if (!doc || !savedDoc) return false;
    return JSON.stringify(doc.schemas[name]) !== JSON.stringify(savedDoc.schemas[name]);
  };
  const configDirty = useMemo(
    () => !!doc && (!savedDoc || JSON.stringify(doc.config) !== JSON.stringify(savedDoc.config)),
    [doc, savedDoc],
  );

  const config = doc?.config;
  const schema = doc && active !== "config" ? doc.schemas[active] : undefined;
  // Never in Code mode: the inspector edits the canvas selection, and the raw
  // pane doesn't render it — reserving its grid column would just squeeze the
  // editor against dead space.
  const inspectorOpen = active !== "config" && !rawMode && !!schema && selection !== null;

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

  // Code-mode live sync: the buffer parses server-side (the same strict parser
  // that reads the file) and, when it converts, replaces the in-memory schema —
  // so undo, the preview, and the one global Save all see YAML edits exactly
  // like visual ones. The sequence guard drops out-of-order replies.
  const parseSeq = useRef(0);
  const [codeProblem, setCodeProblem] = useState<ParseErrorInfo | null>(null);
  // Which index the Code buffer belongs to. Gating the sync on it is what
  // keeps an index switch from parsing the *previous* index's YAML into the
  // new one; the re-seed effect below moves the buffer over.
  const [rawFor, setRawFor] = useState("");
  useEffect(() => {
    if (!rawMode || active === "config" || rawFor !== active) {
      setCodeProblem(null);
      return;
    }
    const seq = ++parseSeq.current;
    const handle = setTimeout(() => {
      api
        .parse(rawText)
        .then((res) => {
          if (seq !== parseSeq.current || !useUiStore.getState().rawMode) return;
          setCodeProblem(
            res.error ? { message: res.error, location: res.location, field: res.field, typeTag: res.type_tag } : null,
          );
          const parsed = res.schema;
          if (!parsed) return;
          const current = useDesignStore.getState().doc?.schemas[active];
          if (JSON.stringify(parsed) !== JSON.stringify(current)) apply(() => parsed);
        })
        .catch(() => setCodeProblem(null));
    }, 300);
    return () => clearTimeout(handle);
  }, [rawMode, rawText, active, rawFor, apply]);

  // Switching index while Code mode is open: the buffer still shows the
  // previous index, so regenerate it from the new index's schema (the same
  // codegen the preview uses) before the sync above may run again.
  useEffect(() => {
    if (!rawMode || active === "config" || rawFor === active || !schema) return;
    let alive = true;
    api
      .preview(active, prunedForPreview(schema))
      .then((p) => {
        if (!alive || !useUiStore.getState().rawMode) return;
        setRawText(p.yaml);
        setRawFor(active);
      })
      .catch(() => {
        if (!alive) return;
        setRawText(project?.indexes.find((i) => i.name === active)?.raw ?? "");
        setRawFor(active);
      });
    return () => {
      alive = false;
    };
  }, [rawMode, active, rawFor, schema, project, setRawText]);

  // …and the applied schema gets the *real* validation (columns exist, types
  // line up) against the database — it's fast, so problems surface in the rail
  // as you type instead of waiting for a manual Validate.
  const [liveDiags, setLiveDiags] = useState<DiagnosticDto[]>([]);
  useEffect(() => {
    if (!rawMode || !schema || !doc || active === "config" || catalog?.error) {
      setLiveDiags([]);
      return;
    }
    const handle = setTimeout(() => {
      api
        .validate(doc.config, [{ name: active, schema }])
        .then((res) => {
          if (!useUiStore.getState().rawMode) return;
          setLiveDiags(res.db_reachable && !res.error ? res.diagnostics : []);
        })
        .catch(() => setLiveDiags([]));
    }, 800);
    return () => clearTimeout(handle);
  }, [rawMode, schema, doc, active, catalog]);

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

  // The schema-file ops a save applies, derived from the saved snapshot → the
  // current doc, correlated by the stable per-index id (so a rename or a path
  // change reads as a Move, not a delete + create). `name` is kept alongside so
  // the snapshot can advance per index after the save.
  const buildOps = (): { op: FileOp; name: string; removed: boolean }[] => {
    if (!doc) return [];
    const prev = savedDoc?.config.index ?? [];
    const prevById = new Map(prev.filter((e) => e.id).map((e) => [e.id, e]));
    const cur = doc.config.index ?? [];
    const seen = new Set<string>();
    const planned: { op: FileOp; name: string; removed: boolean }[] = [];
    for (const e of cur) {
      if (e.id) seen.add(e.id);
      const schema = doc.schemas[e.name];
      if (!schema) continue; // no model to write (failed load) — leave the file alone
      const was = e.id ? prevById.get(e.id) : undefined;
      if (!was) planned.push({ op: { kind: "upsert", path: e.schema, schema }, name: e.name, removed: false });
      else if (was.schema !== e.schema)
        planned.push({ op: { kind: "move", from: was.schema, path: e.schema, schema }, name: e.name, removed: false });
      else if (indexDirty(e.name))
        planned.push({ op: { kind: "upsert", path: e.schema, schema }, name: e.name, removed: false });
    }
    // Indexes dropped from the config → delete their previous files.
    for (const p of prev)
      if (p.id && !seen.has(p.id))
        planned.push({ op: { kind: "delete", path: p.schema }, name: p.name, removed: true });
    return planned;
  };

  // Match a planned op to its resolved diff entry: the server returns absolute
  // paths, the op carries the config-relative one, so compare by suffix (a move
  // may degrade to a plain write when its source is already gone).
  const suffixOf = (abs: string, rel: string) => abs === rel || abs.endsWith(`/${rel}`);
  const diffFor = (list: OpDiff[], p: { op: FileOp }): OpDiff | undefined =>
    list.find((d) =>
      p.op.kind === "delete"
        ? d.op === "delete" && suffixOf(d.path, p.op.path)
        : d.op === "delete"
          ? false
          : suffixOf(d.path, p.op.path),
    );

  // Save first shows a diff of what would change on disk; performSave applies it.
  // When the config model is unchanged we hide its diff (a formatting/comment-only
  // reflow the user didn't ask for) so an untouched flusso.toml is neither shown
  // nor rewritten.
  const save = async () => {
    if (!doc || saving) return;
    setSaving(true);
    try {
      const result = await api.diff(
        doc.config,
        buildOps().map((p) => p.op),
      );
      const shown = configDirty || !result[0] ? result : [{ ...result[0], changed: false }, ...result.slice(1)];
      if (shown.some((d) => d.changed)) setDiffs(shown);
      else setToast({ kind: "ok", text: t("toast.alreadyUpToDate") });
    } catch (e) {
      setToast({ kind: "error", text: t("toast.diffFailed", { err: errText(e) }) });
    } finally {
      setSaving(false);
    }
  };

  // Apply the op set, skipping the entries the user unchecked in the review
  // (plus the config whenever its model didn't change, so an untouched
  // flusso.toml keeps its comments). The saved snapshot advances per index for
  // exactly the ops that were applied, so Reset targets the true on-disk state.
  const performSave = async (skipPaths: string[]) => {
    if (!doc || !diffs) return;
    const planned = buildOps();
    const configPath = diffs[0]?.path;
    const skip = new Set(skipPaths);
    if (!configDirty && configPath) skip.add(configPath);
    setSaving(true);
    try {
      const res = await api.save(
        doc.config,
        planned.map((p) => p.op),
        [...skip],
      );
      // Advance the saved snapshot to the new on-disk state. When everything was
      // applied (the common case) the doc *is* the disk, so snapshot it directly
      // — exact, no key-order drift. When the user skipped some entries, fold
      // only the applied ops onto the previous snapshot so the skipped ones stay
      // dirty.
      const configSkipped = configDirty && !!configPath && skip.has(configPath);
      const anySkipped =
        configSkipped ||
        planned.some((p) => {
          const d = diffFor(diffs, p);
          return !!d && d.changed && skip.has(d.path);
        });
      if (!anySkipped) {
        setSaved(JSON.stringify(doc));
      } else {
        const prev = savedDoc ?? doc;
        const nextSchemas = { ...prev.schemas };
        for (const p of planned) {
          const d = diffFor(diffs, p);
          if (!d || !d.changed || skip.has(d.path)) continue; // not applied
          if (p.removed) delete nextSchemas[p.name];
          else nextSchemas[p.name] = doc.schemas[p.name];
        }
        const nextConfig = configSkipped ? prev.config : doc.config;
        const live = new Set((nextConfig.index ?? []).map((e) => e.name));
        for (const name of Object.keys(nextSchemas)) if (!live.has(name)) delete nextSchemas[name];
        setSaved(JSON.stringify({ config: nextConfig, schemas: nextSchemas }));
      }
      setDiffs(null);
      const n = res.written.length + res.moved.length + res.deleted.length;
      setToast({ kind: "ok", text: t("toast.saved", { n }) });
    } catch (e) {
      setToast({ kind: "error", text: t("toast.saveFailed", { err: errText(e) }) });
    } finally {
      setSaving(false);
    }
  };

  // Reset the edited document to the last-saved snapshot. In Code mode the YAML
  // buffer is a separate editor state, so revert alone would leave it showing the
  // discarded edits (and the debounced parser would re-apply them); clearing
  // `rawFor` re-seeds the buffer from the reverted schema, like an index switch.
  const reset = () => {
    revertChanges();
    if (rawMode) setRawFor("");
  };

  // Code mode: the buffer is just another editor of the in-memory document —
  // it seeds from the current schema's YAML and syncs back through /api/parse
  // as you type. The one global Save then writes files, same as visual edits.
  const openRaw = () => {
    const onDisk = project?.indexes.find((i) => i.name === active)?.raw;
    setRawText(preview?.yaml ?? onDisk ?? "");
    setRawFor(active);
    setRawMode(true);
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
      run: reset,
    },
    {
      id: "cmd.deployment",
      category: "action",
      title: t("sidebar.deployment"),
      keywords: "deployment settings config connection sinks",
      detail: { body: t("search.descDeployment"), enter: runAction },
      run: () => {
        setBrowseCatalog(false);
        setActive("config");
      },
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
    {
      id: "cmd.newIndex",
      category: "action",
      title: t("sidebar.newIndex"),
      keywords: "new index create add schema file",
      detail: { body: t("search.descNewIndex"), enter: runAction },
      run: () => setNewIndexOpen(true),
    },
    {
      id: "cmd.undo",
      category: "action",
      title: t("search.undo"),
      keywords: "undo revert step back history",
      shortcut: "⌘Z",
      detail: { body: t("search.descUndo"), enter: runAction },
      run: () => {
        if (canUndo) undo();
      },
    },
    {
      id: "cmd.redo",
      category: "action",
      title: t("search.redo"),
      keywords: "redo forward step history",
      shortcut: "⇧⌘Z",
      detail: { body: t("search.descRedo"), enter: runAction },
      run: () => {
        if (canRedo) redo();
      },
    },
  ];
  // Visual⟷Code only applies to an index (Deployment has no code form).
  if (active !== "config")
    commands.push({
      id: "cmd.mode",
      category: "action",
      title: rawMode ? t("search.toVisual") : t("search.toCode"),
      keywords: "visual code yaml editor canvas switch mode toggle raw",
      detail: { body: t("search.descMode"), enter: runAction },
      run: () => {
        if (rawMode) setRawMode(false);
        else openRaw();
      },
    });
  // Legend as searchable reference — every node kind and field type with its
  // one-line meaning (shown in the preview pane; informational, no navigation).
  for (const k of KIND_ROWS)
    commands.push({
      id: `legend.kind.${k}`,
      category: "legendKind",
      title: k,
      keywords: `legend node kind relation ${k}`,
      color: `var(--k-${k})`,
      detail: { body: kindDesc(t, k), enter: t("search.reference") },
    });
  for (const f of TYPE_FAMILIES)
    commands.push({
      id: `legend.type.${f.varKey}`,
      category: "legendType",
      title: f.label,
      keywords: `legend field type ${f.label} ${f.varKey}`,
      color: `var(--t-${f.varKey})`,
      detail: { body: typeDesc(t, f.varKey), enter: t("search.reference") },
    });

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
      } else if (selection.kind === "columns") {
        const { path, names } = selection;
        apply((s) => removeFields(s, path, names));
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
    return <div className="p-10 text-muted-foreground">{error || t("common.loading")}</div>;

  return (
    <div className="flex h-screen flex-col">
      <header className="topbar flex items-center gap-3 border-b border-border bg-card px-4 py-2.5">
        <div className="flex flex-1 items-center gap-3">
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
          <span className="brand inline-flex items-center gap-2 font-bold">
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
        </div>

        <button
          type="button"
          onClick={() => setPaletteOpen(true)}
          className="flex h-8 w-72 min-w-0 shrink cursor-pointer items-center gap-2.5 rounded-full border border-primary/25 px-3 pr-1.5 text-xs text-muted-foreground transition-colors hover:border-primary/50"
          style={{ background: "linear-gradient(90deg, var(--accent-soft), transparent 55%), var(--panel-2)" }}
        >
          <GlowDot />
          <span className="truncate">{t("search.placeholder")}</span>
          <Kbd className="ml-auto">⌘K</Kbd>
        </button>

        <div className="flex flex-1 items-center justify-end gap-3">
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

          {/* global: theme toggle + language picker */}
          <Hint label={t("topbar.toggleTheme")}>
            <Button variant="ghost" size="icon-sm" aria-label={t("topbar.toggleTheme")} onClick={toggleTheme}>
              {theme === "dark" ? <Sun /> : <Moon />}
            </Button>
          </Hint>
          <DropdownMenu>
            <Hint label={t("topbar.language")}>
              <DropdownMenuTrigger asChild>
                <Button variant="ghost" size="icon-sm" aria-label={t("topbar.language")}>
                  <Languages />
                </Button>
              </DropdownMenuTrigger>
            </Hint>
            <DropdownMenuContent align="end">
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

          {/* Visual ⟷ Code: swaps the whole body between the canvas and the YAML
            editor. Global because the mode survives index switches; inert on
            the Deployment screen, which has no code representation. */}
          <Hint label={t("topbar.modeHint")}>
            <span className="flex items-center gap-2">
              <Label
                htmlFor="mode-toggle"
                className={cn(
                  "cursor-pointer text-xs",
                  rawMode ? "text-muted-foreground" : "font-medium",
                  active === "config" && "opacity-50",
                )}
              >
                {t("topbar.visual")}
              </Label>
              <Switch
                id="mode-toggle"
                checked={rawMode}
                disabled={active === "config"}
                onCheckedChange={(on) => (on ? openRaw() : setRawMode(false))}
              />
              <Label
                htmlFor="mode-toggle"
                className={cn(
                  "cursor-pointer text-xs",
                  rawMode ? "font-medium" : "text-muted-foreground",
                  active === "config" && "opacity-50",
                )}
              >
                {t("topbar.code")}
              </Label>
            </span>
          </Hint>

          <div className="mx-1 h-5 w-px bg-border" />

          {/* deployment actions — the whole config */}
          <Hint label={t("search.descValidate")}>
            <Button variant="secondary" size="sm" onClick={() => void validate()} disabled={validating}>
              <span className={BTN_ICON}>{validating ? <span className="spinner" /> : <CircleCheck />}</span>
              {t("topbar.validate")}
            </Button>
          </Hint>
          <Hint label={t("topbar.resetHint")}>
            <Button variant="secondary" size="sm" onClick={reset} disabled={!dirty || saving}>
              <span className={BTN_ICON}>
                <RotateCcw />
              </span>
              {t("topbar.reset")}
            </Button>
          </Hint>
          <Hint label={dirty ? t("topbar.unsaved") : t("topbar.upToDate")}>
            <Button size="sm" onClick={() => void save()} disabled={saving || !dirty}>
              <span className={BTN_ICON}>
                {saving ? <span className="spinner" /> : dirty ? <span className="dirty-dot" /> : <Save />}
              </span>
              {t("topbar.save")}
            </Button>
          </Hint>
        </div>
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
        {active !== "config" && schema && !browseCatalog && (
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
            {/* Mode-scoped editor preferences — only meaningful while Code
                mode is showing, so they live on the index bar, not globally. */}
            {rawMode && (
              <>
                <Hint label={t("code.autoFormatHint")}>
                  <span className="flex items-center gap-1.5">
                    <Switch id="autoformat-toggle" checked={autoFormat} onCheckedChange={toggleAutoFormat} />
                    <Label htmlFor="autoformat-toggle" className="cursor-pointer text-xs text-muted-foreground">
                      {t("code.autoFormat")}
                    </Label>
                  </span>
                </Hint>
                <Hint label={t("raw.vimHint")}>
                  <span className="flex items-center gap-1.5">
                    <Switch id="vim-toggle" checked={vimMode} onCheckedChange={toggleVim} />
                    <Label htmlFor="vim-toggle" className="cursor-pointer font-mono text-2xs text-muted-foreground">
                      VIM
                    </Label>
                  </span>
                </Hint>
                <div className="mx-1 h-5 w-px bg-border" />
              </>
            )}
            <Hint label={t("search.descYaml")}>
              <Button variant="secondary" size="sm" onClick={() => setDrawer(true)}>
                <Eye /> {t("topbar.yaml")}
              </Button>
            </Hint>
          </div>
        )}

        {leftOpen && (
          <nav className="sidebar col-start-1 row-start-1 row-span-2 flex min-h-0 flex-col border-r border-border bg-card">
            <div className="min-h-0 flex-1 overflow-y-auto p-2">
              <button
                className={cn(NAV, "flex items-center gap-1.5", active === "config" && !browseCatalog && NAV_ACTIVE)}
                onClick={() => {
                  setBrowseCatalog(false);
                  setActive("config");
                }}
              >
                <Settings className="size-3.5 shrink-0" /> {t("sidebar.deployment")}
              </button>
              <button
                className={cn(NAV, "flex items-center gap-1.5", browseCatalog && NAV_ACTIVE)}
                onClick={() => setBrowseCatalog(true)}
              >
                <Table2 className="size-3.5 shrink-0" /> {t("topbar.tables")}
              </button>
              <div className={NAV_HEADING}>{t("sidebar.indexes")}</div>
              {(config.index ?? []).map((e) => (
                <button
                  key={e.name}
                  className={cn(NAV, active === e.name && !browseCatalog && NAV_ACTIVE)}
                  onClick={() => {
                    setBrowseCatalog(false);
                    openIndex(e.name);
                  }}
                >
                  {indexDirty(e.name) && <span className="dirty-dot" />}
                  {e.name}
                  {!e.enabled && <span className="text-muted-foreground"> {t("sidebar.off")}</span>}
                </button>
              ))}
              <NewIndex
                tables={catalog?.catalog.tables.map((tbl) => tbl.name) ?? []}
                junctions={catalog?.junctions.map((j) => j.table.table) ?? []}
                onCreate={createIndex}
                open={newIndexOpen}
                onOpenChange={setNewIndexOpen}
              />
            </div>
            {/* Colour key — open by default, but collapsible so a long index list
                isn't crowded out. Pinned below the scrolling list. */}
            <details className="legend group shrink-0 border-t border-border py-2" open>
              <summary className="flex cursor-pointer list-none items-center gap-1.5 px-1.5 py-1 text-2xs font-semibold uppercase tracking-caps text-muted-foreground [&::-webkit-details-marker]:hidden">
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

        {browseCatalog ? (
          <main className="col-start-2 row-start-2 flex min-h-0 flex-col">
            {catalog ? (
              <CatalogBrowser catalog={catalog} />
            ) : (
              <div className="flex flex-1 items-center justify-center">
                <span className="spinner" />
              </div>
            )}
          </main>
        ) : active === "config" ? (
          <main className="col-start-2 row-start-2 min-h-0 overflow-y-auto p-4">
            <ConfigPanel config={config} onChange={setConfig} onDuplicate={dupIndex} />
          </main>
        ) : rawMode ? (
          <main className="raw-pane col-start-2 row-start-2 flex min-h-0 flex-col">
            <Suspense
              fallback={
                <div className="flex min-h-0 flex-1 items-center justify-center text-xs text-muted-foreground">
                  <span className="spinner" />
                </div>
              }
            >
              <CodeView
                value={rawText}
                onChange={setRawText}
                fileName={config.index?.find((e) => e.name === active)?.schema ?? `${active}.schema.yml`}
                dirty={indexDirty(active)}
                vim={vimMode}
                autoFormat={autoFormat}
                onSave={() => void save()}
                parseError={codeProblem}
                diagnostics={liveDiags}
              />
            </Suspense>
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
            {inspectorOpen && (
              <aside className="col-start-3 row-start-2 min-h-0 overflow-y-auto border-l border-border bg-card p-3.5">
                <Inspector />
              </aside>
            )}
          </DesignProvider>
        ) : null}

        {/* The preview drawer works in both Visual and Code mode (hence outside
            the canvas branch); its dim backdrop closes it on click (plus Esc / ✕). */}
        {schema && (
          <Drawer open={drawer} onOpenChange={setDrawer} direction="right">
            <DrawerContent className="data-[vaul-drawer-direction=right]:w-[min(46rem,92vw)] data-[vaul-drawer-direction=right]:sm:max-w-none">
              <DrawerHeader className="flex-row items-center gap-2 border-b border-border p-3" data-vaul-no-drag>
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
        )}
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

/// The op tag on a review row: move / delete / new (a plain modify has none, its
/// +/- counts carry it). Tones match the canvas language (warn = move, destructive
/// = delete, accent = new).
function OpBadge({ d }: { d: OpDiff }) {
  const { t } = useT();
  const pill = "rounded px-1 py-px text-3xs font-bold uppercase tracking-caps";
  if (d.op === "delete")
    return <span className={cn(pill, "bg-destructive/10 text-destructive")}>{t("diff.opDelete")}</span>;
  if (d.op === "move") return <span className={cn(pill, "bg-warn/10 text-warn")}>{t("diff.opMove")}</span>;
  if (d.current === "") return <span className={cn(pill, "bg-accent2/10 text-accent2")}>{t("diff.newFile")}</span>;
  return null;
}

function DiffModal({
  diffs,
  doc,
  saving,
  onConfirm,
  onCancel,
}: {
  diffs: OpDiff[];
  doc: Doc;
  saving: boolean;
  onConfirm: (paths: string[]) => void;
  onCancel: () => void;
}) {
  const { t } = useT();
  const [mode, setMode] = useState<DiffMode>("split");
  const [selected, setSelected] = useState(0);
  const [check, setCheck] = useState<Check>({ state: "loading" });
  const searchRef = useRef<HTMLInputElement>(null);
  // Config (flusso.toml) first, then the schema files sorted by path so the same
  // folder's files sit together — the list reads as a folder tree, and ↑/↓ nav
  // walks it in the shown order.
  const changed = useMemo(() => {
    const list = diffs.filter((d) => d.changed);
    const isConfig = (d: OpDiff) => d.path.endsWith("flusso.toml");
    return [...list.filter(isConfig), ...list.filter((d) => !isConfig(d)).sort((a, b) => a.path.localeCompare(b.path))];
  }, [diffs]);
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

  // Group the shown files by folder (they're already path-sorted, so same-folder
  // rows are adjacent) so the review reads as a tree; a folder header toggles its
  // whole subtree.
  const groups: { dir: string; files: OpDiff[] }[] = [];
  for (const d of shown) {
    const short = shortenPath(d.path);
    const cut = short.lastIndexOf("/");
    const dir = cut < 0 ? "" : short.slice(0, cut);
    const last = groups[groups.length - 1];
    if (last?.dir === dir) last.files.push(d);
    else groups.push({ dir, files: [d] });
  }
  const toggleFolder = (files: OpDiff[], clear: boolean) =>
    setInclude((s) => {
      const next = new Set(s);
      for (const f of files)
        if (clear) next.delete(f.path);
        else next.add(f.path);
      return next;
    });

  const ignoreList = () => changed.filter((d) => !include.has(d.path)).map((d) => d.path);

  // Dialog-wide keybindings, so the whole review is keyboard-drivable from the
  // (auto-focused) filter: ↑/↓ move through the shown files, Enter toggles the
  // selected file's inclusion, ⌘/Ctrl+Enter writes. Letters/space fall through
  // to the filter; Esc is handled in onEscapeKeyDown. Position is tracked in
  // `shown` and mapped back to `changed`.
  const onDialogKey = (e: React.KeyboardEvent) => {
    if ((e.metaKey || e.ctrlKey) && e.key === "Enter") {
      e.preventDefault();
      if (include.size > 0 && !saving) onConfirm(ignoreList());
      return;
    }
    if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "a") {
      // Repurpose ⌘/Ctrl+A (would select the filter text) as toggle-all-shown.
      e.preventDefault();
      toggleAll();
      return;
    }
    if (shown.length === 0) return;
    const pos = shown.findIndex((d) => changed.indexOf(d) === selected);
    if (e.key === "ArrowDown") {
      e.preventDefault();
      setSelected(changed.indexOf(shown[Math.min(shown.length - 1, pos + 1)]));
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      setSelected(changed.indexOf(shown[pos <= 0 ? 0 : pos - 1]));
    } else if (e.key === "Enter") {
      e.preventDefault();
      toggle(shown[pos < 0 ? 0 : pos].path);
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
      <DialogContent
        className="flex h-[92vh] w-[96vw] max-w-none flex-col sm:max-w-none"
        aria-label={t("diff.aria")}
        onKeyDown={onDialogKey}
        onEscapeKeyDown={(e) => {
          // Esc never closes this review by accident. If a file filter is
          // active, the first Esc just clears it; otherwise Esc is a no-op —
          // close via Cancel / ✕ / backdrop.
          e.preventDefault();
          if (query) setQuery("");
        }}
        onOpenAutoFocus={(e) => {
          // Land in the file filter when it's present, instead of Radix's default
          // (the first focusable — a view-toggle button).
          if (searchRef.current) {
            e.preventDefault();
            searchRef.current.focus();
          }
        }}
      >
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
                  ref={searchRef}
                  value={query}
                  onChange={(e) => setQuery(e.target.value)}
                  placeholder={t("diff.filterFiles")}
                  className="min-w-0 flex-1 bg-transparent text-xs outline-none placeholder:text-muted-foreground"
                  {...NO_PW_MANAGER}
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
              {groups.map((g) => {
                const folderOn = g.files.every((f) => include.has(f.path));
                const folderNone = g.files.every((f) => !include.has(f.path));
                return (
                  <div key={g.dir || "."}>
                    {g.dir && (
                      <label className="flex cursor-pointer items-center gap-2 border-b border-border/60 bg-secondary/40 px-3 py-1 text-2xs font-medium text-muted-foreground">
                        <Checkbox
                          className="size-3.5"
                          checked={folderOn ? true : folderNone ? false : "indeterminate"}
                          onCheckedChange={() => toggleFolder(g.files, folderOn)}
                          aria-label={g.dir}
                        />
                        <Folder className="size-3.5 shrink-0" />
                        <span className="truncate font-mono">{g.dir}</span>
                      </label>
                    )}
                    {g.files.map((d) => {
                      const i = changed.indexOf(d);
                      const s = diffStats(d.current, d.next);
                      const short = shortenPath(d.path);
                      const base = short.slice(short.lastIndexOf("/") + 1);
                      const on = include.has(d.path);
                      return (
                        <div
                          key={d.path}
                          className={cn(
                            "flex items-center gap-2 border-b border-border/60 border-l-2 pr-2 transition-colors",
                            g.dir ? "pl-6" : "pl-3",
                            i === selected
                              ? "border-l-primary bg-background"
                              : "border-l-transparent hover:bg-accent/50",
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
                            title={d.from ? t("diff.movedFrom", { path: shortenPath(d.from) }) : d.path}
                            onClick={() => setSelected(i)}
                            className={cn(
                              "flex min-w-0 flex-1 cursor-pointer flex-col gap-0.5 py-2 text-left",
                              !on && "opacity-45",
                            )}
                          >
                            <span className="flex items-center gap-1 truncate font-mono text-xs">
                              <span className="truncate font-medium text-foreground">{base}</span>
                              {d.warning === "outside_base" && (
                                <TriangleAlert
                                  className="size-3 shrink-0 text-warn"
                                  aria-label={t("diff.warnOutside")}
                                />
                              )}
                            </span>
                            <span className="flex items-center gap-2 font-mono text-2xs tabular-nums">
                              <OpBadge d={d} />
                              {d.op !== "delete" && <span className="text-diff-add-num">+{s.adds}</span>}
                              {d.op !== "write" || d.current !== "" ? (
                                <span className="text-diff-del-num">-{s.dels}</span>
                              ) : null}
                            </span>
                          </button>
                        </div>
                      );
                    })}
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
        <DialogFooter className="sm:items-center sm:justify-between">
          <div className="hidden flex-wrap items-center gap-x-3 gap-y-1 text-2xs text-muted-foreground sm:flex">
            <span className="flex items-center gap-1.5">
              <KbdGroup>
                <Kbd>↑</Kbd>
                <Kbd>↓</Kbd>
              </KbdGroup>
              {t("diff.kbdNavigate")}
            </span>
            <span className="flex items-center gap-1.5">
              <Kbd>↵</Kbd>
              {t("diff.kbdToggle")}
            </span>
            <span className="flex items-center gap-1.5">
              <KbdGroup>
                <Kbd>⌘</Kbd>
                <Kbd>A</Kbd>
              </KbdGroup>
              {t("diff.kbdToggleAll")}
            </span>
            <span className="flex items-center gap-1.5">
              <KbdGroup>
                <Kbd>⌘</Kbd>
                <Kbd>↵</Kbd>
              </KbdGroup>
              {t("diff.kbdSave")}
            </span>
            <span className="flex items-center gap-1.5">
              <Kbd>Esc</Kbd>
              {t("diff.kbdClear")}
            </span>
          </div>
          <div className="flex gap-2">
            <Button variant="secondary" size="sm" onClick={onCancel}>
              {t("common.cancel")}
            </Button>
            <Button size="sm" onClick={() => onConfirm(ignoreList())} disabled={saving || include.size === 0}>
              {saving && <span className="spinner" />}
              {t("diff.write", { n: include.size })}
            </Button>
          </div>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

// A two-step "New index" wizard: (1) details — name + root table, (2) file —
// where the schema file lands. Opened from the sidebar as a modal.
function NewIndex({
  tables,
  junctions,
  onCreate,
  open,
  onOpenChange,
}: {
  tables: string[];
  junctions: string[];
  onCreate: (name: string, table: string, schemaPath: string) => void;
  open: boolean;
  onOpenChange: (open: boolean) => void;
}) {
  const { t } = useT();
  const [step, setStep] = useState(0);
  const [name, setName] = useState("");
  const [table, setTable] = useState(tables[0] ?? "");
  // Where the schema file lands: a picked directory ("" = the config-dir root)
  // plus a filename. The `.schema.yml` suffix is locked (you type just the base,
  // tracking the index name) unless you override it for a non-standard name.
  const [dir, setDir] = useState("");
  const [base, setBase] = useState("");
  const [baseEdited, setBaseEdited] = useState(false);
  const [override, setOverride] = useState(false);
  const [overrideName, setOverrideName] = useState("");
  const [dirs, setDirs] = useState<string[]>([]);
  const SUFFIX = ".schema.yml";
  const effectiveBase = (baseEdited ? base : name).trim();
  const effectiveFile = override ? overrideName.trim() : effectiveBase ? `${effectiveBase}${SUFFIX}` : "";
  const cleanDir = dir.replace(/^\/+|\/+$/g, "");
  const effectivePath = cleanDir && effectiveFile ? `${cleanDir}/${effectiveFile}` : effectiveFile;
  const junctionSet = new Set(junctions);

  // Load the real subfolders under flusso.toml whenever the dialog opens.
  useEffect(() => {
    if (!open) return;
    let alive = true;
    api
      .dirs()
      .then((d) => alive && setDirs(d))
      .catch(() => {
        /* offline / unreadable — the picker just offers root + custom entry */
      });
    return () => {
      alive = false;
    };
  }, [open]);

  const reset = () => {
    onOpenChange(false);
    setStep(0);
    setName("");
    setTable(tables[0] ?? "");
    setDir("");
    setBase("");
    setBaseEdited(false);
    setOverride(false);
    setOverrideName("");
  };
  const detailsOk = !!name && !!table;
  const create = () => {
    if (!detailsOk || !effectivePath) return;
    onCreate(name, table, effectivePath);
    reset();
  };

  const steps = [t("sidebar.stepDetails"), t("sidebar.stepFile")];
  const dirOptions = [
    { value: "", label: t("sidebar.rootDir") },
    ...dirs.map((d) => ({ value: d, label: d, icon: <Folder className="size-3.5" /> })),
  ];

  return (
    <>
      <button
        className={cn(NAV, "mt-1.5 border border-dashed border-border text-primary")}
        onClick={() => {
          // The initial `table` state captured the catalog before it loaded —
          // default it now, from the tables that actually exist.
          if (!table && tables[0]) setTable(tables[0]);
          onOpenChange(true);
        }}
      >
        + {t("sidebar.newIndex")}
      </button>
      <Dialog open={open} onOpenChange={(o) => (o ? onOpenChange(true) : reset())}>
        <DialogContent
          className="sm:max-w-md"
          onKeyDown={(e) => {
            if (e.key !== "Enter") return;
            // React events bubble through the tree even from a portalled popover,
            // so Enter inside the directory/table combobox (cmdk) would otherwise
            // submit the wizard. Only submit from the plain text fields — skip
            // cmdk and buttons (which handle Enter themselves).
            const el = e.target as HTMLElement;
            if (el.closest("[cmdk-root]") || el.tagName === "BUTTON") return;
            e.preventDefault();
            if (step === 0 && detailsOk) setStep(1);
            else if (step === 1) create();
          }}
        >
          <DialogHeader>
            <DialogTitle>{t("sidebar.newIndexTitle")}</DialogTitle>
            <DialogDescription>{t("sidebar.newIndexDesc")}</DialogDescription>
          </DialogHeader>

          <WizardSteps steps={steps} current={step} />

          {step === 0 ? (
            <div className="flex flex-col gap-3">
              <Field label={t("sidebar.indexName")}>
                <Text value={name} onChange={setName} placeholder={t("sidebar.indexName")} />
              </Field>
              <Field label={t("sidebar.rootTable")}>
                {tables.length ? (
                  <Combobox
                    value={table}
                    onChange={setTable}
                    placeholder={t("sidebar.rootTable")}
                    options={tables.map((tbl) => ({
                      label: tbl,
                      value: tbl,
                      description: junctionSet.has(tbl) ? t("catalog.junction") : undefined,
                      icon: junctionSet.has(tbl) ? (
                        <Waypoints className="size-3.5 text-accent2" />
                      ) : (
                        <Table2 className="size-3.5" />
                      ),
                    }))}
                  />
                ) : (
                  <Text value={table} onChange={setTable} placeholder={t("sidebar.rootTable")} />
                )}
              </Field>
            </div>
          ) : (
            <div className="flex flex-col gap-3">
              <Field label={t("sidebar.schemaDir")}>
                <Combobox
                  value={dir}
                  onChange={setDir}
                  options={dirOptions}
                  allowCustom
                  placeholder={t("sidebar.rootDir")}
                />
              </Field>
              <Field label={t("sidebar.schemaFile")}>
                {override ? (
                  <div className="flex items-center gap-1.5">
                    <Text
                      value={overrideName}
                      onChange={setOverrideName}
                      placeholder={name ? `${name}.schema.yml` : "x.schema.yml"}
                      className="flex-1"
                    />
                    <Hint label={t("sidebar.lockSuffix")} side="left">
                      <Button
                        variant="ghost"
                        size="icon-sm"
                        aria-label={t("sidebar.lockSuffix")}
                        onClick={() => setOverride(false)}
                      >
                        <RotateCcw />
                      </Button>
                    </Hint>
                  </div>
                ) : (
                  <div className="flex items-center gap-1.5">
                    {/* Type just the base name; the `.schema.yml` suffix is a locked adornment. */}
                    <div className="flex h-8 flex-1 items-center rounded-md border border-border bg-secondary pr-2 text-sm transition-colors focus-within:border-ring focus-within:ring-[3px] focus-within:ring-ring/50">
                      <input
                        value={baseEdited ? base : name}
                        onChange={(e) => {
                          setBaseEdited(true);
                          setBase(e.target.value);
                        }}
                        placeholder={name || "name"}
                        className="min-w-0 flex-1 bg-transparent px-2.5 py-1 outline-none"
                        {...NO_PW_MANAGER}
                      />
                      <span className="shrink-0 font-mono text-muted-foreground">{SUFFIX}</span>
                    </div>
                    <Hint label={t("sidebar.overrideName")} side="left">
                      <Button
                        variant="ghost"
                        size="icon-sm"
                        aria-label={t("sidebar.overrideName")}
                        onClick={() => {
                          setOverrideName(effectiveFile);
                          setOverride(true);
                        }}
                      >
                        <Pencil />
                      </Button>
                    </Hint>
                  </div>
                )}
              </Field>
              <p className="truncate font-mono text-2xs text-muted-foreground" title={effectivePath}>
                {effectivePath || t("sidebar.schemaFileHint")}
              </p>
            </div>
          )}

          <DialogFooter className="sm:justify-between">
            <Button variant="ghost" size="sm" onClick={() => (step === 0 ? reset() : setStep(0))}>
              {step === 0 ? t("common.cancel") : t("common.back")}
            </Button>
            {step === 0 ? (
              <Button size="sm" disabled={!detailsOk} onClick={() => setStep(1)}>
                {t("common.next")}
              </Button>
            ) : (
              <Button size="sm" disabled={!effectivePath} onClick={create}>
                {t("sidebar.create")}
              </Button>
            )}
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </>
  );
}

/// A compact step rail for the New-index wizard: a numbered dot per step, the
/// current one filled, with the step labels beneath.
function WizardSteps({ steps, current }: { steps: string[]; current: number }) {
  return (
    <div className="flex items-center gap-2">
      {steps.map((label, i) => (
        <div key={label} className="flex flex-1 items-center gap-2">
          <span
            className={cn(
              "grid size-5 shrink-0 place-items-center rounded-full text-2xs font-bold transition-colors",
              i <= current ? "bg-primary text-background" : "bg-secondary text-muted-foreground",
            )}
          >
            {i + 1}
          </span>
          <span className={cn("text-xs", i === current ? "font-medium text-foreground" : "text-muted-foreground")}>
            {label}
          </span>
          {i < steps.length - 1 && <span className="h-px flex-1 bg-border" />}
        </div>
      ))}
    </div>
  );
}
