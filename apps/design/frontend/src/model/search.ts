// Flattens the whole edited project into a flat list of searchable records for
// the global command palette: every index, every field (leaves + container
// nodes) across every schema, every deployment setting, and the database
// catalog. Each record carries a `detail` payload the palette's preview pane
// renders (breadcrumb, Postgres→OpenSearch type mapping, flags). Navigation
// targets are plain data (a discriminated union), so this stays a pure
// projection — the palette turns a `SearchTarget` into store calls.

import type { CatalogResponse } from "../api";
import type { Translate } from "../i18n";
import type { Doc } from "../store/design";
import { typeClass } from "../theme";
import { pathId, pathLabels, projectGraph } from "./tree";

export type SearchCategory = "action" | "index" | "field" | "setting" | "catalog";

export type SearchTarget =
  | { kind: "index"; name: string }
  | { kind: "field"; index: string; path: number[]; leaf: number }
  | { kind: "node"; index: string; path: number[] }
  | { kind: "config" }
  | { kind: "catalog" };

/// What the preview pane shows for the highlighted record.
export interface SearchDetail {
  /// Ancestor chain shown as a breadcrumb (index ▸ container…).
  crumb?: string[];
  /// Dot colour for the head (fields, by type/relation family).
  color?: string;
  /// A short label beside the head dot (the relation kind, e.g. `has_many`).
  headKind?: string;
  /// Source side of the type mapping card (`accounts.email · varchar`).
  source?: string;
  /// Target side of the mapping card (the resulting OpenSearch type).
  target?: string;
  /// Prose description (relations, settings, actions).
  body?: string;
  /// Small chips (nullability, counts).
  flags?: { text: string; ok?: boolean }[];
  /// A muted one-liner (root table, column count).
  meta?: string;
  /// What pressing Enter does, e.g. "Jump to field · pans the canvas".
  enter: string;
}

export interface SearchRecord {
  id: string;
  category: SearchCategory;
  title: string;
  /// The muted inline path shown on the row's right (index · table).
  subtitle?: string;
  /// Extra text folded into the fuzzy match (kind, column, table names).
  keywords: string;
  /// The index a record belongs to, so on-screen results can rank higher.
  index?: string;
  /// A CSS colour for the row's leading dot (fields, by type/relation family).
  color?: string;
  /// The field/relation kind, shown as the row's coloured chip.
  kind?: string;
  /// A keyboard shortcut shown on the row (actions).
  shortcut?: string;
  detail: SearchDetail;
  /// Where selecting this record navigates. Absent on actions, which `run`.
  target?: SearchTarget;
  /// An action's handler. Absent on navigable records.
  run?: () => void;
}

const NODE_KINDS = new Set(["object", "belongs_to", "has_one", "has_many", "many_to_many"]);

/// The palette colour for a field, matching the canvas palette: relations by
/// their kind hue, everything else by its type family.
function fieldColor(kind: string): string {
  return NODE_KINDS.has(kind) ? `var(--k-${kind})` : `var(--${typeClass(kind)})`;
}

