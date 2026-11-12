// The bridge between the canvas and the schema. The source of truth stays the
// `IndexSchema` tree (so codegen/preview/save are unchanged); the canvas is a
// *projection* of it into nodes + edges, and every edit is a path-addressed,
// immutable mutation of the tree.
//
// A canvas **node** is a *container* in the document: the root, an `object`
// group, or a join (to-one/to-many). A container's non-container fields
// (scalars, geo, map, custom, constant, aggregates) are the node's leaf rows;
// its container fields become child nodes. A node's path is the list of field
// indices from the root (`[]` = root, `[2,0]` = `fields[2].…fields[0]`).
// Paths address edits; a node's *id* is its container's stable `uid`
// (`model/ids.ts`), so view state survives a sibling being removed.

import type { Field, FlussoType, IndexSchema, Join, JoinKind } from "../api";
import { childFields, withChildFields } from "./containers";

export type NodeKind = "root" | "object" | "belongs_to" | "has_one" | "has_many" | "many_to_many";

export interface LeafField {
  index: number;
  name: string;
  kind: string; // from fieldKind(): a scalar type, "geo", "map", "custom", "constant", or an aggregate op
  column?: string;
  ty?: FlussoType;
}

export interface DocNode {
  /// The stable node id: `"root"`, or the container field's `uid`.
  id: string;
  path: number[];
  parentId: string | null;
  depth: number;
  kind: NodeKind;
  /// The table this node's leaf columns come from (a group inherits its parent's).
  table: string;
  /// Root primary key, or a join's primary key. Absent on `object` groups.
  primaryKey?: string;
  /// Display name (the field name) for non-root nodes.
  name?: string;
  leaves: LeafField[];
}

export interface Graph {
  nodes: DocNode[];
  edges: { id: string; source: string; target: string; label: string }[];
}

/// The positional id of `path` — the node id before stable uids, kept to
/// translate view state stored under it.
export const pathId = (path: number[]): string => (path.length ? `root.${path.join(".")}` : "root");

const nodeId = (path: number[], field: Field | null): string => field?.uid ?? pathId(path);

/// The canvas node id of the container at `path`.
export function nodeIdAt(schema: IndexSchema, path: number[]): string {
  return nodeId(path, fieldAtPath(schema, path));
}

function joinVerb(kind: JoinKind): NodeKind {
  if ("belongs_to" in kind) return "belongs_to";
  if ("has_one" in kind) return "has_one";
  if ("has_many" in kind) return "has_many";
  return "many_to_many";
}

import { fieldKind } from "../fields";

/// The child fields of the container at `path` (the root's `fields` for `[]`).
/// Returns `[]` for a stale path (an index past the array) — React Flow's
/// controlled nodes lag a render behind a delete, so a just-removed node may
/// briefly project against the new, shorter tree.
export function nodeFields(schema: IndexSchema, path: number[]): Field[] {
  let fields = schema.fields;
  for (const idx of path) {
    const field = fields[idx];
    if (!field) return [];
    const cf = childFields(field);
    if (!cf) return [];
    fields = cf;
  }
  return fields;
}

/// Replace the child fields of the container at `path`, rebuilding immutably.
export function withNodeFields(schema: IndexSchema, path: number[], next: Field[]): IndexSchema {
  if (path.length === 0) return { ...schema, fields: next };
  const rebuild = (fields: Field[], depth: number): Field[] => {
    const idx = path[depth];
    const field = fields[idx];
    const updated =
      depth === path.length - 1
        ? withChildFields(field, next)
        : withChildFields(field, rebuild(childFields(field) ?? [], depth + 1));
    const copy = fields.slice();
    copy[idx] = updated;
    return copy;
  };
  return { ...schema, fields: rebuild(schema.fields, 0) };
}

/// The field names from the document root to the container at `path`: the
/// path a diagnostic names it by.
export function namePath(schema: IndexSchema, path: number[]): string[] {
  const names: string[] = [];
  let fields = schema.fields;
  for (const idx of path) {
    const field = fields[idx];
    if (!field) return names;
    names.push(field.field);
    fields = childFields(field) ?? [];
  }
  return names;
}

