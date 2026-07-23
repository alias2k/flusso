import { Handle, Position, type NodeProps } from "@xyflow/react";
import { useRef, useState, type ReactNode } from "react";
import { ChevronDownIcon } from "lucide-react";
import { SCALAR_TYPES, type ColumnShape, type FlussoType } from "../api";
import { KIND_HELP } from "../fields";
import { aggregateIncomplete, joinIncomplete } from "../model/complete";
import * as edit from "../model/edit";
import { suggestRelations, type RelationSuggestion } from "../model/relations";
import { fieldAtPath, nodeFields, type DocNode, type LeafField } from "../model/tree";
import { useT } from "../i18n";
import { useDesign } from "../state";
import { kindColorClass, typeClass } from "../theme";
import { Hint } from "./Hint";
import { Icon } from "./Icon";
import { Select, Text } from "./widgets";
import { Checkbox } from "@/components/ui/checkbox";
import { Command, CommandEmpty, CommandGroup, CommandInput, CommandItem, CommandList } from "@/components/ui/command";
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover";
import { cn } from "@/lib/utils";

const FIELD_KINDS = ["object", "geo", "map", "custom", "constant"] as const;
const AGG_KINDS = ["count", "sum", "avg", "min", "max", "ids"] as const;
const JOIN_KINDS = ["belongs_to", "has_one", "has_many", "many_to_many"] as const;
// Order the FK-suggestion groups by how the document usually reads: the tables
// this one points at first, then its back-references, then the m2m sets.
const REL_VERB_ORDER = ["belongs_to", "has_many", "many_to_many"] as const;

