// Types mirroring flusso's validated vocabulary as it serializes to JSON, plus
// the designer's API client. The shapes match `kernel::IndexSchema` and
// `schema_config_toml::ConfigToml` exactly — externally-tagged enums become
// `{ variant: payload }`, unit variants become bare strings — so what the UI
// builds round-trips through the Rust parser unchanged.

import { parseJson } from "./model/exact";
import { stripUids } from "./model/ids";

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

/** The `value_type`s a `sum`/`min`/`max` over an arithmetic expression accepts. */
export const NUMERIC_TYPES: FlussoType[] = ["short", "integer", "long", "float", "double", "decimal"];

export type Transform = "lowercase" | "trim";

export interface Column {
  column: string;
  ty: FlussoType;
  nullable: boolean;
  transforms?: Transform[];
  default?: unknown;
  // An enum's variants in rank order; omitted/empty for a bare enum or non-enum
  // column. When set, the field sorts by this order instead of alphabetically.
  enum_order?: string[];
  // `false` maps a text/keyword/uuid field without the sink's auto subfields;
  // omitted means on.
  subfields?: boolean;
  // `false` stores a json field without indexing it; omitted means indexed.
  indexed?: boolean;
}

/// The leaf types a script field may return.
export const SCRIPT_TYPES: FlussoType[] = [
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
];

/// A field computed by a Rhai script from the built document.
export interface Script {
  source: string;
  ty: FlussoType;
  nullable: boolean;
  // `true` returns an array of `ty`; omitted means a single value.
  array?: boolean;
  enum_order?: string[];
  subfields?: boolean;
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
  // Conditions on the junction rows; the relation's own `filters` apply to its target.
  filters?: Filter[];
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
  // `false` keeps the rows in the stored document without indexing them.
  indexed?: boolean;
  // Extra column equalities on the hop; not on `many_to_many`.
  on?: ColumnPair[];
}

/// One `on` pair: `local` on the table before the hop, `related` on the hop's table.
export interface ColumnPair {
  local: string;
  related: string;
}

export type AggregateOp =
  | "count"
  | "exists"
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
  // Over `through` only: count each target row once instead of once per junction row.
  distinct?: boolean;
  // The tables between the parent and `table`, in order from the parent.
  via?: ViaHop[];
  // Extra column equalities with the table before `table`; only with a direct key.
  on?: ColumnPair[];
}

/// One table an aggregate passes through: its `foreign_key` points at the table before it.
export interface ViaHop {
  table: string;
  foreign_key: string;
  filters?: Filter[];
  on?: ColumnPair[];
}

export type Relation = { join: Join } | { aggregate: Aggregate };

export type FieldSource =
  | { column: Column }
  | { group: Field[] }
  | { geo: Geo }
  | { relation: Relation }
  | { constant: unknown }
  | { script: Script };

export interface Field {
  field: string;
  options?: Record<string, unknown>;
  // `false` makes the field input-only: fetched for scripts, never emitted.
  emit?: boolean;
  source: FieldSource;
  /// Client-only stable id of the field, surviving re-indexing and renames: a
  /// container's canvas node id (collapse state, manual positions), and what
  /// the inspector and a typed value being read address the field by. Stamped
  /// by `model/ids.ts`, stripped before any request.
  uid?: string;
  /// Client-only: option keys the user set to the value a type adds (an
  /// `identifier`'s analyzer), so retyping away keeps them (`retypeField`).
  /// Stripped before any request.
  keptOptions?: string[];
}

// Mirrors kernel's serde exactly: `Filter` is an externally-tagged enum
// (`raw`/`null_check`/`value_op`) wrapping a struct, and `FilterValue` is itself
// tagged (`single`/`list`/`range`). The wire shape must match or the strict
// backend rejects it.
export type FilterOp = "eq" | "neq" | "lt" | "lte" | "gt" | "gte" | "in" | "not_in" | "like" | "ilike" | "between";
export type FilterValue = { single: string } | { list: string[] } | { range: [string, string] };
export type Filter =
  | { raw: { raw: string } }
  | { null_check: { column: string; op: "is_null" | "is_not_null" } }
  | { value_op: { column: string; op: FilterOp; value: FilterValue } };

