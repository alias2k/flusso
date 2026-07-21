import { useEffect, useRef, useState } from "react";
import { KeyRound, Link2, Search, X } from "lucide-react";
import type { CatalogResponse, ColumnShape, TableShape } from "../api";
import { useT } from "../i18n";
import { typeClass } from "../theme";
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { cn } from "@/lib/utils";

const JunctionBadge = ({ label }: { label: string }) => (
  <span className="shrink-0 rounded-full border border-kind-many_to_many/40 bg-kind-many_to_many/15 px-1.5 py-0.5 text-3xs font-bold tracking-wide text-kind-many_to_many uppercase">
    {label}
  </span>
);

/// Emphasise the first case-insensitive match of `q` within `text`.
function Mark({ text, q }: { text: string; q: string }) {
  const i = q ? text.toLowerCase().indexOf(q) : -1;
  if (i < 0) return <>{text}</>;
  return (
    <>
      {text.slice(0, i)}
      <span className="font-semibold text-primary">{text.slice(i, i + q.length)}</span>
      {text.slice(i + q.length)}
    </>
  );
}

const suggested = (c: ColumnShape): string => (typeof c.suggested_type === "string" ? c.suggested_type : "other");

/// A reference to another table, `table.column` — the navigable unit.
interface Ref {
  table: string;
  column: string;
}

/// Derive, from the catalog's foreign keys: each column's outgoing target
/// (`orders.id`) and, per table, the incoming references that point at it.
function relations(tables: TableShape[]) {
  const fkTarget = new Map<string, Map<string, Ref>>();
  const incoming = new Map<string, Ref[]>();
  for (const tbl of tables) {
    const cols = new Map<string, Ref>();
    for (const fk of tbl.foreign_keys) {
      fk.columns.forEach((c, i) =>
        cols.set(c, { table: fk.references_table, column: fk.references_columns[i] ?? fk.references_columns[0] ?? "" }),
      );
      incoming.set(fk.references_table, [
        ...(incoming.get(fk.references_table) ?? []),
        { table: tbl.name, column: fk.columns.join(", ") },
      ]);
    }
    fkTarget.set(tbl.name, cols);
  }
  return { fkTarget, incoming };
}

