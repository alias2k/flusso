import { Handle, Position, type NodeProps } from "@xyflow/react";
import { SCALAR_TYPES, type Column, type Field, type FlussoType } from "../api";
import * as edit from "../model/edit";
import { suggestRelations } from "../model/relations";
import { nodeFields, type DocNode, type LeafField } from "../model/tree";
import { useDesign } from "../state";

const FIELD_KINDS = ["object", "geo", "map", "custom", "constant"] as const;
const AGG_KINDS = ["count", "sum", "avg", "min", "max", "ids"] as const;
const JOIN_KINDS = ["belongs_to", "has_one", "has_many", "many_to_many"] as const;

export function DocNodeView({ data, selected }: NodeProps) {
  const node = (data as { node: DocNode }).node;
  const { catalog, schema, apply, select, columnsFor } = useDesign();
  const cols = columnsFor(node.table);
  const fields = nodeFields(schema, node.path);

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

  const setLeaf = (index: number, field: Field) => apply((s) => edit.setLeaf(s, node.path, index, field));
  const rename = (l: LeafField, name: string, original: Field) => setLeaf(l.index, { ...original, field: name });

  return (
    <div className={`flow-node kind-${node.kind}${selected ? " selected" : ""}`}>
      <Handle type="target" position={Position.Left} />
      <header onClick={() => select(node.path.length ? { kind: "node", path: node.path } : { kind: "root" })}>
        <span className={`badge ${node.kind}`}>{node.kind}</span>
        <span className="node-title">{node.name ?? node.table}</span>
        {node.path.length > 0 && (
          <button
            className="x"
            title="remove"
            onClick={(e) => {
              e.stopPropagation();
              apply((s) => edit.removeNode(s, node.path));
              select(null);
            }}
          >
            ✕
          </button>
        )}
        <div className="node-sub">
          {node.table}
          {node.primaryKey ? ` · pk: ${node.primaryKey}` : ""}
        </div>
      </header>

      <div className="node-cols">
        {cols.map((c) => {
          const inc = includedByCol.get(c.name);
          const field = inc ? fields[inc.index] : null;
          return (
            <div className={`col-row${inc ? " on" : ""}`} key={c.name}>
              <input
                type="checkbox"
                checked={!!inc}
                onChange={(e) =>
                  apply((s) =>
                    edit.toggleColumn(s, node.path, c.name, e.target.checked, c.suggested_type ?? "keyword", c.nullable),
                  )
                }
              />
              {inc && field ? (
                <>
                  <input
                    className="rename"
                    value={inc.name}
                    onClick={(e) => e.stopPropagation()}
                    onChange={(e) => rename(inc, e.target.value, field)}
                  />
                  <select
                    value={inc.ty as string}
                    onClick={(e) => e.stopPropagation()}
                    onChange={(e) => setLeaf(inc.index, withTy(field, e.target.value as FlussoType))}
                  >
                    {(SCALAR_TYPES as string[]).map((t) => (
                      <option key={t}>{t}</option>
                    ))}
                  </select>
                </>
              ) : (
                <>
                  <span className="col-name">{c.name}</span>
                  <span className="col-type">{typeLabel(c.suggested_type)}</span>
                </>
              )}
            </div>
          );
        })}

        {extraLeaves.map((l) => (
          <div
            className="col-row special"
            key={`x${l.index}`}
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
              ✕
            </button>
          </div>
        ))}

        {cols.length === 0 && <ManualColumn onAdd={(name) => apply((s) => edit.toggleColumn(s, node.path, name, true))} />}
      </div>

      <footer className="node-add">
        {catalog &&
          suggestRelations(catalog, node.table).map((sg) => (
            <button key={sg.key} className="suggest" onClick={() => apply((s) => edit.addField(s, node.path, sg.build()))}>
              + {sg.label}
            </button>
          ))}
        <div className="add-menus">
          <AddMenu label="+ join" kinds={JOIN_KINDS} onPick={(k) => apply((s) => edit.addSpecial(s, node.path, k))} />
          <AddMenu label="+ field" kinds={[...FIELD_KINDS, ...AGG_KINDS]} onPick={(k) => apply((s) => edit.addSpecial(s, node.path, k))} />
        </div>
      </footer>

      <Handle type="source" position={Position.Right} />
    </div>
  );
}

function withTy(field: Field, ty: FlussoType): Field {
  if ("column" in field.source) {
    const col: Column = { ...field.source.column, ty };
    return { ...field, source: { column: col } };
  }
  return field;
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

function ManualColumn({ onAdd }: { onAdd: (name: string) => void }) {
  return (
    <input
      className="manual-col"
      placeholder="+ column name, Enter"
      onKeyDown={(e) => {
        if (e.key === "Enter" && e.currentTarget.value.trim()) {
          onAdd(e.currentTarget.value.trim());
          e.currentTarget.value = "";
        }
      }}
    />
  );
}
