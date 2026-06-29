import {
  SCALAR_TYPES,
  type Aggregate,
  type AggregateKey,
  type Column,
  type Field,
  type FieldSource,
  type FlussoType,
  type Join,
  type JoinKind,
  type SoftDelete,
} from "../api";
import { LEAF_TYPES } from "../fields";
import { useT } from "../i18n";
import * as edit from "../model/edit";
import { effectiveTable, fieldAtPath, joinOf, nodeFields, pathLabels } from "../model/tree";
import { useDesign } from "../state";

/// i18n key of the one-line grammar explanation per field/join kind, shown for
/// the selected node/field (resolved through `t(...)`).
const KIND_HELP: Record<string, string> = {
  belongs_to: "kindHelp.belongs_to",
  has_one: "kindHelp.has_one",
  has_many: "kindHelp.has_many",
  many_to_many: "kindHelp.many_to_many",
  object: "kindHelp.object",
  count: "kindHelp.count",
  sum: "kindHelp.sum",
  avg: "kindHelp.avg",
  min: "kindHelp.min",
  max: "kindHelp.max",
  ids: "kindHelp.ids",
  geo: "kindHelp.geo",
  map: "kindHelp.map",
  custom: "kindHelp.custom",
  constant: "kindHelp.constant",
};

function Breadcrumb() {
  const { schema, selection } = useDesign();
  if (!selection) return null;
  const root = schema.table || "(root)";
  let crumbs: string[];
  if (selection.kind === "root") crumbs = [root];
  else if (selection.kind === "node") crumbs = [root, ...pathLabels(schema, selection.path)];
  else {
    const field = nodeFields(schema, selection.path)[selection.index];
    crumbs = [root, ...pathLabels(schema, selection.path), field?.field ?? "?"];
  }
  return <div className="crumbs">{crumbs.join(" › ")}</div>;
}
import { Filters } from "./Filters";
import { Check, Field as Row, Num, Select, Text } from "./widgets";

export function Inspector() {
  const { selection } = useDesign();
  const { t } = useT();
  if (!selection) return <div className="inspector empty">{t("inspector.selectPrompt")}</div>;
  return (
    <>
      <Breadcrumb />
      {selection.kind === "root" ? (
        <RootInspector />
      ) : selection.kind === "node" ? (
        <NodeInspector path={selection.path} />
      ) : (
        <FieldInspector path={selection.path} index={selection.index} />
      )}
    </>
  );
}

function RootInspector() {
  const { schema, apply, catalog } = useDesign();
  const { t } = useT();
  const tables = catalog?.catalog.tables.map((tbl) => tbl.name) ?? [];
  const cols = catalog?.catalog.tables.find((tbl) => tbl.name === schema.table)?.columns.map((c) => c.name) ?? [];
  return (
    <div className="inspector">
      <h3>{t("inspector.indexRoot")}</h3>
      <Row label={t("inspector.rootTable")}>
        <Text value={schema.table} list={tables} onChange={(table) => apply((s) => edit.setRootMeta(s, { table }))} />
      </Row>
      <Row label={t("inspector.schema")}>
        <Text value={schema.db_schema} onChange={(db_schema) => apply((s) => edit.setRootMeta(s, { db_schema }))} placeholder="public" />
      </Row>
      <Row label="primary_key">
        <Text value={schema.primary_key ?? ""} list={cols} onChange={(pk) => apply((s) => edit.setRootMeta(s, { primary_key: pk || undefined }))} />
      </Row>
      <SoftDeleteEditor value={schema.soft_delete} onChange={(soft_delete) => apply((s) => ({ ...s, soft_delete }))} cols={cols} />
      <details>
        <summary>{t("inspector.rootFilters")}</summary>
        <Filters value={schema.filters ?? []} onChange={(filters) => apply((s) => ({ ...s, filters }))} columns={cols} />
      </details>
    </div>
  );
}

