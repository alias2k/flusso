// FK-guided relation suggestions: given the catalog and a node's table, what
// related tables can be pulled into the document, and as which verb.
//
//  - outgoing FK (this table → target)        → belongs_to
//  - incoming FK (another table → this table) → has_many (switch to has_one in the inspector)
//  - junction (two FKs, one to this table)    → many_to_many through it

import type { CatalogResponse, Field, TableShape } from "../api";

export interface RelationSuggestion {
  key: string;
  label: string;
  /// Human explanation of the FK this suggestion uses (shown as a tooltip).
  detail: string;
  verb: "belongs_to" | "has_one" | "has_many" | "many_to_many";
  build: () => Field;
}

const pkOf = (t: TableShape | undefined): string => t?.primary_key[0] ?? "id";

function table(catalog: CatalogResponse, name: string): TableShape | undefined {
  return catalog.catalog.tables.find((t) => t.name === name);
}

function join(field: string, body: Field["source"]): Field {
  return { field, source: body };
}

export function suggestRelations(catalog: CatalogResponse, fromTable: string): RelationSuggestion[] {
  const out: RelationSuggestion[] = [];
  const self = table(catalog, fromTable);

  // belongs_to — this table's outgoing foreign keys.
  for (const fk of self?.foreign_keys ?? []) {
    const target = fk.references_table;
    out.push({
      key: `bt:${target}:${fk.columns[0]}`,
      label: `${target} · belongs_to`,
      detail: `this table's ${fk.columns[0]} → ${target}.${fk.references_columns[0] ?? "id"} (single nested object)`,
      verb: "belongs_to",
      build: () =>
        join(target, {
          relation: {
            join: {
              table: target,
              kind: { belongs_to: { column: fk.columns[0] } },
              primary_key: fk.references_columns[0] ?? pkOf(table(catalog, target)),
              nullable: false,
              fields: [],
            },
          },
        }),
    });
  }

  // has_many — other tables whose FK points back at this one.
  for (const t of catalog.catalog.tables) {
    for (const fk of t.foreign_keys) {
      if (fk.references_table !== fromTable) continue;
      out.push({
        key: `hm:${t.name}:${fk.columns[0]}`,
        label: `${t.name} · has_many`,
        detail: `${t.name}.${fk.columns[0]} points back here (array of objects)`,
        verb: "has_many",
        build: () =>
          join(t.name, {
            relation: {
              join: {
                table: t.name,
                kind: { has_many: { foreign_key: fk.columns[0] } },
                primary_key: pkOf(t),
                nullable: false,
                fields: [],
              },
            },
          }),
      });
    }
  }

  // many_to_many — junction tables with a foreign key to this table.
  for (const j of catalog.junctions) {
    const sides = [j.left, j.right];
    const ours = sides.find((s) => s.references_table === fromTable);
    const other = sides.find((s) => s.references_table !== fromTable);
    if (!ours || !other) continue;
    out.push({
      key: `mm:${j.table.table}:${other.references_table}`,
      label: `${other.references_table} · many_to_many`,
      detail: `through the ${j.table.table} junction (array of objects)`,
      verb: "many_to_many",
      build: () =>
        join(other.references_table, {
          relation: {
            join: {
              table: other.references_table,
              kind: {
                many_to_many: {
                  through: {
                    table: j.table.table,
                    left_key: ours.columns[0],
                    right_key: other.columns[0],
                  },
                },
              },
              primary_key: pkOf(table(catalog, other.references_table)),
              nullable: false,
              fields: [],
            },
          },
        }),
    });
  }

  return out;
}
