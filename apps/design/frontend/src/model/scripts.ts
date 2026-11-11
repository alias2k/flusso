// Script fields in the designer: matching the backend's script errors to the
// field they belong to, and keeping a retyped script valid.
//
// The backend names a script by its path in the document: field names joined by
// `.`, with `[]` after a to-many join in a compile error (`variants[].gross`) and
// the element position in a sample failure (`variants[2].gross`). The canvas
// addresses a field by node path + index, so `scriptPath` turns that into the
// compile-error form and `samePath` compares the two forms.

import type { Field, FlussoType, IndexSchema, Script, ScriptErrorDto } from "../api";
import { childFields } from "./containers";
import { SUBFIELD_TYPES } from "./edit";

/// The compile-error path of the field at `index` under the container at `path`.
export function scriptPath(schema: IndexSchema, path: number[], index: number): string {
  let fields = schema.fields;
  let prefix = "";
  for (const idx of path) {
    const container = fields[idx];
    if (!container) return "";
    const s = container.source;
    const toMany =
      "relation" in s &&
      "join" in s.relation &&
      ("has_many" in s.relation.join.kind || "many_to_many" in s.relation.join.kind);
    prefix += `${container.field}${toMany ? "[]" : ""}.`;
    fields = childFields(container) ?? [];
  }
  const field = fields[index];
  return field ? `${prefix}${field.field}` : "";
}

/// Whether two script paths name the same field, ignoring element positions.
export function samePath(a: string, b: string): boolean {
  const fold = (p: string) => p.replace(/\[\d*\]/g, "[]");
  return fold(a) === fold(b);
}

/// The script errors for the field at `path` (compile-error form).
export function errorsFor(errors: ScriptErrorDto[] | undefined, path: string): ScriptErrorDto[] {
  return path ? (errors ?? []).filter((e) => samePath(e.field, path)) : [];
}

/// A script field whose source is still blank: the strict backend rejects it.
export function scriptIncomplete(field: Field | undefined): boolean {
  return !!field && "script" in field.source && !field.source.script.source.trim();
}

/// Give a script a new result type, dropping what that type no longer takes.
export function retypeScript(script: Script, ty: FlussoType): Script {
  const next: Script = { ...script, ty };
  if (!SUBFIELD_TYPES.includes(ty)) delete next.subfields;
  if (ty !== "enum") delete next.enum_order;
  return next;
}