/// Project the edited document + catalog into every navigable search record.
export function buildSearchRecords(doc: Doc, catalog: CatalogResponse | null, t: Translate): SearchRecord[] {
  const records: SearchRecord[] = [];
  const tables = catalog?.catalog.tables ?? [];
  const column = (table: string, name?: string) =>
    name ? tables.find((tb) => tb.name === table)?.columns.find((c) => c.name === name) : undefined;

  for (const entry of doc.config.index ?? []) {
    const schema = doc.schemas[entry.name];
    const table = schema?.table ?? "";
    const fieldCount = schema?.fields.length ?? 0;
    records.push({
      id: `index.${entry.name}`,
      category: "index",
      title: entry.name,
      subtitle: fieldCount ? `${table} · ${t("node.fields", { n: fieldCount })}` : table,
      keywords: `index ${table}`,
      index: entry.name,
      detail: {
        meta: `${t("inspector.rootTable")}: ${table || "—"}`,
        flags: fieldCount ? [{ text: t("node.fields", { n: fieldCount }) }] : undefined,
        enter: t("search.openIndex"),
      },
      target: { kind: "index", name: entry.name },
    });
  }

  for (const [name, schema] of Object.entries(doc.schemas)) {
    for (const node of projectGraph(schema).nodes) {
      if (node.path.length) {
        const crumb = [name, ...pathLabels(schema, node.path).slice(0, -1)];
        records.push({
          id: `node.${name}.${pathId(node.path)}`,
          category: "field",
          title: node.name ?? node.table,
          subtitle: `${name} · ${node.kind}`,
          keywords: `${node.kind} ${node.table} ${name}`,
          index: name,
          color: fieldColor(node.kind),
          kind: node.kind,
          detail: {
            crumb,
            color: fieldColor(node.kind),
            headKind: node.kind,
            body: t(`kindHelp.${node.kind}`),
            meta: `${node.table} · ${t("node.fields", { n: node.leaves.length })}`,
            enter: t("search.jumpNode"),
          },
          target: { kind: "node", index: name, path: node.path },
        });
      }
      for (const leaf of node.leaves) {
        const col = column(node.table, leaf.column);
        const source = leaf.column ? `${node.table}.${leaf.column}${col ? ` · ${col.sql_type}` : ""}` : undefined;
        records.push({
          id: `field.${name}.${pathId(node.path)}.${leaf.index}`,
          category: "field",
          title: leaf.name,
          subtitle: `${name} · ${node.table}`,
          keywords: `${leaf.kind} ${leaf.column ?? ""} ${node.table} ${name}`,
          index: name,
          color: fieldColor(leaf.kind),
          kind: leaf.kind,
          detail: {
            crumb: [name, ...pathLabels(schema, node.path)],
            color: fieldColor(leaf.kind),
            source,
            target: leaf.kind,
            flags: col
              ? [col.nullable ? { text: t("inspector.colNullable") } : { text: t("inspector.colNotNull"), ok: true }]
              : undefined,
            enter: t("search.jumpField"),
          },
          target: { kind: "field", index: name, path: node.path, leaf: leaf.index },
        });
      }
    }
  }

  const setting = (id: string, title: string, body: string, keywords: string): SearchRecord => ({
    id,
    category: "setting",
    title,
    keywords,
    detail: { body, enter: t("search.openSettings") },
    target: { kind: "config" },
  });
  records.push(setting("set.prefix", t("config.indexPrefix"), t("search.descPrefix"), "prefix name deployment"));
  records.push(
    setting(
      "set.connection",
      t("config.connection"),
      t("search.descConnection"),
      "connection database url host port user password source postgres dsn",
    ),
  );
  records.push(setting("set.onError", "on_error", t("search.descOnError"), "on error policy stop skip failure"));
  records.push(
    setting(
      "set.server",
      t("search.serverAddresses"),
      t("search.descServer"),
      "server public private address http port metrics status",
    ),
  );
  for (const sinkName of Object.keys(doc.config.sinks ?? {})) {
    records.push(
      setting(`set.sink.${sinkName}`, sinkName, t("search.descSink"), `sink ${sinkName} opensearch stdout output url`),
    );
  }

  for (const table of tables) {
    records.push({
      id: `table.${table.name}`,
      category: "catalog",
      title: table.name,
      subtitle: t("catalog.cols", { n: table.columns.length }),
      keywords: `table ${table.columns.map((c) => c.name).join(" ")}`,
      detail: {
        meta: `${table.schema} · ${t("catalog.cols", { n: table.columns.length })}`,
        flags: table.primary_key.length ? [{ text: `pk: ${table.primary_key.join(", ")}`, ok: true }] : undefined,
        enter: t("search.browseTables"),
      },
      target: { kind: "catalog" },
    });
    for (const col of table.columns) {
      records.push({
        id: `column.${table.name}.${col.name}`,
        category: "catalog",
        title: col.name,
        subtitle: `${table.name} · ${col.sql_type}`,
        keywords: `column ${col.sql_type} ${table.name}`,
        detail: {
          crumb: [table.name],
          source: `${table.name}.${col.name} · ${col.sql_type}`,
          flags: [col.nullable ? { text: t("inspector.colNullable") } : { text: t("inspector.colNotNull"), ok: true }],
          enter: t("search.browseTables"),
        },
        target: { kind: "catalog" },
      });
    }
  }

  return records;
}
