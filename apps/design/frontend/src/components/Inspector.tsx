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
import { KIND_HELP, LEAF_TYPES } from "../fields";
import { useT, type Translate } from "../i18n";
import * as edit from "../model/edit";
import { effectiveTable, fieldAtPath, joinOf, nodeFields, pathLabels } from "../model/tree";
import { useDesign } from "../state";
import { typeClass } from "../theme";

/// Scalar-type options for the field TYPE dropdown: each carries its colour
/// family (so the list is colour-coded like the canvas) and a one-line
/// description shown under the label. Literal `t("typeDesc.*")` calls keep the
/// i18n checker able to see every key.
function scalarTypeOptions(t: Translate) {
  const desc: Record<string, string> = {
    text: t("typeDesc.text"),
    identifier: t("typeDesc.identifier"),
    keyword: t("typeDesc.keyword"),
    enum: t("typeDesc.enum"),
    uuid: t("typeDesc.uuid"),
    boolean: t("typeDesc.boolean"),
    short: t("typeDesc.short"),
    integer: t("typeDesc.integer"),
    long: t("typeDesc.long"),
    float: t("typeDesc.float"),
    double: t("typeDesc.double"),
    decimal: t("typeDesc.decimal"),
    date: t("typeDesc.date"),
    timestamp: t("typeDesc.timestamp"),
    binary: t("typeDesc.binary"),
    json: t("typeDesc.json"),
  };
  return SCALAR_TYPES.map((ty) => {
    const name = ty as string;
    return { label: name, value: name, description: desc[name], className: `font-mono ${typeClass(name)}` };
  });
}

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
  return <div className="crumbs mb-2.5 break-words text-2xs text-muted-foreground">{crumbs.join(" › ")}</div>;
}
import { Button } from "@/components/ui/button";
import { Filters } from "./Filters";
import { Block, Bridge, Check, Drawer, Field as Row, GenericInput, Num, SectionTitle, Select, Text } from "./widgets";

/// snake_case / "spaced" → camelCase, the usual document-field convention.
const camel = (s: string) =>
  s.replace(/[_\s]+(.)/g, (_m, c: string) => c.toUpperCase()).replace(/^(.)/, (_m, c: string) => c.toLowerCase());
const pascal = (s: string) => {
  const c = camel(s);
  return c.charAt(0).toUpperCase() + c.slice(1);
};
/// Naive singular — good enough for table→element names (orders → order).
const singular = (s: string) =>
  s.endsWith("ies") ? `${s.slice(0, -3)}y` : s.endsWith("s") && !s.endsWith("ss") ? s.slice(0, -1) : s;

