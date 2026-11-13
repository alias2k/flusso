import { lazy, Suspense, useCallback, useEffect, useMemo, useRef, useState, useSyncExternalStore } from "react";
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
  Link2,
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
import type {
  ColumnShape,
  ConfigIdentity,
  ConfigToml,
  DiagnosticDto,
  FileOp,
  IndexSchema,
  OpDiff,
  SchemaIdentity,
  ValidateResponse,
} from "./api";
import { api } from "./api";
import { Canvas } from "./components/Canvas";
import { CatalogBrowser } from "./components/CatalogBrowser";
import { DiffView } from "./components/DiffView";
import { diffStats, type DiffMode } from "./model/diff";
import { parseJson, roundedFields } from "./model/exact";
import { flushPendingValues, hasPendingValues, subscribePendingValues } from "./model/pending";
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
import { adoptIds, fieldIds, stripUids } from "./model/ids";
import { cleanSchemaPath, indexName, isConfigFile, pairRows, type RowPair, schemaPath } from "./model/indexRows";
import { useNameRefusal } from "./components/nameRefusal";
import { formatRoute, parseRoute, type Route } from "./router";
import { countTypeMismatches, fixAllTypes, requiredDefaultIssues } from "./model/issues";
import { pruneIncomplete } from "./model/prune";
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

/// The doc as JSON without the client-only node uids — what Save would write.
const bareDocJson = (doc: Doc): string =>
  JSON.stringify({
    config: doc.config,
    schemas: Object.fromEntries(Object.entries(doc.schemas).map(([name, s]) => [name, stripUids(s)])),
  });

/// A schema as JSON without the client-only node uids ("" for none).
const bareSchema = (schema: IndexSchema | undefined): string => (schema ? JSON.stringify(stripUids(schema)) : "");

// Which field of a save's base each field of `schema` is: the base is the
// Code-mode text when there is one, else the file as last loaded or saved.
// Without either (a new index) there is nothing to pair with.
const identityFor = (
  schema: IndexSchema,
  text: { ids?: SchemaIdentity["before"] } | undefined,
  savedSchema: IndexSchema | undefined,
): SchemaIdentity | undefined => {
  const before = text ? text.ids : savedSchema && fieldIds(savedSchema.fields);
  const after = fieldIds(schema.fields);
  return before && after && { before, after };
};

// For every sink of `config`, by name, its name in `saved` (the file on disk),
// matched by the session's sink ids; `null` for a sink added since. Without
// ids on either side the save pairs sinks by name.
const sinkIdentity = (saved: ConfigToml, config: ConfigToml): Record<string, string | null> | undefined => {
  const was = saved.sinkIds;
  const is = config.sinkIds;
  if (!was || !is) return undefined;
  const byId = new Map(
    Object.entries(was)
      .filter(([name]) => name in (saved.sinks ?? {}))
      .map(([name, id]) => [id, name]),
  );
  return Object.fromEntries(
    Object.keys(config.sinks ?? {}).map((name) => {
      const id = is[name];
      return [name, (id && byId.get(id)) ?? null];
    }),
  );
};

// What a preview of `schema` (the index `index`) writes it into, as a save
// would: the Code-mode text when it is newer than the file, else the file the
// index was last saved at (a pending path change has not moved it yet). A new
// index has no file of its own: a file at its path lends it no comments.
const schemaFile = (
  index: string,
  schema: IndexSchema,
): { path?: string; base?: string; identity?: SchemaIdentity } => {
  const { doc, saved, texts } = useDesignStore.getState();
  const entry = doc?.config.index?.find((e) => e.name === index);
  const savedDoc = saved ? (parseJson(saved) as Doc) : undefined;
  const was = entry?.id ? savedDoc?.config.index?.find((e) => e.id === entry.id) : undefined;
  const identity = identityFor(schema, texts[index], was && savedDoc?.schemas[was.name]);
  return { path: was?.schema, base: texts[index]?.text, identity };
};

// Bumped to drop a Code-mode parse reply that a newer parse superseded.
let parseSeq = 0;
// Set when Code mode was refused: the route effect then replaces the URL
// instead of pushing one, so a refusal can't bury forward history.
let replaceNextRoute = false;

