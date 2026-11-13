// Helpers for editing the field model: classifying a field into the UI "kind"
// it should be edited as, and producing a fresh default field for a chosen kind
// while preserving the field name. The kind is the union of every authorable
// shape — scalar types, map/custom/geo/object, the four join verbs, the seven
// aggregate ops, and constant.

import type { Aggregate, AggregateOp, Column, Field, FieldSource, FlussoType, Join, JoinKind } from "./api";
import { SCALAR_TYPES } from "./api";

export const JOIN_KINDS = ["belongs_to", "has_one", "has_many", "many_to_many"] as const;

/// i18n key of the one-line explanation per field/join/aggregate kind — shown
/// in the inspector and as the description under each kind in the add menus.
export const KIND_HELP: Record<string, string> = {
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
  exists: "kindHelp.exists",
  geo: "kindHelp.geo",
  map: "kindHelp.map",
  custom: "kindHelp.custom",
  constant: "kindHelp.constant",
  script: "kindHelp.script",
};
export const AGGREGATE_OPS = ["count", "exists", "sum", "avg", "min", "max", "ids"] as const;
export const LEAF_TYPES: FlussoType[] = ["text", "keyword", "integer", "long", "double", "date"];

export type FieldKind = string;

/// Every kind the kind-picker offers, grouped for the dropdown.
export const KIND_GROUPS: { label: string; kinds: FieldKind[] }[] = [
  { label: "Scalar", kinds: SCALAR_TYPES as string[] },
  { label: "Structured", kinds: ["map", "custom", "geo", "object", "constant", "script"] },
  { label: "Join", kinds: [...JOIN_KINDS] },
  { label: "Aggregate", kinds: [...AGGREGATE_OPS] },
];

const TO_MANY = new Set(["has_many", "many_to_many"]);

export function isToMany(kind: FieldKind): boolean {
  return TO_MANY.has(kind);
}

/// Classify a field by its source into the UI kind it edits as.
export function fieldKind(field: Field): FieldKind {
  const s = field.source;
  if ("column" in s) {
    const ty = s.column.ty;
    if (typeof ty === "string") return ty;
    if ("map" in ty) return "map";
    if ("custom" in ty) return "custom";
  }
  if ("geo" in s) return "geo";
  if ("group" in s) return "object";
  if ("constant" in s) return "constant";
  if ("script" in s) return "script";
  if ("relation" in s) {
    const r = s.relation;
    if ("join" in r) return joinVerb(r.join.kind);
    if ("aggregate" in r) return aggOp(r.aggregate.op);
  }
  return "keyword";
}

function joinVerb(kind: JoinKind): FieldKind {
  if ("belongs_to" in kind) return "belongs_to";
  if ("has_one" in kind) return "has_one";
  if ("has_many" in kind) return "has_many";
  return "many_to_many";
}

function aggOp(op: AggregateOp): FieldKind {
  if (op === "count") return "count";
  if (op === "exists") return "exists";
  if ("sum" in op) return "sum";
  if ("avg" in op) return "avg";
  if ("min" in op) return "min";
  if ("max" in op) return "max";
  return "ids";
}

/// The options the parser adds to a column of type `ty` when the YAML leaves
/// them out: an `identifier`'s analyzer, a `map`'s `dynamic: true`. A field the
/// canvas builds carries them too, so it equals what its YAML reads back as.
export function typeOptions(ty: Column["ty"]): Record<string, unknown> {
  if (ty === "identifier") return { analyzer: { String: "flusso_code" } };
  if (typeof ty === "object" && "map" in ty) return { dynamic: { Bool: true } };
  return {};
}

/// `options` moved from type `from` to type `to`: the options the parser added
/// for `from` (still at their added value, and not in `kept`) go, the ones it
/// adds for `to` come in. An option the user set keeps its value either way;
/// `kept` names those the user set to exactly the added value.
export function retypeOptions(
  options: Record<string, unknown> | undefined,
  from: Column["ty"] | undefined,
  to: Column["ty"],
  kept: readonly string[] = [],
): Record<string, unknown> | undefined {
  const next: Record<string, unknown> = { ...options };
  if (from !== undefined) {
    for (const [k, v] of Object.entries(typeOptions(from))) {
      if (!kept.includes(k) && JSON.stringify(next[k]) === JSON.stringify(v)) delete next[k];
    }
  }
  for (const [k, v] of Object.entries(typeOptions(to))) if (!(k in next)) next[k] = v;
  return Object.keys(next).length ? next : undefined;
}