/// One-click name suggestions for the document field, by what the field draws
/// from: a column offers itself + its camelCase; a join its table singular; an
/// aggregate an op-flavoured name. The current name is filtered out by the row.
function nameSuggestions(field: Field): string[] {
  const s = field.source;
  if ("column" in s) {
    const col = s.column.column;
    return [col, camel(col)];
  }
  if ("geo" in s) return ["location", "coordinates", camel(s.geo.lat)];
  if ("constant" in s) return [];
  if ("relation" in s) {
    if ("join" in s.relation) {
      const tbl = s.relation.join.table;
      const k = s.relation.join.kind;
      const many = "has_many" in k || "many_to_many" in k;
      return [tbl, many ? camel(tbl) : camel(singular(tbl)), singular(tbl)];
    }
    const agg = s.relation.aggregate;
    const op =
      typeof agg.op === "string"
        ? "count"
        : "sum" in agg.op
          ? "sum"
          : "avg" in agg.op
            ? "avg"
            : "min" in agg.op
              ? "min"
              : "max" in agg.op
                ? "max"
                : "ids";
    const col =
      typeof agg.op === "string"
        ? ""
        : "sum" in agg.op
          ? agg.op.sum
          : "avg" in agg.op
            ? agg.op.avg
            : "min" in agg.op
              ? agg.op.min
              : "max" in agg.op
                ? agg.op.max
                : "";
    if (op === "count") return [`${camel(singular(agg.table))}Count`, `${camel(agg.table)}Count`];
    if (op === "ids") return [`${camel(singular(agg.table))}Ids`];
    return col ? [camel(col), `${camel(col)}${pascal(op)}`] : [];
  }
  return [];
}

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
      <SectionTitle className="mt-0">{t("inspector.indexRoot")}</SectionTitle>
      <Block variant="src" title={t("inspector.fromDb")}>
        <Row label={t("inspector.rootTable")}>
          <Text value={schema.table} list={tables} onChange={(table) => apply((s) => edit.setRootMeta(s, { table }))} />
        </Row>
        <Row label={t("inspector.schema")}>
          <Text
            value={schema.db_schema}
            onChange={(db_schema) => apply((s) => edit.setRootMeta(s, { db_schema }))}
            placeholder="public"
          />
        </Row>
        <Row label="primary_key">
          <Text
            value={schema.primary_key ?? ""}
            list={cols}
            onChange={(pk) => apply((s) => edit.setRootMeta(s, { primary_key: pk || undefined }))}
          />
        </Row>
      </Block>
      <SoftDeleteEditor
        value={schema.soft_delete}
        onChange={(soft_delete) => apply((s) => ({ ...s, soft_delete }))}
        cols={cols}
      />
      <Drawer title={t("inspector.rootFilters")} count={(schema.filters ?? []).length}>
        <Filters
          value={schema.filters ?? []}
          onChange={(filters) => apply((s) => ({ ...s, filters }))}
          columns={cols}
        />
      </Drawer>
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
        <SectionTitle className="mt-0">{t("inspector.objectGroup")}</SectionTitle>
        <div className="mb-3 flex gap-3.5">
          <Button variant="link" size="sm" onClick={duplicate}>
            {t("inspector.duplicate")}
          </Button>
          <Button variant="link" size="sm" className="text-destructive" onClick={remove}>
            {t("inspector.delete")}
          </Button>
        </div>
        <div className="no-source">⊘ {t("inspector.groupHint")}</div>
        <Block variant="doc" title={t("inspector.inDoc")}>
          <NameField field={field} set={setField} />
        </Block>
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
    ? catalog?.catalog.tables.find((tbl) => tbl.name === parentTable)?.columns.find((c) => c.name === btColumn)
        ?.nullable
    : undefined;

  return (
    <div className="inspector">
      <SectionTitle className="mt-0">
        {t("inspector.join")} · {verb}
      </SectionTitle>
      {KIND_HELP[verb] && <p className="kind-help">{t(KIND_HELP[verb])}</p>}
      <div className="mb-3 flex gap-3.5">
        <Button variant="link" size="sm" onClick={duplicate}>
          {t("inspector.duplicate")}
        </Button>
        <Button variant="link" size="sm" className="text-destructive" onClick={remove}>
          {t("inspector.delete")}
        </Button>
      </div>
      <Block variant="src" title={t("inspector.relationship")}>
        <Row label={t("inspector.verb")}>
          <Select
            value={verb}
            options={["belongs_to", "has_one", "has_many", "many_to_many"]}
            onChange={(v) => setJoin({ ...join, kind: blankKind(v) })}
          />
        </Row>
        <Row label={t("inspector.table")}>
          <Text value={join.table} list={tables} onChange={(table) => setJoin({ ...join, table })} />
        </Row>
        <Row label="primary_key">
          <Text value={join.primary_key} list={relCols} onChange={(primary_key) => setJoin({ ...join, primary_key })} />
        </Row>
        {"belongs_to" in join.kind && (
          <Row label={t("inspector.btColumn")}>
            <Text
              value={join.kind.belongs_to.column}
              onChange={(c) => setJoin({ ...join, kind: { belongs_to: { column: c } } })}
            />
          </Row>
        )}
        {"has_one" in join.kind && (
          <Row label={t("inspector.fkOnTarget")}>
            <Text
              value={join.kind.has_one.foreign_key}
              list={relCols}
              onChange={(c) => setJoin({ ...join, kind: { has_one: { foreign_key: c } } })}
            />
          </Row>
        )}
        {"has_many" in join.kind && (
          <Row label={t("inspector.fkOnTarget")}>
            <Text
              value={join.kind.has_many.foreign_key}
              list={relCols}
              onChange={(c) => setJoin({ ...join, kind: { has_many: { foreign_key: c } } })}
            />
          </Row>
        )}
        {"many_to_many" in join.kind && (
          <ThroughEditor
            through={join.kind.many_to_many.through}
            tables={tables}
            onChange={(through) => setJoin({ ...join, kind: { many_to_many: { through } } })}
          />
        )}
      </Block>
      {!toMany && fkNullable === true && <Bridge>{t("inspector.fkNullable", { col: btColumn ?? "" })}</Bridge>}
      {!toMany && fkNullable === false && <Bridge>{t("inspector.fkNotNull", { col: btColumn ?? "" })}</Bridge>}
      <Block variant="doc" title={t("inspector.inDoc")}>
        <NameField field={field} set={setField} />
        <div className="nested-note">{t("inspector.nestedNote")}</div>
        {!toMany && (
          <Check
            value={!join.nullable}
            label={t("inspector.required")}
            onChange={(req) => setJoin({ ...join, nullable: !req })}
          />
        )}
        {verb !== "belongs_to" && (
          <OrderByEditor
            value={join.order_by ?? []}
            cols={relCols}
            onChange={(order_by) => setJoin({ ...join, order_by })}
          />
        )}
        {toMany && (
          <Row label="limit">
            <Num value={join.limit} onChange={(limit) => setJoin({ ...join, limit })} />
          </Row>
        )}
      </Block>
      <Drawer title={t("inspector.filters")} count={(join.filters ?? []).length}>
        <Filters value={join.filters ?? []} columns={relCols} onChange={(filters) => setJoin({ ...join, filters })} />
      </Drawer>
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
      <SectionTitle className="mt-0">
        {t("inspector.field")} · {field.field}
      </SectionTitle>
      {KIND_HELP[helpKind] && <p className="kind-help">{t(KIND_HELP[helpKind])}</p>}
      <div className="mb-3 flex gap-3.5">
        <Button variant="link" size="sm" onClick={duplicate}>
          {t("inspector.duplicate")}
        </Button>
        <Button variant="link" size="sm" className="text-destructive" onClick={remove}>
          {t("inspector.delete")}
        </Button>
      </div>
      {"column" in s && typeof s.column.ty === "string" && (
        <ScalarBody
          field={field}
          column={s.column}
          srcNullable={srcNullable}
          suggested={srcCol?.suggested_type}
          sqlType={srcCol?.sql_type}
          set={set}
        />
      )}
      {"column" in s && typeof s.column.ty !== "string" && "map" in s.column.ty && (
        <MapBody field={field} column={s.column} cols={cols} srcNullable={srcNullable} set={set} />
      )}
      {"column" in s && typeof s.column.ty !== "string" && "custom" in s.column.ty && (
        <CustomBody field={field} column={s.column} cols={cols} srcNullable={srcNullable} set={set} />
      )}
      {"geo" in s && <GeoBody field={field} set={set} cols={cols} />}
      {"constant" in s && <ConstantBody field={field} set={set} value={s.constant} />}
      {"relation" in s && "aggregate" in s.relation && (
        <AggregateBody field={field} agg={s.relation.aggregate} tables={tables} set={set} />
      )}

      <OptionsEditor field={field} set={set} />
    </div>
  );
}