/// The container field that *is* the node at `path` (null for the root).
export function fieldAtPath(schema: IndexSchema, path: number[]): Field | null {
  if (path.length === 0) return null;
  const parent = nodeFields(schema, path.slice(0, -1));
  return parent[path[path.length - 1]] ?? null;
}

/// A container `next` keeps `prev`'s uid when it has none of its own, so
/// replacing a field (a verb or kind change) keeps its node's view state.
export function carryUid(prev: Field | undefined, next: Field): Field {
  return prev?.uid && !next.uid && childFields(next) !== null ? { ...next, uid: prev.uid } : next;
}

/// Replace the container field that is the node at `path`.
export function setNodeField(schema: IndexSchema, path: number[], field: Field): IndexSchema {
  const parentPath = path.slice(0, -1);
  const parent = nodeFields(schema, parentPath).slice();
  const i = path[path.length - 1];
  parent[i] = carryUid(parent[i], field);
  return withNodeFields(schema, parentPath, parent);
}

/// The field names of the containers along `path` (for an inspector breadcrumb).
export function pathLabels(schema: IndexSchema, path: number[]): string[] {
  const out: string[] = [];
  for (let i = 1; i <= path.length; i += 1) {
    const field = fieldAtPath(schema, path.slice(0, i));
    if (field) out.push(field.field);
  }
  return out;
}

/// The table whose columns a node reads (root table, or the nearest enclosing join's table).
export function effectiveTable(schema: IndexSchema, path: number[]): string {
  let table = schema.table;
  let fields = schema.fields;
  for (const idx of path) {
    const field = fields[idx];
    if (!field) break; // stale path (see `nodeFields`)
    const s = field.source;
    if ("relation" in s && "join" in s.relation) table = s.relation.join.table;
    fields = childFields(field) ?? [];
  }
  return table;
}

function leafOf(field: Field, index: number): LeafField {
  const kind = fieldKind(field);
  const s = field.source;
  if ("column" in s && typeof s.column.ty === "string") {
    return { index, name: field.field, kind, column: s.column.column, ty: s.column.ty };
  }
  return { index, name: field.field, kind };
}

/// Project the whole schema into canvas nodes + edges (positions assigned later).
export function projectGraph(schema: IndexSchema): Graph {
  const nodes: DocNode[] = [];
  const edges: Graph["edges"] = [];

  const visit = (path: number[], parentId: string | null, depth: number) => {
    const fields = nodeFields(schema, path);
    const field = fieldAtPath(schema, path);
    const id = nodeId(path, field);
    const kind: NodeKind = field ? nodeKind(field) : "root";

    nodes.push({
      id,
      path,
      parentId,
      depth,
      kind,
      table: effectiveTable(schema, path),
      primaryKey: nodePrimaryKey(schema, path, field),
      name: field?.field,
      leaves: fields.map(leafOf).filter((l) => !isContainerKind(l.kind)),
    });

    fields.forEach((child, i) => {
      if (childFields(child) !== null) {
        const childPath = [...path, i];
        const childId = nodeId(childPath, child);
        edges.push({
          id: `${id}->${childId}`,
          source: id,
          target: childId,
          label: nodeKind(child),
        });
        visit(childPath, id, depth + 1);
      }
    });
  };

  visit([], null, 0);
  return { nodes, edges };
}

function nodeKind(field: Field): NodeKind {
  const s = field.source;
  if ("group" in s) return "object";
  if ("relation" in s && "join" in s.relation) return joinVerb(s.relation.join.kind);
  return "object";
}

function isContainerKind(kind: string): boolean {
  return (
    kind === "object" || kind === "belongs_to" || kind === "has_one" || kind === "has_many" || kind === "many_to_many"
  );
}

function nodePrimaryKey(schema: IndexSchema, path: number[], field: Field | null): string | undefined {
  if (path.length === 0) return schema.primary_key;
  if (field) {
    const s = field.source;
    if ("relation" in s && "join" in s.relation) return s.relation.join.primary_key;
  }
  return undefined;
}

/// The join carried by a node, if it is a join node.
export function joinOf(field: Field | null): Join | null {
  if (field && "relation" in field.source && "join" in field.source.relation) {
    return field.source.relation.join;
  }
  return null;
}
