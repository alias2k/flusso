// Stable, client-only ids for every field. A field's *path* is positional —
// removing a field re-indexes every sibling after it — and its name can be
// edited, so whatever must follow one field keys on `Field.uid` instead: the
// canvas's view state (collapsed nodes, dragged positions) for containers, and
// the inspector and a typed value still being read (`model/pending.ts`) for
// every field. The uid lives in the edited doc, so undo/redo restore it with the
// field; it never reaches the backend or YAML.
//
// - `stampIds` gives every field a uid (and re-stamps copies), run after every
//   load and edit.
// - `adoptIds` carries the current uids onto a schema that arrived without them
//   (Code mode's parsed YAML), matching fields by name, then position.
// - `uidSkeleton` reduces a schema to names + uids, so the store can persist
//   the saved doc's uids and `adoptIds` them onto the file on the next load.
// - `stripUids` removes them before a schema crosses the wire.

import type { Field, IndexSchema } from "../api";
import { childFields, withChildFields } from "./containers";

const newUid = () => crypto.randomUUID();

const isContainer = (field: Field) => childFields(field) !== null;

/// Give every field a uid, keeping existing ones. A uid seen twice
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
    let out = field;
    if (!field.uid || seen.has(field.uid)) out = { ...field, uid: newUid() };
    seen.add(out.uid!);
    const children = childFields(out);
    if (children === null) return out;
    const nextChildren = stampFields(children);
    return nextChildren === children ? out : withChildFields(out, nextChildren);
  };
  const fields = stampFields(schema.fields);
  return fields === schema.fields ? schema : { ...schema, fields };
}

/// Copy `prev`'s uids onto the uid-less fields of `next`, then stamp whatever
/// is left. A field takes the uid of the sibling with its name (so removing a
/// field above it doesn't shift ids), else of an unclaimed one of its shape
/// (container or leaf) at its index whose name is gone (a rename) — unless
/// `byNameOnly`, for a file that may have changed outside the designer, where a
/// new field landing in a removed one's slot must not inherit it.
export function adoptIds(
  prev: Pick<IndexSchema, "fields"> | undefined,
  next: IndexSchema,
  byNameOnly = false,
): IndexSchema {
  const adopt = (old: Field[], fields: Field[]): Field[] => {
    const names = new Set(fields.map((f) => f.field));
    const claimed = new Set<Field>();
    const match = (field: Field, i: number): Field | undefined => {
      const byName = old.find((o) => o.field === field.field && !claimed.has(o));
      if (byName) return byName;
      const atIndex = byNameOnly ? undefined : old[i];
      return atIndex &&
        isContainer(atIndex) === isContainer(field) &&
        !claimed.has(atIndex) &&
        !names.has(atIndex.field)
        ? atIndex
        : undefined;
    };
    return fields.map((field, i) => {
      const before = match(field, i);
      if (before) claimed.add(before);
      const withUid = field.uid || !before?.uid ? field : { ...field, uid: before.uid };
      const children = childFields(withUid);
      if (children === null) return withUid;
      return withChildFields(withUid, adopt(before ? (childFields(before) ?? []) : [], children));
    });
  };
  return stampIds({ ...next, fields: adopt(prev?.fields ?? [], next.fields) });
}

/// The schema reduced to what `adoptIds` matches on: every field's name and
/// uid, and each container's children. Small enough to persist per index.
export function uidSkeleton(schema: IndexSchema): Pick<IndexSchema, "fields"> {
  const skeleton = (fields: Field[]): Field[] =>
    fields.map((field) => {
      const children = childFields(field);
      return children === null
        ? { field: field.field, uid: field.uid, source: { constant: null } }
        : { field: field.field, uid: field.uid, source: { group: skeleton(children) } };
    });
  return { fields: skeleton(schema.fields) };
}

/// The fields without any uid (or other client-only marker), recursively —
/// the kernel shape.
export function stripFieldUids(fields: Field[]): Field[] {
  return fields.map((field) => {
    const rest = { ...field };
    delete rest.uid;
    delete rest.keptOptions;
    const children = childFields(rest);
    return children === null ? rest : withChildFields(rest, stripFieldUids(children));
  });
}

/// The schema without any uid — what the backend and the YAML see.
export function stripUids(schema: IndexSchema): IndexSchema {
  return { ...schema, fields: stripFieldUids(schema.fields) };
}
