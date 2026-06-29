import type { CatalogResponse, DiagnosticDto, Field, IndexSchema } from "../api";

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
  if (!catalog || catalog.error) return []; // can't know source nullability offline
  const sourceNullable = (table: string, column: string): boolean | undefined =>
    catalog.catalog.tables.find((t) => t.name === table)?.columns.find((c) => c.name === column)?.nullable;

  const issues: DiagnosticDto[] = [];
  const walk = (fields: Field[], table: string) => {
    for (const f of fields) {
      const s = f.source;
      if ("column" in s) {
        const col = s.column;
        const required = !col.nullable;
        if (required && sourceNullable(table, col.column) === true && col.default === undefined) {
          issues.push({
            index,
            field: f.field,
            severity: "error",
            message: `required, but its source column "${col.column}" is nullable — set a default value`,
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