// --- leaf bodies ---

/// The document-field name plus one-click rename chips derived from the source
/// (see [`nameSuggestions`]). Goes first in every Document block.
function NameField({ field, set }: { field: Field; set: (f: Field) => void }) {
  const { t } = useT();
  const chips = nameSuggestions(field)
    .filter((n, i, a) => n && n !== field.field && a.indexOf(n) === i)
    .slice(0, 3);
  return (
    <div className="field mb-2 flex flex-col gap-1">
      <span className="field-label text-3xs font-semibold uppercase tracking-[0.05em] text-muted-foreground">
        {t("inspector.fieldName")}
      </span>
      <Text value={field.field} onChange={(name) => set({ ...field, field: name })} />
      {chips.length > 0 && (
        <div className="rename-chips mt-1.5 flex flex-wrap items-center gap-1.5">
          <span className="text-3xs uppercase tracking-[0.05em] text-muted-foreground">{t("inspector.renameTo")}</span>
          {chips.map((n) => (
            <button
              type="button"
              key={n}
              className="rchip cursor-pointer rounded-full border border-primary/30 bg-primary/15 px-2 py-px font-mono text-2xs text-primary hover:bg-primary/20"
              onClick={() => set({ ...field, field: n })}
            >
              {n}
            </button>
          ))}
        </div>
      )}
    </div>
  );
}

