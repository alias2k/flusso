import type { CatalogResponse, DiagnosticDto, Field, IndexSchema } from "../api";
import { drasticTypeChange } from "../theme";

/// Client-side schema checks that only need the catalog — no DB round-trip.
///
/// Today this enforces the source-guided nullability rule: a field marked
/// **required** whose source column is **nullable** must set a `default`,
/// otherwise the document field could be missing. (A field over a NOT NULL
/// column needs no default; the database guarantees a value.)
///
/// Returned as [`DiagnosticDto`]s keyed by field name so they flow through the
/// exact same highlight/tooltip/list surface as the database validation.
export function requiredDefaultIssues(
  schema: IndexSchema,
  catalog: CatalogResponse | null,
  index: string,
): DiagnosticDto[] {
  if (!catalog || catalog.error) return []; // can't know the source columns offline
  const sourceCol = (table: string, column: string) =>
    catalog.catalog.tables.find((t) => t.name === table)?.columns.find((c) => c.name === column);

  const issues: DiagnosticDto[] = [];
  const walk = (fields: Field[], table: string) => {
    for (const f of fields) {
      const s = f.source;
      if ("column" in s) {
        const col = s.column;
        const src = sourceCol(table, col.column);
        const required = !col.nullable;
        if (required && src?.nullable === true && col.default === undefined) {
          issues.push({
            index,
            field: f.field,
            severity: "error",
            message: `required, but its source column "${col.column}" is nullable — set a default value`,
          });
        }
        if (
          typeof col.ty === "string" &&
          typeof src?.suggested_type === "string" &&
          drasticTypeChange(src.suggested_type, col.ty)
        ) {
          issues.push({
            index,
            field: f.field,
            severity: "warning",
            message: `type "${col.ty}" is a sharp change from source column "${col.column}" (${src.sql_type}) — values may fail to index`,
          });
        }
      } else if ("group" in s) {
        walk(s.group, table); // a group stays on the same table
      } else if ("relation" in s && "join" in s.relation) {
        walk(s.relation.join.fields, s.relation.join.table);
      }
      // geo (two columns) and aggregates/constants have no single source column.
    }
  };
  walk(schema.fields, schema.table);
  return issues;
}
