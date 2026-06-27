import type {
  Aggregate,
  AggregateKey,
  AggregateOp,
  Column,
  Field as FieldModel,
  FlussoType,
  Join,
  OrderBy,
  TableShape,
} from "../api";
import { SCALAR_TYPES } from "../api";
import { KIND_GROUPS, LEAF_TYPES, defaultField, fieldKind, isToMany, relationTable } from "../fields";
import { Filters } from "./Filters";
import { Check, Field, Num, Select, Text } from "./widgets";

export interface CatalogCtx {
  tables: TableShape[];
  columnsFor: (table: string) => string[];
}

interface Props {
  field: FieldModel;
  onChange: (f: FieldModel) => void;
  onRemove: () => void;
  ctx: CatalogCtx;
  /// The table this field reads from by default (column suggestions).
  table: string;
}

export function FieldEditor({ field, onChange, onRemove, ctx, table }: Props) {
  const kind = fieldKind(field);
  const tableNames = ctx.tables.map((t) => t.name);
  const cols = ctx.columnsFor(table);

  const changeKind = (newKind: string) => {
    const oldScalar = "column" in field.source && typeof field.source.column.ty === "string";
    const newScalar = (SCALAR_TYPES as string[]).includes(newKind);
    if (oldScalar && newScalar && "column" in field.source) {
      onChange({
        ...field,
        source: { column: { ...field.source.column, ty: newKind as FlussoType } },
      });
      return;
    }
    onChange(defaultField(field.field, newKind, relationTable(field) || table));
  };

  return (
    <div className="field-card">
      <div className="field-head">
        <input
          className="field-name"
          value={field.field}
          placeholder="fieldName"
          onChange={(e) => onChange({ ...field, field: e.target.value })}
        />
        <select value={kind} onChange={(e) => changeKind(e.target.value)}>
          {KIND_GROUPS.map((g) => (
            <optgroup key={g.label} label={g.label}>
              {g.kinds.map((k) => (
                <option key={k} value={k}>
                  {k}
                </option>
              ))}
            </optgroup>
          ))}
        </select>
        <button className="link danger" onClick={onRemove}>
          ✕
        </button>
      </div>
      <div className="field-body">{body(field, kind, onChange, ctx, table, cols, tableNames)}</div>
    </div>
  );
}

function body(
  field: FieldModel,
  kind: string,
  onChange: (f: FieldModel) => void,
  ctx: CatalogCtx,
  table: string,
  cols: string[],
  tableNames: string[],
) {
  const s = field.source;

  if ("column" in s) {
    const ty = s.column.ty;
    if (typeof ty === "string") return scalarBody(field, s.column, onChange, cols);
    if ("map" in ty) return mapBody(field, s.column, onChange, cols);
    if ("custom" in ty) return customBody(field, s.column, onChange, cols);
  }
  if ("geo" in s) {
    const geo = s.geo;
    return (
      <>
        <Field label="lat column">
          <Text value={geo.lat} onChange={(lat) => onChange({ ...field, source: { geo: { ...geo, lat } } })} list={cols} />
        </Field>
        <Field label="lon column">
          <Text value={geo.lon} onChange={(lon) => onChange({ ...field, source: { geo: { ...geo, lon } } })} list={cols} />
        </Field>
        <Check value={!geo.nullable} label="required" onChange={(req) => onChange({ ...field, source: { geo: { ...geo, nullable: !req } } })} />
      </>
    );
  }
  if ("group" in s) {
    return (
      <NestedFields
        fields={s.group}
        onChange={(group) => onChange({ ...field, source: { group } })}
        ctx={ctx}
        table={table}
      />
    );
  }
  if ("constant" in s) {
    return (
      <Field label="value (JSON)">
        <Text
          value={JSON.stringify(s.constant)}
          onChange={(t) => {
            try {
              onChange({ ...field, source: { constant: JSON.parse(t) } });
            } catch {
              /* keep typing until valid */
            }
          }}
        />
      </Field>
    );
  }
  if ("relation" in s && "join" in s.relation) {
    return (
      <JoinBody
        field={field}
        join={s.relation.join}
        kind={kind}
        onChange={onChange}
        ctx={ctx}
        tableNames={tableNames}
        ownerCols={cols}
      />
    );
  }
  if ("relation" in s && "aggregate" in s.relation) {
    return (
      <AggregateBody
        field={field}
        agg={s.relation.aggregate}
        kind={kind}
        onChange={onChange}
        tableNames={tableNames}
        ctx={ctx}
      />
    );
  }
  return null;
}

