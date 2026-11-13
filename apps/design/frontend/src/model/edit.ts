// Path-addressed, immutable edits on the schema tree. Every canvas/inspector
// action funnels through one of these; each returns a fresh `IndexSchema`.

import type { Column, Field, FlussoType, IndexSchema } from "../api";
import type { Generic } from "./generic";
import { columnField, defaultField, retypeOptions, typeOptions } from "../fields";
import { childFields, withChildFields } from "./containers";
import { cloneJson } from "./exact";
import { stripFieldUids } from "./ids";
import { carryUid, nodeFields, setNodeField, withNodeFields } from "./tree";

/// Include or drop a scalar field backed by `column` on the node at `path`.
export function toggleColumn(
  schema: IndexSchema,
  path: number[],
  column: string,
  on: boolean,
  ty: FlussoType = "keyword",
  nullable = true,
  sqlType?: string,
): IndexSchema {
  const fields = nodeFields(schema, path);
  if (on) {
    return withNodeFields(schema, path, [...fields, columnField(column, catalogColumn(column, ty, nullable, sqlType))]);
  }
  const next = fields.filter(
    (f) => !("column" in f.source && typeof f.source.column.ty === "string" && f.source.column.column === column),
  );
  return withNodeFields(schema, path, next);
}

/// Append a field (leaf or container) to the node at `path`.
export function addField(schema: IndexSchema, path: number[], field: Field): IndexSchema {
  return withNodeFields(schema, path, [...nodeFields(schema, path), field]);
}

/// Include every catalog column not already present as a scalar field.
export function includeColumns(
  schema: IndexSchema,
  path: number[],
  columns: { name: string; ty?: FlussoType; nullable?: boolean; sqlType?: string }[],
): IndexSchema {
  const fields = nodeFields(schema, path);
  const present = new Set(
    fields
      .filter((f) => "column" in f.source && typeof f.source.column.ty === "string")
      .map((f) => ("column" in f.source ? f.source.column.column : "")),
  );
  const added = columns
    .filter((c) => !present.has(c.name))
    .map((c) => columnField(c.name, catalogColumn(c.name, c.ty ?? "keyword", c.nullable ?? true, c.sqlType)));
  return withNodeFields(schema, path, [...fields, ...added]);
}

/// Whether a `default` (in its `GenericValue` form) is a list holding a list
/// or an object, which only a `json` array's default may.
export function listsStructured(def: Generic): boolean {
  if (!isRecord(def) || !Array.isArray(def.Array)) return false;
  return def.Array.some((e) => isRecord(e) && ("Array" in e || "Map" in e));
}

/// Whether a catalog type (as `format_type` spells it) is an array: `text[]`.
export function holdsArray(sqlType?: string): boolean {
  return sqlType?.trim().endsWith("[]") ?? false;
}

/// A column included from the catalog: an array type reads as `array: true`.
function catalogColumn(column: string, ty: FlussoType, nullable: boolean, sqlType?: string): Column {
  return holdsArray(sqlType) ? { column, ty, nullable, array: true } : { column, ty, nullable };
}

/// Remove the given rows from the node in one pass — matches a catalog-backed
/// scalar column by its source column name, and any other field (special leaves:
/// aggregates/geo/map/custom/…) by its field name. The batch counterpart used by
/// the bulk "remove selected" action, so it spans both row kinds.
export function removeFields(schema: IndexSchema, path: number[], names: string[]): IndexSchema {
  const drop = new Set(names);
  return withNodeFields(
    schema,
    path,
    nodeFields(schema, path).filter((f) => {
      const col = "column" in f.source && typeof f.source.column.ty === "string" ? f.source.column.column : undefined;
      return !(col !== undefined && drop.has(col)) && !drop.has(f.field);
    }),
  );
}

/// Drop every plain scalar-column field on the node (keeps geo/map/custom/
/// aggregate/object — only the checkbox-driven columns clear).
export function clearColumns(schema: IndexSchema, path: number[]): IndexSchema {
  return withNodeFields(
    schema,
    path,
    nodeFields(schema, path).filter((f) => !("column" in f.source && typeof f.source.column.ty === "string")),
  );
}

/// Insert a deep copy of the field at `index` (with a fresh unique name) right
/// after it. Works for leaves and whole container subtrees alike; the copy's
/// containers drop their uids so the store stamps fresh ones.
export function duplicateAt(schema: IndexSchema, path: number[], index: number): IndexSchema {
  const fields = nodeFields(schema, path);
  const orig = fields[index];
  if (!orig) return schema;
  const [clone] = stripFieldUids([cloneJson(orig)]);
  const copy: Field = {
    ...clone,
    field: uniqueName(
      fields.map((f) => f.field),
      orig.field,
    ),
  };
  const next = fields.slice();
  next.splice(index + 1, 0, copy);
  return withNodeFields(schema, path, next);
}

/// Duplicate the container field that *is* the node at `path` within its parent.
export function duplicateNode(schema: IndexSchema, path: number[]): IndexSchema {
  if (path.length === 0) return schema;
  return duplicateAt(schema, path.slice(0, -1), path[path.length - 1]);
}

function uniqueName(existing: string[], base: string): string {
  let name = `${base}_copy`;
  let n = 1;
  while (existing.includes(name)) {
    n += 1;
    name = `${base}_copy${n}`;
  }
  return name;
}

