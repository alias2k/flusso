import {
  SCALAR_TYPES,
  type Aggregate,
  type AggregateKey,
  type Column,
  type Field,
  type FlussoType,
  type Join,
  type JoinKind,
  type SoftDelete,
} from "../api";
import { LEAF_TYPES } from "../fields";
import * as edit from "../model/edit";
import { effectiveTable, fieldAtPath, joinOf, nodeFields } from "../model/tree";
import { useDesign } from "../state";
import { Filters } from "./Filters";
import { Check, Field as Row, Num, Select, Text } from "./widgets";

export function Inspector() {
  const { selection } = useDesign();
  if (!selection) return <div className="inspector empty">Select a node or field to edit its details.</div>;
  if (selection.kind === "root") return <RootInspector />;
  if (selection.kind === "node") return <NodeInspector path={selection.path} />;
  return <FieldInspector path={selection.path} index={selection.index} />;
}

function RootInspector() {
  const { schema, apply, catalog } = useDesign();
  const tables = catalog?.catalog.tables.map((t) => t.name) ?? [];
  const cols = catalog?.catalog.tables.find((t) => t.name === schema.table)?.columns.map((c) => c.name) ?? [];
  return (
    <div className="inspector">
      <h3>Index root</h3>
      <Row label="root table">
        <Text value={schema.table} list={tables} onChange={(table) => apply((s) => edit.setRootMeta(s, { table }))} />
      </Row>
      <Row label="schema">
        <Text value={schema.db_schema} onChange={(db_schema) => apply((s) => edit.setRootMeta(s, { db_schema }))} placeholder="public" />
      </Row>
      <Row label="primary_key">
        <Text value={schema.primary_key ?? ""} list={cols} onChange={(pk) => apply((s) => edit.setRootMeta(s, { primary_key: pk || undefined }))} />
      </Row>
      <SoftDeleteEditor value={schema.soft_delete} onChange={(soft_delete) => apply((s) => ({ ...s, soft_delete }))} cols={cols} />
      <details>
        <summary>root filters</summary>
        <Filters value={schema.filters ?? []} onChange={(filters) => apply((s) => ({ ...s, filters }))} columns={cols} />
      </details>
    </div>
  );
}

function NodeInspector({ path }: { path: number[] }) {
  const { schema, apply, catalog } = useDesign();
  const field = fieldAtPath(schema, path);
  if (!field) return null;
  const setField = (f: Field) => apply((s) => edit.setNode(s, path, f));

  if ("group" in field.source) {
    return (
      <div className="inspector">
        <h3>Object group</h3>
        <Row label="field name">
          <Text value={field.field} onChange={(name) => setField({ ...field, field: name })} />
        </Row>
        <p className="hint">A group nests columns of the same table. Add fields on its node.</p>
      </div>
    );
  }

  const join = joinOf(field);
  if (!join) return null;
  const verb = joinVerb(join.kind);
  const tables = catalog?.catalog.tables.map((t) => t.name) ?? [];
  const relCols = catalog?.catalog.tables.find((t) => t.name === join.table)?.columns.map((c) => c.name) ?? [];
  const setJoin = (j: Join) => setField({ ...field, source: { relation: { join: j } } });
  const toMany = verb === "has_many" || verb === "many_to_many";

  return (
    <div className="inspector">
      <h3>Join · {verb}</h3>
      <Row label="field name">
        <Text value={field.field} onChange={(name) => setField({ ...field, field: name })} />
      </Row>
      <Row label="verb">
        <Select value={verb} options={["belongs_to", "has_one", "has_many", "many_to_many"]} onChange={(v) => setJoin({ ...join, kind: blankKind(v) })} />
      </Row>
      <Row label="table">
        <Text value={join.table} list={tables} onChange={(table) => setJoin({ ...join, table })} />
      </Row>
      <Row label="primary_key">
        <Text value={join.primary_key} list={relCols} onChange={(primary_key) => setJoin({ ...join, primary_key })} />
      </Row>
      {"belongs_to" in join.kind && (
        <Row label="column (this table → target)">
          <Text value={join.kind.belongs_to.column} onChange={(c) => setJoin({ ...join, kind: { belongs_to: { column: c } } })} />
        </Row>
      )}
      {"has_one" in join.kind && (
        <Row label="foreign_key (on target)">
          <Text value={join.kind.has_one.foreign_key} list={relCols} onChange={(c) => setJoin({ ...join, kind: { has_one: { foreign_key: c } } })} />
        </Row>
      )}
      {"has_many" in join.kind && (
        <Row label="foreign_key (on target)">
          <Text value={join.kind.has_many.foreign_key} list={relCols} onChange={(c) => setJoin({ ...join, kind: { has_many: { foreign_key: c } } })} />
        </Row>
      )}
      {"many_to_many" in join.kind && (
        <ThroughEditor through={join.kind.many_to_many.through} tables={tables} onChange={(through) => setJoin({ ...join, kind: { many_to_many: { through } } })} />
      )}
      {!toMany && <Check value={!join.nullable} label="required" onChange={(req) => setJoin({ ...join, nullable: !req })} />}
      {verb !== "belongs_to" && <OrderByEditor value={join.order_by ?? []} cols={relCols} onChange={(order_by) => setJoin({ ...join, order_by })} />}
      {toMany && (
        <Row label="limit">
          <Num value={join.limit} onChange={(limit) => setJoin({ ...join, limit })} />
        </Row>
      )}
      <details>
        <summary>filters</summary>
        <Filters value={join.filters ?? []} columns={relCols} onChange={(filters) => setJoin({ ...join, filters })} />
      </details>
    </div>
  );
}