function scalarBody(field: FieldModel, column: Column, onChange: (f: FieldModel) => void, cols: string[]) {
  const set = (c: Column) => onChange({ ...field, source: { column: c } });
  const has = (t: "lowercase" | "trim") => (column.transforms ?? []).includes(t);
  const toggle = (t: "lowercase" | "trim", on: boolean) => {
    const next = new Set(column.transforms ?? []);
    on ? next.add(t) : next.delete(t);
    set({ ...column, transforms: next.size ? [...next] : undefined });
  };
  return (
    <>
      <Field label="column">
        <Text value={column.column} onChange={(c) => set({ ...column, column: c })} list={cols} placeholder={field.field} />
      </Field>
      <Check value={!column.nullable} label="required" onChange={(req) => set({ ...column, nullable: !req })} />
      <Check value={has("lowercase")} label="lowercase" onChange={(on) => toggle("lowercase", on)} />
      <Check value={has("trim")} label="trim" onChange={(on) => toggle("trim", on)} />
    </>
  );
}

function mapBody(field: FieldModel, column: Column, onChange: (f: FieldModel) => void, cols: string[]) {
  const ty = column.ty as { map: { values: FlussoType } };
  const set = (c: Column) => onChange({ ...field, source: { column: c } });
  return (
    <>
      <Field label="values">
        <Select
          value={ty.map.values as string}
          onChange={(v) => set({ ...column, ty: { map: { values: v as FlussoType } } })}
          options={LEAF_TYPES as string[]}
        />
      </Field>
      <Field label="column (json/jsonb)">
        <Text value={column.column} onChange={(c) => set({ ...column, column: c })} list={cols} />
      </Field>
      <Check value={!column.nullable} label="required" onChange={(req) => set({ ...column, nullable: !req })} />
    </>
  );
}

function customBody(field: FieldModel, column: Column, onChange: (f: FieldModel) => void, cols: string[]) {
  const ty = column.ty as { custom: { postgres: string[]; opensearch: string } };
  const set = (c: Column) => onChange({ ...field, source: { column: c } });
  return (
    <>
      <Field label="postgres types (comma)">
        <Text
          value={ty.custom.postgres.join(", ")}
          onChange={(t) =>
            set({
              ...column,
              ty: { custom: { ...ty.custom, postgres: t.split(",").map((s) => s.trim()).filter(Boolean) } },
            })
          }
        />
      </Field>
      <Field label="opensearch type">
        <Text value={ty.custom.opensearch} onChange={(o) => set({ ...column, ty: { custom: { ...ty.custom, opensearch: o } } })} />
      </Field>
      <Field label="column">
        <Text value={column.column} onChange={(c) => set({ ...column, column: c })} list={cols} />
      </Field>
      <Check value={!column.nullable} label="required" onChange={(req) => set({ ...column, nullable: !req })} />
    </>
  );
}

