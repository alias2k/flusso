// Types mirroring flusso's validated vocabulary as it serializes to JSON, plus
// the designer's API client. The shapes match `schema_core::IndexSchema` and
// `schema_config_toml::ConfigToml` exactly — externally-tagged enums become
// `{ variant: payload }`, unit variants become bare strings — so what the UI
// builds round-trips through the Rust parser unchanged.

export type FlussoType =
  | "text"
  | "identifier"
  | "keyword"
  | "enum"
  | "uuid"
  | "boolean"
  | "short"
  | "integer"
  | "long"
  | "float"
  | "double"
  | "decimal"
  | "date"
  | "timestamp"
  | "binary"
  | "json"
  | "geo_point"
  | { map: { values: FlussoType } }
  | { custom: { postgres: string[]; opensearch: string } };

export const SCALAR_TYPES: FlussoType[] = [
  "text",
  "identifier",
  "keyword",
  "enum",
  "uuid",
  "boolean",
  "short",
  "integer",
  "long",
  "float",
  "double",
  "decimal",
  "date",
  "timestamp",
  "binary",
  "json",
];

export type Transform = "lowercase" | "trim";

export interface Column {
  column: string;
  ty: FlussoType;
  nullable: boolean;
  transforms?: Transform[];
  default?: unknown;
}

export interface Geo {
  lat: string;
  lon: string;
  nullable: boolean;
}

export interface Through {
  table: string;
  left_key: string;
  right_key: string;
}

export type JoinKind =
  | { belongs_to: { column: string } }
  | { has_one: { foreign_key: string } }
  | { has_many: { foreign_key: string } }
  | { many_to_many: { through: Through } };

export interface OrderBy {
  column: string;
  direction?: "asc" | "desc";
}

export interface Join {
  table: string;
  kind: JoinKind;
  primary_key: string;
  nullable: boolean;
  filters?: Filter[];
  order_by?: OrderBy[];
  limit?: number;
  fields: Field[];
}

export type AggregateOp =
  | "count"
  | { sum: string }
  | { avg: string }
  | { min: string }
  | { max: string }
  | { ids: { element_type: FlussoType } };

export type AggregateKey = { direct: string } | { through: Through };

export interface Aggregate {
  table: string;
  op: AggregateOp;
  key: AggregateKey;
  value_type?: FlussoType;
  filters?: Filter[];
}

export type Relation = { join: Join } | { aggregate: Aggregate };

export type FieldSource =
  | { column: Column }
  | { group: Field[] }
  | { geo: Geo }
  | { relation: Relation }
  | { constant: unknown };

export interface Field {
  field: string;
  options?: Record<string, unknown>;
  source: FieldSource;
}

export type Filter =
  | { raw: string }
  | { column: string; op: "is_null" | "is_not_null" }
  | { column: string; op: string; value: unknown };

export type SoftDelete =
  | { field: string; when?: Filter[] }
  | { column: string; when?: Filter[] };

export interface IndexSchema {
  version: number;
  table: string;
  db_schema: string;
  primary_key?: string;
  doc_id?: string;
  soft_delete?: SoftDelete;
  filters?: Filter[];
  fields: Field[];
}

// --- catalog ---

export interface ColumnShape {
  name: string;
  sql_type: string;
  nullable: boolean;
  is_primary_key: boolean;
  suggested_type?: FlussoType;
}

export interface ForeignKey {
  columns: string[];
  references_schema: string;
  references_table: string;
  references_columns: string[];
}

export interface TableShape {
  schema: string;
  name: string;
  columns: ColumnShape[];
  primary_key: string[];
  foreign_keys: ForeignKey[];
}

export interface JunctionCandidate {
  table: { schema: string; table: string };
  left: ForeignKey;
  right: ForeignKey;
}

export interface CatalogResponse {
  catalog: { tables: TableShape[] };
  junctions: JunctionCandidate[];
  error?: string;
}

// --- project / config (a loose view; we only edit the parts the UI exposes) ---

export interface IndexEntry {
  name: string;
  schema: string;
  enabled: boolean;
  on_error?: unknown;
}

export interface ConfigToml {
  source: Record<string, unknown>;
  sinks?: Record<string, unknown>;
  index?: IndexEntry[];
  prefix?: string;
  on_error?: unknown;
  server?: Record<string, unknown>;
}

export interface IndexFile {
  name: string;
  enabled: boolean;
  schema_path: string;
  schema?: IndexSchema;
  raw?: string;
  error?: string;
}

export interface FileDiff {
  path: string;
  current: string;
  next: string;
  changed: boolean;
}

export interface SaveSchemaInput {
  schema_path: string;
  schema: IndexSchema;
  raw?: string;
}

export interface Project {
  config_path: string;
  config: ConfigToml;
  indexes: IndexFile[];
}

export interface DocumentNode {
  name: string;
  type: string;
  nullable: boolean;
  array: boolean;
  children?: DocumentNode[];
}

export interface PreviewResponse {
  yaml: string;
  preview: { mapping: unknown; document: DocumentNode[] };
  parse_ok: boolean;
  parse_error?: string;
}

export interface DiagnosticDto {
  index: string;
  field: string;
  severity: string;
  message: string;
}

export interface ValidateResponse {
  diagnostics: DiagnosticDto[];
  db_reachable: boolean;
  error?: string;
}

async function json<T>(res: Response): Promise<T> {
  if (!res.ok) {
    const body = await res.text();
    // The server reports handler failures as `{ "error": "..." }`; surface that
    // message rather than the raw JSON/HTTP status.
    let message = body || `${res.status} ${res.statusText}`;
    try {
      const parsed = JSON.parse(body) as { error?: string };
      if (parsed.error) message = parsed.error;
    } catch {
      /* not JSON — use the text */
    }
    throw new Error(message);
  }
  return res.json() as Promise<T>;
}

export const api = {
  project: () => fetch("/api/project").then((r) => json<Project>(r)),
  catalog: () => fetch("/api/catalog").then((r) => json<CatalogResponse>(r)),
  testConnection: (config: ConfigToml) =>
    fetch("/api/test-connection", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify(config),
    }).then((r) => json<CatalogResponse>(r)),
  preview: (index: string, schema: IndexSchema) =>
    fetch("/api/preview", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ index, schema }),
    }).then((r) => json<PreviewResponse>(r)),
  validate: (config: ConfigToml, indexes: { name: string; schema: IndexSchema }[]) =>
    fetch("/api/validate", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ config, indexes }),
    }).then((r) => json<ValidateResponse>(r)),
  diff: (config: ConfigToml, indexes: SaveSchemaInput[]) =>
    fetch("/api/diff", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ config, indexes }),
    }).then((r) => json<FileDiff[]>(r)),
  save: (config: ConfigToml, indexes: SaveSchemaInput[]) =>
    fetch("/api/save", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ config, indexes }),
    }).then((r) => json<{ written: string[] }>(r)),
};