function NodeInspector({ path }: { path: number[] }) {
  const { schema, apply, catalog, select } = useDesign();
  const { t } = useT();
  const duplicate = () => {
    apply((s) => edit.duplicateNode(s, path));
    select({ kind: "node", path: [...path.slice(0, -1), path[path.length - 1] + 1] });
  };
  const remove = () => {
    apply((s) => edit.removeNode(s, path));
    select(null);
  };
  const field = fieldAtPath(schema, path);
  if (!field) return null;
  const setField = (f: Field) => apply((s) => edit.setNode(s, path, f));

  if ("group" in field.source) {
    return (
      <div className="inspector">
        <h3>{t("inspector.objectGroup")}</h3>
        <div className="inspector-actions">
          <button onClick={duplicate}>{t("inspector.duplicate")}</button>
          <button className="link danger" onClick={remove}>
            {t("inspector.delete")}
          </button>
        </div>
        <Row label={t("inspector.fieldName")}>
          <Text value={field.field} onChange={(name) => setField({ ...field, field: name })} />
        </Row>
        <p className="hint">{t("inspector.groupHint")}</p>
      </div>
    );
  }

  const join = joinOf(field);
  if (!join) return null;
  const verb = joinVerb(join.kind);
  const tables = catalog?.catalog.tables.map((tbl) => tbl.name) ?? [];
  const relCols = catalog?.catalog.tables.find((tbl) => tbl.name === join.table)?.columns.map((c) => c.name) ?? [];
  const setJoin = (j: Join) => setField({ ...field, source: { relation: { join: j } } });
  const toMany = verb === "has_many" || verb === "many_to_many";

  // A belongs_to's optionality is owned by its FK column (on the parent table):
  // a nullable FK means the target may be absent. Surface it so the required
  // toggle is a guided choice, not a guess. Undefined when the column isn't in
  // the catalog (offline, or hand-typed).
  const btColumn = "belongs_to" in join.kind ? join.kind.belongs_to.column : undefined;
  const parentTable = effectiveTable(schema, path.slice(0, -1));
  const fkNullable = btColumn
    ? catalog?.catalog.tables.find((tbl) => tbl.name === parentTable)?.columns.find((c) => c.name === btColumn)?.nullable
    : undefined;

  return (
    <div className="inspector">
      <h3>{t("inspector.join")} · {verb}</h3>
      {KIND_HELP[verb] && <p className="kind-help">{t(KIND_HELP[verb])}</p>}
      <div className="inspector-actions">
        <button onClick={duplicate}>{t("inspector.duplicate")}</button>
        <button className="link danger" onClick={remove}>
          {t("inspector.delete")}
        </button>
      </div>
      <Row label={t("inspector.fieldName")}>
        <Text value={field.field} onChange={(name) => setField({ ...field, field: name })} />
      </Row>
      <Row label={t("inspector.verb")}>
        <Select value={verb} options={["belongs_to", "has_one", "has_many", "many_to_many"]} onChange={(v) => setJoin({ ...join, kind: blankKind(v) })} />
      </Row>
      <Row label={t("inspector.table")}>
        <Text value={join.table} list={tables} onChange={(table) => setJoin({ ...join, table })} />
      </Row>
      <Row label="primary_key">
        <Text value={join.primary_key} list={relCols} onChange={(primary_key) => setJoin({ ...join, primary_key })} />
      </Row>
      {"belongs_to" in join.kind && (
        <Row label={t("inspector.btColumn")}>
          <Text value={join.kind.belongs_to.column} onChange={(c) => setJoin({ ...join, kind: { belongs_to: { column: c } } })} />
        </Row>
      )}
      {"has_one" in join.kind && (
        <Row label={t("inspector.fkOnTarget")}>
          <Text value={join.kind.has_one.foreign_key} list={relCols} onChange={(c) => setJoin({ ...join, kind: { has_one: { foreign_key: c } } })} />
        </Row>
      )}
      {"has_many" in join.kind && (
        <Row label={t("inspector.fkOnTarget")}>
          <Text value={join.kind.has_many.foreign_key} list={relCols} onChange={(c) => setJoin({ ...join, kind: { has_many: { foreign_key: c } } })} />
        </Row>
      )}
      {"many_to_many" in join.kind && (
        <ThroughEditor through={join.kind.many_to_many.through} tables={tables} onChange={(through) => setJoin({ ...join, kind: { many_to_many: { through } } })} />
      )}
      {!toMany && (
        <>
          {fkNullable === true && (
            <p className="hint">{t("inspector.fkNullable", { col: btColumn ?? "" })}</p>
          )}
          {fkNullable === false && (
            <p className="hint">{t("inspector.fkNotNull", { col: btColumn ?? "" })}</p>
          )}
          <Check value={!join.nullable} label={t("inspector.required")} onChange={(req) => setJoin({ ...join, nullable: !req })} />
        </>
      )}
      {verb !== "belongs_to" && <OrderByEditor value={join.order_by ?? []} cols={relCols} onChange={(order_by) => setJoin({ ...join, order_by })} />}
      {toMany && (
        <Row label="limit">
          <Num value={join.limit} onChange={(limit) => setJoin({ ...join, limit })} />
        </Row>
      )}
      <details>
        <summary>{t("inspector.filters")}</summary>
        <Filters value={join.filters ?? []} columns={relCols} onChange={(filters) => setJoin({ ...join, filters })} />
      </details>
    </div>
  );
}