/// The path of what the main area shows now (see router.ts).
const shownPath = (): string => {
  const { browseCatalog, rawMode } = useUiStore.getState();
  const { active } = useDesignStore.getState();
  return formatRoute(
    browseCatalog
      ? { view: "tables" }
      : active === "config"
        ? { view: "deployment" }
        : { view: "index", name: active, code: rawMode },
  );
};

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
  const clearTexts = useDesignStore((s) => s.clearTexts);
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

  // Auto-dismiss toasts; one that asks for a choice stays longer.
  useEffect(() => {
    if (!toast) return;
    const t = setTimeout(() => setToast(null), toast.action ? 10000 : 3500);
    return () => clearTimeout(t);
  }, [toast, setToast]);
  // A choice about the buffer goes stale once the buffer changes.
  useEffect(() => {
    if (useUiStore.getState().toast?.action) setToast(null);
  }, [rawText, setToast]);

  // URL → state, set below once its handlers exist; a ref so the one popstate
  // listener always sees the latest ones.
  const applyRouteRef = useRef<(route: Route) => void>(() => undefined);

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
    const replace = !routedOnce.current || replaceNextRoute;
    replaceNextRoute = false;
    if (window.location.pathname === routePath) return;
    window.history[replace ? "replaceState" : "pushState"](null, "", routePath);
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
  // Compared without the client-only node uids: a uid-only change (a container
  // removed and re-added as-is) writes nothing, so it must not read as unsaved.
  const savedDoc = useMemo(() => (saved ? (parseJson(saved) as Doc) : null), [saved]);
  const savedBare = useMemo(() => (savedDoc ? bareDocJson(savedDoc) : null), [savedDoc]);
  const savedSchemasBare = useMemo(
    () => new Map(Object.entries(savedDoc?.schemas ?? {}).map(([name, s]) => [name, JSON.stringify(stripUids(s))])),
    [savedDoc],
  );
  const dirty = useMemo(() => !!doc && bareDocJson(doc) !== savedBare, [doc, savedBare]);
  // Typed in Code mode but not applied yet (the sync's debounce): Save flushes
  // it first, so it must not wait for the document to move.
  const bufferAhead = useUiStore((s) => s.rawMode && !!s.seed && s.rawText !== s.seed.text);
  // A value still being read may be the only change: Save waits for it.
  const valuesPending = useSyncExternalStore(subscribePendingValues, hasPendingValues);
  const typingName = useUiStore((s) => s.nameDraft !== null);
  // Code-mode text not yet saved; may differ from the file in comments alone.
  const textsPending = useDesignStore((s) => Object.keys(s.texts).length > 0);
  const canSave = dirty || bufferAhead || valuesPending || typingName || textsPending;
  // An index is compared with its saved self, the same row: a renamed index
  // whose schema didn't change is not dirty.
  const indexDirty = (name: string, of: Doc | null = doc): boolean => {
    if (!of || !savedDoc) return false;
    const old = pairRows(savedDoc.config.index, of.config.index).find((p) => p.entry.name === name)?.old;
    const current = of.schemas[name];
    const saved = old ? savedSchemasBare.get(old.name) : undefined;
    return (current ? JSON.stringify(stripUids(current)) : undefined) !== saved;
  };
  const isConfigDirty = useCallback(
    (of: Doc | null) => !!of && (!savedDoc || JSON.stringify(of.config) !== JSON.stringify(savedDoc.config)),
    [savedDoc],
  );
  const configDirty = useMemo(() => isConfigDirty(doc), [doc, isConfigDirty]);

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

  // Debounced live preview of the active index. Its failure is its own banner,
  // not the shared `error`, so the next preview that succeeds can clear it
  // without wiping a project or catalog load error. A reply for a schema that
  // has since changed is dropped, so a slow failure can't land after a success.
  const [previewError, setPreviewError] = useState("");
  useEffect(() => setPreviewError(""), [active]);
  const pruned = useMemo(() => (schema ? pruneIncomplete(schema) : undefined), [schema]);
  useEffect(() => {
    if (!pruned || active === "config") {
      setPreview(null);
      setPreviewError("");
      return;
    }
    let current = true;
    const handle = setTimeout(() => {
      api
        .preview(active, pruned.schema, schemaFile(active, pruned.schema))
        .then((p) => {
          if (!current) return;
          setPreview(p);
          setPreviewError("");
        })
        .catch((e) => current && setPreviewError(String(e)));
    }, 250);
    return () => {
      current = false;
      clearTimeout(handle);
    };
  }, [active, pruned, setPreview]);

  // Code mode: the buffer is another editor of the in-memory document. `seed`
  // records the text the buffer last agreed with the document on (seeded from
  // it, or applied to it) and that schema, bare. Two rules keep the two from
  // disagreeing silently:
  // - the schema moving away from `seed.schema` (undo, redo, Reset) re-seeds
  //   the buffer, and the stale buffer is never applied;
  // - the buffer is applied when its text moved away from `seed.text`: by the
  //   debounced sync while typing, and at once by `flushBuffer` on every way
  //   out of Code mode and on Save.
  // The sync parses server-side (the same strict parser that reads the file),
  // so undo, the preview, and Save see YAML edits exactly like visual ones.
  const seed = useUiStore((s) => s.seed);
  const setSeed = useUiStore((s) => s.setSeed);
  const rawFor = seed?.index ?? "";
  // Is the buffer the current, agreed-on editor of `index`'s schema?
  const bufferLive = (index: string): boolean => {
    const s = useUiStore.getState().seed;
    return !!s && s.index === index && bareSchema(useDesignStore.getState().doc?.schemas[index]) === s.schema;
  };
  // Write a parsed buffer into the document, and record the agreement.
  const applyBuffer = (index: string, text: string, parsed: IndexSchema) => {
    if (useDesignStore.getState().active !== index || !bufferLive(index)) return;
    const current = useDesignStore.getState().doc?.schemas[index];
    if (!current || JSON.stringify(parsed) !== bareSchema(current)) apply((prev) => adoptIds(prev, parsed));
    const now = useDesignStore.getState().doc?.schemas[index];
    useDesignStore.getState().setText(index, { text, ids: fieldIds(now?.fields ?? []) });
    setSeed({ index, text, schema: bareSchema(now), base: text });
  };

  const [codeProblem, setCodeProblem] = useState<ParseErrorInfo | null>(null);
  useEffect(() => {
    if (!rawMode || active === "config" || rawFor !== active) {
      setCodeProblem(null);
      return;
    }
    const seq = ++parseSeq;
    const handle = setTimeout(() => {
      api
        .parse(rawText)
        .then((res) => {
          if (seq !== parseSeq || !useUiStore.getState().rawMode) return;
          setCodeProblem(
            res.error ? { message: res.error, location: res.location, field: res.field, typeTag: res.type_tag } : null,
          );
          // The text the document last agreed on is never applied: a seed
          // whose YAML doesn't read back identically would rewrite the document
          // on a mere visit. `seed.text` moves with every applied edit, so an
          // edit back to the first seed's text after one is still applied.
          if (res.schema && rawText !== useUiStore.getState().seed?.text) applyBuffer(active, rawText, res.schema);
        })
        .catch(() => setCodeProblem(null));
    }, 300);
    return () => clearTimeout(handle);
    // applyBuffer reads the stores when the reply lands, so it is no dependency.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [rawMode, rawText, active, rawFor]);

  // Apply the buffer now, if it moved since it last agreed with the document.
  // `false` means it doesn't parse, so its edits since then would be lost.
  const flushBuffer = async (): Promise<boolean> => {
    const index = useDesignStore.getState().active;
    const text = useUiStore.getState().rawText;
    if (!useUiStore.getState().rawMode || !bufferLive(index) || text === useUiStore.getState().seed?.text) return true;
    parseSeq++;
    const res = await api.parse(text).catch(() => null);
    if (!res?.schema) return !bufferLive(index);
    applyBuffer(index, text, res.schema);
    // Typed while the parse was out: apply that too, or the move would leave it.
    if (useUiStore.getState().rawText !== text) return flushBuffer();
    return true;
  };

  // Every way out of the buffer (the toggle, another index, Deployment, back/
  // forward) goes through here: flush, then go. A buffer that doesn't parse
  // holds the move; only the toast's Discard drops those edits, so no click
  // pattern can lose them by accident.
  const leaveBuffer = async (go: () => void, held?: () => void) => {
    if (await flushBuffer()) return go();
    held?.();
    setToast({
      kind: "error",
      text: t("code.unparsed"),
      action: {
        label: t("code.discard"),
        run: () => {
          setSeed(null);
          go();
        },
      },
    });
  };

  // Seeding the buffer: on entering Code mode, switching index in it, or the
  // schema moving away from the buffer (undo, redo, Reset). The buffer renders
  // the *whole* schema, never the preview's YAML, which leaves out incomplete
  // fields and would delete them on the way back. A schema the backend can't
  // render yet (incomplete, or refused) goes back to the canvas instead, naming
  // what to finish. Only an index whose file failed to load has no schema; its
  // raw file is then the one thing to edit.
  const codeBlocked = useCallback(
    (left: string[] | undefined): boolean => {
      if (!left?.length) return false;
      setToast({ kind: "error", text: t("code.incomplete", { list: left.join(", ") }) });
      return true;
    },
    [t, setToast],
  );
  const refuseCode = useCallback(() => {
    replaceNextRoute = true;
    setRawMode(false);
  }, [setRawMode]);
  // Leaving Code mode, by any route, forgets the buffer: the canvas may change
  // the schema before Code opens again, so it must re-seed then.
  useEffect(() => {
    if (!rawMode) setSeed(null);
  }, [rawMode, setSeed]);
  const activeText = useDesignStore((s) => s.texts[s.active]?.text);
  useEffect(() => {
    if (!rawMode || !doc || active === "config") return;
    const at = bareSchema(schema);
    const s = useUiStore.getState().seed;
    // A Code-mode text undone (or redone) moves `activeText` with no change to
    // the schema: the buffer follows it too.
    if (s?.index === active && s.schema === at && s.base === activeText) return;
    if (!schema) {
      const text = project?.indexes.find((i) => i.name === active)?.raw ?? "";
      useUiStore.setState({ rawText: text, seed: { index: active, text, schema: at, base: activeText } });
      return;
    }
    if (codeBlocked(pruneIncomplete(schema).left)) {
      refuseCode();
      return;
    }
    let alive = true;
    api
      .preview(active, schema, schemaFile(active, schema))
      .then((p) => {
        if (!alive || !useUiStore.getState().rawMode) return;
        // One update, so the editor sees the new text and its seed together.
        useUiStore.setState({ rawText: p.yaml, seed: { index: active, text: p.yaml, schema: at, base: activeText } });
      })
      .catch((e) => {
        if (!alive) return;
        refuseCode();
        setToast({ kind: "error", text: t("code.renderFailed", { err: errText(e) }) });
      });
    return () => {
      alive = false;
    };
  }, [
    rawMode,
    doc,
    active,
    seed,
    schema,
    project,
    activeText,
    setRawText,
    setSeed,
    refuseCode,
    setToast,
    t,
    codeBlocked,
  ]);

  // ── routing: the hash mirrors what the main area shows (see router.ts) ────

  // URL → state. Behind a ref so the one popstate listener always sees the
  // latest handlers; validated against the loaded project so a stale deep link
  // can't select a nonexistent index.
  const applyRoute = (route: Route) => {
    // Back/forward out of the Code buffer leaves it like the toggle does; a
    // held move puts the URL back on what is still shown.
    const { active: current } = useDesignStore.getState();
    const leaving = route.view === "deployment" || (route.view === "index" && (route.name !== current || !route.code));
    if (useUiStore.getState().rawMode && useUiStore.getState().seed?.index === current && leaving) {
      void leaveBuffer(
        () => {
          window.history.replaceState(null, "", formatRoute(route));
          enterRoute(route);
        },
        () => window.history.replaceState(null, "", shownPath()),
      );
      return;
    }
    enterRoute(route);
  };
  const enterRoute = (route: Route) => {
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
    // Code refused: replace the entry, never push, or Back could never get
    // past it. Entering is otherwise the seed effect's job.
    const schema = useDesignStore.getState().doc?.schemas[route.name];
    if (route.code && schema && codeBlocked(pruneIncomplete(schema).left)) {
      window.history.replaceState(null, "", formatRoute({ ...route, code: false }));
      setRawMode(false);
      return;
    }
    setRawMode(route.code);
  };
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

  // …and the applied schema gets the *real* validation (columns exist, types
  // line up) against the database — it's fast, so problems surface in the rail
  // as you type instead of waiting for a manual Validate.
  const [liveDiags, setLiveDiags] = useState<DiagnosticDto[]>([]);
  useEffect(() => {
    if (!rawMode || !schema || !doc || active === "config" || catalog?.error) {
      setLiveDiags([]);
      return;
    }
    let current = true;
    const handle = setTimeout(() => {
      api
        .validate(doc.config, [{ name: active, schema }])
        .then((res) => {
          if (!current || !useUiStore.getState().rawMode) return;
          setLiveDiags(res.db_reachable && !res.error ? res.diagnostics : []);
        })
        .catch(() => current && setLiveDiags([]));
    }, 800);
    return () => {
      current = false;
      clearTimeout(handle);
    };
  }, [rawMode, schema, doc, active, catalog]);

  // Warn before leaving with unsaved changes.
  useEffect(() => {
    if (!dirty && !valuesPending && !typingName && !textsPending) return;
    const warn = (e: BeforeUnloadEvent) => {
      e.preventDefault();
      e.returnValue = "";
    };
    window.addEventListener("beforeunload", warn);
    return () => window.removeEventListener("beforeunload", warn);
  }, [dirty, valuesPending, typingName, textsPending]);

  useEffect(() => {
    loadCollapsed(active);
  }, [active, loadCollapsed]);

  // The schema-file ops a save applies, derived from the saved snapshot → the
  // current doc, each row paired with its saved self (so a rename or a path
  // change reads as a Move). `name` is kept alongside so the snapshot can
  // advance per index after the save, and `group` names the row whose op the
  // server reports for a file several rows share. A removed index plans
  // nothing: its file stays on disk. A file a row still uses is never moved
  // away. Rows on one file are all written once any of them is, and must agree
  // on its content; a row on the config file refuses the save (`refusal`).
  const buildOps = (of: Doc | null = doc): { planned: PlannedOp[]; refusal?: string } => {
    if (!of) return { planned: [] };
    const pairs = pairRows(savedDoc?.config.index, of.config.index);
    const texts = useDesignStore.getState().texts;
    for (const { entry } of pairs) {
      if (project && isConfigFile(entry.schema, project.config_path))
        return { planned: [], refusal: t("toast.schemaIsConfig", { name: entry.name, path: entry.schema }) };
      const twin = pairs.find((p) => p.entry !== entry && indexName(p.entry.name) === indexName(entry.name));
      if (twin) return { planned: [], refusal: t("toast.duplicateIndexName", { name: indexName(entry.name) }) };
    }
    const livePaths = new Set(pairs.map((p) => schemaPath(p.entry.schema)));
    const byFile = new Map<string, RowPair[]>();
    for (const pair of pairs) {
      if (!of.schemas[pair.entry.name]) {
        // No model to write (a row added in the Deployment panel, or a schema
        // that failed to load): its file is left alone, so it must be the
        // row's own file on disk.
        const { entry, old } = pair;
        if (!old)
          return { planned: [], refusal: t("toast.indexNeedsSchema", { name: entry.name, path: entry.schema }) };
        if (schemaPath(old.schema) !== schemaPath(entry.schema))
          return {
            planned: [],
            refusal: t("toast.unloadedSchemaMoved", { name: entry.name, from: old.schema, path: entry.schema }),
          };
        continue;
      }
      const path = schemaPath(pair.entry.schema);
      byFile.set(path, [...(byFile.get(path) ?? []), pair]);
    }
    const planned: PlannedOp[] = [];
    for (const [path, rows] of byFile) {
      const [first] = rows;
      const content = (p: RowPair) => JSON.stringify(stripUids(of.schemas[p.entry.name]));
      const other = rows.find((p) => content(p) !== content(first));
      if (other)
        return {
          planned: [],
          refusal: t("toast.sharedSchemaClash", { a: first.entry.name, b: other.entry.name }),
        };
      const changed = rows.some(
        ({ entry, old }) =>
          !old || schemaPath(old.schema) !== path || indexDirty(entry.name, of) || texts[entry.name] !== undefined,
      );
      if (!changed) continue;
      for (const { entry, old } of rows) {
        const schema = of.schemas[entry.name];
        const text = texts[entry.name];
        const base = text?.text;
        const identity = old ? identityFor(schema, text, savedDoc?.schemas[old.name]) : undefined;
        const content = { schema, ...(base === undefined ? {} : { base }), ...(identity ? { identity } : {}) };
        const from = old && schemaPath(old.schema);
        const op: FileOp =
          from && from !== path && !livePaths.has(from)
            ? { kind: "move", index: entry.name, from, path, ...content }
            : { kind: "upsert", index: entry.name, path, ...content };
        planned.push({ op, name: entry.name, group: first.entry.name });
      }
    }
    return { planned };
  };

  // Which `[[index]]` entry of the file on disk (the saved snapshot) each
  // entry of `of` is, by its session id.
  const configIdentity = (of: Doc): ConfigIdentity | undefined => {
    const prev = savedDoc?.config.index;
    if (!prev) return undefined;
    return {
      before: prev.map((e) => e.name),
      after: (of.config.index ?? []).map((e) => {
        const at = e.id ? prev.findIndex((p) => p.id === e.id) : -1;
        return at < 0 ? null : at;
      }),
      sinks: sinkIdentity(savedDoc.config, of.config),
    };
  };

  // Match a planned op to its resolved diff entry by index name, as the server
  // reads it.
  const diffFor = (list: OpDiff[], p: PlannedOp): OpDiff | undefined =>
    list.find((d) => d.index !== undefined && indexName(d.index) === indexName(p.group));

  // Save first shows a diff of what would change on disk; performSave applies it.
  // When the config model is unchanged we hide its diff (a formatting/comment-only
  // reflow the user didn't ask for) so an untouched flusso.toml is neither shown
  // nor rewritten.
  // In Code mode the buffer is flushed first, so the review shows exactly what
  // a confirm writes; a buffer that doesn't parse refuses the save.
  const save = async () => {
    if (!doc || useUiStore.getState().saving) return;
    const nameDraft = useUiStore.getState().nameDraft;
    if (nameDraft?.refused) {
      setToast({ kind: "error", text: t("toast.nameRefusedSave", { row: nameDraft.row, reason: nameDraft.refused }) });
      return;
    }
    // Leaving the name box applies the name typed in it.
    if (nameDraft && document.activeElement instanceof HTMLElement) document.activeElement.blur();
    setSaving(true);
    try {
      const unread = await flushPendingValues();
      if (unread.length) {
        setToast({ kind: "error", text: t("value.unreadSave", { fields: unread.join(", ") }) });
        return;
      }
      if (!(await flushBuffer())) {
        setToast({ kind: "error", text: t("code.unparsedSave") });
        return;
      }
      const loaded = useDesignStore.getState().doc;
      const entries = loaded?.config.index ?? [];
      if (loaded && entries.some((e) => cleanSchemaPath(e.schema) !== e.schema))
        setConfig({ ...loaded.config, index: entries.map((e) => ({ ...e, schema: cleanSchemaPath(e.schema) })) });
      const current = useDesignStore.getState().doc;
      if (!current) return;
      const rounded = [
        ...roundedFields(current.config, "flusso.toml"),
        ...Object.entries(current.schemas).flatMap(([name, s]) => roundedFields(s, name)),
      ];
      if (rounded.length) {
        setToast({ kind: "error", text: t("value.roundedSave", { fields: rounded.join(", ") }) });
        return;
      }
      const { planned, refusal } = buildOps(current);
      if (refusal) {
        setToast({ kind: "error", text: refusal });
        return;
      }
      const result = await api.diff(
        current.config,
        planned.map((p) => p.op),
        configIdentity(current),
      );
      const shown =
        isConfigDirty(current) || !result[0] ? result : [{ ...result[0], changed: false }, ...result.slice(1)];
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
    const { planned } = buildOps();
    const configPath = diffs[0]?.path;
    const skip = new Set(skipPaths);
    if (!configDirty && configPath) skip.add(configPath);
    setSaving(true);
    try {
      const res = await api.save(
        doc.config,
        planned.map((p) => p.op),
        [...skip],
        configIdentity(doc),
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
        clearTexts();
      } else {
        const prev = savedDoc ?? doc;
        const nextSchemas = { ...prev.schemas };
        for (const p of planned) {
          const d = diffFor(diffs, p);
          if (!d || !d.changed || skip.has(d.path)) continue; // not applied
          nextSchemas[p.name] = doc.schemas[p.name];
          clearTexts([p.name]);
        }
        const nextConfig = configSkipped ? prev.config : doc.config;
        const live = new Set((nextConfig.index ?? []).map((e) => e.name));
        for (const name of Object.keys(nextSchemas)) if (!live.has(name)) delete nextSchemas[name];
        setSaved(JSON.stringify({ config: nextConfig, schemas: nextSchemas }));
      }
      setDiffs(null);
      const n = res.written.length + res.moved.length;
      setToast({ kind: "ok", text: t("toast.saved", { n }) });
    } catch (e) {
      setToast({ kind: "error", text: t("toast.saveFailed", { err: errText(e) }) });
    } finally {
      setSaving(false);
    }
  };

  // Reset the edited document to the last-saved snapshot. In Code mode the
  // buffer re-seeds from the reverted schema (the seed effect).
  const reset = revertChanges;

  const openRaw = () =>
    void flushPendingValues().then(() => {
      const current = useDesignStore.getState().doc?.schemas[active];
      if (!codeBlocked(current ? pruneIncomplete(current).left : undefined)) setRawMode(true);
    });
  const closeRaw = () => leaveBuffer(() => setRawMode(false));

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
      run: () =>
        void leaveBuffer(() => {
          setBrowseCatalog(false);
          setActive("config");
        }),
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
        if (rawMode) void closeRaw();
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
                onCheckedChange={(on) => (on ? openRaw() : void closeRaw())}
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
            <Button variant="secondary" size="sm" onClick={reset} disabled={!(dirty || textsPending) || saving}>
              <span className={BTN_ICON}>
                <RotateCcw />
              </span>
              {t("topbar.reset")}
            </Button>
          </Hint>
          <Hint label={canSave ? t("topbar.unsaved") : t("topbar.upToDate")}>
            <Button
              size="sm"
              // Keeps focus in an index name box, so Save sees a name still being typed.
              onMouseDown={(e) => e.preventDefault()}
              onClick={() => void save()}
              disabled={saving || !canSave}
            >
              <span className={BTN_ICON}>
                {saving ? <span className="spinner" /> : canSave ? <span className="dirty-dot" /> : <Save />}
              </span>
              {t("topbar.save")}
            </Button>
          </Hint>
        </div>
      </header>

      {(error || previewError) && (
        <div className="banner error bg-destructive/10 px-4 py-2 text-xs text-destructive">{error || previewError}</div>
      )}
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
                onClick={() =>
                  void leaveBuffer(() => {
                    setBrowseCatalog(false);
                    setActive("config");
                  })
                }
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
                    // The buffer's own index is not a way out of it: from Tables
                    // this returns to its Code view, held text and all.
                    const { rawMode: inCode, seed: buffer } = useUiStore.getState();
                    if (inCode && buffer?.index === e.name && active === e.name) {
                      setBrowseCatalog(false);
                      return;
                    }
                    void leaveBuffer(() => {
                      setBrowseCatalog(false);
                      openIndex(e.name);
                    });
                  }}
                >
                  {indexDirty(e.name) && <span className="dirty-dot" />}
                  {e.name}
                  {!e.enabled && <span className="text-muted-foreground"> {t("sidebar.off")}</span>}
                </button>
              ))}
              <button
                className={cn(NAV, "mt-1.5 border border-dashed border-border text-primary")}
                onClick={() => setNewIndexOpen(true)}
              >
                + {t("sidebar.newIndex")}
              </button>
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
            <ConfigPanel
              config={config}
              onChange={setConfig}
              onDuplicate={dupIndex}
              onAddIndex={() => setNewIndexOpen(true)}
            />
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
              {rawFor === active ? (
                <CodeView
                  value={rawText}
                  seed={seed ?? undefined}
                  onChange={setRawText}
                  fileName={config.index?.find((e) => e.name === active)?.schema ?? `${active}.schema.yml`}
                  dirty={indexDirty(active)}
                  vim={vimMode}
                  autoFormat={autoFormat}
                  onSave={() => void save()}
                  parseError={codeProblem}
                  diagnostics={liveDiags}
                />
              ) : (
                <div className="flex min-h-0 flex-1 items-center justify-center text-xs text-muted-foreground">
                  <span className="spinner" />
                </div>
              )}
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
                left={pruned?.left ?? []}
                diagnostics={allDiagnostics.filter((d) => d.index === active)}
                onSample={
                  doc && schema && active !== "config"
                    ? (id?: string) => api.sample(doc.config, active, schema, id)
                    : undefined
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

      <NewIndex
        tables={catalog?.catalog.tables.map((tbl) => tbl.name) ?? []}
        junctions={catalog?.junctions.map((j) => j.table.table) ?? []}
        taken={config?.index?.map((e) => e.name) ?? []}
        onCreate={(...args) => void leaveBuffer(() => createIndex(...args))}
        open={newIndexOpen}
        onOpenChange={setNewIndexOpen}
      />
      <CommandPalette
        open={paletteOpen}
        onOpenChange={setPaletteOpen}
        doc={doc}
        catalog={catalog}
        active={active}
        commands={commands}
        leave={(go) => void leaveBuffer(go)}
      />

      {toast && (
        <div className={`toast ${toast.kind}`} role="status">
          {toast.text}
          {toast.action && (
            <Button
              variant="secondary"
              size="sm"
              className="ml-3"
              onClick={() => {
                toast.action?.run();
                setToast(null);
              }}
            >
              {toast.action.label}
            </Button>
          )}
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
/// A schema-file op a save sends, with the row it writes for and the row whose
/// op the server reports for that file (`group`, the first row on a shared file).
interface PlannedOp {
  op: FileOp;
  name: string;
  group: string;
}

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
            {d.index}.{d.path.join(".")}
          </span>{" "}
          {d.message}
        </div>
      ))}
    </div>
  );
}

