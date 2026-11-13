// Structural completeness checks: a field is "incomplete" when something the
// strict backend requires is still blank or malformed — its name, its column,
// a join/aggregate key. Surfacing this on the canvas turns a late `flusso check`
// error ("aggregate must specify a key") into an immediate visual hint while
// building, and `prune.ts` uses the same checks to keep such a field out of the
// live preview.

import type { Aggregate, AggregateKey, ColumnPair, Field } from "../api";
import { trimmed } from "./names";
import { scriptIncomplete } from "./scripts";
import { joinOf } from "./tree";

/// The kernel's `FieldName` rule, applied after its trim.
const FIELD_NAME = /^[a-zA-Z_][a-zA-Z0-9_]*$/;
const FIELD_NAME_MAX_CHARS = 63;

/// Is this a name the backend's `FieldName` refuses (blank included)?
export function nameInvalid(raw: string): boolean {
  const name = trimmed(raw);
  return !FIELD_NAME.test(name) || [...name].length > FIELD_NAME_MAX_CHARS;
}

/// Is this field's name one the backend's `FieldName` refuses (blank included)?
export const nameIncomplete = (field: Field | undefined): boolean => !!field && nameInvalid(field.field);

/// Is this field, of any kind, incomplete on its own level: a bad name, a blank
/// column or geo coordinate, a blank script, or an incomplete join/aggregate?
/// Its children are not checked; a group or join with an incomplete child is
/// still complete itself.
export function fieldIncomplete(field: Field | undefined): boolean {
  if (!field) return false;
  if (nameIncomplete(field)) return true;
  const s = field.source;
  if ("column" in s) return !s.column.column;
  if ("geo" in s) return !s.geo.lat || !s.geo.lon;
  return scriptIncomplete(field) || joinIncomplete(field) || aggregateIncomplete(field);
}

/// A pair missing a side, or listed twice: both are rejected at load.
export const pairsIncomplete = (on: ColumnPair[] | undefined): boolean =>
  (on ?? []).some(
    (pair, i, all) =>
      !pair.local ||
      !pair.related ||
      all.findIndex((other) => other.local === pair.local && other.related === pair.related) !== i,
  );

function keyIncomplete(key: AggregateKey): boolean {
  if ("direct" in key) return !key.direct;
  const t = key.through;
  return !t.table || !t.left_key || !t.right_key;
}

/// Is the join that *is* this node still missing a required key/table, or a
/// side of an `on` pair (or listing one twice)?
export function joinIncomplete(field: Field | null): boolean {
  const join = joinOf(field);
  if (!join) return false;
  if (!join.table || !join.primary_key || pairsIncomplete(join.on)) return true;
  const k = join.kind;
  if ("belongs_to" in k) return !k.belongs_to.column;
  if ("has_one" in k) return !k.has_one.foreign_key;
  if ("has_many" in k) return !k.has_many.foreign_key;
  const t = k.many_to_many.through;
  return !t.table || !t.left_key || !t.right_key;
}

/// Is this aggregate field missing its table, key, a `via` hop's table or key,
/// a side of an `on` pair, its column, or its result type?
/// Tolerates `undefined` — a just-removed node briefly re-renders against the
/// new, shorter tree (see `nodeFields`).
export function aggregateIncomplete(field: Field | undefined): boolean {
  if (!field) return false;
  const s = field.source;
  if (!("relation" in s) || !("aggregate" in s.relation)) return false;
  const agg: Aggregate = s.relation.aggregate;
  if (!agg.table || keyIncomplete(agg.key) || pairsIncomplete(agg.on)) return true;
  if ((agg.via ?? []).some((hop) => !hop.table || !hop.foreign_key || pairsIncomplete(hop.on))) return true;
  const op = agg.op;
  if (typeof op === "string") return false; // count / exists
  if ("ids" in op) return false;
  if ("avg" in op) return !op.avg;
  if ("sum" in op) return !op.sum || !agg.value_type;
  if ("min" in op) return !op.min || !agg.value_type;
  if ("max" in op) return !op.max || !agg.value_type;
  return false;
}