function FieldInspector({ path, index }: { path: number[]; index: number }) {
  const { schema, apply, catalog, select } = useDesign();
  const { t } = useT();
  const duplicate = () => {
    apply((s) => edit.duplicateAt(s, path, index));
    select({ kind: "field", path, index: index + 1 });
  };
  const remove = () => {
    apply((s) => edit.removeAt(s, path, index));
    select(null);
  };
  const field = nodeFields(schema, path)[index];
  if (!field) return null;
  const table = effectiveTable(schema, path);
  const cols = catalog?.catalog.tables.find((tbl) => tbl.name === table)?.columns.map((c) => c.name) ?? [];
  const tables = catalog?.catalog.tables.map((tbl) => tbl.name) ?? [];
  const set = (f: Field) => apply((s) => edit.setLeaf(s, path, index, f));
  const s = field.source;

  // The bound source column, when known — drives the source-guided required/
  // default rule (its nullability) and the type suggestion. Undefined when the
  // column isn't in the catalog (offline, or a hand-typed name).
  const boundColumn = "column" in s ? s.column.column : undefined;
  const srcCol = boundColumn
    ? catalog?.catalog.tables.find((tbl) => tbl.name === table)?.columns.find((c) => c.name === boundColumn)
    : undefined;
  const srcNullable = srcCol?.nullable;

  const helpKind = fieldHelpKind(s);
  return (
    <div className="inspector">
      <h3>{t("inspector.field")} · {field.field}</h3>
      {KIND_HELP[helpKind] && <p className="kind-help">{t(KIND_HELP[helpKind])}</p>}
      <div className="inspector-actions">
        <button onClick={duplicate}>{t("inspector.duplicate")}</button>
        <button className="link danger" onClick={remove}>
          {t("inspector.delete")}
        </button>
      </div>
      <Row label={t("inspector.fieldName")}>
        <Text value={field.field} onChange={(name) => set({ ...field, field: name })} />
      </Row>

      {"column" in s && typeof s.column.ty === "string" && (
        <ScalarBody field={field} column={s.column} srcNullable={srcNullable} suggested={srcCol?.suggested_type} sqlType={srcCol?.sql_type} set={set} />
      )}
      {"column" in s && typeof s.column.ty !== "string" && "map" in s.column.ty && <MapBody field={field} column={s.column} cols={cols} srcNullable={srcNullable} set={set} />}
      {"column" in s && typeof s.column.ty !== "string" && "custom" in s.column.ty && <CustomBody field={field} column={s.column} cols={cols} srcNullable={srcNullable} set={set} />}
      {"geo" in s && <GeoBody field={field} set={set} cols={cols} />}
      {"constant" in s && (
        <Row label={t("inspector.valueJson")}>
          <Text
            value={JSON.stringify(s.constant)}
            onChange={(text) => {
              try {
                set({ ...field, source: { constant: JSON.parse(text) } });
              } catch {
                /* keep typing */
              }
            }}
          />
        </Row>
      )}
      {"relation" in s && "aggregate" in s.relation && <AggregateBody field={field} agg={s.relation.aggregate} tables={tables} set={set} />}

      <OptionsEditor field={field} set={set} />
    </div>
  );
}

