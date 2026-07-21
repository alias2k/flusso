import { Handle, Position, type NodeProps } from "@xyflow/react";
import { CheckCheck, X } from "lucide-react";
import { useState } from "react";
import { SCALAR_TYPES, type ColumnShape, type FlussoType } from "../api";
import { KIND_HELP } from "../fields";
import { aggregateIncomplete, joinIncomplete } from "../model/complete";
import * as edit from "../model/edit";
import { suggestRelations } from "../model/relations";
import { fieldAtPath, nodeFields, type DocNode, type LeafField } from "../model/tree";
import { useT } from "../i18n";
import { useDesign } from "../state";
import { kindColorClass, typeClass } from "../theme";
import { Hint } from "./Hint";
import { Icon } from "./Icon";
import { Select, Text } from "./widgets";
import { Checkbox } from "@/components/ui/checkbox";

const FIELD_KINDS = ["object", "geo", "map", "custom", "constant"] as const;
const AGG_KINDS = ["count", "sum", "avg", "min", "max", "ids"] as const;
const JOIN_KINDS = ["belongs_to", "has_one", "has_many", "many_to_many"] as const;

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
        <span className="node-title">{node.name ?? node.table}</span>
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
        !isCollapsed && (
          <>
            {cols.length > 0 && (
              <div className="col-tools flex items-center gap-1.5 px-2 pt-2" onClick={(e) => e.stopPropagation()}>
                <Text
                  className="col-filter flex-1"
                  value={filter}
                  onChange={setFilter}
                  placeholder={t("node.filterCols")}
                />
                <div className="flex shrink-0 items-center overflow-hidden rounded-md border border-border bg-secondary">
                  <Hint label={t("node.includeAll")} side="top">
                    <button
                      type="button"
                      aria-label={t("node.includeAll")}
                      className="flex cursor-pointer items-center px-2 py-1.5 text-muted-foreground transition-colors hover:bg-accent hover:text-primary"
                      onClick={() =>
                        apply((s) =>
                          edit.includeColumns(
                            s,
                            node.path,
                            cols.map((c) => ({ name: c.name, ty: c.suggested_type, nullable: c.nullable })),
                          ),
                        )
                      }
                    >
                      <CheckCheck className="size-3.5" />
                    </button>
                  </Hint>
                  <span className="h-4 w-px bg-border" />
                  <Hint label={t("node.clearAll")} side="top">
                    <button
                      type="button"
                      aria-label={t("node.clearAll")}
                      className="flex cursor-pointer items-center px-2 py-1.5 text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
                      onClick={() => apply((s) => edit.clearColumns(s, node.path))}
                    >
                      <X className="size-3.5" />
                    </button>
                  </Hint>
                </div>
              </div>
            )}

            <div className="node-cols px-2 py-1.5">
              {cols
                .filter((c) => matches(c.name))
                .map((c) => {
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
                        onClick={(e) => e.stopPropagation()}
                        onCheckedChange={(ch) => (ch === true ? includeColumn(c) : inc && excludeColumn(c, inc))}
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
              {catalog &&
                suggestRelations(catalog, node.table).map((sg) => (
                  <button
                    key={sg.key}
                    className="suggest"
                    title={sg.detail}
                    onClick={() => apply((s) => edit.addField(s, node.path, sg.build()))}
                  >
                    + {sg.label}
                  </button>
                ))}
              <div className="add-menus flex gap-1.5">
                <AddMenu
                  label={t("node.addJoin")}
                  kinds={JOIN_KINDS}
                  onPick={(k) => apply((s) => edit.addSpecial(s, node.path, k))}
                />
                <AddMenu
                  label={t("node.addField")}
                  kinds={[...FIELD_KINDS, ...AGG_KINDS]}
                  onPick={(k) => apply((s) => edit.addSpecial(s, node.path, k))}
                />
              </div>
            </footer>
          </>
        )
      )}

      <Handle type="source" position={Position.Right} />
    </div>
  );
}

function typeLabel(ty?: FlussoType): string {
  if (!ty) return "";
  return typeof ty === "string" ? ty : "object" in ty ? "?" : "map" in ty ? "map" : "custom";
}

function AddMenu({ label, kinds, onPick }: { label: string; kinds: readonly string[]; onPick: (k: string) => void }) {
  const { t } = useT();
  // An action menu: value stays "" (placeholder = label), each pick fires onPick.
  // Each kind carries its hue (relation kinds + geo) and a one-line description.
  const opts = kinds.map((k) => ({
    label: k,
    value: k,
    description: KIND_HELP[k] ? t(KIND_HELP[k]) : undefined,
    className: kindColorClass(k),
  }));
  return <Select<string> value="" placeholder={label} options={opts} onChange={onPick} className="add-menu flex-1" />;
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