/// The source-imposed nullability rule, as a [`Bridge`] — nothing when the
/// column's nullability is unknown (offline / hand-typed).
function NullBridge({ srcNullable }: { srcNullable?: boolean }) {
  const { t } = useT();
  if (srcNullable === false) return <Bridge>{t("inspector.bridgeNotNull")}</Bridge>;
  if (srcNullable === true) return <Bridge>{t("inspector.bridgeNullable")}</Bridge>;
  return null;
}

// The column is fixed by the catalog checkbox on the node; the Document block
// edits only what's *about* the field — its type, transforms, required/default.
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
    if (on) next.add(tr);
    else next.delete(tr);
    setCol({ ...column, transforms: next.size ? [...next] : undefined });
  };
  // A soft nudge, not a rule: the SQL type only *suggests* a flusso type
  // (keyword vs text is a legitimate authoring choice), so this surfaces the
  // suggestion only when the current pick diverges from it.
  const showSuggestion = typeof suggested === "string" && suggested !== column.ty;
  return (
    <>
      <Block variant="src" title={t("inspector.fromDb")}>
        <SourceColumn name={column.column} sqlType={sqlType} suggested={suggested} srcNullable={srcNullable} />
      </Block>
      <NullBridge srcNullable={srcNullable} />
      <Block variant="doc" title={t("inspector.inDoc")}>
        <NameField field={field} set={set} />
        <Row label={t("inspector.type")}>
          <Select
            value={column.ty as string}
            options={scalarTypeOptions(t)}
            onChange={(ty) => setCol({ ...column, ty: ty as FlussoType })}
            className={`font-mono ${typeClass(column.ty as string)}`}
          />
        </Row>
        {showSuggestion && (
          <p className="nudge mt-1.5 text-2xs text-muted-foreground">
            <span className={`font-mono ${typeClass(suggested)}`}>{suggested}</span> {t("inspector.suggested")} ·{" "}
            <button
              type="button"
              className="cursor-pointer text-primary hover:underline"
              onClick={() => setCol({ ...column, ty: suggested })}
            >
              {t("inspector.use")}
            </button>
          </p>
        )}
        <div className="check-row my-1.5 flex flex-wrap gap-4">
          <Check value={has("lowercase")} label={t("inspector.lowercase")} onChange={(on) => toggle("lowercase", on)} />
          <Check value={has("trim")} label={t("inspector.trim")} onChange={(on) => toggle("trim", on)} />
        </div>
        <RequiredDefault column={column} srcNullable={srcNullable} setCol={setCol} />
      </Block>
    </>
  );
}

