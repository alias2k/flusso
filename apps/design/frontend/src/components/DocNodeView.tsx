import { Handle, Position, type NodeProps } from "@xyflow/react";
import { useState } from "react";
import { SCALAR_TYPES, type ColumnShape, type FlussoType } from "../api";
import { aggregateIncomplete, joinIncomplete } from "../model/complete";
import * as edit from "../model/edit";
import { suggestRelations } from "../model/relations";
import { fieldAtPath, nodeFields, type DocNode, type LeafField } from "../model/tree";
import { useT } from "../i18n";
import { useDesign } from "../state";
import { typeClass } from "../theme";
import { Icon } from "./Icon";
import { Text } from "./widgets";

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
        <div className="empty-state">
          <span>{t("node.pickRoot")}</span>
          {(catalog?.catalog.tables.length ?? 0) > 0 ? (
            <select value="" onChange={(e) => pickRootTable(e.target.value)}>
              <option value="">{t("node.chooseTable")}</option>
              {catalog?.catalog.tables.map((tbl) => (
                <option key={tbl.name} value={tbl.name}>
                  {tbl.name}
                </option>
              ))}
            </select>
          ) : (
            <RootTableInput onPick={pickRootTable} />
          )}
        </div>
      ) : (
        !isCollapsed && (
        <>
          {cols.length > 0 && (
            <div className="col-tools" onClick={(e) => e.stopPropagation()}>
              <Text className="col-filter" value={filter} onChange={setFilter} placeholder={t("node.filterCols")} />
              <button
                className="link"
                title={t("node.includeAll")}
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
                {t("node.all")}
              </button>
              <button className="link" title={t("node.clearAll")} onClick={() => apply((s) => edit.clearColumns(s, node.path))}>
                {t("node.none")}
              </button>
            </div>
          )}

          <div className="node-cols">
            {cols.filter((c) => matches(c.name)).map((c) => {
              const inc = includedByCol.get(c.name);
              const renamed = inc && inc.name !== c.name;
              const diag = inc ? diagByField.get(inc.name) : undefined;
              return (
                <div
                  className={`col-row${inc ? " on" : ""}${inc && fieldSelected(inc.index) ? " sel" : ""}${diag ? ` diag-${diag.severity}` : ""}`}
                  key={c.name}
                  title={diag?.message}
                  onClick={() => inc && select({ kind: "field", path: node.path, index: inc.index })}
                >
                  <input
                    type="checkbox"
                    checked={!!inc}
                    onClick={(e) => e.stopPropagation()}
                    onChange={(e) => (e.target.checked ? includeColumn(c) : inc && excludeColumn(c, inc))}
                  />
                  <span className="col-name" title={renamed ? t("node.columnOf", { name: c.name }) : undefined}>
                    {inc ? inc.name : c.name}
                    {renamed ? <span className="col-from"> ← {c.name}</span> : null}
                  </span>
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

          <footer className="node-add">
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
            <div className="add-menus">
              <AddMenu label={t("node.addJoin")} kinds={JOIN_KINDS} onPick={(k) => apply((s) => edit.addSpecial(s, node.path, k))} />
              <AddMenu label={t("node.addField")} kinds={[...FIELD_KINDS, ...AGG_KINDS]} onPick={(k) => apply((s) => edit.addSpecial(s, node.path, k))} />
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

function AddMenu({
  label,
  kinds,
  onPick,
}: {
  label: string;
  kinds: readonly string[];
  onPick: (k: string) => void;
}) {
  return (
    <select
      className="add-menu"
      value=""
      onChange={(e) => {
        if (e.target.value) onPick(e.target.value);
        e.target.value = "";
      }}
    >
      <option value="">{label}</option>
      {kinds.map((k) => (
        <option key={k} value={k}>
          {k}
        </option>
      ))}
    </select>
  );
}

/// Type a column name the catalog doesn't list (offline, or a view) and Enter to
/// add it.
function ManualColumn({ onAdd }: { onAdd: (name: string) => void }) {
  const { t } = useT();
  const [name, setName] = useState("");
  return (
    <Text
      className="manual-col"
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
