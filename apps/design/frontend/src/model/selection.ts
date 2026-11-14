// The inspector's selection follows a field, not a position. A selection is
// addressed by path (what every consumer reads), but it also carries the uid of
// the field it points at (`model/ids.ts`): the field for a `field` selection,
// the container for a `node` or `columns` one. After every change to the
// document (an edit, Code mode, undo/redo) the store moves the path to wherever
// that uid sits now, so removing or inserting a field above it changes nothing
// the user sees. When the field itself is gone the selection clears, as a
// delete from the canvas or the inspector does.
//
// ```ts
// const sel = anchorSelection({ kind: "field", path: [], index: 1 }, schema); // c2, uid "u2"
// followSelection(sel, withoutFirstField); // { kind: "field", path: [], index: 0, uid: "u2" }
// followSelection(sel, withoutC2); // null
// ```

import type { Field, IndexSchema } from "../api";
import type { Selection } from "../state";
import { childFields } from "./containers";
import { fieldAtPath, nodeFields } from "./tree";

/// Where the field `uid` sits: the path of its container and its index there.
export function locateField(schema: IndexSchema, uid: string): { path: number[]; index: number } | null {
  const walk = (fields: Field[], path: number[]): { path: number[]; index: number } | null => {
    for (const [index, field] of fields.entries()) {
      if (field.uid === uid) return { path, index };
      const children = childFields(field);
      const found = children && walk(children, [...path, index]);
      if (found) return found;
    }
    return null;
  };
  return walk(schema.fields, []);
}

/// The names of the other fields at the level of the field `uid`: the names a
/// rename of it must not take.
export function siblingNames(schema: IndexSchema, uid: string | undefined): string[] {
  const at = uid ? locateField(schema, uid) : null;
  if (!at) return [];
  return nodeFields(schema, at.path)
    .filter((_, i) => i !== at.index)
    .map((f) => f.field);
}

/// The uid of the field a selection points at, read at its path now.
function uidAt(selection: NonNullable<Selection>, schema: IndexSchema): string | undefined {
  switch (selection.kind) {
    case "field":
      return nodeFields(schema, selection.path)[selection.index]?.uid;
    case "node":
    case "columns":
      return fieldAtPath(schema, selection.path)?.uid ?? undefined;
    case "root":
      return undefined;
  }
}

/// `selection` with the uid of the field at its path, when there is one there.
export function anchorSelection(selection: Selection, schema: IndexSchema | undefined): Selection {
  if (!selection || selection.kind === "root" || !schema) return selection;
  const uid = uidAt(selection, schema);
  return uid === undefined || uid === selection.uid ? selection : { ...selection, uid };
}

/// `selection` moved to where its field sits in `schema`; `null` once that
/// field is gone. A selection still without a uid (made before its field was
/// added) takes the uid of the field at its path. The same object comes back
/// when nothing moved.
export function followSelection(selection: Selection, schema: IndexSchema): Selection {
  if (!selection || selection.kind === "root") return selection;
  if (selection.uid === undefined) return anchorSelection(selection, schema);
  const at = locateField(schema, selection.uid);
  if (!at) return null;
  if (selection.kind === "field") {
    return samePath(at.path, selection.path) && at.index === selection.index
      ? selection
      : { ...selection, path: at.path, index: at.index };
  }
  const path = [...at.path, at.index];
  return samePath(path, selection.path) ? selection : { ...selection, path };
}

const samePath = (a: number[], b: number[]) => a.length === b.length && a.every((x, i) => x === b[i]);