export function DocNodeView({ data, selected }: NodeProps) {
  const node = (data as { node: DocNode }).node;
  const { catalog, schema, apply, select, selection, columnsFor, diagnostics, collapsed, toggleCollapsed } =
    useDesign();
  const { t } = useT();
  const cols = columnsFor(node.table);
  const fields = nodeFields(schema, node.path);
  const [filter, setFilter] = useState("");
  const isCollapsed = collapsed.has(node.id);
  const matches = (name: string) => name.toLowerCase().includes(filter.toLowerCase());

  // Inline rename of a nested node's field name (the root has no field, so it's
  // not renamable here). Commits through the same path-addressed edit the
  // Inspector uses; Escape cancels, empty/unchanged is a no-op.
  const [renaming, setRenaming] = useState(false);
  const [draft, setDraft] = useState("");
  const canRename = node.path.length > 0;
  const startRename = () => {
    setDraft(node.name ?? "");
    setRenaming(true);
  };
  const commitRename = () => {
    setRenaming(false);
    const name = draft.trim();
    if (!name || name === node.name) return;
    apply((s) => {
      const f = fieldAtPath(s, node.path);
      return f ? edit.setNode(s, node.path, { ...f, field: name }) : s;
    });
  };
  const pickRootTable = (t: string) => {
    if (!t) return;
    const pk = catalog?.catalog.tables.find((x) => x.name === t)?.primary_key[0];
    apply((s) => edit.setRootMeta(s, { table: t, primary_key: pk }));
  };

  // Diagnostics are reported by field name (no path), so match by name. Build a
  // lookup once; a node shows a count badge, a row shows its message on hover.
  const diagByField = new Map(diagnostics.map((d) => [d.field, d]));
  const nodeIssues = node.leaves.filter((l) => diagByField.has(l.name)).length;
  const selfIncomplete = joinIncomplete(fieldAtPath(schema, node.path));

  const includedByCol = new Map<string, LeafField>();
  for (const l of node.leaves) {
    if (l.column && (SCALAR_TYPES as string[]).includes(l.kind)) includedByCol.set(l.column, l);
  }
  // How many catalog columns are currently included — drives the master
  // (select-all) checkbox: all / none / indeterminate.
  const includedCount = cols.filter((c) => includedByCol.has(c.name)).length;
  const allIncluded = cols.length > 0 && includedCount === cols.length;
  // Leaves not represented by a catalog-column checkbox: special types, or a
  // scalar whose column isn't in the catalog (offline / typed by hand).
  const catalogCols = new Set(cols.map((c) => c.name));
  const extraLeaves = node.leaves.filter(
    (l) => !((SCALAR_TYPES as string[]).includes(l.kind) && l.column && catalogCols.has(l.column)),
  );

  const fieldSelected = (index: number) =>
    selection?.kind === "field" && selection.path.join(".") === node.path.join(".") && selection.index === index;

  const includeColumn = (c: ColumnShape) => {
    // The new scalar appends to the container, so its index is the current count.
    apply((s) => edit.toggleColumn(s, node.path, c.name, true, c.suggested_type ?? "keyword", c.nullable));
    select({ kind: "field", path: node.path, index: fields.length });
  };
  const excludeColumn = (c: ColumnShape, inc: LeafField) => {
    if (fieldSelected(inc.index)) select(null);
    apply((s) => edit.toggleColumn(s, node.path, c.name, false));
  };

  // Shift-click range check/uncheck across the visible column rows: a Shift-click
  // sets every row between the last plainly-clicked column (the anchor) and the
  // clicked one to the clicked box's new state, in a single edit. `shiftHeld` is
  // captured on the checkbox's click (which fires before onCheckedChange).
  const visibleCols = cols.filter((c) => matches(c.name));
  const anchorCol = useRef<string | null>(null);
  const shiftHeld = useRef(false);
  const toggleColumnAt = (c: ColumnShape, target: boolean) => {
    const anchor = anchorCol.current;
    if (shiftHeld.current && anchor && anchor !== c.name) {
      const names = visibleCols.map((x) => x.name);
      const a = names.indexOf(anchor);
      const b = names.indexOf(c.name);
      if (a >= 0 && b >= 0) {
        const range = visibleCols.slice(Math.min(a, b), Math.max(a, b) + 1);
        apply((s) =>
          target
            ? edit.includeColumns(
                s,
                node.path,
                range.map((x) => ({ name: x.name, ty: x.suggested_type, nullable: x.nullable })),
              )
            : edit.excludeColumns(
                s,
                node.path,
                range.map((x) => x.name),
              ),
        );
        return; // keep the anchor so the range can be extended
      }
    }
    anchorCol.current = c.name;
    const inc = includedByCol.get(c.name);
    if (target) includeColumn(c);
    else if (inc) excludeColumn(c, inc);
  };

  return (
    <div className={`flow-node kind-${node.kind}${selected ? " selected" : ""}`}>
      <Handle type="target" position={Position.Left} />
      <header onClick={() => select(node.path.length ? { kind: "node", path: node.path } : { kind: "root" })}>
        <button
          className={`chevron${isCollapsed ? " collapsed" : ""}`}
          title={isCollapsed ? t("node.expand") : t("node.collapse")}
          onClick={(e) => {
            e.stopPropagation();
            toggleCollapsed(node.id);
          }}
        >
          <Icon name="chevron" size={12} />
        </button>
        <span className={`badge ${node.kind}`}>{node.kind}</span>
        {renaming ? (
          <input
            className="node-title-edit"
            ref={(el) => el?.focus()}
            value={draft}
            aria-label={t("node.renameField")}
            onChange={(e) => setDraft(e.target.value)}
            onClick={(e) => e.stopPropagation()}
            onBlur={commitRename}
            onKeyDown={(e) => {
              e.stopPropagation();
              if (e.key === "Enter") {
                e.preventDefault();
                commitRename();
              } else if (e.key === "Escape") {
                e.preventDefault();
                setRenaming(false);
              }
            }}
          />
        ) : (
          <span
            className="node-title"
            title={canRename ? t("node.renameHint") : undefined}
            onDoubleClick={
              canRename
                ? (e) => {
                    e.stopPropagation();
                    startRename();
                  }
                : undefined
            }
          >
            {node.name ?? node.table}
          </span>
        )}
        {nodeIssues > 0 && (
          <span className="issue-badge" title={t("node.diagCount", { n: nodeIssues })}>
            {nodeIssues}
          </span>
        )}
        {selfIncomplete && (
          <span className="issue-badge warn" title={t("node.joinIncomplete")}>
            !
          </span>
        )}
        {node.path.length > 0 && (
          <button
            className="x"
            title={t("common.remove")}
            onClick={(e) => {
              e.stopPropagation();
              apply((s) => edit.removeNode(s, node.path));
              select(null);
            }}
          >
            <Icon name="close" size={13} />
          </button>
        )}
        <div className="node-sub">
          {node.table}
          {node.primaryKey ? ` · ${t("node.pk")}: ${node.primaryKey}` : ""}
          {node.leaves.length > 0 ? ` · ${t("node.fields", { n: node.leaves.length })}` : ""}
        </div>
      </header>

      {node.kind === "root" && !node.table ? (
        <div className="empty-state flex flex-col gap-2 p-3 text-2xs text-muted-foreground">
          <span>{t("node.pickRoot")}</span>
          {(catalog?.catalog.tables.length ?? 0) > 0 ? (
            <Select<string>
              value=""
              placeholder={t("node.chooseTable")}
              options={catalog?.catalog.tables.map((tbl) => tbl.name) ?? []}
              onChange={pickRootTable}
            />
          ) : (
            <RootTableInput onPick={pickRootTable} />
          )}
        </div>
      ) : (
        // Collapse animates height-to-auto via the 0fr ⇄ 1fr grid-row trick (same
        // as the preview's document tree); the body stays mounted, `inert` while
        // collapsed so the clipped inputs can't be tabbed into.
        <div
          className={cn(
            "grid transition-[grid-template-rows] duration-200 ease-out motion-reduce:transition-none",
            isCollapsed ? "grid-rows-[0fr]" : "grid-rows-[1fr]",
          )}
        >
          <div className="min-h-0 overflow-hidden" inert={isCollapsed}>
            <>
              {cols.length > 0 && (
                <div className="col-tools flex items-center gap-1.5 px-2 pt-2" onClick={(e) => e.stopPropagation()}>
                  <Text
                    className="col-filter flex-1"
                    value={filter}
                    onChange={setFilter}
                    placeholder={t("node.filterCols")}
                  />
                  <Hint label={allIncluded ? t("node.clearAll") : t("node.includeAll")} side="top">
                    <Checkbox
                      className="size-4"
                      aria-label={allIncluded ? t("node.clearAll") : t("node.includeAll")}
                      checked={allIncluded ? true : includedCount === 0 ? false : "indeterminate"}
                      onCheckedChange={() =>
                        apply((s) =>
                          allIncluded
                            ? edit.clearColumns(s, node.path)
                            : edit.includeColumns(
                                s,
                                node.path,
                                cols.map((c) => ({ name: c.name, ty: c.suggested_type, nullable: c.nullable })),
                              ),
                        )
                      }
                    />
                  </Hint>
                </div>
              )}

              <div className="node-cols px-2 py-1.5">
                {visibleCols.map((c) => {
                  const inc = includedByCol.get(c.name);
                  const renamed = inc && inc.name !== c.name;
                  const diag = inc ? diagByField.get(inc.name) : undefined;
                  // Required/default state, relative to the source column, so it
                  // reads at a glance: a dot = required (muted when it just mirrors a
                  // NOT NULL column, accent when it overrides a nullable one), and an
                  // `=` when a default fills the gap. No dot = optional.
                  const field = inc ? fields[inc.index] : undefined;
                  const col = field && "column" in field.source ? field.source.column : undefined;
                  const required = !!col && !col.nullable;
                  const override = required && c.nullable;
                  const hasDefault = col?.default !== undefined;
                  return (
                    <div
                      className={`col-row${inc ? " on" : ""}${inc && fieldSelected(inc.index) ? " sel" : ""}${diag ? ` diag-${diag.severity}` : ""}`}
                      key={c.name}
                      title={diag?.message}
                      onClick={() => inc && select({ kind: "field", path: node.path, index: inc.index })}
                    >
                      <Checkbox
                        checked={!!inc}
                        aria-label={c.name}
                        onClick={(e) => {
                          e.stopPropagation();
                          shiftHeld.current = e.shiftKey;
                        }}
                        onCheckedChange={(ch) => toggleColumnAt(c, ch === true)}
                      />
                      <span className="col-name" title={renamed ? t("node.columnOf", { name: c.name }) : undefined}>
                        {inc ? inc.name : c.name}
                        {renamed ? <span className="col-from"> ← {c.name}</span> : null}
                      </span>
                      {required && (
                        <span
                          className={`col-req${override ? " override" : ""}`}
                          title={override ? t("node.reqOverride") : t("node.reqAligned")}
                        >
                          *
                        </span>
                      )}
                      {hasDefault && (
                        <span className="col-default" title={t("node.colDefault")}>
                          =
                        </span>
                      )}
                      {(() => {
                        const label = inc ? (inc.ty as string) : typeLabel(c.suggested_type);
                        return <span className={`col-type ${typeClass(label)}`}>{label}</span>;
                      })()}
                    </div>
                  );
                })}

                {extraLeaves
                  .filter((l) => matches(l.name))
                  .map((l) => {
                    const diag = diagByField.get(l.name);
                    const incomplete = aggregateIncomplete(fields[l.index]);
                    return (
                      <div
                        className={`col-row special${fieldSelected(l.index) ? " sel" : ""}${diag ? ` diag-${diag.severity}` : ""}${incomplete ? " diag-warning" : ""}`}
                        key={`x${l.index}`}
                        title={diag?.message ?? (incomplete ? t("node.incomplete") : undefined)}
                        onClick={() => select({ kind: "field", path: node.path, index: l.index })}
                      >
                        <span className="leaf-kind">{l.kind}</span>
                        <span className="col-name">{l.name}</span>
                        <button
                          className="x"
                          title={t("common.remove")}
                          aria-label={t("common.remove")}
                          onClick={(e) => {
                            e.stopPropagation();
                            apply((s) => edit.removeAt(s, node.path, l.index));
                          }}
                        >
                          <Icon name="close" size={13} />
                        </button>
                      </div>
                    );
                  })}

                {cols.length === 0 && (
                  <ManualColumn onAdd={(name) => apply((s) => edit.toggleColumn(s, node.path, name, true))} />
                )}
              </div>

              <footer className="node-add flex flex-col gap-1.5 border-t border-border p-2">
                {catalog && (
                  <RelationPicker
                    suggestions={suggestRelations(catalog, node.table)}
                    onPick={(sg) => apply((s) => edit.addField(s, node.path, sg.build()))}
                  />
                )}
                <div className="add-menus flex gap-1.5">
                  <AddMenu
                    label={t("node.addJoin")}
                    kinds={JOIN_KINDS}
                    onPick={(k) => {
                      // addSpecial appends, so the new field's index is the current count;
                      // select it so the inspector opens on the (incomplete) new join.
                      apply((s) => edit.addSpecial(s, node.path, k));
                      select({ kind: "field", path: node.path, index: fields.length });
                    }}
                  />
                  <FieldMenu
                    onPick={(k) => {
                      apply((s) => edit.addSpecial(s, node.path, k));
                      select({ kind: "field", path: node.path, index: fields.length });
                    }}
                  />
                </div>
              </footer>
            </>
          </div>
        </div>
      )}

      <Handle type="source" position={Position.Right} />
    </div>
  );
}