// --- leaf bodies ---

// The column is fixed by the catalog checkbox on the node; the inspector edits
// only what's *about* the field — its type, nullability, transforms, default.
// (The document field name is renamed in the header above.)
function ScalarBody({
  field,
  column,
  srcNullable,
  suggested,
  sqlType,
  set,
}: {
  field: Field;
  column: Column;
  srcNullable?: boolean;
  suggested?: FlussoType;
  sqlType?: string;
  set: (f: Field) => void;
}) {
  const { t } = useT();
  const setCol = (c: Column) => set({ ...field, source: { column: c } });
  const has = (tr: "lowercase" | "trim") => (column.transforms ?? []).includes(tr);
  const toggle = (tr: "lowercase" | "trim", on: boolean) => {
    const next = new Set(column.transforms ?? []);
    on ? next.add(tr) : next.delete(tr);
    setCol({ ...column, transforms: next.size ? [...next] : undefined });
  };
  // A soft nudge, not a rule: the SQL type only *suggests* a flusso type
  // (keyword vs text is a legitimate authoring choice), so this surfaces the
  // suggestion only when the current pick diverges from it.
  const showSuggestion = typeof suggested === "string" && suggested !== column.ty;
  return (
    <>
      <SourceColumn name={column.column} sqlType={sqlType} srcNullable={srcNullable} />
      <Row label={t("inspector.type")}>
        <Select value={column.ty as string} options={SCALAR_TYPES as string[]} onChange={(ty) => setCol({ ...column, ty: ty as FlussoType })} />
      </Row>
      {showSuggestion && (
        <p className="hint">
          {t("inspector.suggestType", { sql: sqlType ?? "", ty: suggested })}{" "}
          <button type="button" className="link" onClick={() => setCol({ ...column, ty: suggested })}>
            {t("inspector.use")}
          </button>
        </p>
      )}
      <Check value={has("lowercase")} label={t("inspector.lowercase")} onChange={(on) => toggle("lowercase", on)} />
      <Check value={has("trim")} label={t("inspector.trim")} onChange={(on) => toggle("trim", on)} />
      <RequiredDefault column={column} srcNullable={srcNullable} setCol={setCol} />
    </>
  );
}

/// A compact, read-only line of facts about the bound source column — its
/// name, SQL type, and nullability — so the panel actually says what the field
/// draws from. Omits what it doesn't know (offline / hand-typed name).
function SourceColumn({ name, sqlType, srcNullable }: { name: string; sqlType?: string; srcNullable?: boolean }) {
  const { t } = useT();
  return (
    <div className="src-col">
      <span className="src-col-name">{name}</span>
      {sqlType && <span className="src-col-tag">{sqlType}</span>}
      {srcNullable === false && <span className="src-col-tag notnull">{t("inspector.colNotNull")}</span>}
      {srcNullable === true && <span className="src-col-tag">{t("inspector.colNullable")}</span>}
    </div>
  );
}

/// The source-guided **required** + **default** pair. A column's nullability is
/// determined by the database, so this constrains the choice rather than leaving
/// it free:
/// - a **NOT NULL** source column is required by default; you may relax it to
///   optional, and need no default;
/// - a **nullable** source column is optional by default; you may mark it
///   required, but then a `default` is mandatory (else the document field could
///   be missing). The default input is flagged invalid until one is set.
///
/// When the column isn't in the catalog (offline, or a hand-typed name) the
/// source nullability is unknown and both stay freely editable.
function RequiredDefault({ column, srcNullable, setCol }: { column: Column; srcNullable?: boolean; setCol: (c: Column) => void }) {
  const { t } = useT();
  const required = !column.nullable;
  const mustDefault = srcNullable === true && required;
  const defaultMissing = mustDefault && column.default === undefined;
  return (
    <>
      {srcNullable === false && (
        <p className="hint">{t("inspector.srcNotNull")}</p>
      )}
      {srcNullable === true && (
        <p className="hint">{t("inspector.srcNullable")}</p>
      )}
      <Check value={required} label={t("inspector.required")} onChange={(req) => setCol({ ...column, nullable: !req })} />
      {(srcNullable !== false || column.default !== undefined) && (
        <Row label={mustDefault ? t("inspector.defaultRequired") : t("inspector.defaultOptional")}>
          <Text
            invalid={defaultMissing}
            value={column.default === undefined ? "" : JSON.stringify(column.default)}
            onChange={(text) => {
              if (!text.trim()) {
                setCol({ ...column, default: undefined });
                return;
              }
              try {
                setCol({ ...column, default: JSON.parse(text) });
              } catch {
                /* keep typing until valid JSON */
              }
            }}
            placeholder='e.g. 0 or "n/a"'
          />
        </Row>
      )}
      {defaultMissing && (
        <p className="error-hint">{t("inspector.defaultError")}</p>
      )}
    </>
  );
}

