import { useEffect, useMemo, useState, type ComponentType, type KeyboardEvent } from "react";
import { BookOpen, Boxes, Clock, CornerDownLeft, Settings2, Table2, X, Zap } from "lucide-react";
import type { CatalogResponse } from "../api";
import { useT } from "../i18n";
import { frecencyScores, recordPick } from "../model/frecency";
import { createSearch, type Ranked, runSearch } from "../model/rank";
import { recentPicks, recordPickRecent, removeRecent } from "../model/recent";
import {
  buildSearchRecords,
  type SearchCategory,
  type SearchDetail,
  type SearchRecord,
  type SearchTarget,
} from "../model/search";
import { pathId } from "../model/tree";
import { type Doc, useDesignStore } from "../store/design";
import { useUiStore } from "../store/ui";
import { GlowDot } from "./widgets";
import { Command, CommandEmpty, CommandGroup, CommandInput, CommandItem, CommandList } from "@/components/ui/command";
import { Dialog, DialogContent, DialogTitle } from "@/components/ui/dialog";
import { Kbd } from "@/components/ui/kbd";

const CAT_ICON: Partial<Record<SearchCategory, ComponentType<{ className?: string }>>> = {
  action: Zap,
  index: Boxes,
  setting: Settings2,
  catalog: Table2,
  legendKind: BookOpen,
  legendType: BookOpen,
};

const CAT_COLOR: Partial<Record<SearchCategory, string>> = {
  action: "var(--warn)",
  index: "var(--k-root)",
  setting: "var(--slate)",
  catalog: "var(--accent-2)",
  legendKind: "var(--k-object)",
  legendType: "var(--k-object)",
};

/// A palette colour string → the same colour softened for a border / a tile fill.
const softBorder = (color: string) => `color-mix(in srgb, ${color} 40%, transparent)`;
const softFill = (color: string) => `color-mix(in srgb, ${color} 16%, var(--panel-3))`;

/// The tinted category glyph (actions/indexes/settings/tables) or the typed dot
/// (fields). Shared by the row and the preview head.
function Glyph({ record }: { record: SearchRecord }) {
  if (record.color)
    return <span className="inline-block size-2.5 shrink-0 rounded-full" style={{ background: record.color }} />;
  const Icon = CAT_ICON[record.category];
  const color = CAT_COLOR[record.category] ?? "var(--muted)";
  return (
    <span
      className="grid size-6 shrink-0 place-items-center rounded-md border"
      style={{ background: softFill(color), borderColor: softBorder(color), color }}
    >
      {Icon && <Icon className="size-3.5" />}
    </span>
  );
}

/// Renders `text` with the matched character ranges emphasised.
function Highlighted({ text, positions }: { text: string; positions: number[] }) {
  if (!positions.length) return <>{text}</>;
  const hit = new Set(positions);
  const parts: { on: boolean; text: string }[] = [];
  for (let i = 0; i < text.length; i += 1) {
    const on = hit.has(i);
    const last = parts[parts.length - 1];
    if (last?.on === on) last.text += text[i];
    else parts.push({ on, text: text[i] ?? "" });
  }
  return (
    <>
      {parts.map((p, i) =>
        p.on ? (
          <span key={i} className="font-semibold text-primary">
            {p.text}
          </span>
        ) : (
          <span key={i}>{p.text}</span>
        ),
      )}
    </>
  );
}