/// A read-only browser of the introspected database: a table list on the left, the
/// selected table's columns + relationships on the right. Foreign keys are shown
/// inline on their column (→ target), incoming references in a footer. The filter
/// spans table *and* column names — a column hit surfaces its table.
export function CatalogBrowser({ catalog, onClose }: { catalog: CatalogResponse; onClose: () => void }) {
  const { t } = useT();
  const [q, setQ] = useState("");
  const [selectedName, setSelectedName] = useState(catalog.catalog.tables[0]?.name ?? "");
  const needle = q.trim().toLowerCase();
  const tables = catalog.catalog.tables;

  const junctions = new Set(catalog.junctions.map((j) => j.table.table));
  const { fkTarget, incoming } = relations(tables);

  const matchesCol = (tbl: TableShape) =>
    needle ? tbl.columns.filter((c) => c.name.toLowerCase().includes(needle)) : [];
  const filtered = tables.filter(
    (tbl) =>
      !needle ||
      tbl.name.toLowerCase().includes(needle) ||
      tbl.columns.some((c) => c.name.toLowerCase().includes(needle)),
  );
  // Keep the selection valid without a state write: fall back to the first match.
  const selected = filtered.find((tbl) => tbl.name === selectedName) ?? filtered[0] ?? null;

  // Jump to a related table — clear the filter so the target is always selectable,
  // and scroll its row into view once it re-renders.
  const listRefs = useRef(new Map<string, HTMLButtonElement | null>());
  const pendingScroll = useRef<string | null>(null);
  const goto = (table: string) => {
    pendingScroll.current = table;
    setQ("");
    setSelectedName(table);
  };
  useEffect(() => {
    const target = pendingScroll.current;
    if (!target) return;
    pendingScroll.current = null;
    listRefs.current.get(target)?.scrollIntoView({ block: "nearest" });
  }, [selectedName]);

  return (
    <Dialog open onOpenChange={(open) => !open && onClose()}>
      <DialogContent
        className="flex w-[min(58rem,94vw)] max-w-none flex-col gap-0 overflow-hidden p-0 max-h-[85vh] sm:max-w-none"
        aria-label={t("catalog.aria")}
      >
        <DialogHeader className="px-4 pt-4 pb-2">
          <DialogTitle>{t("catalog.title", { n: tables.length })}</DialogTitle>
        </DialogHeader>

        {catalog.error ? (
          <p className="banner warn mx-4 mb-4">{t("catalog.dbError", { err: catalog.error })}</p>
        ) : (
          <>
            <div className="mx-4 mb-2 flex h-9 items-center gap-2.5 rounded-md border border-border bg-secondary px-3 focus-within:border-primary">
              <Search className="size-4 shrink-0 text-muted-foreground" />
              <input
                value={q}
                onChange={(e) => setQ(e.target.value)}
                placeholder={t("catalog.filter")}
                className="min-w-0 flex-1 bg-transparent text-sm outline-none placeholder:text-muted-foreground"
                autoComplete="off"
                autoCorrect="off"
                autoCapitalize="off"
                spellCheck={false}
                data-1p-ignore="true"
                data-lpignore="true"
                data-bwignore="true"
                data-form-type="other"
              />
              {needle && (
                <span className="shrink-0 text-2xs text-muted-foreground tabular-nums">
                  {t("catalog.matchCount", { n: filtered.length })}
                </span>
              )}
              {q && (
                <button
                  type="button"
                  onClick={() => setQ("")}
                  aria-label={t("common.clear")}
                  className="shrink-0 text-muted-foreground hover:text-foreground"
                >
                  <X className="size-3.5" />
                </button>
              )}
            </div>

            <div className="grid min-h-0 flex-1 grid-cols-[16rem_1fr] border-t border-border">
              <div className="min-h-0 overflow-y-auto border-r border-border p-1.5">
                {filtered.length === 0 ? (
                  <p className="p-3 text-2xs text-muted-foreground">{t("catalog.noMatch")}</p>
                ) : (
                  filtered.map((tbl) => {
                    const cols = matchesCol(tbl);
                    const nameHit = !needle || tbl.name.toLowerCase().includes(needle);
                    return (
                      <button
                        key={tbl.name}
                        type="button"
                        ref={(el) => {
                          listRefs.current.set(tbl.name, el);
                        }}
                        onClick={() => setSelectedName(tbl.name)}
                        className={cn(
                          "flex w-full items-center gap-2 rounded-md border-l-2 px-2.5 py-2 text-left",
                          selected?.name === tbl.name
                            ? "border-primary bg-primary/10"
                            : "border-transparent hover:bg-accent",
                        )}
                      >
                        <span className="flex min-w-0 flex-col">
                          <span className="flex items-center gap-1.5">
                            <span className="truncate text-sm font-medium">
                              <Mark text={tbl.name} q={nameHit ? needle : ""} />
                            </span>
                            {junctions.has(tbl.name) && <JunctionBadge label={t("catalog.junction")} />}
                          </span>
                          {cols.length > 0 && !tbl.name.toLowerCase().includes(needle) && (
                            <span className="truncate pt-0.5 font-mono text-2xs text-accent2">
                              <Mark text={cols.map((c) => c.name).join(", ")} q={needle} />
                            </span>
                          )}
                        </span>
                        <span className="ml-auto shrink-0 text-2xs text-muted-foreground tabular-nums">
                          {tbl.columns.length}
                        </span>
                      </button>
                    );
                  })
                )}
              </div>

              <div className="min-h-0 overflow-y-auto px-4 py-3.5">
                {selected && (
                  <>
                    <div className="flex items-center gap-2">
                      <h4 className="text-base font-semibold">{selected.name}</h4>
                      {junctions.has(selected.name) && <JunctionBadge label={t("catalog.junction")} />}
                    </div>
                    <div className="mb-3 font-mono text-2xs text-muted-foreground">
                      {selected.schema} · {t("catalog.cols", { n: selected.columns.length })}
                    </div>

                    <div className="grid grid-cols-[auto_minmax(0,1fr)_auto] items-stretch">
                      {selected.columns.map((c, i) => {
                        const ref = fkTarget.get(selected.name)?.get(c.name);
                        const hit = !!needle && c.name.toLowerCase().includes(needle);
                        const last = i === selected.columns.length - 1;
                        const cell = cn(
                          "flex items-center py-1.5",
                          !last && "border-b border-border/50",
                          hit && "bg-primary/10",
                        );
                        return (
                          <div key={c.name} className="contents">
                            <span className={cn(cell, "justify-center pr-2 pl-1.5", hit && "rounded-l")}>
                              {c.is_primary_key ? (
                                <KeyRound className="size-3.5 text-kind-root" aria-label={t("catalog.pk")} />
                              ) : ref ? (
                                <Link2 className="size-3.5 text-accent2" />
                              ) : (
                                <span className="size-3.5" />
                              )}
                            </span>
                            <span className={cn(cell, "min-w-0 gap-2 pr-4")}>
                              <span className="truncate text-sm">
                                <Mark text={c.name} q={needle} />
                              </span>
                              {c.nullable && (
                                <span className="shrink-0 rounded border border-border px-1 text-3xs text-muted-foreground">
                                  {t("inspector.colNullable")}
                                </span>
                              )}
                              {ref && (
                                <span className="min-w-0 truncate font-mono text-2xs">
                                  <span className="text-muted-foreground/60">→</span>{" "}
                                  <button
                                    type="button"
                                    onClick={() => goto(ref.table)}
                                    className="cursor-pointer text-accent2/85 hover:text-accent2 hover:underline"
                                  >
                                    {ref.table}.{ref.column}
                                  </button>
                                </span>
                              )}
                            </span>
                            <span className={cn(cell, "justify-start pr-1.5", hit && "rounded-r")}>
                              <span
                                className={cn(
                                  "rounded border border-current/30 px-1.5 py-0.5 font-mono text-2xs whitespace-nowrap",
                                  typeClass(suggested(c)),
                                )}
                              >
                                {c.sql_type}
                              </span>
                            </span>
                          </div>
                        );
                      })}
                    </div>

                    {(incoming.get(selected.name)?.length ?? 0) > 0 && (
                      <div className="mt-4 border-t border-border pt-3">
                        <div className="mb-2 text-3xs font-bold tracking-wide text-muted-foreground uppercase">
                          {t("catalog.referencedBy")}
                        </div>
                        <div className="flex flex-wrap gap-1.5">
                          {incoming.get(selected.name)?.map((ref) => (
                            <button
                              key={`${ref.table}.${ref.column}`}
                              type="button"
                              onClick={() => goto(ref.table)}
                              className="inline-flex cursor-pointer items-center gap-1.5 rounded border border-border/60 bg-secondary px-2 py-1 font-mono text-2xs text-muted-foreground hover:border-accent2/50 hover:text-foreground"
                            >
                              <Link2 className="size-3 shrink-0 -scale-x-100 text-accent2" />
                              {ref.table}.{ref.column}
                            </button>
                          ))}
                        </div>
                      </div>
                    )}
                  </>
                )}
              </div>
            </div>
          </>
        )}

        <DialogFooter className="border-t border-border px-4 py-3">
          <Button variant="secondary" size="sm" onClick={onClose}>
            {t("common.close")}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