function JoinBody({
  field,
  join,
  kind,
  onChange,
  ctx,
  tableNames,
  ownerCols,
}: {
  field: FieldModel;
  join: Join;
  kind: string;
  onChange: (f: FieldModel) => void;
  ctx: CatalogCtx;
  tableNames: string[];
  ownerCols: string[];
}) {
  const set = (j: Join) => onChange({ ...field, source: { relation: { join: j } } });
  const relCols = ctx.columnsFor(join.table);
  return (
    <>
      <Field label="table">
        <Text value={join.table} onChange={(t) => set({ ...join, table: t })} list={tableNames} />
      </Field>
      <Field label="primary_key">
        <Text value={join.primary_key} onChange={(pk) => set({ ...join, primary_key: pk })} list={relCols} />
      </Field>
      {"belongs_to" in join.kind && (
        <Field label="column (this table → target)">
          <Text value={join.kind.belongs_to.column} onChange={(c) => set({ ...join, kind: { belongs_to: { column: c } } })} list={ownerCols} />
        </Field>
      )}
      {"has_one" in join.kind && (
        <Field label="foreign_key (on target)">
          <Text value={join.kind.has_one.foreign_key} onChange={(c) => set({ ...join, kind: { has_one: { foreign_key: c } } })} list={relCols} />
        </Field>
      )}
      {"has_many" in join.kind && (
        <Field label="foreign_key (on target)">
          <Text value={join.kind.has_many.foreign_key} onChange={(c) => set({ ...join, kind: { has_many: { foreign_key: c } } })} list={relCols} />
        </Field>
      )}
      {"many_to_many" in join.kind && (
        <ThroughEditor
          through={join.kind.many_to_many.through}
          onChange={(through) => set({ ...join, kind: { many_to_many: { through } } })}
          tableNames={tableNames}
        />
      )}
      {!isToMany(kind) && (
        <Check value={!join.nullable} label="required" onChange={(req) => set({ ...join, nullable: !req })} />
      )}
      {kind !== "belongs_to" && (
        <OrderByEditor value={join.order_by ?? []} onChange={(order_by) => set({ ...join, order_by })} cols={relCols} />
      )}
      {isToMany(kind) && (
        <Field label="limit">
          <Num value={join.limit} onChange={(limit) => set({ ...join, limit })} />
        </Field>
      )}
      <details>
        <summary>filters</summary>
        <Filters value={join.filters ?? []} onChange={(filters) => set({ ...join, filters })} columns={relCols} />
      </details>
      <div className="nested">
        <NestedFields fields={join.fields} onChange={(fields) => set({ ...join, fields })} ctx={ctx} table={join.table} />
      </div>
    </>
  );
}

function AggregateBody({
  field,
  agg,
  kind,
  onChange,
  tableNames,
  ctx,
}: {
  field: FieldModel;
  agg: Aggregate;
  kind: string;
  onChange: (f: FieldModel) => void;
  tableNames: string[];
  ctx: CatalogCtx;
}) {
  const set = (a: Aggregate) => onChange({ ...field, source: { relation: { aggregate: a } } });
  const relCols = ctx.columnsFor(agg.table);
  const opCol = aggregateColumn(agg.op);
  const setOpCol = (c: string) => set({ ...agg, op: withColumn(kind, agg.op, c) });
  return (
    <>
      <Field label="table">
        <Text value={agg.table} onChange={(t) => set({ ...agg, table: t })} list={tableNames} />
      </Field>
      {opCol !== null && (
        <Field label="column (to aggregate)">
          <Text value={opCol} onChange={setOpCol} list={relCols} />
        </Field>
      )}
      {(kind === "sum" || kind === "min" || kind === "max") && (
        <Field label="value_type">
          <Select value={(agg.value_type as string) ?? "integer"} onChange={(v) => set({ ...agg, value_type: v as FlussoType })} options={SCALAR_TYPES as string[]} />
        </Field>
      )}
      {kind === "ids" && typeof agg.op !== "string" && "ids" in agg.op && (
        <Field label="element_type">
          <Select value={agg.op.ids.element_type as string} onChange={(v) => set({ ...agg, op: { ids: { element_type: v as FlussoType } } })} options={SCALAR_TYPES as string[]} />
        </Field>
      )}
      <AggregateKeyEditor value={agg.key} onChange={(key) => set({ ...agg, key })} relCols={relCols} tableNames={tableNames} />
      <details>
        <summary>filters</summary>
        <Filters value={agg.filters ?? []} onChange={(filters) => set({ ...agg, filters })} columns={relCols} />
      </details>
    </>
  );
}