/// The right-hand preview: what the currently-highlighted record is, with a
/// breadcrumb, a Postgres→OpenSearch type mapping, flags, and what Enter does.
function DetailPane({ record }: { record: SearchRecord | null }) {
  const { t } = useT();
  if (!record)
    return <div className="hidden p-5 text-2xs text-muted-foreground sm:block">{t("search.emptyDetail")}</div>;
  const d: SearchDetail = record.detail;
  return (
    <div className="hidden min-w-0 flex-col gap-4 p-5 sm:flex">
      {d.crumb && d.crumb.length > 0 && (
        <div className="flex flex-wrap items-center gap-1 text-2xs text-muted-foreground">
          {d.crumb.map((c, i) => (
            <span key={i} className="flex items-center gap-1">
              {i > 0 && <span className="text-muted-foreground/50">▸</span>}
              {c}
            </span>
          ))}
        </div>
      )}

      <h3 className="flex items-center gap-2 text-base font-semibold">
        <Glyph record={record} />
        <span className="min-w-0 truncate">{record.title}</span>
        {d.headKind && <span className="shrink-0 text-xs font-normal text-muted-foreground">{d.headKind}</span>}
        {record.shortcut && <Kbd className="ml-auto">{record.shortcut}</Kbd>}
      </h3>

      {(d.source ?? d.target) && (
        <div className="flex flex-wrap items-center gap-2 rounded-md border border-border bg-background px-2.5 py-2 font-mono text-2xs">
          {d.source && <span className="text-muted-foreground">{d.source}</span>}
          {d.source && d.target && <span className="text-muted-foreground/60">→</span>}
          {d.target && <span style={{ color: record.color }}>{d.target}</span>}
        </div>
      )}

      {d.body && <p className="text-xs leading-relaxed text-muted-foreground">{d.body}</p>}

      {d.meta && <p className="text-2xs text-muted-foreground">{d.meta}</p>}

      {d.flags && d.flags.length > 0 && (
        <div className="flex flex-wrap gap-1.5">
          {d.flags.map((f, i) => (
            <span
              key={i}
              className={`rounded border px-1.5 py-0.5 text-2xs ${f.ok ? "border-primary/40 text-primary" : "border-border text-muted-foreground"}`}
            >
              {f.text}
            </span>
          ))}
        </div>
      )}

      <div className="mt-auto flex items-center gap-2 border-t border-border pt-3 text-xs text-primary">
        <CornerDownLeft className="size-3.5 shrink-0" />
        <span className="truncate">{d.enter}</span>
      </div>
    </div>
  );
}

