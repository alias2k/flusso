import type { CatalogResponse, ColumnShape, DiagnosticDto, Field, IndexSchema } from "../api";
import type { Translate } from "../i18n";
import { drasticTypeChange } from "../theme";
import { childFields } from "./containers";
import { retypeField } from "./edit";
import { stripFieldUids } from "./ids";
import { trimmed } from "./names";

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
/// field-anchored [`DiagnosticDto`]s, by path, so they flow through the same
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
  const walk = (fields: Field[], table: string, parent: string[], parentPositions: (number | null)[]) => {
    for (const [i, f] of fields.entries()) {
      const s = f.source;
      const path = [...parent, f.field];
      const shared = fields.filter((other) => trimmed(other.field) === trimmed(f.field)).length > 1;
      const positions = [...parentPositions, shared ? i + 1 : null];
      if ("column" in s) {
        const col = s.column;
        const src = sourceColOf(catalog, table, col.column);
        if (!col.nullable && src?.nullable === true && col.default === undefined) {
          issues.push({
            index,
            field: f.field,
            path,
            positions,
            anchor: "field",
            severity: "error",
            message: t("diag.requiredDefault", { col: col.column }),
          });
        }
        if (columnMismatch(col, src)) {
          issues.push({
            index,
            field: f.field,
            path,
            positions,
            anchor: "field",
            severity: "warning",
            message: t("diag.typeMismatch", { ty: col.ty as string, col: col.column, src: src?.sql_type ?? "" }),
          });
        }
      } else if ("group" in s) {
        walk(s.group, table, path, positions); // a group stays on the same table
      } else if ("relation" in s && "join" in s.relation) {
        walk(s.relation.join.fields, s.relation.join.table, path, positions);
      }
      // geo (two columns) and aggregates/constants have no single source column.
    }
  };
  walk(schema.fields, schema.table, [], []);
  return issues;
}

/// The server's kind for two fields sharing a name at one level. The designer
/// computes it itself (`duplicateNameIssues`), with or without a database, so a
/// validation's own copy is dropped.
export const DUPLICATE_FIELD_NAME = "duplicate_field_name";

/// One warning per name two or more fields share at one level, as flusso reads
/// field names (trimmed), at any depth: at most one reaches the index. A
/// file written by hand may hold such a schema; the Inspector refuses to make one.
/// Its words are true whatever the fields are; `exactDuplicateIssues` swaps in
/// the server's, which say what the index keeps.
export function duplicateNameIssues(schema: IndexSchema, index: string, t: Translate): DiagnosticDto[] {
  const issues: DiagnosticDto[] = [];
  const walk = (fields: Field[], parent: string[], parentPositions: (number | null)[]) => {
    const counts = new Map<string, number>();
    for (const f of fields) counts.set(trimmed(f.field), (counts.get(trimmed(f.field)) ?? 0) + 1);
    for (const [name, n] of counts) {
      if (n < 2 || !name) continue;
      issues.push({
        index,
        field: name,
        path: [...parent, name],
        positions: [...parentPositions, null],
        anchor: "field",
        kind: DUPLICATE_FIELD_NAME,
        severity: "warning",
        message: t("diag.duplicateName", { name, n }),
      });
    }
    fields.forEach((f, i) => {
      const children = childFields(f);
      const name = trimmed(f.field);
      if (children) walk(children, [...parent, name], [...parentPositions, (counts.get(name) ?? 0) > 1 ? i + 1 : null]);
    });
  };
  walk(schema.fields, [], []);
  return issues;
}

/// The fields a duplicate-name diagnostic at `path` is about, as `schema` has
/// them: each one's position and content. `null` when the path leads nowhere.
/// Two schemas giving the same signature give the same message.
export function duplicateSignature(
  schema: IndexSchema,
  path: string[],
  positions: (number | null)[] | undefined,
): string | null {
  let fields = schema.fields;
  for (const [i, name] of path.slice(0, -1).entries()) {
    const position = positions?.[i] ?? null;
    const field = position === null ? fields.find((f) => trimmed(f.field) === name) : fields[position - 1];
    if (!field || trimmed(field.field) !== name) return null;
    fields = childFields(field) ?? [];
  }
  const name = path[path.length - 1];
  const same = fields.flatMap((f, i) => (trimmed(f.field) === name ? [[i, stripFieldUids([f])[0]] as const] : []));
  return JSON.stringify(same);
}

/// `duplicates` (the client's) with the server's words where the server
/// described these very fields: `exact` came from a check of `checked`, and a
/// message is taken only while the fields it names are the same in `schema`.
export function exactDuplicateIssues(
  duplicates: DiagnosticDto[],
  exact: DiagnosticDto[],
  checked: IndexSchema | undefined,
  schema: IndexSchema,
): DiagnosticDto[] {
  const samePath = (a: string[], b: string[]) => a.length === b.length && a.every((n, i) => n === b[i]);
  return duplicates.map((d) => {
    const told =
      checked &&
      exact.find(
        (s) =>
          s.kind === DUPLICATE_FIELD_NAME &&
          s.index === d.index &&
          samePath(s.path, d.path) &&
          samePath((s.positions ?? []).map(String), (d.positions ?? []).map(String)) &&
          duplicateSignature(checked, s.path, s.positions) === duplicateSignature(schema, s.path, s.positions),
      );
    return told ? { ...d, message: told.message } : d;
  });
}

/// A diagnostic's path as `check` prints it: a segment another field at its
/// level shares carries its position (`orders#2.total`).
export const diagnosticPath = (d: DiagnosticDto): string =>
  d.path.map((name, i) => (d.positions?.[i] ? `${name}#${d.positions[i]}` : name)).join(".");

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
        return retypeField(f, col, src.suggested_type);
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