function FieldInspector({ path, index }: { path: number[]; index: number }) {
  const { schema, apply, catalog } = useDesign();
  const field = nodeFields(schema, path)[index];
  if (!field) return null;
  const table = effectiveTable(schema, path);
  const cols = catalog?.catalog.tables.find((t) => t.name === table)?.columns.map((c) => c.name) ?? [];
  const tables = catalog?.catalog.tables.map((t) => t.name) ?? [];
  const set = (f: Field) => apply((s) => edit.setLeaf(s, path, index, f));
  const s = field.source;

  return (
    <div className="inspector">
      <h3>Field · {field.field}</h3>
      <Row label="field name">
        <Text value={field.field} onChange={(name) => set({ ...field, field: name })} />
      </Row>

      {"column" in s && typeof s.column.ty === "string" && <ScalarBody field={field} column={s.column} cols={cols} set={set} />}
      {"column" in s && typeof s.column.ty !== "string" && "map" in s.column.ty && <MapBody field={field} column={s.column} cols={cols} set={set} />}
      {"column" in s && typeof s.column.ty !== "string" && "custom" in s.column.ty && <CustomBody field={field} column={s.column} cols={cols} set={set} />}
      {"geo" in s && <GeoBody field={field} set={set} cols={cols} />}
      {"constant" in s && (
        <Row label="value (JSON)">
          <Text
            value={JSON.stringify(s.constant)}
            onChange={(t) => {
              try {
                set({ ...field, source: { constant: JSON.parse(t) } });
              } catch {
                /* keep typing */
              }
            }}
          />
        </Row>
      )}
      {"relation" in s && "aggregate" in s.relation && <AggregateBody field={field} agg={s.relation.aggregate} tables={tables} set={set} />}
    </div>
  );
}

// --- leaf bodies ---

function ScalarBody({ field, column, cols, set }: { field: Field; column: Column; cols: string[]; set: (f: Field) => void }) {
  const setCol = (c: Column) => set({ ...field, source: { column: c } });
  const has = (t: "lowercase" | "trim") => (column.transforms ?? []).includes(t);
  const toggle = (t: "lowercase" | "trim", on: boolean) => {
    const next = new Set(column.transforms ?? []);
    on ? next.add(t) : next.delete(t);
    setCol({ ...column, transforms: next.size ? [...next] : undefined });
  };
  return (
    <>
      <Row label="column">
        <Text value={column.column} list={cols} onChange={(c) => setCol({ ...column, column: c })} />
      </Row>
      <Row label="type">
        <Select value={column.ty as string} options={SCALAR_TYPES as string[]} onChange={(ty) => setCol({ ...column, ty: ty as FlussoType })} />
      </Row>
      <Check value={!column.nullable} label="required" onChange={(req) => setCol({ ...column, nullable: !req })} />
      <Check value={has("lowercase")} label="lowercase" onChange={(on) => toggle("lowercase", on)} />
      <Check value={has("trim")} label="trim" onChange={(on) => toggle("trim", on)} />
    </>
  );
}