function typeLabel(ty?: FlussoType): string {
  if (!ty) return "";
  return typeof ty === "string" ? ty : "object" in ty ? "?" : "map" in ty ? "map" : "custom";
}

/// A flat action menu (value stays `""`, each pick fires `onPick`). Used for
/// `+ join`, whose four relation kinds need no grouping or search.
function AddMenu({ label, kinds, onPick }: { label: string; kinds: readonly string[]; onPick: (k: string) => void }) {
  const { t } = useT();
  // Each kind carries its hue (relation kinds + geo) and a one-line description.
  const opts = kinds.map((k) => ({
    label: k,
    value: k,
    description: KIND_HELP[k] ? t(KIND_HELP[k]) : undefined,
    className: kindColorClass(k),
  }));
  return <Select<string> value="" placeholder={label} options={opts} onChange={onPick} className="add-menu flex-1" />;
}

interface MenuEntry {
  /// Unique within the menu; combined with the label as the cmdk search text.
  key: string;
  label: string;
  detail?: string;
  className?: string;
  onSelect: () => void;
}

interface MenuSection {
  heading: string;
  entries: MenuEntry[];
}

/// A searchable, grouped popover menu — the shared shell behind the footer's
/// suggestion pickers. The cmdk list is bounded and scrolls on its own, and the
/// Popover is `modal` + portalled: without `modal` the enclosing React Flow node
/// swallows the outside pointer-down before Radix's dismiss layer sees it (so it
/// wouldn't close on an outside click), and the portal keeps the wheel away from
/// React Flow's zoom.
function SearchMenu({
  trigger,
  placeholder,
  sections,
}: {
  trigger: ReactNode;
  placeholder: string;
  sections: MenuSection[];
}) {
  const { t } = useT();
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState("");
  const dismiss = (fn: () => void) => {
    fn();
    setOpen(false);
    setQuery("");
  };
  return (
    <Popover open={open} onOpenChange={setOpen} modal>
      <PopoverTrigger asChild>{trigger}</PopoverTrigger>
      <PopoverContent
        className="w-auto max-w-[92vw] min-w-(--radix-popover-trigger-width) p-0"
        onClick={(e) => e.stopPropagation()}
      >
        <Command>
          <CommandInput value={query} onValueChange={setQuery} placeholder={placeholder} />
          <CommandList className="max-h-80">
            <CommandEmpty>{t("common.noMatch")}</CommandEmpty>
            {sections.map((s) => (
              <CommandGroup key={s.heading} heading={s.heading}>
                {s.entries.map((e) => (
                  <CommandItem key={e.key} value={`${e.label} ${e.key}`} onSelect={() => dismiss(e.onSelect)}>
                    <div className="flex min-w-0 flex-col gap-0.5">
                      <span className={cn("truncate font-mono", e.className)}>{e.label}</span>
                      {e.detail && <span className="text-2xs text-muted-foreground">{e.detail}</span>}
                    </div>
                  </CommandItem>
                ))}
              </CommandGroup>
            ))}
          </CommandList>
        </Command>
      </PopoverContent>
    </Popover>
  );
}

