import type { CatalogResponse, ColumnShape, DiagnosticDto, Field, IndexSchema } from "../api";
import type { Translate } from "../i18n";
import { drasticTypeChange } from "../theme";

const sourceColOf = (catalog: CatalogResponse, table: string, column: string): ColumnShape | undefined =>
  catalog.catalog.tables.find((t) => t.name === table)?.columns.find((c) => c.name === column);

/// Whether a column field maps its source to a sharply different type
/// (e.g. a timestamp column to uuid) — the source's natural type can't satisfy
/// the chosen one. `undefined` when the source column isn't known.
function columnMismatch(col: { column: string; ty: unknown }, src: ColumnShape | undefined): boolean {
  return (
    typeof col.ty === "string" &&
    typeof src?.suggested_type === "string" &&
    drasticTypeChange(src.suggested_type, col.ty)
  );
}

/// Client-side schema checks that only need the catalog — no DB round-trip.
///
/// Enforces the source-guided nullability rule (a **required** field over a
/// **nullable** column must set a `default`) and flags a **drastic type change**
/// (a column mapped to a type its values can't satisfy). Returned as
/// [`DiagnosticDto`]s keyed by field name so they flow through the same
/// highlight/tooltip/list surface as the database validation. Messages are
/// translated through `t`.
export function requiredDefaultIssues(
  schema: IndexSchema,
  catalog: CatalogResponse | null,
  index: string,
  t: Translate,
): DiagnosticDto[] {
  if (!catalog || catalog.error) return []; // can't know the source columns offline

  const issues: DiagnosticDto[] = [];
  const walk = (fields: Field[], table: string) => {
    for (const f of fields) {
      const s = f.source;
      if ("column" in s) {
        const col = s.column;
        const src = sourceColOf(catalog, table, col.column);
        if (!col.nullable && src?.nullable === true && col.default === undefined) {
          issues.push({
            index,
            field: f.field,
            severity: "error",
            message: t("diag.requiredDefault", { col: col.column }),
          });
        }
        if (columnMismatch(col, src)) {
          issues.push({
            index,
            field: f.field,
            severity: "warning",
            message: t("diag.typeMismatch", { ty: col.ty as string, col: col.column, src: src?.sql_type ?? "" }),
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

/// How many column fields in `schema` map their source to a sharply different
/// type (drives the "fix all / ignore" banner).
export function countTypeMismatches(schema: IndexSchema, catalog: CatalogResponse | null): number {
  if (!catalog || catalog.error) return 0;
  let n = 0;
  const walk = (fields: Field[], table: string) => {
    for (const f of fields) {
      const s = f.source;
      if ("column" in s) {
        if (columnMismatch(s.column, sourceColOf(catalog, table, s.column.column))) n++;
      } else if ("group" in s) {
        walk(s.group, table);
      } else if ("relation" in s && "join" in s.relation) {
        walk(s.relation.join.fields, s.relation.join.table);
      }
    }
  };
  walk(schema.fields, schema.table);
  return n;
}

/// Reset every drastically-typed column field to its source's suggested type —
/// the bulk "fix all" the banner offers.
export function fixAllTypes(schema: IndexSchema, catalog: CatalogResponse | null): IndexSchema {
  if (!catalog || catalog.error) return schema;
  const fix = (f: Field, table: string): Field => {
    const s = f.source;
    if ("column" in s) {
      const col = s.column;
      const src = sourceColOf(catalog, table, col.column);
      if (columnMismatch(col, src) && typeof src?.suggested_type === "string") {
        return { ...f, source: { column: { ...col, ty: src.suggested_type } } };
      }
      return f;
    }
    if ("group" in s) return { ...f, source: { group: s.group.map((g) => fix(g, table)) } };
    if ("relation" in s && "join" in s.relation) {
      const join = s.relation.join;
      return { ...f, source: { relation: { join: { ...join, fields: join.fields.map((g) => fix(g, join.table)) } } } };
    }
    return f;
  };
  return { ...schema, fields: schema.fields.map((f) => fix(f, schema.table)) };
}
