// Flattens the whole edited project into a flat list of searchable records for
// the global command palette: every index, every field (leaves + container
// nodes) across every schema, every deployment setting, and the database
// catalog. Navigation targets are plain data (a discriminated union), so this
// stays a pure projection — the palette turns a `SearchTarget` into store calls.

import type { CatalogResponse } from "../api";
import type { Translate } from "../i18n";
import type { Doc } from "../store/design";
import { typeClass } from "../theme";
import { pathId, projectGraph } from "./tree";

export type SearchCategory = "action" | "index" | "field" | "setting" | "catalog";

export type SearchTarget =
  | { kind: "index"; name: string }
  | { kind: "field"; index: string; path: number[]; leaf: number }
  | { kind: "node"; index: string; path: number[] }
  | { kind: "config" }
  | { kind: "catalog" };

export interface SearchRecord {
  id: string;
  category: SearchCategory;
  title: string;
  /// A muted second line (the index/table a field belongs to, a column's table).
  subtitle?: string;
  /// Extra text folded into the fuzzy match (kind, column, table names).
  keywords: string;
  /// The index a record belongs to, so on-screen results can rank higher.
  index?: string;
  /// A CSS colour for the leading dot (fields, by type/relation family).
  color?: string;
  /// Where selecting this record navigates. Absent on actions, which `run`.
  target?: SearchTarget;
  /// An action's handler. Absent on navigable records.
  run?: () => void;
}

const NODE_KINDS = new Set(["object", "belongs_to", "has_one", "has_many", "many_to_many"]);

/// The palette dot colour for a field, matching the canvas palette: relations by
/// their kind hue, everything else by its type family.
function fieldColor(kind: string): string {
  return NODE_KINDS.has(kind) ? `var(--k-${kind})` : `var(--${typeClass(kind)})`;
}

/// Project the edited document + catalog into every navigable search record.
export function buildSearchRecords(doc: Doc, catalog: CatalogResponse | null, t: Translate): SearchRecord[] {
  const records: SearchRecord[] = [];

  for (const entry of doc.config.index ?? []) {
    const table = doc.schemas[entry.name]?.table ?? "";
    records.push({
      id: `index.${entry.name}`,
      category: "index",
      title: entry.name,
      subtitle: table,
      keywords: `index ${table}`,
      index: entry.name,
      target: { kind: "index", name: entry.name },
    });
  }

  for (const [name, schema] of Object.entries(doc.schemas)) {
    for (const node of projectGraph(schema).nodes) {
      if (node.path.length) {
        records.push({
          id: `node.${name}.${pathId(node.path)}`,
          category: "field",
          title: node.name ?? node.table,
          subtitle: `${name} · ${node.kind}`,
          keywords: `${node.kind} ${node.table} ${name}`,
          index: name,
          color: fieldColor(node.kind),
          target: { kind: "node", index: name, path: node.path },
        });
      }
      for (const leaf of node.leaves) {
        records.push({
          id: `field.${name}.${pathId(node.path)}.${leaf.index}`,
          category: "field",
          title: leaf.name,
          subtitle: `${name} · ${node.table}`,
          keywords: `${leaf.kind} ${leaf.column ?? ""} ${node.table} ${name}`,
          index: name,
          color: fieldColor(leaf.kind),
          target: { kind: "field", index: name, path: node.path, leaf: leaf.index },
        });
      }
    }
  }

  const setting = (id: string, title: string, keywords: string): SearchRecord => ({
    id,
    category: "setting",
    title,
    keywords,
    target: { kind: "config" },
  });
  records.push(setting("set.prefix", t("config.indexPrefix"), "prefix name deployment"));
  records.push(
    setting(
      "set.connection",
      t("config.connection"),
      "connection database url host port user password source postgres dsn",
    ),
  );
  records.push(setting("set.onError", "on_error", "on error policy stop skip failure"));
  records.push(
    setting("set.server", t("search.serverAddresses"), "server public private address http port metrics status"),
  );
  for (const sinkName of Object.keys(doc.config.sinks ?? {})) {
    records.push(setting(`set.sink.${sinkName}`, sinkName, `sink ${sinkName} opensearch stdout output url`));
  }

  for (const table of catalog?.catalog.tables ?? []) {
    records.push({
      id: `table.${table.name}`,
      category: "catalog",
      title: table.name,
      subtitle: t("catalog.cols", { n: table.columns.length }),
      keywords: `table ${table.columns.map((c) => c.name).join(" ")}`,
      target: { kind: "catalog" },
    });
    for (const col of table.columns) {
      records.push({
        id: `column.${table.name}.${col.name}`,
        category: "catalog",
        title: col.name,
        subtitle: `${table.name} · ${col.sql_type}`,
        keywords: `column ${col.sql_type} ${table.name}`,
        target: { kind: "catalog" },
      });
    }
  }

  return records;
}