/// A compact, read-only line of facts about the bound source column — its
/// name, SQL type, and nullability — so the panel actually says what the field
/// draws from. Omits what it doesn't know (offline / hand-typed name).
function SourceColumn({
  name,
  sqlType,
  suggested,
  srcNullable,
}: {
  name: string;
  sqlType?: string;
  suggested?: FlussoType;
  srcNullable?: boolean;
}) {
  const { t } = useT();
  const tag = "rounded border border-border bg-secondary px-1.5 text-2xs leading-[1.125rem]";
  // Colour the SQL-type chip by the family its column maps to (the suggested
  // flusso type), so it reads the same hue as the type everywhere else.
  const sqlFamily = typeClass((suggested ?? sqlType ?? "") as string);
  return (
    <div className="src-col mb-2 flex flex-wrap items-center gap-1.5">
      <span className="font-mono text-xs text-foreground">{name}</span>
      {sqlType && <span className={`${tag} ${sqlFamily}`}>{sqlType}</span>}
      {srcNullable === false && (
        <span className={`${tag} border-primary/40 text-primary`}>{t("inspector.colNotNull")}</span>
      )}
      {srcNullable === true && <span className={`${tag} text-muted-foreground`}>{t("inspector.colNullable")}</span>}
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
function RequiredDefault({
  column,
  srcNullable,
  setCol,
}: {
  column: Column;
  srcNullable?: boolean;
  setCol: (c: Column) => void;
}) {
  const { t } = useT();
  const required = !column.nullable;
  const mustDefault = srcNullable === true && required;
  const defaultMissing = mustDefault && column.default === undefined;
  // A default only matters when it's mandatory (nullable column made required)
  // or one's already set — otherwise it's noise (NOT NULL always has a value;
  // an optional column just passes nulls through).
  const showDefault = mustDefault || column.default !== undefined;
  // Required is "from source" only when it matches a NOT NULL column's default.
  const fromSource = srcNullable === false && required;
  return (
    <>
      <div className="req-check my-2 flex items-center gap-2">
        <Check
          value={required}
          label={t("inspector.required")}
          onChange={(req) => setCol({ ...column, nullable: !req })}
        />
        {fromSource && <span className="text-2xs text-primary">🔒 {t("inspector.fromSource")}</span>}
      </div>
      {showDefault && (
        <>
          <Row label={mustDefault ? t("inspector.defaultRequired") : t("inspector.defaultOptional")}>
            <GenericInput
              invalid={defaultMissing}
              value={column.default}
              onChange={(def) => setCol({ ...column, default: def })}
              placeholder='e.g. 0 or "n/a"'
            />
          </Row>
          {mustDefault && (
            <p
              className={
                defaultMissing ? "error-hint mt-0.5 mb-1.5 text-xs text-destructive" : "text-2xs text-muted-foreground"
              }
            >
              {t("inspector.defaultError")}
            </p>
          )}
        </>
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
  // Quick-adds for the common OpenSearch mapping knobs; numeric ones seed a
  // number, the rest a string — both valid GenericValues, so adding never errors.
  const KNOBS: { key: string; seed: unknown }[] = [
    { key: "analyzer", seed: { String: "" } },
    { key: "search_analyzer", seed: { String: "" } },
    { key: "boost", seed: { Double: 1 } },
    { key: "null_value", seed: { String: "" } },
    { key: "copy_to", seed: { String: "" } },
    { key: "scaling_factor", seed: { Double: 100 } },
  ];
  const available = KNOBS.filter((k) => !(k.key in options));
  return (
    <Drawer title={t("inspector.advanced")} count={entries.length}>
      <p className="mb-2.5 text-2xs leading-relaxed text-muted-foreground">{t("inspector.optionsHelp")}</p>
      {available.length > 0 && (
        <div className="knobs mb-2.5 flex flex-wrap gap-1.5">
          {available.map((k) => (
            <button
              type="button"
              key={k.key}
              className="knob cursor-pointer rounded-full border border-border bg-secondary px-2 py-0.5 font-mono text-2xs text-slate before:text-muted-foreground before:content-['+_'] hover:border-slate hover:text-foreground"
              onClick={() => setOpt(k.key, k.seed)}
            >
              {k.key}
            </button>
          ))}
        </div>
      )}
      {entries.map(([k, v]) => (
        <div className="opt-row mb-1.5 grid grid-cols-[1fr_1fr_auto] items-center gap-1.5" key={k}>
          <Text value={k} onChange={(nk) => renameOpt(k, nk)} placeholder={t("inspector.optKey")} />
          <GenericInput
            value={v}
            emptyTo={{ String: "" }}
            onChange={(nv) => setOpt(k, nv ?? { String: "" })}
            placeholder={t("inspector.optValue")}
          />
          <button
            type="button"
            className="cursor-pointer p-0 text-xs text-destructive"
            onClick={() => removeOpt(k)}
            aria-label={t("common.remove")}
          >
            ✕
          </button>
        </div>
      ))}
      <button
        type="button"
        className="addline cursor-pointer pt-0.5 text-xs text-primary"
        onClick={() => setOpt(`option${entries.length + 1}`, { String: "" })}
      >
        + {t("inspector.option")}
      </button>
    </Drawer>
  );
}

function MapBody({
  field,
  column,
  cols,
  srcNullable,
  set,
}: {
  field: Field;
  column: Column;
  cols: string[];
  srcNullable?: boolean;
  set: (f: Field) => void;
}) {
  const { t } = useT();
  const ty = column.ty as { map: { values: FlussoType } };
  const setCol = (c: Column) => set({ ...field, source: { column: c } });
  return (
    <>
      <Block variant="src" title={t("inspector.fromDb")}>
        <Row label={t("inspector.columnJson")}>
          <Text value={column.column} list={cols} onChange={(c) => setCol({ ...column, column: c })} />
        </Row>
      </Block>
      <NullBridge srcNullable={srcNullable} />
      <Block variant="doc" title={t("inspector.inDoc")}>
        <NameField field={field} set={set} />
        <Row label={t("inspector.values")}>
          <Select
            value={ty.map.values as string}
            options={LEAF_TYPES as string[]}
            onChange={(v) => setCol({ ...column, ty: { map: { values: v as FlussoType } } })}
          />
        </Row>
        <RequiredDefault column={column} srcNullable={srcNullable} setCol={setCol} />
      </Block>
    </>
  );
}

function CustomBody({
  field,
  column,
  cols,
  srcNullable,
  set,
}: {
  field: Field;
  column: Column;
  cols: string[];
  srcNullable?: boolean;
  set: (f: Field) => void;
}) {
  const { t } = useT();
  const ty = column.ty as { custom: { postgres: string[]; opensearch: string } };
  const setCol = (c: Column) => set({ ...field, source: { column: c } });
  return (
    <>
      <Block variant="src" title={t("inspector.fromDb")}>
        <Row label={t("common.column")}>
          <Text value={column.column} list={cols} onChange={(c) => setCol({ ...column, column: c })} />
        </Row>
        <Row label={t("inspector.pgTypes")}>
          <Text
            value={ty.custom.postgres.join(", ")}
            onChange={(text) =>
              setCol({
                ...column,
                ty: {
                  custom: {
                    ...ty.custom,
                    postgres: text
                      .split(",")
                      .map((x) => x.trim())
                      .filter(Boolean),
                  },
                },
              })
            }
          />
        </Row>
      </Block>
      <NullBridge srcNullable={srcNullable} />
      <Block variant="doc" title={t("inspector.inDoc")}>
        <NameField field={field} set={set} />
        <Row label={t("inspector.osType")}>
          <Text
            value={ty.custom.opensearch}
            onChange={(o) => setCol({ ...column, ty: { custom: { ...ty.custom, opensearch: o } } })}
          />
        </Row>
        <RequiredDefault column={column} srcNullable={srcNullable} setCol={setCol} />
      </Block>
    </>
  );
}

function ConstantBody({ field, value, set }: { field: Field; value: unknown; set: (f: Field) => void }) {
  const { t } = useT();
  return (
    <>
      <div className="no-source">⊘ {t("inspector.noSource")}</div>
      <Block variant="doc" title={t("inspector.inDoc")}>
        <NameField field={field} set={set} />
        <Row label={t("inspector.valueJson")}>
          <GenericInput
            value={value}
            emptyTo="Null"
            onChange={(constant) => set({ ...field, source: { constant: constant ?? "Null" } })}
          />
        </Row>
      </Block>
    </>
  );
}

function GeoBody({ field, set, cols }: { field: Field; set: (f: Field) => void; cols: string[] }) {
  const { t } = useT();
  if (!("geo" in field.source)) return null;
  const geo = field.source.geo;
  return (
    <>
      <Block variant="src" title={t("inspector.fromDb")}>
        <Row label={t("inspector.latColumn")}>
          <Text value={geo.lat} list={cols} onChange={(lat) => set({ ...field, source: { geo: { ...geo, lat } } })} />
        </Row>
        <Row label={t("inspector.lonColumn")}>
          <Text value={geo.lon} list={cols} onChange={(lon) => set({ ...field, source: { geo: { ...geo, lon } } })} />
        </Row>
      </Block>
      <Bridge>{t("inspector.geoHint")}</Bridge>
      <Block variant="doc" title={t("inspector.inDoc")}>
        <NameField field={field} set={set} />
        <Check
          value={!geo.nullable}
          label={t("inspector.required")}
          onChange={(req) => set({ ...field, source: { geo: { ...geo, nullable: !req } } })}
        />
      </Block>
    </>
  );
}

function AggregateBody({
  field,
  agg,
  tables,
  set,
}: {
  field: Field;
  agg: Aggregate;
  tables: string[];
  set: (f: Field) => void;
}) {
  const { columnsFor } = useDesign();
  const { t } = useT();
  const setAgg = (a: Aggregate) => set({ ...field, source: { relation: { aggregate: a } } });
  const op = agg.op;
  const opCol =
    typeof op === "string"
      ? null
      : "sum" in op
        ? op.sum
        : "avg" in op
          ? op.avg
          : "min" in op
            ? op.min
            : "max" in op
              ? op.max
              : null;
  const kind =
    typeof op === "string"
      ? "count"
      : "sum" in op
        ? "sum"
        : "avg" in op
          ? "avg"
          : "min" in op
            ? "min"
            : "max" in op
              ? "max"
              : "ids";
  const aggCols = columnsFor(agg.table).map((c) => c.name);
  const hasMappingType = kind === "sum" || kind === "min" || kind === "max" || kind === "ids";
  return (
    <>
      <Block variant="src" title={t("inspector.aggFrom")}>
        <Row label={t("inspector.relatedTable")}>
          <Text value={agg.table} list={tables} onChange={(table) => setAgg({ ...agg, table })} />
        </Row>
        {opCol !== null && (
          <Row label={t("inspector.aggColumn")}>
            <Text value={opCol} list={aggCols} onChange={(c) => setAgg({ ...agg, op: withAggColumn(kind, c) })} />
          </Row>
        )}
        <AggregateKeyEditor value={agg.key} tables={tables} onChange={(key) => setAgg({ ...agg, key })} />
      </Block>
      <Block variant="doc" title={t("inspector.inDoc")}>
        <NameField field={field} set={set} />
        {(kind === "sum" || kind === "min" || kind === "max") && (
          <Row label="value_type">
            <Select
              value={(agg.value_type as string) ?? "integer"}
              options={SCALAR_TYPES as string[]}
              onChange={(v) => setAgg({ ...agg, value_type: v as FlussoType })}
            />
          </Row>
        )}
        {kind === "ids" && typeof op !== "string" && "ids" in op && (
          <Row label="element_type">
            <Select
              value={op.ids.element_type as string}
              options={SCALAR_TYPES as string[]}
              onChange={(v) => setAgg({ ...agg, op: { ids: { element_type: v as FlussoType } } })}
            />
          </Row>
        )}
        {!hasMappingType && <p className="hint">{t("inspector.countResult")}</p>}
      </Block>
      <Drawer title={t("inspector.filters")} count={(agg.filters ?? []).length}>
        <Filters value={agg.filters ?? []} onChange={(filters) => setAgg({ ...agg, filters })} />
      </Drawer>
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

function AggregateKeyEditor({
  value,
  tables,
  onChange,
}: {
  value: AggregateKey;
  tables: string[];
  onChange: (k: AggregateKey) => void;
}) {
  const { t } = useT();
  const direct = "direct" in value;
  return (
    <div className="my-1.5 flex flex-wrap items-end gap-2">
      <Row label={t("inspector.optKey")}>
        <Select
          value={direct ? "direct" : "through"}
          options={["direct", "through"]}
          onChange={(k) =>
            onChange(k === "direct" ? { direct: "" } : { through: { table: "", left_key: "", right_key: "" } })
          }
        />
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

function ThroughEditor({
  through,
  tables,
  onChange,
}: {
  through: { table: string; left_key: string; right_key: string };
  tables: string[];
  onChange: (t: { table: string; left_key: string; right_key: string }) => void;
}) {
  const { t } = useT();
  return (
    <div className="my-1.5 flex flex-wrap items-end gap-2">
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

function OrderByEditor({
  value,
  cols,
  onChange,
}: {
  value: { column: string; direction?: "asc" | "desc" }[];
  cols: string[];
  onChange: (v: { column: string; direction?: "asc" | "desc" }[] | undefined) => void;
}) {
  const { t } = useT();
  const set = (i: number, ob: { column: string; direction?: "asc" | "desc" }) => {
    const next = value.slice();
    next[i] = ob;
    onChange(next);
  };
  return (
    <div className="my-1.5">
      <div className="mb-1 text-3xs font-semibold uppercase tracking-[0.05em] text-muted-foreground">order_by</div>
      {value.map((ob, i) => (
        <div className="my-1 flex items-center gap-1.5" key={i}>
          <Text
            value={ob.column}
            list={cols}
            onChange={(column) => set(i, { ...ob, column })}
            placeholder={t("common.column")}
            className="min-w-0 flex-1"
          />
          <Select
            value={ob.direction ?? "asc"}
            options={["asc", "desc"]}
            onChange={(direction) => set(i, { ...ob, direction })}
            className="w-24"
          />
          <Button
            variant="link"
            size="sm"
            className="text-destructive"
            onClick={() =>
              onChange(value.filter((_, j) => j !== i).length ? value.filter((_, j) => j !== i) : undefined)
            }
          >
            ✕
          </Button>
        </div>
      ))}
      <Button variant="link" size="sm" onClick={() => onChange([...value, { column: "", direction: "asc" }])}>
        + order_by
      </Button>
    </div>
  );
}

function SoftDeleteEditor({
  value,
  onChange,
  cols,
}: {
  value: SoftDelete | undefined;
  onChange: (v: SoftDelete | undefined) => void;
  cols: string[];
}) {
  const { t } = useT();
  const kind = value === undefined ? "none" : "field" in value ? "field" : "column";
  return (
    <div className="my-1.5 flex flex-wrap items-end gap-2">
      <Row label={t("inspector.softDelete")}>
        <Select
          value={kind}
          options={["none", "field", "column"]}
          onChange={(k) =>
            onChange(k === "none" ? undefined : k === "field" ? { field: { field: "" } } : { column: { column: "" } })
          }
        />
      </Row>
      {value && "field" in value && (
        <Text
          value={value.field.field}
          onChange={(f) => onChange({ field: { ...value.field, field: f } })}
          placeholder={t("inspector.documentField")}
        />
      )}
      {value && "column" in value && (
        <Text
          value={value.column.column}
          list={cols}
          onChange={(c) => onChange({ column: { ...value.column, column: c } })}
          placeholder={t("common.column")}
        />
      )}
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
