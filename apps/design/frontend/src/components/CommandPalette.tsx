import { useEffect, useMemo, useState, type ComponentType } from "react";
import Fuse from "fuse.js";
import { Boxes, Settings2, Table2, Zap } from "lucide-react";
import type { CatalogResponse } from "../api";
import { useT } from "../i18n";
import { buildSearchRecords, type SearchCategory, type SearchRecord, type SearchTarget } from "../model/search";
import { pathId } from "../model/tree";
import { type Doc, useDesignStore } from "../store/design";
import { useUiStore } from "../store/ui";
import { Command, CommandEmpty, CommandGroup, CommandInput, CommandItem, CommandList } from "@/components/ui/command";
import { Dialog, DialogContent, DialogTitle } from "@/components/ui/dialog";

const CAT_ICON: Partial<Record<SearchCategory, ComponentType<{ className?: string }>>> = {
  action: Zap,
  index: Boxes,
  setting: Settings2,
  catalog: Table2,
};

/// The global search — a Cmd+K command palette over the whole project: run a UI
/// action, or jump to any index, field, setting, or database table/column. It
/// fuzzy-ranks with Fuse, boosts whatever's currently on screen, and navigates
/// by dispatching store calls (panning the canvas via a focus request for a
/// field/node, since it lives outside React Flow).
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

  const setActive = useDesignStore((s) => s.setActive);
  const setSelection = useDesignStore((s) => s.setSelection);
  const openIndex = useDesignStore((s) => s.openIndex);
  const requestFocus = useDesignStore((s) => s.requestFocus);
  const setBrowseCatalog = useUiStore((s) => s.setBrowseCatalog);

  useEffect(() => {
    if (!open) setQ("");
  }, [open]);

  // Building every field record is O(fields); only do it while the palette is open.
  const records = useMemo(
    () => (open ? [...commands, ...buildSearchRecords(doc, catalog, t)] : []),
    [open, commands, doc, catalog, t],
  );
  const fuse = useMemo(
    () =>
      new Fuse(records, {
        includeScore: true,
        ignoreLocation: true,
        threshold: 0.4,
        minMatchCharLength: 1,
        keys: [
          { name: "title", weight: 3 },
          { name: "subtitle", weight: 1 },
          { name: "keywords", weight: 1 },
        ],
      }),
    [records],
  );

  const needle = q.trim();
  const ranked = useMemo(() => {
    // Empty query: show the actionable top level (commands, indexes, settings),
    // not the whole field/column corpus.
    if (!needle)
      return records.filter((r) => r.category === "action" || r.category === "index" || r.category === "setting");
    const onScreen = (r: SearchRecord) =>
      (r.index !== undefined && r.index === active) || (active === "config" && r.category === "setting");
    return fuse
      .search(needle)
      .map((h) => ({ r: h.item, score: (h.score ?? 0) * (onScreen(h.item) ? 0.45 : 1) }))
      .sort((a, b) => a.score - b.score)
      .map((h) => h.r);
  }, [needle, fuse, records, active]);

  const navigate = (target: SearchTarget) => {
    switch (target.kind) {
      case "index":
        openIndex(target.name);
        break;
      case "field":
        setActive(target.index);
        setSelection({ kind: "field", path: target.path, index: target.leaf });
        requestFocus(target.index, pathId(target.path));
        break;
      case "node":
        setActive(target.index);
        setSelection(target.path.length ? { kind: "node", path: target.path } : { kind: "root" });
        requestFocus(target.index, pathId(target.path));
        break;
      case "config":
        setActive("config");
        break;
      case "catalog":
        setBrowseCatalog(true);
        break;
    }
  };

  const onSelect = (r: SearchRecord) => {
    onOpenChange(false);
    if (r.run) r.run();
    else if (r.target) navigate(r.target);
  };

  const headings: Record<SearchCategory, string> = {
    action: t("search.actions"),
    index: t("search.indexes"),
    field: t("search.fields"),
    setting: t("search.settings"),
    catalog: t("search.tables"),
  };
  const groups: SearchCategory[] = ["action", "index", "field", "setting", "catalog"];

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent
        showCloseButton={false}
        aria-describedby={undefined}
        className="top-[12%] translate-y-0 gap-0 overflow-hidden p-0 sm:max-w-xl"
      >
        <DialogTitle className="sr-only">{t("search.title")}</DialogTitle>
        <Command shouldFilter={false}>
          <CommandInput value={q} onValueChange={setQ} placeholder={t("search.placeholder")} />
          <CommandList className="max-h-80">
            <CommandEmpty>{t("search.empty")}</CommandEmpty>
            {groups.map((cat) => {
              const items = ranked.filter((r) => r.category === cat).slice(0, 8);
              if (!items.length) return null;
              return (
                <CommandGroup key={cat} heading={headings[cat]}>
                  {items.map((r) => {
                    const CatIcon = CAT_ICON[r.category];
                    return (
                      <CommandItem key={r.id} value={r.id} onSelect={() => onSelect(r)}>
                        {r.color ? (
                          <span
                            className="inline-block size-2.5 shrink-0 rounded-full"
                            style={{ background: r.color }}
                          />
                        ) : CatIcon ? (
                          <CatIcon className="size-3.5 shrink-0 text-muted-foreground" />
                        ) : null}
                        <span className="truncate">{r.title}</span>
                        {r.subtitle && (
                          <span className="ml-auto truncate pl-3 text-2xs text-muted-foreground">{r.subtitle}</span>
                        )}
                      </CommandItem>
                    );
                  })}
                </CommandGroup>
              );
            })}
          </CommandList>
        </Command>
      </DialogContent>
    </Dialog>
  );
}