/// Edit a field's arbitrary `options` (analyzer, boost, OpenSearch mapping
/// knobs…) as key → JSON-value rows.
function OptionsEditor({ field, set }: { field: Field; set: (f: Field) => void }) {
  const { t } = useT();
  const options = field.options ?? {};
  const entries = Object.entries(options);
  const setOpt = (key: string, value: unknown) => set({ ...field, options: { ...options, [key]: value } });
  const renameOpt = (oldKey: string, newKey: string) => {
    const next: Record<string, unknown> = {};
    for (const [k, v] of Object.entries(options)) next[k === oldKey ? newKey : k] = v;
    set({ ...field, options: next });
  };
  const removeOpt = (key: string) => {
    const next = { ...options };
    delete next[key];
    set({ ...field, options: Object.keys(next).length ? next : undefined });
  };
  return (
    <details>
      <summary>{t("inspector.options", { n: entries.length })}</summary>
      {entries.map(([k, v]) => (
        <div className="opt-row" key={k}>
          <Text value={k} onChange={(nk) => renameOpt(k, nk)} placeholder={t("inspector.optKey")} />
          <Text
            value={JSON.stringify(v)}
            onChange={(text) => {
              try {
                setOpt(k, JSON.parse(text));
              } catch {
                /* keep typing */
              }
            }}
            placeholder={t("inspector.optValue")}
          />
          <button className="link danger" onClick={() => removeOpt(k)}>
            ✕
          </button>
        </div>
      ))}
      <button className="link" onClick={() => setOpt(`option${entries.length + 1}`, "")}>
        + {t("inspector.option")}
      </button>
    </details>
  );
}

function MapBody({ field, column, cols, srcNullable, set }: { field: Field; column: Column; cols: string[]; srcNullable?: boolean; set: (f: Field) => void }) {
  const { t } = useT();
  const ty = column.ty as { map: { values: FlussoType } };
  const setCol = (c: Column) => set({ ...field, source: { column: c } });
  return (
    <>
      <Row label={t("inspector.values")}>
        <Select value={ty.map.values as string} options={LEAF_TYPES as string[]} onChange={(v) => setCol({ ...column, ty: { map: { values: v as FlussoType } } })} />
      </Row>
      <Row label={t("inspector.columnJson")}>
        <Text value={column.column} list={cols} onChange={(c) => setCol({ ...column, column: c })} />
      </Row>
      <RequiredDefault column={column} srcNullable={srcNullable} setCol={setCol} />
    </>
  );
}

function CustomBody({ field, column, cols, srcNullable, set }: { field: Field; column: Column; cols: string[]; srcNullable?: boolean; set: (f: Field) => void }) {
  const { t } = useT();
  const ty = column.ty as { custom: { postgres: string[]; opensearch: string } };
  const setCol = (c: Column) => set({ ...field, source: { column: c } });
  return (
    <>
      <Row label={t("inspector.pgTypes")}>
        <Text value={ty.custom.postgres.join(", ")} onChange={(text) => setCol({ ...column, ty: { custom: { ...ty.custom, postgres: text.split(",").map((x) => x.trim()).filter(Boolean) } } })} />
      </Row>
      <Row label={t("inspector.osType")}>
        <Text value={ty.custom.opensearch} onChange={(o) => setCol({ ...column, ty: { custom: { ...ty.custom, opensearch: o } } })} />
      </Row>
      <Row label={t("common.column")}>
        <Text value={column.column} list={cols} onChange={(c) => setCol({ ...column, column: c })} />
      </Row>
      <RequiredDefault column={column} srcNullable={srcNullable} setCol={setCol} />
    </>
  );
}