/// The FK-relationship picker: a searchable, verb-grouped popover over every
/// suggested relation (`belongs_to`/`has_many`/`many_to_many`). Collapsing them
/// behind one trigger is what keeps a hub table's node from running off the
/// canvas.
function RelationPicker({
  suggestions,
  onPick,
}: {
  suggestions: RelationSuggestion[];
  onPick: (sg: RelationSuggestion) => void;
}) {
  const { t } = useT();
  if (suggestions.length === 0) return null;
  const sections = REL_VERB_ORDER.map((verb) => ({
    heading: verb,
    entries: suggestions
      .filter((s) => s.verb === verb)
      .map((sg) => ({
        key: sg.key,
        label: sg.target,
        detail: sg.detail,
        className: kindColorClass(sg.verb),
        onSelect: () => onPick(sg),
      })),
  })).filter((s) => s.entries.length > 0);
  return (
    <SearchMenu
      placeholder={t("node.searchRelations")}
      sections={sections}
      trigger={
        <button className="suggest flex w-full items-center gap-1.5" title={t("node.addRelationHint")}>
          <Icon name="plus" size={12} />
          {t("node.addRelation", { n: suggestions.length })}
        </button>
      }
    />
  );
}

/// The `+ field` picker: the special field + aggregate kinds, grouped and
/// searchable (same shell as the relation picker). Its trigger mirrors the
/// neutral `+ join` menu button beside it.
function FieldMenu({ onPick }: { onPick: (k: string) => void }) {
  const { t } = useT();
  const section = (heading: string, kinds: readonly string[]): MenuSection => ({
    heading,
    entries: kinds.map((k) => ({
      key: k,
      label: k,
      detail: KIND_HELP[k] ? t(KIND_HELP[k]) : undefined,
      className: kindColorClass(k),
      onSelect: () => onPick(k),
    })),
  });
  return (
    <SearchMenu
      placeholder={t("node.searchFields")}
      sections={[section(t("node.fieldGroup"), FIELD_KINDS), section(t("node.aggGroup"), AGG_KINDS)]}
      trigger={
        <button
          className="add-menu flex h-8 flex-1 cursor-pointer items-center justify-between gap-2 rounded-md border border-border bg-secondary px-2.5 py-1 text-sm whitespace-nowrap text-muted-foreground transition-colors outline-none hover:border-muted-foreground focus-visible:border-ring focus-visible:ring-[3px] focus-visible:ring-ring/50"
          title={t("node.addField")}
        >
          {t("node.addField")}
          <ChevronDownIcon className="size-4 shrink-0 opacity-50" />
        </button>
      }
    />
  );
}

/// Type a column name the catalog doesn't list (offline, or a view) and Enter to
/// add it.
function ManualColumn({ onAdd }: { onAdd: (name: string) => void }) {
  const { t } = useT();
  const [name, setName] = useState("");
  return (
    <Text
      className="manual-col w-full text-xs"
      value={name}
      onChange={setName}
      placeholder={t("node.addColumn")}
      onKeyDown={(e) => {
        if (e.key === "Enter" && name.trim()) {
          onAdd(name.trim());
          setName("");
        }
      }}
    />
  );
}

/// The empty-state root-table entry (when the catalog has no tables to pick).
function RootTableInput({ onPick }: { onPick: (table: string) => void }) {
  const { t } = useT();
  const [name, setName] = useState("");
  return (
    <Text
      value={name}
      onChange={setName}
      placeholder={t("node.rootTableEnter")}
      onKeyDown={(e) => {
        if (e.key === "Enter" && name.trim()) onPick(name.trim());
      }}
    />
  );
}
