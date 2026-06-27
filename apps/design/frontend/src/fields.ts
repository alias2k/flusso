// Helpers for editing the field model: classifying a field into the UI "kind"
// it should be edited as, and producing a fresh default field for a chosen kind
// while preserving the field name. The kind is the union of every authorable
// shape — scalar types, map/custom/geo/object, the four join verbs, the six
// aggregate ops, and constant.

import type {
  Aggregate,
  AggregateOp,
  Field,
  FieldSource,
  FlussoType,
  Join,
  JoinKind,
} from "./api";
import { SCALAR_TYPES } from "./api";

export const JOIN_KINDS = [
  "belongs_to",
  "has_one",
  "has_many",
  "many_to_many",
] as const;
export const AGGREGATE_OPS = ["count", "sum", "avg", "min", "max", "ids"] as const;
export const LEAF_TYPES: FlussoType[] = [
  "text",
  "keyword",
  "integer",
  "long",
  "double",
  "date",
];

export type FieldKind = string;

/// Every kind the kind-picker offers, grouped for the dropdown.
export const KIND_GROUPS: { label: string; kinds: FieldKind[] }[] = [
  { label: "Scalar", kinds: SCALAR_TYPES as string[] },
  { label: "Structured", kinds: ["map", "custom", "geo", "object", "constant"] },
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
  if ("sum" in op) return "sum";
  if ("avg" in op) return "avg";
  if ("min" in op) return "min";
  if ("max" in op) return "max";
  return "ids";
}

/// Build a fresh field of `kind`, keeping the existing `name`. Reuses an obvious
/// related table when changing between relation kinds.
export function defaultField(name: string, kind: FieldKind, prevTable = ""): Field {
  const source = defaultSource(name, kind, prevTable);
  return { field: name, source };
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
      return { constant: "" };
    case "belongs_to":
    case "has_one":
    case "has_many":
    case "many_to_many":
      return { relation: { join: defaultJoin(kind, table) } };
    case "count":
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
  let op: AggregateOp;
  switch (kind) {
    case "count":
      op = "count";
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
  const agg: Aggregate = { table, op, key: { direct: "" } };
  if (kind === "sum" || kind === "min" || kind === "max") agg.value_type = "integer";
  return agg;
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
