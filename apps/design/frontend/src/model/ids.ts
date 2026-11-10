// Stable, client-only ids for the canvas's container nodes (object groups and
// joins). A node's *path* is positional — removing a field re-indexes every
// sibling after it — so the user's view state (collapsed nodes, dragged
// positions) keys on `Field.uid` instead. The uid lives in the edited doc, so
// undo/redo restore it with the field; it never reaches the backend or YAML.
//
// - `stampIds` gives every container a uid (and re-stamps copies), run after
//   every load and edit.
// - `adoptIds` carries the current uids onto a schema that arrived without them
//   (Code mode's parsed YAML), matching containers by position.
// - `uidsByPath` / `withUidsByPath` flatten the uids to a path-keyed table and
//   back, so the store can persist the saved doc's uids and re-apply them to
//   the same file on the next page load.
// - `stripUids` removes them before a schema crosses the wire.

import type { Field, IndexSchema } from "../api";
import { childFields, withChildFields } from "./containers";

const newUid = () => crypto.randomUUID();

const isContainer = (field: Field) => childFields(field) !== null;

/// Give every container field a uid, keeping existing ones. A uid seen twice
/// (a copied subtree) is replaced on every occurrence after the first in tree
/// order. Returns the same object when nothing changed.
export function stampIds(schema: IndexSchema): IndexSchema {
  const seen = new Set<string>();
  const stampFields = (fields: Field[]): Field[] => {
    let changed = false;
    const next = fields.map((field) => {
      const stamped = stampField(field);
      if (stamped !== field) changed = true;
      return stamped;
    });
    return changed ? next : fields;
  };
  const stampField = (field: Field): Field => {
    const children = childFields(field);
    if (children === null) return field;
    let out = field;
    if (!field.uid || seen.has(field.uid)) out = { ...field, uid: newUid() };
    seen.add(out.uid!);
    const nextChildren = stampFields(children);
    return nextChildren === children ? out : withChildFields(out, nextChildren);
  };
  const fields = stampFields(schema.fields);
  return fields === schema.fields ? schema : { ...schema, fields };
}

/// Copy `prev`'s container uids onto the uid-less containers of `next` that sit
/// at the same path, then stamp whatever is left.
export function adoptIds(prev: IndexSchema | undefined, next: IndexSchema): IndexSchema {
  const adopt = (old: Field[] | null, fields: Field[]): Field[] =>
    fields.map((field, i) => {
      const children = childFields(field);
      if (children === null) return field;
      const before = old?.[i];
      const fromBefore = before && isContainer(before) ? before : undefined;
      const withUid = field.uid || !fromBefore?.uid ? field : { ...field, uid: fromBefore.uid };
      return withChildFields(withUid, adopt(fromBefore ? childFields(fromBefore) : null, children));
    });
  return stampIds({ ...next, fields: adopt(prev?.fields ?? null, next.fields) });
}

/// Every container's uid, keyed by its positional path (`"2.0"`).
export function uidsByPath(schema: IndexSchema): Record<string, string> {
  const out: Record<string, string> = {};
  const walk = (fields: Field[], prefix: string) =>
    fields.forEach((field, i) => {
      const children = childFields(field);
      if (children === null) return;
      const key = prefix ? `${prefix}.${i}` : `${i}`;
      if (field.uid) out[key] = field.uid;
      walk(children, key);
    });
  walk(schema.fields, "");
  return out;
}

/// Give each uid-less container the uid `table` records for its path, then
/// stamp whatever is left.
export function withUidsByPath(schema: IndexSchema, table: Record<string, string>): IndexSchema {
  const assign = (fields: Field[], prefix: string): Field[] =>
    fields.map((field, i) => {
      const children = childFields(field);
      if (children === null) return field;
      const key = prefix ? `${prefix}.${i}` : `${i}`;
      const uid = field.uid ?? table[key];
      return withChildFields(uid ? { ...field, uid } : field, assign(children, key));
    });
  return stampIds({ ...schema, fields: assign(schema.fields, "") });
}

/// The fields without any uid, recursively — the kernel shape.
export function stripFieldUids(fields: Field[]): Field[] {
  return fields.map((field) => {
    const rest = { ...field };
    delete rest.uid;
    const children = childFields(rest);
    return children === null ? rest : withChildFields(rest, stripFieldUids(children));
  });
}

/// The schema without any uid — what the backend and the YAML see.
export function stripUids(schema: IndexSchema): IndexSchema {
  return { ...schema, fields: stripFieldUids(schema.fields) };
}