/// The op tag on a review row: move / new (a plain modify has none, its +/-
/// counts carry it). Tones match the canvas language (warn = move, accent = new).
function OpBadge({ d }: { d: OpDiff }) {
  const { t } = useT();
  const pill = "rounded px-1 py-px text-3xs font-bold uppercase tracking-caps";
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
  // flusso.toml and the files that only fit the new one (a new, renamed or
  // moved index) are written together or not at all, as the server requires:
  // checking or unchecking one does the same to the others.
  const linked = useMemo(() => {
    const needing = changed.filter((d) => d.needs_config).map((d) => d.path);
    const config = changed.find((d) => d.index === undefined);
    return new Set(needing.length && config ? [config.path, ...needing] : needing);
  }, [changed]);
  const setPaths = (paths: string[], on: boolean) =>
    setInclude((s) => {
      const next = new Set(s);
      const all = paths.some((p) => linked.has(p)) ? [...paths, ...linked] : paths;
      for (const p of all)
        if (on) next.add(p);
        else next.delete(p);
      return next;
    });
  const toggle = (path: string) => setPaths([path], !include.has(path));
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
    setPaths(
      files.map((f) => f.path),
      !clear,
    );

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
    setPaths(
      shown.map((d) => d.path),
      !allShown,
    );

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
            {linked.size > 1 && (
              <p className="flex shrink-0 gap-1.5 border-b border-border px-3 py-2 text-2xs text-muted-foreground">
                <Link2 className="mt-0.5 size-3 shrink-0" />
                {t("diff.linkedHint")}
              </p>
            )}
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
                              {linked.size > 1 && linked.has(d.path) && (
                                <Link2
                                  className="size-3 shrink-0 text-muted-foreground"
                                  aria-label={t("diff.linked")}
                                />
                              )}
                              {d.warning === "overwrites" && (
                                <TriangleAlert
                                  className="size-3 shrink-0 text-warn"
                                  aria-label={t("diff.warnOverwrites")}
                                />
                              )}
                              {d.notice === "layout_reset" && (
                                <TriangleAlert
                                  className="size-3 shrink-0 text-warn"
                                  aria-label={t("diff.layoutReset")}
                                />
                              )}
                              {!!d.dropped && (
                                <TriangleAlert
                                  className="size-3 shrink-0 text-warn"
                                  aria-label={t("diff.commentsDropped", { n: d.dropped })}
                                />
                              )}
                            </span>
                            <span className="flex items-center gap-2 font-mono text-2xs tabular-nums">
                              <OpBadge d={d} />
                              <span className="text-diff-add-num">+{s.adds}</span>
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
          <div className="flex min-h-0 flex-1 flex-col">
            {active?.notice === "layout_reset" && (
              <p className="flex items-center gap-1.5 border-b border-border/60 px-3 py-1.5 text-2xs text-warn">
                <TriangleAlert className="size-3 shrink-0" />
                {t("diff.layoutReset")}
              </p>
            )}
            {!!active?.dropped && (
              <p className="flex items-center gap-1.5 border-b border-border/60 px-3 py-1.5 text-2xs text-warn">
                <TriangleAlert className="size-3 shrink-0" />
                {t("diff.commentsDropped", { n: active.dropped })}
              </p>
            )}
            {active && (
              <div className="min-h-0 flex-1">
                <DiffView
                  key={active.path}
                  path={active.path}
                  current={active.current}
                  next={active.next}
                  mode={mode}
                />
              </div>
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
  taken,
  onCreate,
  open,
  onOpenChange,
}: {
  tables: string[];
  junctions: string[];
  /// The names other indexes already have.
  taken: string[];
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
  const effectiveBase = baseEdited ? base.trim() : indexName(name);
  const effectiveFile = override ? overrideName.trim() : effectiveBase ? `${effectiveBase}${SUFFIX}` : "";
  const cleanDir = dir.replace(/^\/+|\/+$/g, "");
  const effectivePath = cleanDir && effectiveFile ? `${cleanDir}/${effectiveFile}` : effectiveFile;
  const junctionSet = new Set(junctions);

  // The initial `table` state captured the catalog before it loaded: default
  // it on opening, from the tables that actually exist.
  useEffect(() => {
    if (open && !table && tables[0]) setTable(tables[0]);
  }, [open, table, tables]);

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
  const nameRefusal = useNameRefusal()(name, taken);
  const detailsOk = !!indexName(name) && !nameRefusal && !!table;
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
                <Text value={name} onChange={setName} invalid={!!nameRefusal} placeholder={t("sidebar.indexName")} />
                {nameRefusal && <span className="text-2xs text-destructive">{nameRefusal}</span>}
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