function MapBody({ field, column, cols, set }: { field: Field; column: Column; cols: string[]; set: (f: Field) => void }) {
  const ty = column.ty as { map: { values: FlussoType } };
  const setCol = (c: Column) => set({ ...field, source: { column: c } });
  return (
    <>
      <Row label="values">
        <Select value={ty.map.values as string} options={LEAF_TYPES as string[]} onChange={(v) => setCol({ ...column, ty: { map: { values: v as FlussoType } } })} />
      </Row>
      <Row label="column (json/jsonb)">
        <Text value={column.column} list={cols} onChange={(c) => setCol({ ...column, column: c })} />
      </Row>
      <Check value={!column.nullable} label="required" onChange={(req) => setCol({ ...column, nullable: !req })} />
    </>
  );
}

function CustomBody({ field, column, cols, set }: { field: Field; column: Column; cols: string[]; set: (f: Field) => void }) {
  const ty = column.ty as { custom: { postgres: string[]; opensearch: string } };
  const setCol = (c: Column) => set({ ...field, source: { column: c } });
  return (
    <>
      <Row label="postgres types (comma)">
        <Text value={ty.custom.postgres.join(", ")} onChange={(t) => setCol({ ...column, ty: { custom: { ...ty.custom, postgres: t.split(",").map((x) => x.trim()).filter(Boolean) } } })} />
      </Row>
      <Row label="opensearch type">
        <Text value={ty.custom.opensearch} onChange={(o) => setCol({ ...column, ty: { custom: { ...ty.custom, opensearch: o } } })} />
      </Row>
      <Row label="column">
        <Text value={column.column} list={cols} onChange={(c) => setCol({ ...column, column: c })} />
      </Row>
      <Check value={!column.nullable} label="required" onChange={(req) => setCol({ ...column, nullable: !req })} />
    </>
  );
}

function GeoBody({ field, set, cols }: { field: Field; set: (f: Field) => void; cols: string[] }) {
  if (!("geo" in field.source)) return null;
  const geo = field.source.geo;
  return (
    <>
      <Row label="lat column">
        <Text value={geo.lat} list={cols} onChange={(lat) => set({ ...field, source: { geo: { ...geo, lat } } })} />
      </Row>
      <Row label="lon column">
        <Text value={geo.lon} list={cols} onChange={(lon) => set({ ...field, source: { geo: { ...geo, lon } } })} />
      </Row>
      <Check value={!geo.nullable} label="required" onChange={(req) => set({ ...field, source: { geo: { ...geo, nullable: !req } } })} />
    </>
  );
}

function AggregateBody({ field, agg, tables, set }: { field: Field; agg: Aggregate; tables: string[]; set: (f: Field) => void }) {
  const { columnsFor } = useDesign();
  const setAgg = (a: Aggregate) => set({ ...field, source: { relation: { aggregate: a } } });
  const op = agg.op;
  const opCol = typeof op === "string" ? null : "sum" in op ? op.sum : "avg" in op ? op.avg : "min" in op ? op.min : "max" in op ? op.max : null;
  const kind = typeof op === "string" ? "count" : "sum" in op ? "sum" : "avg" in op ? "avg" : "min" in op ? "min" : "max" in op ? "max" : "ids";
  const aggCols = columnsFor(agg.table).map((c) => c.name);
  return (
    <>
      <Row label="related table">
        <Text value={agg.table} list={tables} onChange={(table) => setAgg({ ...agg, table })} />
      </Row>
      {opCol !== null && (
        <Row label="column (to aggregate)">
          <Text value={opCol} list={aggCols} onChange={(c) => setAgg({ ...agg, op: withAggColumn(kind, c) })} />
        </Row>
      )}
      {(kind === "sum" || kind === "min" || kind === "max") && (
        <Row label="value_type">
          <Select value={(agg.value_type as string) ?? "integer"} options={SCALAR_TYPES as string[]} onChange={(v) => setAgg({ ...agg, value_type: v as FlussoType })} />
        </Row>
      )}
      {kind === "ids" && typeof op !== "string" && "ids" in op && (
        <Row label="element_type">
          <Select value={op.ids.element_type as string} options={SCALAR_TYPES as string[]} onChange={(v) => setAgg({ ...agg, op: { ids: { element_type: v as FlussoType } } })} />
        </Row>
      )}
      <AggregateKeyEditor value={agg.key} tables={tables} onChange={(key) => setAgg({ ...agg, key })} />
      <details>
        <summary>filters</summary>
        <Filters value={agg.filters ?? []} onChange={(filters) => setAgg({ ...agg, filters })} />
      </details>
    </>
  );
}

