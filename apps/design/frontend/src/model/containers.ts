// The one shape question every tree walk asks: is this field a container (an
// `object` group or a join), and what are its children. Dependency-free (types
// only), so `api.ts` can reach it through `model/ids.ts` without an import cycle.

import type { Field } from "../api";

/// The child fields of a container field, or `null` for a leaf.
export function childFields(field: Field): Field[] | null {
  const s = field.source;
  if ("group" in s) return s.group;
  if ("relation" in s && "join" in s.relation) return s.relation.join.fields;
  return null;
}

/// The container `field` with its child fields replaced (unchanged for a leaf).
export function withChildFields(field: Field, fields: Field[]): Field {
  const s = field.source;
  if ("group" in s) return { ...field, source: { group: fields } };
  if ("relation" in s && "join" in s.relation) {
    return { ...field, source: { relation: { join: { ...s.relation.join, fields } } } };
  }
  return field;
}