function aggregateColumn(op: AggregateOp): string | null {
  if (typeof op === "string") return null; // count
  if ("ids" in op) return null;
  if ("sum" in op) return op.sum;
  if ("avg" in op) return op.avg;
  if ("min" in op) return op.min;
  if ("max" in op) return op.max;
  return null;
}

function withColumn(kind: string, _op: AggregateOp, col: string): AggregateOp {
  switch (kind) {
    case "sum":
      return { sum: col };
    case "avg":
      return { avg: col };
    case "min":
      return { min: col };
    case "max":
      return { max: col };
    default:
      return _op;
  }
}

function AggregateKeyEditor({
  value,
  onChange,
  relCols,
  tableNames,
}: {
  value: AggregateKey;
  onChange: (k: AggregateKey) => void;
  relCols: string[];
  tableNames: string[];
}) {
  const isDirect = "direct" in value;
  return (
    <div className="key-editor">
      <Field label="key">
        <Select
          value={isDirect ? "direct" : "through"}
          onChange={(k) =>
            onChange(k === "direct" ? { direct: "" } : { through: { table: "", left_key: "", right_key: "" } })
          }
          options={["direct", "through"]}
        />
      </Field>
      {isDirect ? (
        <Field label="foreign_key">
          <Text value={value.direct} onChange={(c) => onChange({ direct: c })} list={relCols} />
        </Field>
      ) : (
        <ThroughEditor through={value.through} onChange={(through) => onChange({ through })} tableNames={tableNames} />
      )}
    </div>
  );
}

function ThroughEditor({
  through,
  onChange,
  tableNames,
}: {
  through: { table: string; left_key: string; right_key: string };
  onChange: (t: { table: string; left_key: string; right_key: string }) => void;
  tableNames: string[];
}) {
  return (
    <div className="through">
      <Field label="junction table">
        <Text value={through.table} onChange={(table) => onChange({ ...through, table })} list={tableNames} />
      </Field>
      <Field label="left_key">
        <Text value={through.left_key} onChange={(left_key) => onChange({ ...through, left_key })} />
      </Field>
      <Field label="right_key">
        <Text value={through.right_key} onChange={(right_key) => onChange({ ...through, right_key })} />
      </Field>
    </div>
  );
}

function OrderByEditor({ value, onChange, cols }: { value: OrderBy[]; onChange: (v: OrderBy[] | undefined) => void; cols: string[] }) {
  const set = (i: number, ob: OrderBy) => {
    const next = value.slice();
    next[i] = ob;
    onChange(next);
  };
  return (
    <div className="order-by">
      {value.map((ob, i) => (
        <div className="order-row" key={i}>
          <Text value={ob.column} onChange={(column) => set(i, { ...ob, column })} list={cols} placeholder="column" />
          <Select value={ob.direction ?? "asc"} onChange={(direction) => set(i, { ...ob, direction })} options={["asc", "desc"]} />
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

export function NestedFields({
  fields,
  onChange,
  ctx,
  table,
}: {
  fields: FieldModel[];
  onChange: (f: FieldModel[]) => void;
  ctx: CatalogCtx;
  table: string;
}) {
  return (
    <div className="fields">
      {fields.map((f, i) => (
        <FieldEditor
          key={i}
          field={f}
          ctx={ctx}
          table={table}
          onChange={(nf) => {
            const next = fields.slice();
            next[i] = nf;
            onChange(next);
          }}
          onRemove={() => onChange(fields.filter((_, j) => j !== i))}
        />
      ))}
      <button className="add" onClick={() => onChange([...fields, defaultField(`field${fields.length + 1}`, "keyword", table)])}>
        + add field
      </button>
    </div>
  );
}
