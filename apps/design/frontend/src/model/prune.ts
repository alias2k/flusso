// The live preview strictly deserializes the schema on the backend, where names
// are validated newtypes (FieldName, ColumnName, …) that reject the empty string.
// So a field still being built — a cleared name, a blank order_by column, a
// just-added join with no key yet — would 422 the whole preview with a raw serde
// error. `pruneIncomplete` copies the schema without those pieces, so the preview
// shows everything that *is* valid, and lists what it left out.
//
// The copy is for the preview only. It is lossy, so nothing may feed it back into
// the document: Code mode refuses to open while `left` is non-empty (App.tsx).

import type { Aggregate, ColumnPair, Field, Filter, IndexSchema, SoftDelete, Through } from "../api";
import { fieldIncomplete, nameInvalid } from "./complete";

/// What a pruning pass collects: the readable path of every piece it dropped.
type Left = string[];

const at = (path: string, key: string): string => (path ? `${path}.${key}` : key);

/// A field's name for a path, or its 1-based position when the name is blank.
const label = (field: Field, i: number): string => field.field.trim() || `#${i + 1}`;

const filterOk = (f: Filter): boolean =>
  "raw" in f ? !!f.raw.raw : "null_check" in f ? !!f.null_check.column : !!f.value_op.column;

function prunedFilters(filters: Filter[] | undefined, where: string, left: Left): Filter[] | undefined {
  const kept = (filters ?? []).filter(filterOk);
  if (kept.length < (filters ?? []).length) left.push(where);
  return kept.length ? kept : undefined;
}

/// Complete pairs, each once; `undefined` when none is left.
function prunedOn(on: ColumnPair[] | undefined, where: string, left: Left): ColumnPair[] | undefined {
  const kept: ColumnPair[] = [];
  for (const pair of on ?? []) {
    const seen = kept.some((p) => p.local === pair.local && p.related === pair.related);
    if (pair.local && pair.related && !seen) kept.push(pair);
  }
  if (kept.length < (on ?? []).length) left.push(where);
  return kept.length ? kept : undefined;
}

/// The field with its unfinished `on` pairs dropped, so one being built hides
/// only itself from the preview, not the whole relation.
function withPrunedPairs(field: Field, path: string, left: Left): Field {
  const s = field.source;
  if (!("relation" in s)) return field;
  if ("join" in s.relation) {
    const join = s.relation.join;
    return { ...field, source: { relation: { join: { ...join, on: prunedOn(join.on, at(path, "on"), left) } } } };
  }
  const agg = s.relation.aggregate;
  const via = agg.via?.map((hop, i) => ({ ...hop, on: prunedOn(hop.on, at(path, `via.${i + 1}.on`), left) }));
  return {
    ...field,
    source: { relation: { aggregate: { ...agg, on: prunedOn(agg.on, at(path, "on"), left), via } } },
  };
}

const prunedThrough = (through: Through, path: string, left: Left): Through => ({
  ...through,
  filters: prunedFilters(through.filters, at(path, "through.filters"), left),
});

function prunedAggregate(agg: Aggregate, path: string, left: Left): Aggregate {
  const key = "through" in agg.key ? { through: prunedThrough(agg.key.through, path, left) } : agg.key;
  const via = agg.via?.map((hop, i) => ({
    ...hop,
    filters: prunedFilters(hop.filters, at(path, `via.${i + 1}.filters`), left),
  }));
  return {
    ...agg,
    key,
    filters: prunedFilters(agg.filters, at(path, "filters"), left),
    via: via?.length ? via : undefined,
  };
}

function prunedFields(fields: Field[], path: string, left: Left): Field[] {
  return fields
    .map((field, i) => prunedField(field, at(path, label(field, i)), left))
    .filter((x): x is Field => x !== null);
}

function prunedField(input: Field, path: string, left: Left): Field | null {
  if (fieldIncomplete(withPrunedPairs(input, path, []))) {
    left.push(path);
    return null;
  }
  const field = withPrunedPairs(input, path, left);
  const s = field.source;
  if ("relation" in s) {
    if ("aggregate" in s.relation) {
      return { ...field, source: { relation: { aggregate: prunedAggregate(s.relation.aggregate, path, left) } } };
    }
    if ("join" in s.relation) {
      const join = s.relation.join;
      const order_by = (join.order_by ?? []).filter((o) => o.column);
      if (order_by.length < (join.order_by ?? []).length) left.push(at(path, "order_by"));
      const kind =
        "many_to_many" in join.kind
          ? { many_to_many: { through: prunedThrough(join.kind.many_to_many.through, path, left) } }
          : join.kind;
      return {
        ...field,
        source: {
          relation: {
            join: {
              ...join,
              kind,
              fields: prunedFields(join.fields, path, left),
              order_by: order_by.length ? order_by : undefined,
              filters: prunedFilters(join.filters, at(path, "filters"), left),
            },
          },
        },
      };
    }
  }
  if ("group" in s) return { ...field, source: { group: prunedFields(s.group, path, left) } };
  return field;
}

function prunedSoftDelete(soft: SoftDelete | undefined, left: Left): SoftDelete | undefined {
  if (!soft) return undefined;
  const marker = "column" in soft ? soft.column.column : soft.field.field;
  if ("column" in soft ? !marker : nameInvalid(marker)) {
    left.push("soft_delete");
    return undefined;
  }
  return "column" in soft
    ? { column: { ...soft.column, when: prunedFilters(soft.column.when, "soft_delete.when", left) } }
    : { field: { ...soft.field, when: prunedFilters(soft.field.when, "soft_delete.when", left) } };
}

/// A copy of `schema` without its incomplete fields and entries, safe to send to
/// the strict preview endpoint, plus the path of each piece it left out (a field
/// with a blank name shows as its position, `#3`).
export function pruneIncomplete(schema: IndexSchema): { schema: IndexSchema; left: string[] } {
  const left: Left = [];
  const fields = prunedFields(schema.fields, "", left);
  const soft_delete = prunedSoftDelete(schema.soft_delete, left);
  const filters = prunedFilters(schema.filters, "filters", left);
  return { schema: { ...schema, fields, soft_delete, filters }, left };
}