function GeoBody({ field, set, cols }: { field: Field; set: (f: Field) => void; cols: string[] }) {
  const { t } = useT();
  if (!("geo" in field.source)) return null;
  const geo = field.source.geo;
  return (
    <>
      <Row label={t("inspector.latColumn")}>
        <Text value={geo.lat} list={cols} onChange={(lat) => set({ ...field, source: { geo: { ...geo, lat } } })} />
      </Row>
      <Row label={t("inspector.lonColumn")}>
        <Text value={geo.lon} list={cols} onChange={(lon) => set({ ...field, source: { geo: { ...geo, lon } } })} />
      </Row>
      <Check value={!geo.nullable} label={t("inspector.required")} onChange={(req) => set({ ...field, source: { geo: { ...geo, nullable: !req } } })} />
      <p className="hint">{t("inspector.geoHint")}</p>
    </>
  );
}

function AggregateBody({ field, agg, tables, set }: { field: Field; agg: Aggregate; tables: string[]; set: (f: Field) => void }) {
  const { columnsFor } = useDesign();
  const { t } = useT();
  const setAgg = (a: Aggregate) => set({ ...field, source: { relation: { aggregate: a } } });
  const op = agg.op;
  const opCol = typeof op === "string" ? null : "sum" in op ? op.sum : "avg" in op ? op.avg : "min" in op ? op.min : "max" in op ? op.max : null;
  const kind = typeof op === "string" ? "count" : "sum" in op ? "sum" : "avg" in op ? "avg" : "min" in op ? "min" : "max" in op ? "max" : "ids";
  const aggCols = columnsFor(agg.table).map((c) => c.name);
  return (
    <>
      <Row label={t("inspector.relatedTable")}>
        <Text value={agg.table} list={tables} onChange={(table) => setAgg({ ...agg, table })} />
      </Row>
      {opCol !== null && (
        <Row label={t("inspector.aggColumn")}>
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
        <summary>{t("inspector.filters")}</summary>
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
  const { t } = useT();
  const direct = "direct" in value;
  return (
    <div className="key-editor">
      <Row label={t("inspector.optKey")}>
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
  const { t } = useT();
  return (
    <div className="through">
      <Row label={t("inspector.junctionTable")}>
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
  const { t } = useT();
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
          <Text value={ob.column} list={cols} onChange={(column) => set(i, { ...ob, column })} placeholder={t("common.column")} />
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
  const { t } = useT();
  const kind = value === undefined ? "none" : "field" in value ? "field" : "column";
  return (
    <div className="soft-delete">
      <Row label={t("inspector.softDelete")}>
        <Select value={kind} options={["none", "field", "column"]} onChange={(k) => onChange(k === "none" ? undefined : k === "field" ? { field: "" } : { column: "" })} />
      </Row>
      {value && "field" in value && <Text value={value.field} onChange={(f) => onChange({ ...value, field: f })} placeholder={t("inspector.documentField")} />}
      {value && "column" in value && <Text value={value.column} list={cols} onChange={(c) => onChange({ ...value, column: c })} placeholder={t("common.column")} />}
    </div>
  );
}

function joinVerb(kind: JoinKind): "belongs_to" | "has_one" | "has_many" | "many_to_many" {
  if ("belongs_to" in kind) return "belongs_to";
  if ("has_one" in kind) return "has_one";
  if ("has_many" in kind) return "has_many";
  return "many_to_many";
}

/// The KIND_HELP key for a leaf field's source (empty for plain scalars).
function fieldHelpKind(s: FieldSource): string {
  if ("geo" in s) return "geo";
  if ("constant" in s) return "constant";
  if ("column" in s && typeof s.column.ty !== "string") return "map" in s.column.ty ? "map" : "custom";
  if ("relation" in s && "aggregate" in s.relation) {
    const op = s.relation.aggregate.op;
    if (typeof op === "string") return "count";
    return "sum" in op ? "sum" : "avg" in op ? "avg" : "min" in op ? "min" : "max" in op ? "max" : "ids";
  }
  return "";
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
