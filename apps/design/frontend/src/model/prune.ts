// The live preview strictly deserializes the schema on the backend, where names
// are validated newtypes (ColumnName, …) that reject the empty string. So a
// field still being built — a blank order_by column, a just-added join with no
// key yet — would 400 the whole preview with a raw serde error. This prunes the
// incomplete pieces so the preview shows everything that *is* valid; it runs
// only for preview, the edited doc keeps the in-progress work untouched.

import type { Aggregate, ColumnPair, Field, Filter, IndexSchema, SoftDelete, Through } from "../api";
import { aggregateIncomplete, joinIncomplete } from "./complete";

const filterOk = (f: Filter): boolean =>
  "raw" in f ? !!f.raw.raw : "null_check" in f ? !!f.null_check.column : !!f.value_op.column;

const prunedFilters = (filters: Filter[] | undefined): Filter[] | undefined => {
  const kept = (filters ?? []).filter(filterOk);
  return kept.length ? kept : undefined;
};

/// Complete pairs, each once; `undefined` when none is left.
const prunedOn = (on: ColumnPair[] | undefined): ColumnPair[] | undefined => {
  const kept: ColumnPair[] = [];
  for (const pair of on ?? []) {
    const seen = kept.some((p) => p.local === pair.local && p.related === pair.related);
    if (pair.local && pair.related && !seen) kept.push(pair);
  }
  return kept.length ? kept : undefined;
};

/// The field with its unfinished `on` pairs dropped, so one being built hides
/// only itself from the preview, not the whole relation.
function withPrunedPairs(field: Field): Field {
  const s = field.source;
  if (!("relation" in s)) return field;
  if ("join" in s.relation) {
    const join = s.relation.join;
    return { ...field, source: { relation: { join: { ...join, on: prunedOn(join.on) } } } };
  }
  const agg = s.relation.aggregate;
  const via = agg.via?.map((hop) => ({ ...hop, on: prunedOn(hop.on) }));
  return { ...field, source: { relation: { aggregate: { ...agg, on: prunedOn(agg.on), via } } } };
}

const prunedThrough = (through: Through): Through => ({ ...through, filters: prunedFilters(through.filters) });

function prunedAggregate(agg: Aggregate): Aggregate {
  const key = "through" in agg.key ? { through: prunedThrough(agg.key.through) } : agg.key;
  const via = agg.via?.map((hop) => ({ ...hop, filters: prunedFilters(hop.filters) }));
  return { ...agg, key, filters: prunedFilters(agg.filters), via: via?.length ? via : undefined };
}

function prunedField(input: Field): Field | null {
  const field = withPrunedPairs(input);
  const s = field.source;
  if ("relation" in s) {
    if ("aggregate" in s.relation) {
      if (aggregateIncomplete(field)) return null;
      return { ...field, source: { relation: { aggregate: prunedAggregate(s.relation.aggregate) } } };
    }
    if ("join" in s.relation) {
      if (joinIncomplete(field)) return null;
      const join = s.relation.join;
      const fields = join.fields.map(prunedField).filter((x): x is Field => x !== null);
      const order_by = (join.order_by ?? []).filter((o) => o.column);
      const kind =
        "many_to_many" in join.kind
          ? { many_to_many: { through: prunedThrough(join.kind.many_to_many.through) } }
          : join.kind;
      return {
        ...field,
        source: {
          relation: {
            join: {
              ...join,
              kind,
              fields,
              order_by: order_by.length ? order_by : undefined,
              filters: prunedFilters(join.filters),
            },
          },
        },
      };
    }
  }
  if ("group" in s) {
    return { ...field, source: { group: s.group.map(prunedField).filter((x): x is Field => x !== null) } };
  }
  if ("column" in s && !s.column.column) return null;
  if ("script" in s && !s.script.source.trim()) return null;
  if ("geo" in s && (!s.geo.lat || !s.geo.lon)) return null;
  return field;
}

/// A copy of `schema` with incomplete fields/entries removed — safe to send to
/// the strict preview endpoint without a deserialization 400.
export function prunedForPreview(schema: IndexSchema): IndexSchema {
  const fields = schema.fields.map(prunedField).filter((x): x is Field => x !== null);
  let soft_delete: SoftDelete | undefined = schema.soft_delete;
  if (soft_delete && "column" in soft_delete && !soft_delete.column.column) soft_delete = undefined;
  if (soft_delete && "field" in soft_delete && !soft_delete.field.field) soft_delete = undefined;
  const filters = (schema.filters ?? []).filter(filterOk);
  return { ...schema, fields, soft_delete, filters: filters.length ? filters : undefined };
}
