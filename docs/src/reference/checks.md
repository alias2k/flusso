# Checks and findings

What `flusso check` and the start of `flusso run` verify, every finding they can report, and the report both print. The flags are in [CLI](cli.md#check).

| Finding | Severity | `check` | plain `run` | `run --strict` |
| --- | --- | --- | --- | --- |
| An error that [stops the start](#what-stops-a-plain-start) | error | fails | refuses to start | refuses to start |
| Any other schema error, or a [capture gap](#capture-gaps) | error | fails | starts, and reports it as an [`error` event and a gauge](#errors-a-start-goes-on-with) | refuses to start |
| A [stale or unreadable lock](lock.md#checked-by-flusso-check) | error | fails | not looked at | not looked at |
| A warning | warning | passes | starts | starts |

**An error means an index won't work as its schema says**: a document holds something other than the schema declares, misses changes, or isn't indexed at all. A warning means nothing is known to be wrong. `check` exits non-zero on any error; its exit code always matches its last line.

## The report

Each section that passes is one line. A section with a problem prints in full, with the SQL that fixes it. Warnings come next, then errors, so what failed is on the last screen, and a summary line ends it. A clean run:

```text
✓ config valid: flusso.toml: 3 indexes, 2 sinks
✓ flusso.lock is current
✓ source server: the primary, and flusso is not a synchronous standby of it
✓ publication covers all 9 tables the indexes read
✓ replica identity: every table an index reads logs its whole old row

✓ check passed
```

One with schema errors, a warning and a stale lock:

```text
✓ config valid: flusso.toml: 3 indexes, 2 sinks
✓ source server: the primary, and flusso is not a synchronous standby of it
✓ publication covers all 9 tables the indexes read
✓ replica identity: every table an index reads logs its whole old row

Database validation
───────────────────
  [warning] users.profile  `primary_key` references unknown column `public.profiles.userid`; ignored: the join is keyed by the table's own primary key
  [error] users.fullName  declared non-null (`required: true`) but the database column allows null: a document can hold null where the schema says it never does, so a consumer reading by the schema fails on it; declare `required: false`, or make the column `NOT NULL`, or give the field a `default`
  [error] users.nicknames  the column's database type `text[]` is an array, but the field does not declare `array: true`: the schema says one value while the index holds a list, so a consumer reading by the schema fails on every document holding one; declare `array: true` (that changes the index's content hash, so the index is rebuilt, and turns on the array clean-up: null elements dropped, a lone value wrapped)

Lock
────
  [error] flusso.lock  `flusso.lock` was compiled from another config than this one (the index `users` changed): `flusso run --locked`, or an image baked from this lock, would run the older config. Run `flusso check --fix` or `flusso build` (or `flusso run`, which rewrites it) and commit the result

✗ check failed: 3 errors, 1 warning
Error: check failed: 2 schema errors and flusso.lock is stale
```

- **Each finding** names the index and the field path (`users.account.createdAt`; a field that shares its name with another at its level carries its position, `orders#2.total`, see [One name per level](identifiers.md#one-name-per-level)), says what is wrong, and how to fix it.
- **What `run` does by itself** is folded into its section's line: `` ✓ publication: the next `flusso run` covers the 9 tables it misses ``, `` ✓ replica identity: the next `flusso run` sets REPLICA IDENTITY FULL on 9 tables ``.
- **Something to do by hand that only costs speed** (a table that doesn't log its whole old row, which `run` won't set) prints in full with its SQL and is no error. See [Replica identity](source-postgres.md#replica-identity).
- **Colour.** On a terminal, errors are bold red and warnings bold yellow. `--color auto|always|never` (`FLUSSO_COLOR`) decides; `auto` colours only a terminal with `NO_COLOR` unset, so a pipe or a CI log gets plain text.
- **`--offline`** connects to nothing. It still loads and validates every file, compiles every script, compares the lock, and reports what needs no database (marked in the [table below](#every-finding)). It notes `! offline: the database was not checked (types, nullability, the source server, publication and replica identity)` and ends on `✓ offline checks passed; database checks not run`.
- **The config and the mapping** aren't part of the report: [`flusso show`](cli.md#show) prints them.

`--fix` rewrites a lock that isn't current, and nothing else: see [Checked by flusso check](lock.md#checked-by-flusso-check).

## What is checked

Every problem of one run is listed together, each naming the index, the field path, and the missing table or column with the schema key that names it (`` `foreign_key` references unknown column `public.orders.userid` ``).

- **Fields.** Each column a field reads exists, and its database type is one the declared type accepts. Its [`transforms`](field-types.md#transforms) take that type: Postgres plans them over no rows, so a domain over `text` passes and an enum fails. A field declared `required` over a nullable column is an error; over a column of a view, a materialized view or a foreign table, whose nullability Postgres doesn't record, it's a warning. An [`array`](field-types.md#array) field's column holds an array (a `[]` type, or `json`/`jsonb`, whose elements aren't type-checked), its transforms are checked against the element type, and a `[]` column read without `array: true` is an error. A date field's `format` [reads a date](field-types.md#dates-and-times).
- **Geo points.** Both coordinate columns exist and are numeric.
- **Aggregate input.** A `sum`/`min`/`max` column's type is one `value_type` accepts; each column of an [expression](aggregates.md#arithmetic-input) exists and is numeric.
- **Relations, at any depth.** For every join and aggregate:
  - the related `table`;
  - the key: a `belongs_to` `column` on the table that holds the field, a `has_one`/`has_many`/aggregate `foreign_key` on the related table;
  - a junction's `table`, `left_key`, and `right_key`;
  - each `via` hop's `table` and `foreign_key`;
  - both sides of every `on:` pair: `local` on the table before the hop, `related` on the hop's table;
  - the columns of the value and null-check `filters` on the relation, its junction, and each hop (a `raw` filter is passed to the database unread, so it isn't checked);
  - every `order_by` column, and a join's `primary_key` (a warning: relations are keyed by the table's own primary key, so a name that misses is ignored);
  - that the related table, and each hop's, has a single-column primary key;
  - that each link compares with the key it names: a `belongs_to` `column` with the target's key, a `foreign_key` with the parent's (or the last hop's), a junction's `left_key` and `right_key` with both sides' keys, a hop's `foreign_key` with the level before, and the two sides of each `on:` pair (see [A link of another type](source-postgres.md#a-link-of-another-type));
  - that an [`ids`](aggregates.md#ids) `element_type` holds the related table's keys.

  A missing table is reported once, and nothing on it is looked up.
- **Root keys, on the root table.** That the table has a single-column primary key, reported under `table`; that the declared `primary_key` column exists, is that key, and is of a [key type](source-postgres.md#key-types) flusso reads, reported under `primary_key`, whether or not a field reads it; the columns of the top-level value and null-check [`filters`](filters-and-soft-delete.md#root-filters), reported under `filters`, and the [`soft_delete`](filters-and-soft-delete.md#soft_delete) marker, reported under `soft_delete`: its `column`, its `when` columns, and its `field` (see [How the marker is verified](filters-and-soft-delete.md#how-the-marker-is-verified)).
- **Two fields of one name at one level**, at any depth: see [One name per level](identifiers.md#one-name-per-level).
- **The source server.** That it is the primary and doesn't name flusso a synchronous standby: the same check that stops `run` at start, with the same message (see [Behind a synchronous standby](source-postgres.md#behind-a-synchronous-standby)).
- **Capture.** That the publication streams every table an index reads, and that each table's replica identity logs what flusso needs: see [Capture gaps](#capture-gaps).
- **The lock.** That `flusso.lock` is what the config compiles to: see [flusso.lock](lock.md#checked-by-flusso-check).

The transform and link checks ask Postgres to try the operation. When it won't (the role has no `USAGE` on a type's schema, which reading the column doesn't need), the check is reported as `check_skipped`, a warning naming what wasn't checked and Postgres's reason, instead of either verdict. Grant the role `USAGE` on that schema to run it.

## Every finding

**Offline** marks a finding `--offline` reports too. **Stops `run`** is what a plain start does; `--strict` refuses every error.

| `kind` | Found | Severity | Offline | Stops `run` |
| --- | --- | --- | --- | --- |
| `unknown_table` | A table that doesn't exist. | error | | yes |
| `unknown_column` | A column the document query reads that doesn't exist. | error | | yes |
| `unread_soft_delete_marker` | A `soft_delete` `field` that reads no column. | error | yes | yes |
| `relation_primary_key` | A relation table without a single-column primary key. | error | | yes |
| `transform_not_applicable` | `transforms` the column's type doesn't take. | error | | yes |
| `root_primary_key_mismatch` | A root `primary_key` other than the table's own single-column key. | error | | yes |
| `unsupported_key_type` | A root `primary_key` of a type flusso can't read back as a document id (see [Key types](source-postgres.md#key-types)). | error | | yes |
| `not_an_array_column` | `array: true` over a column that's neither a `[]` type nor `json`/`jsonb`, nor a domain over one. | error | | yes |
| `link_type_mismatch` | A link column of a type Postgres can't compare with the key it names (see [A link of another type](source-postgres.md#a-link-of-another-type)). | error | | yes |
| `duplicate_field_name_fails_to_build` | Two fields of one name, at the root or in plain objects, whose every document fails to build. | error | yes | yes |
| `duplicate_field_name_refused` | Two fields of one name, at the root or in plain objects, whose every document OpenSearch refuses. | error | yes | while every sink is a [plain OpenSearch sink](#plain-opensearch-sinks) |
| `time_of_day_as_date` | A `time` or `timetz` column under a `timestamp` or `date` field with no `options`, at the root or in plain objects. | error | | while every sink is a [plain OpenSearch sink](#plain-opensearch-sinks) without `mapping.ignore_malformed` |
| `root_primary_key` | A root table with no primary key or a composite one (the message says which); on one, with no relation, a `primary_key` that names no column. | error | | no |
| `type_mismatch` | A declared type that doesn't accept the column's type. A `date` field over a timestamp column also says when its `format` refuses the timestamp. | error | | no |
| `not_numeric` | An expression or geo column that isn't numeric. | error | | no |
| `nullability` | `required` over a column of a table that allows null. | error | | no |
| `undeclared_array_column` | A `[]` column (or a domain over one) read by a field without `array: true`. | error | | no |
| `ids_element_mismatch` | An `ids` `element_type` that holds no key of the related table's type (see [ids](aggregates.md#ids)). | error | | no |
| `ids_element_lossy` | An `ids` `element_type` that holds only some keys of the related table's type, or changes them. | error | | no |
| `duplicate_field_name_wrong` | Two fields of one name whose documents hold something other than the schema declares (see [One name per level](identifiers.md#one-name-per-level)). | error | yes | no |
| `time_of_day_format` | A date-mapped field whose `format` reads only a time of day, over a value that isn't one (see [dates and times](field-types.md#dates-and-times)). | error | partly: see [which, when](field-types.md#a-format-of-your-own) | no |
| `publication_gap` | A table an index reads that the publication doesn't stream, and `run` won't add. | error | | no |
| `replica_identity_gap` | A table whose replica identity misses its primary key or a parent link, and `run` won't set it. | error | | no |
| `lock_stale` | A `flusso.lock` compiled from another config. `check` only. | error | yes | not looked at |
| `lock_unreadable` | A `flusso.lock` that can't be read, or is of an older format. `check` only. | error | yes | not looked at |
| `nullability_untracked` | `required` over a column of a view, materialized view or foreign table: Postgres records no nullability there, so flusso can't check it. | warning | | no |
| `ignored_primary_key` | A join `primary_key` that names no column. | warning | | no |
| `duplicate_field_name` | Two fields of one name whose documents still hold a value the index maps, or where nothing is known either way. | warning | yes | no |
| `check_skipped` | A transform or link check Postgres wouldn't run; the message names what wasn't checked and why. | warning | | no |

The two gap kinds and the two lock kinds appear in the report's own sections and under `gaps` and `lock` in [JSON](#json-output), not under `diagnostics`.

## Capture gaps

A gap is a table an index reads whose changes don't all reach flusso, and that `flusso run` won't fix by itself:

- **`publication_gap`**: the publication doesn't stream the table, or streams a partitioned table under its partitions' names, and the role can't change the publication or [`manage_publication`](source-postgres.md#capture) is off. Its changes never reach the indexes that read it.
- **`replica_identity_gap`**: the table's replica identity misses its primary key or a parent link, and the role doesn't own it or [`manage_replica_identity`](source-postgres.md#replica-identity) is off. Deleting or re-parenting a row leaves the old parent's document stale.

Not gaps: what the next `run` closes itself (it says so on one line), a table that logs its key and links but not the rest of the row (by hand, no error), and an identity index flusso keeps on purpose. A gap prints with the SQL that closes it:

```text
Capture gaps
────────────
  [error] public.reviews  `public.reviews` is not in the publication, and `flusso run` won't add it (role does not own table public.reviews; role does not own publication "flusso"): its changes never reach the indexes that read it
  [error] public.addresses  `public.addresses` doesn't log its parent link (`user_id`) on delete or update, and `flusso run` won't set REPLICA IDENTITY FULL (role does not own table public.addresses): deleting or re-parenting a row leaves the old parent's document stale

Publication coverage
────────────────────
  ! 1 table(s) the indexes read are not yet streamed:
    • public.reviews
  → flusso will NOT fix this automatically:
    - role does not own table public.reviews
    - role does not own publication "flusso"

Run to stream every table
─────────────────────────
  ALTER PUBLICATION "flusso" ADD TABLE "public"."reviews";
```

## Validation at start

Once connected, and before any index is touched, `run` runs the same checks against the database as `check`, on every start, `--locked` and lock-only runs included, then inspects the publication and replica identities. It doesn't compare the lock. What it does next depends on what it found.

### What stops a plain start

- a table that doesn't exist, in any position;
- a column the document query reads that doesn't exist: a field's column, a key (`column`, `foreign_key`, `left_key`, `right_key`, a hop's `foreign_key`), either side of an `on:` pair, a value or null-check filter column (relation, junction, hop, root `filters`, `soft_delete` `when`), an `order_by` column, a geo coordinate, an aggregate's input column or expression operand, and the `soft_delete` column;
- a `soft_delete` `field` that reads no column;
- a root `primary_key` that names no column, when the root table has a single-column primary key or the index has any relation (a relation keys on the root, or traces a change back to it, by that column);
- a root `primary_key` that names a column other than the root table's single-column primary key: changes and the backfill carry the table's key, the build matches it on the declared column, so a document is built from another row or written as a tombstone;
- a root `primary_key` of a type flusso can't read back as a document id: every key reads as none, so no document is ever written;
- a relation table without a single-column primary key (a junction needs none);
- [`transforms`](field-types.md#transforms) the column's type doesn't take. When Postgres won't run the check, it's `check_skipped` instead, and the start goes on;
- a link column Postgres can't compare with the key it names: every build of the index fails;
- `array: true` over a column that holds no array;
- two fields of one name, at the root or in plain objects, that can never give an indexed document (see [Pairs that stop the start](identifiers.md#pairs-that-stop-the-start));
- a `time` or `timetz` column under a `timestamp` or `date` field with no `options`, at the root or in plain objects.

A **plain object** is an `object` field with no `emit: false` and no `options`, under plain objects up to the root. Under a join, a parent without the related row still indexes, and under `emit: false` or `indexed: false` the value is never mapped, so neither stops.

`run` then exits non-zero with `the schemas cannot be built against this database (N problem(s) above)`. Nothing is staged, no slot is created, no index is touched. Under Kubernetes or compose that's a restart loop with the same message, until the config is fixed. An unreachable database fails before validation, as `connecting to Postgres`, and a source server `run` refuses stops it too.

#### Plain OpenSearch sinks

The refused pair and the time of day stop only while every sink is a **plain OpenSearch sink**: one with no `pipeline` option and no index settings (sink-wide or for the index) but the shard counts, `max_result_window`, the `mapping.*.limit` settings, and, for the refused pair only, `mapping.ignore_malformed`. Any other setting (an `index.default_pipeline`, `mapping.coerce`, …) may let the value in or keep OpenSearch from seeing it, and so may another sink writing the index (stdout, or an OpenSearch sink that isn't plain), so flusso doesn't stop. An index template on the cluster is outside what flusso reads: if one makes these documents index, say so in the sink's `index_settings` (the same `default_pipeline`, say) and the start goes on.

### Errors a start goes on with

Every other error, schema or capture, starts as it did before 0.19, and is reported on every start:

- **An `error` event per finding**, target `flusso::schema_errors`, with `index`, `kind` and the message, plus `field` (the dotted path) for a schema error or `table` for a gap. A gap is reported once for each index that reads its table.
- **A gauge**, [`flusso.index.schema_errors`](metrics.md) (`flusso_index_schema_errors{index,kind}` in Prometheus): how many such errors of that kind the last start found for the index. Alert on `> 0`. A start after the fix has no series for it.

Each finding reaches a reader once. With stderr on a terminal, the start's report prints every finding and the stderr log leaves the events out. Into a log (a container, a file), the report prints only the warnings and the events are the copy:

```text
Database validation
───────────────────
  [warning] users.profile  `primary_key` references unknown column `public.profiles.userid`; ignored: the join is keyed by the table's own primary key

starting anyway with 3 schema error(s) and 2 capture gap(s) `flusso check` fails on, each logged next as an `error` line (`--strict` refuses them)

2026-11-26T12:43:44.273525Z ERROR daemon.start: flusso::schema_errors: the column's database type `text[]` is an array, but the field does not declare `array: true`: … index=users kind="undeclared_array_column" field=nicknames
2026-11-26T12:43:44.273600Z ERROR daemon.start: flusso::schema_errors: `public.reviews` is not in the publication, and `flusso run` won't add it (…): its changes never reach the indexes that read it index=products kind="publication_gap" table=public.reviews
```

Over [OTLP](../operate/traces-otlp.md) the events are span events of the `daemon.start` span: flusso exports no logs.

### --strict

`run --strict` (`FLUSSO_STRICT=true`) refuses to start on any error `check` would report from the schemas and the capture, all of them at once and printed as `check` prints them, before anything is staged. With `--locked` too. It doesn't look at the lock: `check` does that.

```text
Error: the schemas and the capture do not give a working index (3 error(s) and 2 gap(s) above, refused under --strict); fix them, confirm with `flusso check`, then start again
```

Use it where a wrong index should never go live: a production deploy that `flusso check` already gates in CI, or a staging environment that must match it.

## JSON output

`check --format json` prints the same report as one JSON document, also with `--offline`. The database sections (`server`, `coverage`, `pre_image`, `gaps`) are `null` offline.

| Key | Holds |
| --- | --- |
| `config_file` | `path`, `status`, `indexes`, `disabled_indexes`, `sinks` |
| `lock` | `status` (`ok`/`problem`), `state`, `path`, `rewritten`, `message`; the states are in [flusso.lock](lock.md#checked-by-flusso-check) |
| `server` | `status`, `ok`, `problem` (the refusal, or `null`) |
| `coverage` | `status`, `satisfied`, `manageable`, `will_manage`, `present`, `missing`, `misrouted`, `blockers`, `remediation` (the SQL) |
| `pre_image` | `status`, `satisfied`, `complete`, `gaps` and `partial`: each `table`, `missing`, `missing_key`, `manageable`, `will_manage`, `blockers`, `remediation`, `kept` |
| `gaps` | one per capture gap: `kind` (`publication_gap`, `replica_identity_gap`), `table`, `indexes`, `message` |
| `diagnostics` | one per schema finding: `index`, `field`, `path`, `positions`, `anchor`, `severity` (`error`/`warning`), `kind`, `message` |
| `summary` | `passed`, `errors` (gaps and a failing lock included), `warnings`, `offline`, `database_checked`, `server_refused`, `line` (the report's last line) |

A section's `status`:

| `status` | Means |
| --- | --- |
| `ok` | Nothing to do. |
| `run_fixes` | The next `flusso run` closes it itself, or keeps it on purpose. |
| `by_hand` | SQL to run by hand that only makes flusso faster (an update that changes no column a document reads still rebuilds); no error. |
| `problem` | An error: the report prints it in full. |

In a diagnostic, `anchor` says what `path` names: `field`, a field's path from the document root, or `index_key`, one of the index-level keys `table`, `primary_key`, `filters`, `soft_delete` (a root field may have the same name). `positions` has one entry per segment of `path`: its 1-based position at its level when another field there shares its name, else `null`.

```json
{
  "index": "users",
  "field": "nicknames",
  "path": "nicknames",
  "positions": [null],
  "anchor": "field",
  "severity": "error",
  "kind": "undeclared_array_column",
  "message": "the column's database type `text[]` is an array, but the field does not declare `array: true`: …"
}
```

## Related

- [CLI](cli.md): the `check`, `run` and `show` flags.
- [Identifiers and validation](identifiers.md): what loading enforces with no database.
- [Upgrade to 0.19](../operate/upgrade-to-0-19.md#check-fails-on-an-index-that-wont-work-as-written): what became an error.
