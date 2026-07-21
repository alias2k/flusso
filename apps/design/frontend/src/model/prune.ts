// The live preview strictly deserializes the schema on the backend, where names
// are validated newtypes (ColumnName, …) that reject the empty string. So a
// field still being built — a blank order_by column, a just-added join with no
// key yet — would 400 the whole preview with a raw serde error. This prunes the
// incomplete pieces so the preview shows everything that *is* valid; it runs
// only for preview, the edited doc keeps the in-progress work untouched.

import type { Field, Filter, IndexSchema, SoftDelete } from "../api";
import { aggregateIncomplete, joinIncomplete } from "./complete";

const filterOk = (f: Filter): boolean =>
  "raw" in f ? !!f.raw.raw : "null_check" in f ? !!f.null_check.column : !!f.value_op.column;

function prunedField(field: Field): Field | null {
  const s = field.source;
  if ("relation" in s) {
    if ("aggregate" in s.relation) return aggregateIncomplete(field) ? null : field;
    if ("join" in s.relation) {
      if (joinIncomplete(field)) return null;
      const join = s.relation.join;
      const fields = join.fields.map(prunedField).filter((x): x is Field => x !== null);
      const order_by = (join.order_by ?? []).filter((o) => o.column);
      const filters = (join.filters ?? []).filter(filterOk);
      return {
        ...field,
        source: {
          relation: {
            join: {
              ...join,
              fields,
              order_by: order_by.length ? order_by : undefined,
              filters: filters.length ? filters : undefined,
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