/// A column field as the parser builds it: with its type's added options.
export function columnField(field: string, column: Column): Field {
  const options = retypeOptions(undefined, undefined, column.ty);
  return options ? { field, options, source: { column } } : { field, source: { column } };
}

/// Build a fresh field of `kind`, keeping the existing `name`. Reuses an obvious
/// related table when changing between relation kinds.
export function defaultField(name: string, kind: FieldKind, prevTable = ""): Field {
  const source = defaultSource(name, kind, prevTable);
  return "column" in source ? columnField(name, source.column) : { field: name, source };
}

function defaultSource(name: string, kind: FieldKind, table: string): FieldSource {
  if ((SCALAR_TYPES as string[]).includes(kind)) {
    return { column: { column: name, ty: kind as FlussoType, nullable: false } };
  }
  switch (kind) {
    case "map":
      return { column: { column: name, ty: { map: { values: "text" } }, nullable: false } };
    case "custom":
      return {
        column: { column: name, ty: { custom: { postgres: [], opensearch: "keyword" } }, nullable: false },
      };
    case "geo":
      return { geo: { lat: "lat", lon: "lon", nullable: false } };
    case "object":
      return { group: [] };
    case "constant":
      return { constant: "Null" };
    case "script":
      return { script: { source: "", ty: "keyword", nullable: false } };
    case "belongs_to":
    case "has_one":
    case "has_many":
    case "many_to_many":
      return { relation: { join: defaultJoin(kind, table) } };
    case "count":
    case "exists":
    case "sum":
    case "avg":
    case "min":
    case "max":
    case "ids":
      return { relation: { aggregate: defaultAggregate(kind, table) } };
    default:
      return { column: { column: name, ty: "keyword", nullable: false } };
  }
}

function defaultJoin(kind: FieldKind, table: string): Join {
  let joinKind: JoinKind;
  switch (kind) {
    case "belongs_to":
      joinKind = { belongs_to: { column: "" } };
      break;
    case "has_one":
      joinKind = { has_one: { foreign_key: "" } };
      break;
    case "has_many":
      joinKind = { has_many: { foreign_key: "" } };
      break;
    default:
      joinKind = { many_to_many: { through: { table: "", left_key: "", right_key: "" } } };
  }
  return { table, kind: joinKind, primary_key: "id", nullable: false, fields: [] };
}

function defaultAggregate(kind: FieldKind, table: string): Aggregate {
  return withAggregateOp({ table, op: "count", key: { direct: "" } }, kind);
}

/// Switch an aggregate to the op `kind`, keeping what every op shares: its
/// table, key, filters, `via` hops and `on` pairs. Only the old op's own parts reset: the
/// column it read, its `value_type` or `element_type`, and `distinct` once the
/// key no longer goes `through` a junction.
export function withAggregateOp(agg: Aggregate, kind: FieldKind): Aggregate {
  let op: AggregateOp;
  switch (kind) {
    case "count":
      op = "count";
      break;
    case "exists":
      op = "exists";
      break;
    case "sum":
      op = { sum: "" };
      break;
    case "avg":
      op = { avg: "" };
      break;
    case "min":
      op = { min: "" };
      break;
    case "max":
      op = { max: "" };
      break;
    default:
      op = { ids: { element_type: "long" } };
  }
  const next: Aggregate = { table: agg.table, op, key: agg.key };
  if (agg.filters) next.filters = agg.filters;
  if (agg.via) next.via = agg.via;
  if (agg.on) next.on = agg.on;
  if (kind === "sum" || kind === "min" || kind === "max") next.value_type = "integer";
  if (agg.distinct && "through" in agg.key) next.distinct = agg.distinct;
  return next;
}

/// The related table a relation field reads from (for column suggestions).
export function relationTable(field: Field): string {
  const s = field.source;
  if ("relation" in s) {
    const r = s.relation;
    if ("join" in r) return r.join.table;
    if ("aggregate" in r) return r.aggregate.table;
  }
  return "";
}