/// The global search — a Cmd+K command palette over the whole project: run a UI
/// action, or jump to any index, field, setting, or database table/column. It
/// fuzzy-ranks with MiniSearch, boosts whatever's currently on screen, previews
/// the highlighted result on the right, and navigates by dispatching store calls
/// (panning the canvas via a focus request for a field/node).
export function CommandPalette({
  open,
  onOpenChange,
  doc,
  catalog,
  active,
  commands,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  doc: Doc;
  catalog: CatalogResponse | null;
  active: string;
  commands: SearchRecord[];
}) {
  const { t } = useT();
  const [q, setQ] = useState("");
  const [value, setValue] = useState("");

  const setActive = useDesignStore((s) => s.setActive);
  const setSelection = useDesignStore((s) => s.setSelection);
  const openIndex = useDesignStore((s) => s.openIndex);
  const requestFocus = useDesignStore((s) => s.requestFocus);
  const setBrowseCatalog = useUiStore((s) => s.setBrowseCatalog);
  const setRawMode = useUiStore((s) => s.setRawMode);

  useEffect(() => {
    if (!open) setQ("");
  }, [open]);

  const headings: Record<SearchCategory, string> = {
    action: t("search.actions"),
    index: t("search.indexes"),
    field: t("search.fields"),
    setting: t("search.settings"),
    catalog: t("search.tables"),
    legendKind: t("search.legendKinds"),
    legendType: t("search.legendTypes"),
  };

  // Building every field record is O(fields); only do it while the palette is
  // open. Each record carries its group label (+ raw category token) so a group
  // name like "actions" surfaces the whole group in search.
  const records = useMemo(
    () =>
      open
        ? [...commands, ...buildSearchRecords(doc, catalog, t)].map((r) => ({
            ...r,
            group: `${headings[r.category]} ${r.category}`,
          }))
        : [],
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [open, commands, doc, catalog, t],
  );
  const byId = useMemo(() => new Map(records.map((r) => [r.id, r])), [records]);
  const search = useMemo(() => createSearch(records), [records]);
  // Frecency snapshot per open — picks made this session apply on the next open.
  const frecency = useMemo(() => (open ? frecencyScores() : {}), [open]);

  const needle = q.trim();
  const ranked: Ranked[] = useMemo(() => {
    const onScreen = (r: SearchRecord) =>
      (r.index !== undefined && r.index === active) || (active === "config" && r.category === "setting");
    const weight = (r: SearchRecord) => (onScreen(r) ? 1.4 : 1) * (1 + Math.min(0.6, (frecency[r.id] ?? 0) * 0.12));
    // Empty query: the actionable top level (commands, indexes, settings),
    // ordered by frecency so your most-used surface first.
    if (!needle)
      return records
        .filter((r) => r.category === "action" || r.category === "index" || r.category === "setting")
        .sort((a, b) => weight(b) - weight(a))
        .map((record) => ({ record, positions: [] }));
    return runSearch(search, needle, byId, weight);
  }, [needle, search, records, byId, active, frecency]);

  // Inline autocomplete: the top completion whose prefix is what you've typed,
  // shown as ghost text and accepted with Tab.
  const completion = useMemo(() => {
    if (!needle) return "";
    // Suggest from titles only — the `keywords`/`group` fields would autosuggest
    // multi-word completions (e.g. "action actions"), which read as broken ghost
    // text against a single-token query.
    const s =
      search.autoSuggest(needle, { prefix: true, fuzzy: 0.2, boost: { title: 3 }, fields: ["title"] })[0]?.suggestion ??
      "";
    return s.toLowerCase().startsWith(q.toLowerCase()) && s.length > q.length ? s.slice(q.length) : "";
  }, [needle, q, search]);

  // Recent *picks* still present in the current project (stale ids drop out).
  // `recentTick` forces a recompute after a manual removal.
  const [recentTick, setRecentTick] = useState(0);
  const recent = useMemo(
    () => (open && !needle ? recentPicks().filter((p) => byId.has(p.id)) : []),
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [open, needle, byId, recentTick],
  );
  const removePick = (id: string) => {
    removeRecent(id);
    setRecentTick((n) => n + 1);
  };

  const onInputKeyDown = (e: KeyboardEvent<HTMLInputElement>) => {
    if (e.key === "Tab" && completion) {
      e.preventDefault();
      setQ(q + completion);
      return;
    }
    // ⌘/Ctrl+⌫ removes the highlighted recent pick from history.
    if ((e.metaKey || e.ctrlKey) && e.key === "Backspace" && value.startsWith("recent:")) {
      e.preventDefault();
      removePick(value.slice("recent:".length));
    }
  };

  // The highlighted record drives the preview; fall back to the top result when
  // cmdk's controlled value is empty or points at a now-filtered-out row (and
  // tolerate cmdk normalising the value's case).
  const current =
    byId.get(value) ??
    ranked.find((x) => x.record.id.toLowerCase() === value.toLowerCase())?.record ??
    ranked[0]?.record ??
    null;

  // The main area is chosen by (browseCatalog, active, rawMode) together, so a
  // jump must clear the sibling view flags or the target won't actually show
  // (e.g. opening an index while the Tables browser is up). A field/node jump
  // also leaves Code mode so the canvas focus is visible; a plain index open
  // preserves Code mode (it survives index switches).
  const navigate = (target: SearchTarget) => {
    switch (target.kind) {
      case "index":
        setBrowseCatalog(false);
        openIndex(target.name);
        break;
      case "field":
        setBrowseCatalog(false);
        setRawMode(false);
        setActive(target.index);
        setSelection({ kind: "field", path: target.path, index: target.leaf });
        requestFocus(target.index, pathId(target.path));
        break;
      case "node":
        setBrowseCatalog(false);
        setRawMode(false);
        setActive(target.index);
        setSelection(target.path.length ? { kind: "node", path: target.path } : { kind: "root" });
        requestFocus(target.index, pathId(target.path));
        break;
      case "config":
        setBrowseCatalog(false);
        setActive("config");
        break;
      case "catalog":
        setBrowseCatalog(true);
        break;
    }
  };

  const onSelect = (r: SearchRecord) => {
    recordPick(r.id);
    recordPickRecent({ id: r.id, title: r.title });
    onOpenChange(false);
    if (r.run) r.run();
    else if (r.target) navigate(r.target);
  };

  const groups: SearchCategory[] = ["action", "index", "field", "setting", "legendKind", "legendType", "catalog"];

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent
        showCloseButton={false}
        aria-describedby={undefined}
        className="top-[11%] translate-y-0 gap-0 overflow-hidden p-0 sm:max-w-3xl"
      >
        <DialogTitle className="sr-only">{t("search.title")}</DialogTitle>
        <Command shouldFilter={false} value={value} onValueChange={setValue}>
          <CommandInput
            value={q}
            onValueChange={setQ}
            onKeyDown={onInputKeyDown}
            placeholder={t("search.placeholder")}
            ghost={
              completion ? (
                <>
                  <span className="invisible">{q}</span>
                  <span className="text-muted-foreground/40">{completion}</span>
                </>
              ) : undefined
            }
            leading={<GlowDot />}
            trailing={
              needle ? (
                <span className="shrink-0 text-2xs text-muted-foreground tabular-nums">
                  {t("search.resultCount", { n: ranked.length })}
                </span>
              ) : undefined
            }
          />
          <div className="grid sm:grid-cols-[1.55fr_1fr]">
            <CommandList className="max-h-96 p-2 sm:border-r sm:border-border">
              <CommandEmpty>{t("search.empty")}</CommandEmpty>
              {recent.length > 0 && (
                <CommandGroup
                  heading={
                    <span className="flex w-full items-center gap-2">
                      <span>{t("search.recent")}</span>
                      <span className="h-px flex-1 bg-border" />
                    </span>
                  }
                >
                  {recent.map((pick) => {
                    const r = byId.get(pick.id);
                    return (
                      <CommandItem
                        key={`recent:${pick.id}`}
                        value={`recent:${pick.id}`}
                        onSelect={() => r && onSelect(r)}
                        className="group relative gap-2.5 py-2 data-[selected=true]:bg-primary/10 data-[selected=true]:text-foreground"
                      >
                        <span
                          aria-hidden
                          className="absolute inset-y-1 left-0 w-0.5 rounded-full bg-primary opacity-0 group-data-[selected=true]:opacity-100"
                        />
                        <span className="grid size-6 shrink-0 place-items-center rounded-md border border-border bg-accent text-muted-foreground">
                          <Clock className="size-3.5" />
                        </span>
                        <span className="min-w-0 flex-1 truncate">{pick.title}</span>
                        <button
                          type="button"
                          aria-label={t("search.removeRecent")}
                          title={t("search.removeRecent")}
                          onPointerDown={(e) => {
                            // Beat cmdk's onSelect (which fires on pointer-down).
                            e.preventDefault();
                            e.stopPropagation();
                            removePick(pick.id);
                          }}
                          className="shrink-0 rounded p-0.5 text-muted-foreground opacity-0 transition-opacity group-data-[selected=true]:opacity-100 hover:text-foreground focus-visible:opacity-100 focus-visible:outline-none"
                        >
                          <X className="size-3.5" />
                        </button>
                      </CommandItem>
                    );
                  })}
                </CommandGroup>
              )}
              {groups.map((cat) => {
                const all = ranked.filter((x) => x.record.category === cat);
                if (!all.length) return null;
                const items = all.slice(0, 8);
                return (
                  <CommandGroup
                    key={cat}
                    heading={
                      <span className="flex w-full items-center gap-2">
                        <span>{headings[cat]}</span>
                        <span className="h-px flex-1 bg-border" />
                        <span className="rounded-full bg-accent px-1.5 text-3xs font-medium tabular-nums text-muted-foreground">
                          {all.length}
                        </span>
                      </span>
                    }
                  >
                    {items.map(({ record: r, positions }) => (
                      <CommandItem
                        key={r.id}
                        value={r.id}
                        onSelect={() => onSelect(r)}
                        className="group relative gap-2.5 py-2 data-[selected=true]:bg-primary/10 data-[selected=true]:text-foreground"
                      >
                        <span
                          aria-hidden
                          className="absolute inset-y-1 left-0 w-0.5 rounded-full bg-primary opacity-0 group-data-[selected=true]:opacity-100"
                        />
                        <Glyph record={r} />
                        <span className="min-w-0 flex-1 truncate">
                          <Highlighted text={r.title} positions={positions} />
                        </span>
                        <span className="flex shrink-0 items-center gap-2 pl-2">
                          {r.subtitle && (
                            <span className="hidden max-w-40 truncate text-2xs text-muted-foreground sm:inline">
                              {r.subtitle}
                            </span>
                          )}
                          {r.color && r.kind && (
                            <span
                              className="rounded border px-1.5 py-0.5 font-mono text-2xs"
                              style={{ color: r.color, borderColor: softBorder(r.color) }}
                            >
                              {r.kind}
                            </span>
                          )}
                          {r.shortcut && <Kbd>{r.shortcut}</Kbd>}
                        </span>
                      </CommandItem>
                    ))}
                  </CommandGroup>
                );
              })}
            </CommandList>
            <DetailPane record={current} />
          </div>
          <div className="flex items-center gap-4 border-t border-border bg-card px-3 py-2 text-3xs text-muted-foreground">
            <span className="flex items-center gap-1.5">
              <Kbd>↑</Kbd>
              <Kbd>↓</Kbd>
              {t("search.navigate")}
            </span>
            <span className="flex items-center gap-1.5">
              <Kbd>↵</Kbd>
              {t("search.selectHint")}
            </span>
            <span className="flex items-center gap-1.5">
              <Kbd>esc</Kbd>
              {t("search.closeHint")}
            </span>
            {completion && (
              <span className="flex items-center gap-1.5 text-primary">
                <Kbd className="text-primary">⇥</Kbd>
                {t("search.complete")}
              </span>
            )}
            {recent.length > 0 && value.startsWith("recent:") && (
              <span className="flex items-center gap-1.5">
                <Kbd>⌘</Kbd>
                <Kbd>⌫</Kbd>
                {t("search.removeRecent")}
              </span>
            )}
            <span className="ml-auto text-muted-foreground">{t("search.onScreenFirst")}</span>
          </div>
        </Command>
      </DialogContent>
    </Dialog>
  );
}
