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
  { column: Column } | { group: Field[] } | { geo: Geo } | { relation: Relation } | { constant: unknown };

export interface Field {
  field: string;
  options?: Record<string, unknown>;
  source: FieldSource;
}

// Mirrors schema-core's serde exactly: `Filter` is an externally-tagged enum
// (`raw`/`null_check`/`value_op`) wrapping a struct, and `FilterValue` is itself
// tagged (`single`/`list`/`range`). The wire shape must match or the strict
// backend rejects it.
export type FilterOp = "eq" | "neq" | "lt" | "lte" | "gt" | "gte" | "in" | "not_in" | "like" | "ilike" | "between";
export type FilterValue = { single: string } | { list: string[] } | { range: [string, string] };
export type Filter =
  | { raw: { raw: string } }
  | { null_check: { column: string; op: "is_null" | "is_not_null" } }
  | { value_op: { column: string; op: FilterOp; value: FilterValue } };

// Externally-tagged enum over struct payloads (matches schema-core's serde):
// `{ field: { field, when? } }` or `{ column: { column, when? } }`.
export interface SoftDeleteField {
  field: string;
  when?: Filter[];
}
export interface SoftDeleteColumn {
  column: string;
  when?: Filter[];
}
export type SoftDelete = { field: SoftDeleteField } | { column: SoftDeleteColumn };

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
  /// A session-local, stable id used to correlate an index across renames and
  /// path changes when computing the save op set. Never written to disk —
  /// stripped from the config before any backend call.
  id?: string;
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

/// A schema-file operation the client computed from saved -> current, sent to
/// diff/save. The backend just applies it (stateless).
export type OpKind = "upsert" | "move" | "delete";

export interface FileOp {
  kind: OpKind;
  /// Destination (upsert/move) or target (delete), relative to flusso.toml.
  path: string;
  /// Source path for a move, relative to flusso.toml.
  from?: string;
  /// Content for upsert/move (raw wins over schema).
  schema?: IndexSchema;
  raw?: string;
}

/// One op resolved against disk, for the review. `write` covers create + modify.
export type DiffOp = "write" | "move" | "delete";

export interface OpDiff {
  op: DiffOp;
  path: string;
  from?: string;
  current: string;
  next: string;
  changed: boolean;
  /// A stable warning code (e.g. "outside_base"); the UI translates it.
  warning?: string;
}

export interface MovedFile {
  from: string;
  to: string;
}

/// What a save did on disk.
export interface SaveResult {
  written: string[];
  moved: MovedFile[];
  deleted: string[];
  pruned: string[];
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

export interface SampleResponse {
  document?: unknown;
  synthetic: boolean;
  db_reachable: boolean;
  note?: string;
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

export interface ParseResponse {
  schema?: IndexSchema;
  error?: string;
  /// 1-based position, present when the parser's location is trustworthy.
  location?: { line: number; column: number };
  /// The document field a field-scoped error names, with its type tag.
  field?: string;
  type_tag?: string;
}

/// Drop the session-local index ids before a config crosses to the backend —
/// they're a client-only correlation handle and the strict parser rejects
/// unknown fields. (`undefined` values are omitted by JSON.stringify.)
const stripIds = (config: ConfigToml): ConfigToml =>
  config.index ? { ...config, index: config.index.map((e) => ({ ...e, id: undefined })) } : config;

export const api = {
  project: () => fetch("/api/project").then((r) => json<Project>(r)),
  /// Parse a raw schema buffer into the validated model (Code mode's live sync).
  parse: (yaml: string) =>
    fetch("/api/parse", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ yaml }),
    }).then((r) => json<ParseResponse>(r)),
  catalog: () => fetch("/api/catalog").then((r) => json<CatalogResponse>(r)),
  /// Relative subdirectories under the config dir, for the schema-folder picker.
  dirs: () => fetch("/api/dirs").then((r) => json<string[]>(r)),
  testConnection: (config: ConfigToml) =>
    fetch("/api/test-connection", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify(stripIds(config)),
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
      body: JSON.stringify({ config: stripIds(config), indexes }),
    }).then((r) => json<ValidateResponse>(r)),
  sample: (config: ConfigToml, name: string, schema: IndexSchema) =>
    fetch("/api/sample", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ config: stripIds(config), name, schema }),
    }).then((r) => json<SampleResponse>(r)),
  diff: (config: ConfigToml, ops: FileOp[]) =>
    fetch("/api/diff", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ config: stripIds(config), ops }),
    }).then((r) => json<OpDiff[]>(r)),
  save: (config: ConfigToml, ops: FileOp[], skip: string[] = []) =>
    fetch("/api/save", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ config: stripIds(config), ops, skip }),
    }).then((r) => json<SaveResult>(r)),
};
