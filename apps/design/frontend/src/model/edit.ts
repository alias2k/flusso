// Path-addressed, immutable edits on the schema tree. Every canvas/inspector
// action funnels through one of these; each returns a fresh `IndexSchema`.

import type { Field, FlussoType, IndexSchema } from "../api";
import { defaultField } from "../fields";
import { nodeFields, setNodeField, withNodeFields } from "./tree";

/// Include or drop a scalar field backed by `column` on the node at `path`.
export function toggleColumn(
  schema: IndexSchema,
  path: number[],
  column: string,
  on: boolean,
  ty: FlussoType = "keyword",
  nullable = true,
): IndexSchema {
  const fields = nodeFields(schema, path);
  if (on) {
    const field: Field = { field: column, source: { column: { column, ty, nullable } } };
    return withNodeFields(schema, path, [...fields, field]);
  }
  const next = fields.filter((f) => !("column" in f.source && typeof f.source.column.ty === "string" && f.source.column.column === column));
  return withNodeFields(schema, path, next);
}

/// Append a field (leaf or container) to the node at `path`.
export function addField(schema: IndexSchema, path: number[], field: Field): IndexSchema {
  return withNodeFields(schema, path, [...nodeFields(schema, path), field]);
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
  next[i] = field;
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