// Externally-tagged enum over struct payloads (matches kernel's serde):
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
  /// Per-sink options for this index only, keyed by sink name
  /// (`[index.sinks.<name>]`); the shape is the sink adapter's `index_schema`.
  sinks?: Record<string, Record<string, unknown>>;
  /// A session-local, stable id used to correlate an index across renames and
  /// path changes when computing the save op set. Never written to disk —
  /// stripped from the config before any backend call.
  id?: string;
}

export interface ConfigToml {
  source: Record<string, unknown>;
  stream?: Record<string, unknown>;
  sinks?: Record<string, unknown>;
  index?: IndexEntry[];
  prefix?: string;
  on_error?: unknown;
  batch?: { max_changes?: number; max_delay_ms?: number };
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
  /// The text to write `schema` into, keeping its comments (the Code-mode
  /// buffer); absent, the file on disk is used.
  base?: string;
  /// Which field of the base each field of `schema` is, so a comment follows
  /// its field through a rename or a move.
  identity?: SchemaIdentity;
}

/// One field's name and client-only uid, with its child fields: what the save
/// pairs a field with its old self by (`model/ids.ts`).
export interface FieldId {
  name: string;
  uid: string;
  fields: FieldId[];
}

/// The fields of the model a base text reads as, and of the model being saved.
export interface SchemaIdentity {
  before: FieldId[];
  after: FieldId[];
}

/// The `[[index]]` names of the file on disk, and for each entry being saved
/// its position there (`null`: a new entry).
export interface ConfigIdentity {
  before: string[];
  after: (number | null)[];
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
  /// Set when the write could not keep the file's comments and formatting.
  notice?: "layout_reset";
  /// How many comments the write drops although what they describe still exists.
  dropped?: number;
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
  // `false` when the field is stored but not indexed (omitted otherwise).
  indexed?: boolean;
  children?: DocumentNode[];
}

export interface PreviewResponse {
  yaml: string;
  preview: { mapping: unknown; document: DocumentNode[] };
  /// Whether the generated YAML parses back to exactly this schema.
  parse_ok: boolean;
  parse_error?: string;
  /// The first field that reads back differently, when the YAML parses.
  differs_at?: string;
  // Every script field that does not compile.
  script_errors?: ScriptErrorDto[];
}

/// A script failure: a compile error, or one raised on a sample row.
export interface ScriptErrorDto {
  /// The field path, e.g. `variants[2].grossPrice` (`[]` in a compile error).
  field: string;
  message: string;
  line?: number;
  column?: number;
  excerpt?: string;
}

export interface DiagnosticDto {
  index: string;
  /// The last segment of `path`.
  field: string;
  /// The field's path from the document root, or an index-level key alone
  /// (`table`, `primary_key`, `filters`, `soft_delete`) when `anchor` is `index_key`.
  path: string[];
  /// What `path` names: a field, or an index-level key a root field may share a name with.
  anchor: "field" | "index_key";
  /// What the database check found, e.g. `unknown_column`; absent on client-side checks.
  kind?: string;
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
  // A script that failed on the sample; `document` is then the row as built.
  script_failure?: ScriptErrorDto;
  // What the scripts printed, as `field: text`.
  printed?: string[];
}

/// A value read with no answer by then fails, so its box says so and Save can
/// try it again instead of waiting on it for good. Below `PENDING_LIMIT_MS`.
const VALUE_TIMEOUT_MS = 3000;

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
  return parseJson(await res.text()) as T;
}

/// An aggregate input checked by the loader's own parser: the columns it
/// reads, or what is wrong and where.
export interface ExpressionCheck {
  columns: string[];
  error?: {
    /// A stable token naming the mistake, translated by the editor.
    code:
      | "empty"
      | "unexpected_character"
      | "incomplete_number"
      | "invalid_column"
      | "expected_operand"
      | "expected_operator"
      | "unclosed_parenthesis"
      | "unmatched_parenthesis"
      | "too_deep";
    /// 1-based character position.
    position: number;
    /// The offending character or column name, or the depth cap.
    detail?: string;
    /// The parser's own English message.
    message: string;
  };
}

/// Where a typed value goes; each has the schema parser's own rules (a
/// `default` takes only a scalar).
export type ValueSlot = "constant" | "default" | "option";