function withAggColumn(kind: string, col: string): Aggregate["op"] {
  switch (kind) {
    case "sum":
      return { sum: col };
    case "avg":
      return { avg: col };
    case "min":
      return { min: col };
    default:
      return { max: col };
  }
}

// --- shared sub-editors ---

function AggregateKeyEditor({ value, tables, onChange }: { value: AggregateKey; tables: string[]; onChange: (k: AggregateKey) => void }) {
  const direct = "direct" in value;
  return (
    <div className="key-editor">
      <Row label="key">
        <Select value={direct ? "direct" : "through"} options={["direct", "through"]} onChange={(k) => onChange(k === "direct" ? { direct: "" } : { through: { table: "", left_key: "", right_key: "" } })} />
      </Row>
      {direct ? (
        <Row label="foreign_key">
          <Text value={value.direct} onChange={(c) => onChange({ direct: c })} />
        </Row>
      ) : (
        <ThroughEditor through={value.through} tables={tables} onChange={(through) => onChange({ through })} />
      )}
    </div>
  );
}

function ThroughEditor({ through, tables, onChange }: { through: { table: string; left_key: string; right_key: string }; tables: string[]; onChange: (t: { table: string; left_key: string; right_key: string }) => void }) {
  return (
    <div className="through">
      <Row label="junction table">
        <Text value={through.table} list={tables} onChange={(table) => onChange({ ...through, table })} />
      </Row>
      <Row label="left_key">
        <Text value={through.left_key} onChange={(left_key) => onChange({ ...through, left_key })} />
      </Row>
      <Row label="right_key">
        <Text value={through.right_key} onChange={(right_key) => onChange({ ...through, right_key })} />
      </Row>
    </div>
  );
}

function OrderByEditor({ value, cols, onChange }: { value: { column: string; direction?: "asc" | "desc" }[]; cols: string[]; onChange: (v: { column: string; direction?: "asc" | "desc" }[] | undefined) => void }) {
  const set = (i: number, ob: { column: string; direction?: "asc" | "desc" }) => {
    const next = value.slice();
    next[i] = ob;
    onChange(next);
  };
  return (
    <div className="order-by">
      <span className="field-label">order_by</span>
      {value.map((ob, i) => (
        <div className="order-row" key={i}>
          <Text value={ob.column} list={cols} onChange={(column) => set(i, { ...ob, column })} placeholder="column" />
          <Select value={ob.direction ?? "asc"} options={["asc", "desc"]} onChange={(direction) => set(i, { ...ob, direction })} />
          <button className="link danger" onClick={() => onChange(value.filter((_, j) => j !== i).length ? value.filter((_, j) => j !== i) : undefined)}>
            ✕
          </button>
        </div>
      ))}
      <button className="link" onClick={() => onChange([...value, { column: "", direction: "asc" }])}>
        + order_by
      </button>
    </div>
  );
}

function SoftDeleteEditor({ value, onChange, cols }: { value: SoftDelete | undefined; onChange: (v: SoftDelete | undefined) => void; cols: string[] }) {
  const kind = value === undefined ? "none" : "field" in value ? "field" : "column";
  return (
    <div className="soft-delete">
      <Row label="soft delete">
        <Select value={kind} options={["none", "field", "column"]} onChange={(k) => onChange(k === "none" ? undefined : k === "field" ? { field: "" } : { column: "" })} />
      </Row>
      {value && "field" in value && <Text value={value.field} onChange={(f) => onChange({ ...value, field: f })} placeholder="document field" />}
      {value && "column" in value && <Text value={value.column} list={cols} onChange={(c) => onChange({ ...value, column: c })} placeholder="column" />}
    </div>
  );
}

function joinVerb(kind: JoinKind): "belongs_to" | "has_one" | "has_many" | "many_to_many" {
  if ("belongs_to" in kind) return "belongs_to";
  if ("has_one" in kind) return "has_one";
  if ("has_many" in kind) return "has_many";
  return "many_to_many";
}

function blankKind(verb: string): JoinKind {
  switch (verb) {
    case "belongs_to":
      return { belongs_to: { column: "" } };
    case "has_one":
      return { has_one: { foreign_key: "" } };
    case "has_many":
      return { has_many: { foreign_key: "" } };
    default:
      return { many_to_many: { through: { table: "", left_key: "", right_key: "" } } };
  }
}