/// Append a fresh special/leaf field of `kind` (geo/map/custom/constant/aggregate
/// op) or an `object` group to the node at `path`.
export function addSpecial(schema: IndexSchema, path: number[], kind: string): IndexSchema {
  const existing = nodeFields(schema, path).map((f) => f.field);
  let name = kind === "object" ? "group" : kind;
  let n = 1;
  while (existing.includes(name)) name = `${kind}${++n}`;
  return addField(schema, path, defaultField(name, kind));
}

/// Replace the field at index `i` within the node at `path`.
export function setLeaf(schema: IndexSchema, path: number[], i: number, field: Field): IndexSchema {
  const next = nodeFields(schema, path).slice();
  next[i] = carryUid(next[i], field);
  return withNodeFields(schema, path, next);
}

/// Remove the field at index `i` within the node at `path` (drops its subtree).
export function removeAt(schema: IndexSchema, path: number[], i: number): IndexSchema {
  return withNodeFields(
    schema,
    path,
    nodeFields(schema, path).filter((_, j) => j !== i),
  );
}

/// Remove the container node at `path` from its parent.
export function removeNode(schema: IndexSchema, path: number[]): IndexSchema {
  if (path.length === 0) return schema;
  return removeAt(schema, path.slice(0, -1), path[path.length - 1]);
}

/// Replace the container field that *is* the node at `path` (verb/keys/filters/…).
export function setNode(schema: IndexSchema, path: number[], field: Field): IndexSchema {
  return setNodeField(schema, path, field);
}

/// Patch root-level metadata (table, schema, primary key).
export function setRootMeta(
  schema: IndexSchema,
  patch: Partial<Pick<IndexSchema, "table" | "db_schema" | "primary_key">>,
): IndexSchema {
  return { ...schema, ...patch };
}

/// The scalar types whose sink adds automatic subfields a field may turn off.
export const SUBFIELD_TYPES: FlussoType[] = ["text", "keyword", "uuid"];

/// Give a column field a new type, dropping the opt-outs it no longer takes, so
/// a retyped field never carries a sibling the schema would reject, and moving
/// the options the parser adds per type (see `retypeOptions`). Every retype
/// (the type picker, the suggested-type chip, "fix all") goes through here.
export function retypeField(field: Field, column: Column, ty: FlussoType): Field {
  const next: Column = { ...column, ty };
  if (!SUBFIELD_TYPES.includes(ty)) delete next.subfields;
  if (ty !== "json") delete next.indexed;
  if (ty === "json" && next.array) delete next.transforms;
  if (ty !== "json" && next.array && listsStructured(next.default)) delete next.default;
  const retyped: Field = { ...field, source: { column: next } };
  // An option `ty` would add that is already there was set by the user.
  const userSet = Object.keys(typeOptions(ty)).filter((k) => field.options && k in field.options);
  const kept = [...new Set([...(field.keptOptions ?? []), ...userSet])];
  const options = retypeOptions(field.options, column.ty, ty, kept);
  if (options) retyped.options = options;
  else delete retyped.options;
  const stillKept = kept.filter((k) => options && k in options);
  if (stillKept.length) retyped.keptOptions = stillKept;
  else delete retyped.keptOptions;
  return retyped;
}

/// `mine` (an edit of `base`) carried onto `theirs` (what `base` became since):
/// a key `mine` left as it was in `base` takes `theirs`, one it changed takes
/// `mine`, objects key by key; a key `theirs` removed stays removed (an option
/// renamed or dropped since). So an edit made from an older render, such as a
/// typed value read after its box lost focus, keeps newer edits to the rest.
export function rebase<T>(base: T, mine: T, theirs: T): T {
  if (sameJson(mine, base)) return theirs;
  if (sameJson(theirs, base)) return mine;
  if (!isRecord(base) || !isRecord(mine) || !isRecord(theirs)) return mine;
  const out: Record<string, unknown> = {};
  for (const key of new Set([...Object.keys(base), ...Object.keys(mine), ...Object.keys(theirs)])) {
    if (base[key] !== undefined && theirs[key] === undefined) continue;
    const value = rebase(base[key], mine[key], theirs[key]);
    if (value !== undefined) out[key] = value;
  }
  return out as T;
}

/// Replace the field whose uid is `uid` — wherever it sits now, under whatever
/// name — with `fn` of it. The schema is returned unchanged when no field has
/// that uid any more (it was removed), so a late edit never lands elsewhere.
export function updateField(schema: IndexSchema, uid: string, fn: (now: Field) => Field): IndexSchema {
  const walk = (fields: Field[]): Field[] => {
    const at = fields.findIndex((f) => f.uid === uid);
    if (at >= 0) {
      const replaced = carryUid(fields[at], fn(fields[at]));
      if (sameJson(replaced, fields[at])) return fields;
      const next = fields.slice();
      next[at] = replaced;
      return next;
    }
    for (const [i, f] of fields.entries()) {
      const children = childFields(f);
      if (children === null) continue;
      const nextChildren = walk(children);
      if (nextChildren === children) continue;
      const next = fields.slice();
      next[i] = withChildFields(f, nextChildren);
      return next;
    }
    return fields;
  };
  const fields = walk(schema.fields);
  return fields === schema.fields ? schema : { ...schema, fields };
}

const sameJson = (a: unknown, b: unknown) => JSON.stringify(a) === JSON.stringify(b);

/// A plain JSON object (not an array, and not an exact integer held as raw JSON).
const isRecord = (v: unknown): v is Record<string, unknown> =>
  typeof v === "object" && v !== null && !Array.isArray(v) && Object.getPrototypeOf(v) === Object.prototype;