/// A value typed into the inspector, read the way its saved file reads it: the
/// `GenericValue` to store, or why the text was refused.
export interface ValueCheck {
  value?: unknown;
  /// How the saved file spells the value, when a number in it is written
  /// other than typed (`1e3` → `1000.0`).
  saved_as?: string;
  error?:
    | { code: "syntax"; position: number }
    | { code: "inexact"; number: string }
    | { code: "out_of_range"; number: string }
    | { code: "too_deep"; limit: number }
    | { code: "not_scalar" }
    | { code: "reads_back_differently" };
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

/// Drop the client-only field uids (`model/ids.ts`) from a file op's schema.
const stripOpUids = (op: FileOp): FileOp => (op.schema ? { ...op, schema: stripUids(op.schema) } : op);

/// Drop the session-local index ids before a config crosses to the backend —
/// they're a client-only correlation handle and the strict parser rejects
/// unknown fields. (`undefined` values are omitted by JSON.stringify.)
const stripIds = (config: ConfigToml): ConfigToml =>
  config.index ? { ...config, index: config.index.map((e) => ({ ...e, id: undefined })) } : config;

/// What one registered adapter declares about its options (mirrors
/// `kernel::AdapterDescription`): the designer renders its forms from these.
export interface AdapterDescription {
  port: "source" | "stream" | "sink";
  kind: string;
  /// A draft-07 JSON schema of the options table (`type` excluded).
  schema: Record<string, unknown>;
  /// A complete example of the options.
  example: Record<string, unknown>;
  /// `.`-joined paths of the options that take an override variable.
  secrets: string[];
  /// A draft-07 JSON schema of the options the adapter takes per index, under
  /// `[index.sinks.<name>]`; absent when it takes none.
  index_schema?: Record<string, unknown>;
}

export const api = {
  project: () => fetch("/api/project").then((r) => json<Project>(r)),
  /// The registered adapters and their option schemas.
  adapters: () => fetch("/api/adapters").then((r) => json<AdapterDescription[]>(r)),
  /// Parse a raw schema buffer into the validated model (Code mode's live sync).
  parse: (yaml: string) =>
    fetch("/api/parse", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ yaml }),
    }).then((r) => json<ParseResponse>(r)),
  /// Check an aggregate input (a column or an arithmetic expression).
  expression: (input: string) =>
    fetch("/api/expression", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ input }),
    }).then((r) => json<ExpressionCheck>(r)),
  /// Read a typed JSON value (a constant, a default, an option) as its saved file will.
  value: (text: string, slot: ValueSlot) =>
    fetch("/api/value", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ text, slot }),
      signal: AbortSignal.timeout(VALUE_TIMEOUT_MS),
    }).then((r) => json<ValueCheck>(r)),
  catalog: () => fetch("/api/catalog").then((r) => json<CatalogResponse>(r)),
  /// Relative subdirectories under the config dir, for the schema-folder picker.
  dirs: () => fetch("/api/dirs").then((r) => json<string[]>(r)),
  testConnection: (config: ConfigToml) =>
    fetch("/api/test-connection", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify(stripIds(config)),
    }).then((r) => json<CatalogResponse>(r)),
  /// `file` names the schema file (config-relative) or the Code-mode text the
  /// returned `yaml` is written into, so it shows what a save would write.
  preview: (
    index: string,
    schema: IndexSchema,
    file: { path?: string; base?: string; identity?: SchemaIdentity } = {},
  ) =>
    fetch("/api/preview", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ index, schema: stripUids(schema), ...file }),
    }).then((r) => json<PreviewResponse>(r)),
  validate: (config: ConfigToml, indexes: { name: string; schema: IndexSchema }[]) =>
    fetch("/api/validate", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({
        config: stripIds(config),
        indexes: indexes.map((i) => ({ ...i, schema: stripUids(i.schema) })),
      }),
    }).then((r) => json<ValidateResponse>(r)),
  sample: (config: ConfigToml, name: string, schema: IndexSchema, id?: string) =>
    fetch("/api/sample", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ config: stripIds(config), name, schema: stripUids(schema), id }),
    }).then((r) => json<SampleResponse>(r)),
  diff: (config: ConfigToml, ops: FileOp[], identity?: ConfigIdentity) =>
    fetch("/api/diff", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ config: stripIds(config), ops: ops.map(stripOpUids), identity }),
    }).then((r) => json<OpDiff[]>(r)),
  save: (config: ConfigToml, ops: FileOp[], skip: string[] = [], identity?: ConfigIdentity) =>
    fetch("/api/save", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ config: stripIds(config), ops: ops.map(stripOpUids), skip, identity }),
    }).then((r) => json<SaveResult>(r)),
};
