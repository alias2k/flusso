// Structural completeness checks: a join/aggregate is "incomplete" when a key
// the grammar requires is still blank. Surfacing this on the canvas turns a
// late `flusso check` error ("aggregate must specify a key") into an immediate
// visual hint while building.

import type { Aggregate, AggregateKey, Field } from "../api";
import { joinOf } from "./tree";

function keyIncomplete(key: AggregateKey): boolean {
  if ("direct" in key) return !key.direct;
  const t = key.through;
  return !t.table || !t.left_key || !t.right_key;
}

/// Is the join that *is* this node still missing a required key/table?
export function joinIncomplete(field: Field | null): boolean {
  const join = joinOf(field);
  if (!join) return false;
  if (!join.table || !join.primary_key) return true;
  const k = join.kind;
  if ("belongs_to" in k) return !k.belongs_to.column;
  if ("has_one" in k) return !k.has_one.foreign_key;
  if ("has_many" in k) return !k.has_many.foreign_key;
  const t = k.many_to_many.through;
  return !t.table || !t.left_key || !t.right_key;
}

/// Is this aggregate field missing its table, key, column, or result type?
/// Tolerates `undefined` — a just-removed node briefly re-renders against the
/// new, shorter tree (see `nodeFields`).
export function aggregateIncomplete(field: Field | undefined): boolean {
  if (!field) return false;
  const s = field.source;
  if (!("relation" in s) || !("aggregate" in s.relation)) return false;
  const agg: Aggregate = s.relation.aggregate;
  if (!agg.table || keyIncomplete(agg.key)) return true;
  const op = agg.op;
  if (typeof op === "string") return false; // count
  if ("ids" in op) return false;
  if ("avg" in op) return !op.avg;
  if ("sum" in op) return !op.sum || !agg.value_type;
  if ("min" in op) return !op.min || !agg.value_type;
  if ("max" in op) return !op.max || !agg.value_type;
  return false;
}
