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
  // `true` reads a column holding an array of `ty` (`text[]`, a json array);
  // omitted means a single value. Its `default` is then a list.
  array?: boolean;
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
  /// A session-local, stable id per sink, keyed by the sink's name: a save
  /// tells a renamed sink from a removed one by it. Never written to disk.
  sinkIds?: Record<string, string>;
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
export type OpKind = "upsert" | "move";

export interface FileOp {
  kind: OpKind;
  /// The index whose schema file this is; `path` must be its schema path.
  index: string;
  /// Destination, relative to flusso.toml.
  path: string;
  /// Source path for a move, relative to flusso.toml.
  from?: string;
  /// Content for upsert/move.
  schema?: IndexSchema;
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
/// its position there (`null`: a new entry); for each sink being saved, by
/// name, its name on disk (`null`: a new sink).
export interface ConfigIdentity {
  before: string[];
  after: (number | null)[];
  sinks?: Record<string, string | null>;
}

/// One op resolved against disk, for the review. `write` covers create + modify.
export type DiffOp = "write" | "move";

export interface OpDiff {
  op: DiffOp;
  /// The index whose schema file this is; absent for the config.
  index?: string;
  path: string;
  from?: string;
  current: string;
  next: string;
  changed: boolean;
  /// A stable warning code (e.g. "outside_base"); the UI translates it.
  warning?: string;
  /// The op only fits the new flusso.toml (a new, renamed or moved index): it
  /// is saved together with the config or not at all.
  needs_config?: boolean;
  /// Set when the write could not keep the file's comments and formatting.
  notice?: "layout_reset";
  /// How many comments the write drops although what they describe still exists.
  dropped?: number;
  /// The files this entry writes or removes that changed on disk since the page
  /// loaded them; for flusso.toml, set whether or not it is written (every save
  /// is checked against it). A save refuses them.
  stale?: StaleFile[];
}

/// Per file, relative to flusso.toml's folder, the SHA-256 of its bytes as the
/// page loaded or last saved it; `null` for a file that did not exist.
export type Digests = Record<string, string | null>;

/// A file that is not as the page loaded it.
export interface StaleFile {
  path: string;
  change: "changed" | "deleted" | "created" | "unloaded";
  /// The digest of the file on disk now: sent as its digest, a save overwrites it anyway.
  on_disk: string | null;
}

export interface MovedFile {
  from: string;
  to: string;
}

/// What a save did on disk.
export interface SaveResult {
  written: string[];
  moved: MovedFile[];
  pruned: string[];
  /// Interrupted saves rolled back before this one.
  recovered?: RecoveredSave[];
  /// The digest of each file the save wrote (`null`: removed).
  digests: Digests;
  /// Files another program wrote in the instant the save replaced them.
  displaced?: DisplacedFile[];
}

/// A file another program wrote in place in the instant a save replaced it:
/// it holds the save's text, and the program's bytes are in `copy`.
export interface DisplacedFile {
  path: string;
  copy: string;
}

/// What a diff found, and the interrupted saves it rolled back first.
export interface Review {
  diffs: OpDiff[];
  recovered?: RecoveredSave[];
}

export interface Project {
  config_path: string;
  config: ConfigToml;
  indexes: IndexFile[];
  /// What the server found of an earlier, interrupted save; absent when nothing.
  recovery?: SaveRecovery;
  /// The digest of each file the project was read from; sent back with every diff and save.
  digests: Digests;
}

/// A save a killed designer left half done, settled before the project was read.
export interface RecoveredSave {
  /// Which save it was: the page shows each once.
  id: string;
  outcome: "rolled_back" | "completed";
  /// When the save started, in milliseconds since the Unix epoch.
  started_at_ms: number;
  /// The files it changed, relative to flusso.toml.
  files: string[];
  /// Files another program wrote or deleted during or after the save, left as
  /// they are, with the save's backup of the text before when it kept one.
  kept?: { path: string; backup?: string; deleted?: boolean }[];
  /// Files whose backup holds what another program wrote as the save replaced them.
  displaced?: DisplacedFile[];
  /// The notice could not be stored: it is shown only until the designer stops.
  unsaved?: boolean;
}

export type BlockedProblem = "backup_missing" | "unreadable" | "journal_unusable" | "failed";

/// Why the server does not act on a save's journal.
export type JournalCause =
  "no_record" | "other_folder" | "no_state" | "too_large" | "unreadable" | "malformed" | "not_a_save" | "conflict";

/// What a file a journal names holds now.
export type FileHolds = "before" | "after" | "neither" | "unchecked" | "outside";

/// One step by hand that brings a file to one side of a save.
export type FileFix = { do: "nothing" } | { do: "copy"; from: string } | { do: "delete" } | { do: "unavailable" };

/// One step by hand on the blocked page, paths relative to flusso.toml.
export type HandStep =
  { do: "copy"; from: string; to: string } | { do: "delete"; path: string } | { do: "remove_dir"; path: string };

/// A journal the server does not act on, what each file it names holds now,
/// and how to settle it by hand.
export interface JournalReport {
  cause: JournalCause;
  /// The journal files found, each to be deleted.
  journals: string[];
  /// For a journal this designer wrote while saving another folder: that folder.
  recorded_for?: string;
  files: {
    path: string;
    /// Outside the project: never looked at.
    outside?: boolean;
    holds: FileHolds;
    backup?: string;
    temp?: string;
    back: FileFix;
    forward: FileFix;
  }[];
  /// How many more files it names, not listed.
  more?: number;
  /// Every step that takes the project back to before the save, the journal last.
  back_route?: HandStep[];
  /// Every step that takes the project forward to after the save, the journal last.
  forward_route?: HandStep[];
  /// The save's temp files and backups on disk, whatever they hold.
  leftovers: string[];
}

/// An interrupted save the server could not settle: nothing was changed.
export interface BlockedSave {
  /// Another designer is saving this project right now.
  busy: boolean;
  problems: {
    path: string;
    problem: BlockedProblem;
    detail?: string;
    /// The backup the save keeps of the file, whatever it holds.
    backup?: string;
    /// The step that gives the file a text recovery accepts; absent when no copy is left.
    fix?: HandStep;
    /// A file that holds the file's text from before the save right now.
    before_copy?: string;
  }[];
  /// The journal, temp files and backups still on disk.
  leftovers: string[];
  /// For a journal the server does not act on: what the files it names hold.
  report?: JournalReport;
  /// For a save of this designer's: which way recovery was taking it.
  direction?: "back" | "forward";
  /// The steps that keep the changed files as they are and settle the rest, the journal last.
  keep?: HandStep[];
}

export interface SaveRecovery {
  recovered: RecoveredSave[];
  blocked?: BlockedSave;
  /// Temp files and backups no journal names: reported, left alone.
  leftovers: string[];
  /// Files a save of this page found written by another program as it replaced them.
  displaced?: DisplacedFile[];
  /// Copies earlier saves kept, whose notice was dismissed, still on disk.
  copies?: KeptCopy[];
}

/// A copy a save kept beside a file, still on disk after its notice was dismissed.
export interface KeptCopy {
  copy: string;
  path: string;
  /// `before`: the file's text from before a save rolled back without it;
  /// `displaced`: what another program wrote as a save replaced the file.
  holds: "before" | "displaced";
  started_at_ms: number;
}

/// A save the server refused because files it writes changed on disk since the
/// page loaded them (a 409): nothing was written.
export class StaleError extends Error {
  constructor(
    message: string,
    readonly files: StaleFile[],
  ) {
    super(message);
  }
}

/// A diff or save the server does not read because this page comes from
/// another version of the designer (a 400 asking for a reload).
export class ReloadError extends Error {}

/// A load, diff or save the server refused over an interrupted save (a 409).
export class RecoveryError extends Error {
  constructor(
    message: string,
    readonly recovery: SaveRecovery,
  ) {
    super(message);
  }
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
  /// What `flusso check --offline` finds in this schema, such as two fields sharing a name.
  schema_diagnostics?: DiagnosticDto[];
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
  /// For each segment of `path`, its 1-based position at its level when another field there has its name.
  positions?: (number | null)[];
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
    let recovery: SaveRecovery | undefined;
    let stale: StaleFile[] | undefined;
    let reload = false;
    try {
      const parsed = JSON.parse(body) as {
        error?: string;
        recovery?: SaveRecovery;
        stale?: StaleFile[];
        reload?: boolean;
      };
      if (parsed.error) message = parsed.error;
      recovery = parsed.recovery;
      stale = parsed.stale;
      reload = parsed.reload === true;
    } catch {
      /* not JSON — use the text */
    }
    if (reload) throw new ReloadError(message);
    if (stale) throw new StaleError(message, stale);
    throw recovery ? new RecoveryError(message, recovery) : new Error(message);
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
/// `array_default` lists scalars, `json_array_default` any non-null JSON.
export type ValueSlot = "constant" | "default" | "array_default" | "json_array_default" | "option";

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
    | { code: "not_a_list" }
    | { code: "null_element" }
    | { code: "not_scalar_element" }
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
const stripIds = (config: ConfigToml): ConfigToml => ({
  ...config,
  sinkIds: undefined,
  ...(config.index ? { index: config.index.map((e) => ({ ...e, id: undefined })) } : {}),
});

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

/// The digest of each file as this page loaded it, then as its own saves wrote
/// it: the server refuses a save over a file that no longer has it. A reload
/// reads them afresh; Reset keeps them, since it goes back to that same state.
let digests: Digests = {};

export const api = {
  project: () =>
    fetch("/api/project")
      .then((r) => json<Project>(r))
      .then((p) => {
        digests = p.digests;
        return p;
      }),
  /// The user dismissed the notice of these settled saves: the server stops telling them.
  dismissRecovery: (ids: string[]) =>
    fetch("/api/recovery/dismiss", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ ids }),
    }).then((r) => (r.ok ? undefined : json<undefined>(r))),
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
  /// `overwrite` as for `save`: the review is computed with those files taken as they are on disk.
  diff: (config: ConfigToml, ops: FileOp[], identity?: ConfigIdentity, overwrite: Digests = {}) =>
    fetch("/api/diff", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({
        config: stripIds(config),
        ops: ops.map(stripOpUids),
        identity,
        digests: { ...digests, ...overwrite },
      }),
    }).then((r) => json<Review>(r)),
  /// `overwrite` gives, for the files the user chose to overwrite anyway, the
  /// digest they have on disk now (from the review), in place of the page's.
  save: (config: ConfigToml, ops: FileOp[], skip: string[] = [], identity?: ConfigIdentity, overwrite: Digests = {}) =>
    fetch("/api/save", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({
        config: stripIds(config),
        ops: ops.map(stripOpUids),
        skip,
        identity,
        digests: { ...digests, ...overwrite },
      }),
    })
      .then((r) => json<SaveResult>(r))
      .then((res) => {
        digests = { ...digests, ...res.digests };
        return res;
      }),
};
