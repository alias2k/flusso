# Stage 3: keyset backfill on the builder pool

ADR 0012, stage 3 (#386). The design as built, with what its review changed. Commits: implementation `a7938740`, e2e `b6cf8729`, docs `2ee5e5b8`, ADR `01e7c7dc`; review fixes `f9bfcfb3`…, below.

## What changes, in one table

| | Today (`cdc/backfill.rs`) | After |
| --- | --- | --- |
| Who reads keys | `ChangeCapture::snapshot(tables)`, a pool of its own (`max_connections(2)`, one connection held) | `DocumentBuilder::keys(index, after, limit)` on the builder pool |
| Transaction | one `REPEATABLE READ READ ONLY` for the whole snapshot | none: one statement per page |
| Read order | heap order (`DECLARE … CURSOR FOR SELECT key FROM t`) | key order, by the primary key's index |
| Unit | per root table; a row is routed to every index rooted at a table *of that name* | per index: an index reads its own root's keys |
| Locks held | `AccessShareLock` on every root table read, **until the snapshot ends** | one page's statement |
| `backend_xmin` | pinned for the whole backfill | per statement |
| Restart mid-backfill | starts the snapshot over | starts over (resume is stage 8b; see "Resume") |

Checked on Postgres 16: with today's transaction open after reading `t` and closing its cursor, `pg_locks` still shows `t | AccessShareLock`, and `SET lock_timeout = '1s'; ALTER TABLE t ADD COLUMN …` fails with `canceling statement due to lock timeout`. **Today, a migration on a root table waits for the whole backfill, and every query queued behind its lock waits too.** After this stage it waits at most one page.

## 1. Consistency without the long transaction

**A page gives ids, never document contents.** That is already true today: the cursor's snapshot only decides *which* keys are read, and `build_many` reads each document from the current, Read Committed state when the batch is built (`Snapshot::buffer` → `commit_snapshot` → `self.build`). So the long transaction never made a document consistent. It only fixed the key set.

**What makes the final document right is build order, not read time.**

- Snapshot and live builds run on the one ingest task. `commit_snapshot` and `commit_live` each build and then publish before the loop goes on (`ingest.rs`), so **lane order is build order**.
- Stage 2's admission: on a lane, an item starts only when no earlier unfinished item shares a document with it. "Live first" picks a later live item over an earlier backfill item *only if they share no document* (`sink_engine/writes.rs`). So **per document, write order is lane order.**
- Per document, a later build reads a later or equal state (Read Committed takes a new snapshot per statement, and later snapshots see more).
- So per document, the last write is the last build. **The last build follows the last change**: a change is emitted only once its transaction is visible (ADR 0010), and then it is built live.

**No key is missed.** `prepare()` creates the slot before any snapshot (#120; the daemon finishes staging, then the ingest engine runs `prepare` before it serves a request). Take a row present at the end whose key no page read. Its key range was paged at some time `t`, when the row wasn't visible under that key. So it got the key (insert, or key update) after `t`, which is after the slot's consistent point. That commit is in the WAL, streamed live, and built. A row present throughout under one key is in exactly one page, because the key is unique (primary key) and the pages partition the key space.

**Interleavings** (`k` is a root key, `P` the page that reads it, `B` that page's build, `L` a live build):

| Order of events | Lane, for `k` | Final |
| --- | --- | --- |
| `P` reads `k`; row updated; `L`; `B` | `L` (new), `B` (new or newer) | newest |
| `P` reads `k`; `B` (old state); row updated; `L` | `B` (old), `L` (new) | new: `L` is later on the lane and admission keeps it after `B` |
| `P` reads `k`; row **deleted**; `L` tombstone; `B` | tombstone, then `B` | **`B` builds a tombstone**: the build reads the database, finds no row, and answers `Delete` (`matching::match_rows`, an unanswered position is a tombstone). No stale upsert. |
| `P` reads `k`; `B` upsert; row deleted; `L` tombstone | upsert, tombstone | absent |
| row soft-deleted before or after `P` | as a delete: the build applies `soft_delete` and answers a tombstone | absent |
| row inserted with `k` below the cursor | only `L` | present, from `L` |
| key changes `k → k'`, `k'` above the cursor | `L`: tombstone `k`, upsert `k'`; later `P'` reads `k'`, `B'` | `k` absent, `k'` present (built twice, harmless) |
| key changes `k → k'`, `k'` below the cursor | `L` only | same |
| re-parent of a child during the backfill | live resolve rebuilds the old and the new parent (pre-image, ADR 0007); pages don't resolve | both parents current |
| filtered root: row leaves the filter before `B` | `B` builds a tombstone | absent |

The one thing a page adds over today: a row can be read *twice* (its key moved above the cursor). That costs one more build of a correct document.

**What stage 4 must keep.** This argument needs per-document build order to be lane order. Stage 4 runs builds at once with a reorder buffer, so the buffer must keep a backfill page behind any earlier build of the same id. The ADR already says so (section 2, one admission rule at both stages).

## 2. Keys

**Which roots are paged.** The same roots as today, by the same test: the root table's **catalog** primary key is one column, and it is the declared `primary_key` (`RootKey::Unique`). Any other root (no primary key, a composite one, a view, a partitioned root keyed with its partition column, #391) still gets nothing, with today's warning, and its `SnapshotComplete` still marks it seeded. Not fixed here, not made worse.

**Why keyset order is sound for every type in `KEY_TYPES`.** A primary key is a unique btree index under the column's default operator class and collation.

- `ORDER BY key` on the bare column, and `key > $last::<bound>::<key type>`, both use that operator class. The column's implicit collation wins over the parameter's default collation, so a `COLLATE` column (deterministic or not) compares and orders by its own collation, and the index serves the page (EXPLAIN below).
- Uniqueness is decided by the same operator class's `=`. So two keys equal under it (`citext` `'Ab'`/`'AB'`, a nondeterministic collation, `numeric` `1.5`/`1.50`, float `0`/`-0`) can't both exist, and "every key `> $last`" skips no row.
- `NaN` (float and `numeric`) is a single value that sorts above every other. `±infinity` (timestamps, dates) sorts at the ends. Both are ordinary values to the btree, and nulls can't be primary keys.

**The cursor value round-trips through equality, which is what the build already needs.**

- The page selects `value::key_select` (the key's text under `text_value::key_text`, beside the typed column). `decode_key` parses it into the same `GenericValue` a live change carries.
- The next page binds that value with `sql::key_cast`, as `build_many` binds every key.
- Order is preserved if and only if the bound value is `=` to the stored key under the column's operator class: then `> $last` is exactly "after this row".
- That equality is what `build(id)` already relies on to find the row. Each type, and how it comes back:

| Type | Key text | Bound as |
| --- | --- | --- |
| `int2/4/8`, `bool`, `uuid` | output text | own type |
| `float4/8` | `float4send`/`float8send` bits, exact | own type; `NaN` comes back as `NaN` |
| `numeric` | output text, every digit | `numeric`; `NaN` as `$n::text::numeric` |
| `text`, `varchar`, `name` | output text | `text` |
| `bpchar` | output text, padding kept | `text`; `bpchar` ignores the padding |
| `citext` | output text | `text` |
| `date`, `time`, `timestamp`, `timestamptz` | ISO 8601, UTC (any year width, BC) | own type; `±infinity` as `$n::text::<type>` |
| `bytea` | `\x` hex | `bytea` |

- Already guarded: `key_matching` (4,560 requested/stored pairs over 19 key types, single and batched build), `wal`'s `every_relation_position_keyed_by_*` and the `*_past_9999` cases.
- New guard: page every key type with a page size of 2, with the edge values (`NaN`, `±infinity`, BC, year 10000, `-0`, empty `bytea`, padded `char(n)`, mixed-case `citext`, a nondeterministic collation, a domain, a domain over a type in a schema the role may not use, a role refused `has_schema_privilege`). Each row must be read exactly once, in the order of `ORDER BY key`.
- **Excluded types: none.** A type outside `KEY_TYPES` is already refused at start (`UnsupportedKeyType`).

**Key type and statement cache.**

- The page statement is spelled with the same key type the builder casts every key to: `ColumnMeta.key_type`, the base type's schema-qualified name with no modifier, or the declared type when the role may not use the base type's schema.
- `$n::<bound>::<key type>` carries the parameter's type in the text (issue #335), so sqlx's per-text statement cache never sends a value in another binary format.
- Each index has at most three page texts: the first page with no bound, a next page per bound type (usually one; `text` too when a key is `NaN` or infinite).

**A type change between pages.** The builder's rule applies:

- Postgres refusing the prepared page with `0A000` (result type changed), or a cast failing, forgets the root table's column types, reads them again, and retries the page once. If it fails again, the snapshot fails.
- The ingest engine then restarts and the request is served again from the start, as an error mid-snapshot does today.
- Today the same `ALTER` simply waits for the end of the backfill (the lock above).

**Partitioned roots.** A single-column primary key on a partitioned table is its partition key.

- Range partitions: an ordered `Append`, with the partitions below `$last` pruned at run time under a generic plan (`Subplans Removed: 1`).
- Hash partitions: a `Merge Append` of each partition's index range scan, which reads one descent per partition per page.

Both read through the parent, every partition included, as today.

## 3. Plans

**One correction found by the tests.** The page orders `ORDER BY key USING <`, not a bare `ORDER BY key`. `>` picks its operator through the `search_path`, while the index's order is the type's btree class. For `citext` installed in a schema outside the path, the page scanned in `citext` order but compared as `text`, and skipped the rows the two orders disagree on (`keyset`'s `a_key_whose_operators_are_off_the_search_path_is_paged_once` fails without it). `USING <` picks its operator the way `>` does. Where that is the index's own operator, which is every other case, the plans below are unchanged (checked with `USING <` on the partitioned and collated roots). In that edge case the page is correct but sorts.


Postgres 16, `jit = off`, `PREPARE`d with the page's real select list (`format('%s', id), id::<key type>`). Every page is an index-only range scan with no sort, under both `force_custom_plan` and `force_generic_plan`:

| Root | Plan (generic) |
| --- | --- |
| `int4` | `Limit → Index Only Scan using t_pkey, Index Cond: (id > $1)` |
| `uuid` | the same, on `ku_pkey` |
| `text COLLATE` nondeterministic ICU | the same, on `kt_pkey` (the index's collation) |
| `numeric` (with `NaN`) | the same, on `kn_pkey` |
| range-partitioned `bigint` | `Limit → Append (Subplans Removed: 1) → Index Only Scan` on each remaining partition; the next partition `never executed` |
| hash-partitioned `int4` (4) | `Limit → Merge Append (Sort Key: id) → Index Only Scan` × 4 |
| soft-delete + filtered root | the bare key's plan: the page carries no filter |

**Why the page carries no filter.** The ADR said "root filters and soft-delete on the page"; this PR corrects it.

- With a filter on the page, its plan depends on the filter. A selective indexed filter can make the planner pick that index and a sort, which re-sorts the remaining rows on every page. A raw filter can read anything.
- Without it, the build applies the filter and soft-delete and answers a tombstone for a row outside them, exactly as the cursor's rows did. So the backfill writes the same documents and the same tombstones as today.
- Measured with the filter on the page: still the primary key's range scan with `Filter:` here (14,336 rows removed per page at 10% kept), but that is one data set, not a guarantee.
- Dropping those tombstones is a separate question, left out of this stage.

`just review-plans` gets a page case per root of `cases.rs`: plans per call and the root scan, under `auto` and `force_generic_plan`.

**Page size.**

- `1024` keys, the ADR's constant and today's `FETCH_SIZE`.
- The keys feed the batch as rows do today: `max_changes` (256) per batch, and `build_many` splits a batch into 512-key chunks.
- `KeyForm` doesn't change: a batch of 16 or more keys goes as `key_arrays`, a partitioned root's as a padded `key_constructor`.
- A page shorter than the limit ends that index: a later insert above it is in the WAL.

## 4. Connections and the pool

- **Gone:** the snapshot's own pool. `PgPoolOptions::new().max_connections(2)`, connected for every snapshot, holding one connection for its whole length.
- **Used instead:** the builder pool. It is sized as today (sqlx's default of at most 10; `PgDocumentBuilder::connect`), and its sessions already set `jit = off`.
- **No starvation, until stage 4.** Builds stay serial on the ingest task, and a page is one more statement on it. The ingest task holds **at most two** builder connections: a page parked in `Walk.page` keeps its connection while a live change wins the `select!` and its build takes another (review of stage 3). Stage 4's pool is `builds + 2` (`apps/cli/src/backends.rs`): one resolving change and this one page beside the builds.
  - Today the cursor's `FETCH` and a build already alternate on that task (the snapshot arm and the live arm of one `select!`).
  - Live and backfill take turns as they do now: `select!` picks among the ready arms, and a page is one round trip per 1,024 keys.
- **Cancel safety.** The page is a boxed future owned by the running `Snapshot`, polled from the `select!` and kept across iterations, like the cursor stream's `unfold` state today. A live change winning the `select!` doesn't drop it.
- **PgBouncer transaction mode.** A page is one statement outside any transaction, which transaction mode serves on any server connection.
  - Today's snapshot pins one server connection for its whole `BEGIN … COMMIT`.
  - Tested by extending `the_pipeline_runs_through_pgbouncer_in_transaction_mode` with a backfill that pages: page size small enough for several pages, and a server connection reset between them.

## 5. WAL retention and bloat

- **WAL retention:** unchanged. The slot pins it, and a backfill item carries no position.
- **Bloat: the `xmin` pin goes.** Today `backend_xmin` of the snapshot's session is held for the whole backfill, so vacuum keeps every dead tuple of the database newer than it. After this stage a page's snapshot lives for one statement.
- **To measure:** the maximum `age(backend_xmin)` and transaction length of flusso's sessions during a `reference` `default` backfill with a writer updating rows, and `n_dead_tup` of the updated table at the end, before and after.

## 6. Resume (designed, not built)

| | A restart mid-backfill |
| --- | --- |
| Today | an engine restart redelivers the unacknowledged request; a process restart re-stages and requests again. Either way the snapshot starts over, re-reading and rebuilding every key. |
| After this stage | the same |
| Stage 8b | resumes from the last key |

**What 8b stores**, per `(sink, index)`: the last key whose page every document of has been written on that sink.

- It is stored as the typed `GenericValue` (its serde is lossless), with the key column's `key_type` and the index's content hash.
- It goes in the sink's meta document beside `seeded`, written by the sink engine after the lane item holding that page's last document is acknowledged.
- A coalesced snapshot resumes from the lowest key over the requesting sinks.
- A stored key whose `key_type` or hash no longer matches is ignored, and the backfill starts over.
- Why it is safe:
  - every key at or below the stored one was built after the slot existed, and has been written;
  - every later change to it is in the WAL that the slot still holds, because confirmation (`Progress::landed`) only passes positions every lane has landed.

**What this stage does for it.** The engine owns the cursor: it calls `keys(index, after, through, limit)` with the last key it buffered. So 8b only has to store that key and pass it back.

## 7. Stop, reindex, coalescing, `SnapshotComplete`

Same semantics. Where the code changes:

- `ChangeCapture::snapshot` and `SnapshotTable` go. `cdc/backfill.rs` goes; its catalog queries (`key_type`, `primary_key`) are the builder's own (`column_meta`, `root_table`).
- `DocumentBuilder::backfill_scopes` becomes `backfill_indexes() -> Vec<IndexName>`.
- `DocumentBuilder` gains `keys(index, after: Option<&RowKey>, limit) -> Result<Vec<RowKey>>`.
  - It is **required**, so a decorator can't silently backfill nothing: `ScriptedBuilder` forwards it, and every mock implements it.
  - It answers an empty page for an index it doesn't page, with today's warning, once.
- `ingest.rs`'s `Snapshot` holds, instead of a row stream:
  - the indexes left;
  - the current index's cursor (`after`);
  - the keys of the page in hand;
  - the page future in flight.

  The `select!` arm takes keys into the batch up to `max_changes`, commits the batch, and asks for the next page once the keys run out.
- **Coalescing, `SnapshotComplete` after the last page's batch is published, the request acknowledged after it, a stop abandoning the snapshot unacknowledged, a reindex as a new request:** unchanged code paths.
- `Stage::Page` times the page waits as it timed the cursor's.
- **Indexes on one root table** now read it once each, not once together. The cost is one key scan per extra index (the whole key read of `reference` is about 0.3 s, #389).
  - The page reads the index's own schema-qualified root, so this also closes the backfill half of #390 (two roots with one name in two schemas take each other's rows). The live half of #390 stays open.

## 8. Performance (measured)

Release builds of `origin/main` (`ffd54d22`) and this stage (`2ee5e5b8`), each from a clean worktree. The harness and flusso ran pinned to cores 0-3,12-15, with containers per run. The other tracks were building at the same time (load 10–43), so the spread is wide.

| `default` scale, 3 runs each | main | stage 3 |
| --- | --- | --- |
| `reference` backfill docs/s | 8,495 / 6,169 / 8,202 (median 8,202) | 8,316 / 7,141 / 8,054 (median 8,054) |
| `reference` peak RSS MiB | 111.5 / 113.2 / 109.6 | 103.1 / 125.2 / 114.4 |
| `reference` `page` busy s (key reads, 62,000 keys) | 0.2 / 0.4 / 0.2 | 0.3 / 0.3 / 0.3 |
| `reference` `build` busy s | 5.6 / 7.3 / 5.7 | 5.7 / 6.4 / 5.7 |
| `complex` backfill docs/s (5,000 docs, ~3 s) | 1,931 / 406 / 1,976 | 1,644 / 1,294 / 1,551 |
| `complex` peak RSS MiB | 157.7 / 85.4 / 154.7 | 171.9 / 141.5 / 136.1 |

- **`reference`**: no difference beyond the spread.
- **`complex`**: the ingest side is identical, `page` 0.02 s and `build` 1.45 vs 1.46 s over 20 batches (round 1). The gap is the sink: `seed` (OpenSearch's `mark_seeded`) took 0.60 s against 0.17 s under that load, and nothing in this stage touches it.
- **Live latency and drain** are unchanged within the spread.
- **The open transaction.** Sampled every 0.2 s on a `reference` run: on main the cursor's `REPEATABLE READ` transaction stays open **6.67 s**, the whole backfill. On this stage no flusso transaction is open longer than **0.04 s**. `xmin` age reads 0–63 on both, because nothing writes during that phase; the pin's length is the cost.
- **Large table, keys only** (psql, 5M narrow rows, warm cache, load 4–6.5, 3 rounds). Both walks send every row to the client:
  - `int8`: cursor 2.3 / 3.4 / 3.6 s, keyset 2.4 / 2.9 / 3.2 s;
  - `uuid`: cursor 4.3 / 4.2 / 4.0 s, keyset 3.9 / 5.5 / 3.6 s;
  - the keyset times exclude a separate probe for each page's last key, which psql needs and flusso doesn't (0.6–1.0 s, measured alone);
  - the same within the noise, about 0.5–0.9 µs a key. Key reading is about 3% of `reference`'s backfill.
- **In-process `snapshot/rows`**, pinned, `snapshot/rows` 3 rounds: main 101.3 / 101.7 / 108.2 ms, stage 3 98.1 / 113.8 / 108.1 ms. Within the spread. CI's gate decides; note that its mock now pages keys from the builder instead of streaming rows from the capture.

## Tests (all passing)

- **`source-postgres` `keyset`** (Docker):
  - every key type in key order, at page sizes 1, 2, 7 and 1,024, with hash- and range-partitioned twins and edge values (`NaN`, `±infinity`, BC, year 10000, `-0`, empty `bytea`, padded `char(n)`, `citext`, a nondeterministic collation, a domain);
  - a type whose operators are off the search path, as the owner and as a role without `USAGE` reaching it through a domain;
  - every `cases.rs` root reads the keys the cursor read;
  - a migration between two pages waits for nothing.
- **`engine` `keyset_backfill`** (Docker): writes run right after each full page, against rows behind the cursor, ahead of it and in the page just read. They cover updates, deletes, soft-delete toggles, rows leaving a root filter, inserts at both ends, key changes both ways across the cursor, re-parents and a parent rename. It runs on a plain root, on a hash-partitioned root, and across a stop after the second page followed by a restart. Each time the sink ends equal to a `build` of every row.
- **`engine` `wal`**: the PgBouncer test, session and transaction mode, now backfills 2,500 `bytea` keys over three pages.
- **`engine` `backfill`**: unchanged, still passing (its reverse-lookup counter now skips key pages).
- **Engine unit tests**:
  - each page follows the last key read, and a short page ends the walk;
  - a full last page is followed by one empty page;
  - indexes are walked one after another;
  - a stop while waiting on a page abandons the snapshot, which is served again;
  - the timings report the page.
- **`ScriptedBuilder`** forwards `keys` and `backfill_indexes`.
- **`review-ab --own-review`**, with its tests in `test_review_modes.py`.

## Docs

- `docs/src`:
  - `start/how-it-works.md`;
  - `reference/source-postgres.md`: backfill, connections, PgBouncer;
  - `contribute/pipeline.md`;
  - the upgrade page: no long transaction, locks, one connection fewer, key order, indexes on one root read once each.
- `CLAUDE.md`. `CONTEXT.md` only if the vocabulary moves.

## What the review changed

The review of `01e7c7dc` found the design sound and these to fix:

- **The walk is bounded.** Unbounded, `Walk::step` ended an index only on a short page: writers inserting greater keys kept it going for N/(B − R) pages (never ending when they kept pace), and every row they added was built twice. Now a walk reads the index's greatest key once when it starts (`DocumentBuilder::last_key`) and pages up to it (`keys`' `through`).
  - **Why that is enough.** A row whose key is past the bound got that key after the walk started, which is after the slot existed, so the live stream delivers it: an insert past the bound, or a key changed to past it. A row at or below it is read by a page, or changed later and delivered live.
  - **It always ends.** The bound is compared by value, so deleting the bound's row mid-walk still ends the walk.
  - **`NaN` and `infinity` as the greatest key** are ordinary values to the btree: `key <= 'NaN'` holds for every key (`every_key_type_is_paged_once_in_key_order` stores both).
  - **Partitioned roots.** The greatest key is a `Merge Append` of backward index scans over the partitions (hash), or the remaining partitions' backward scans (range), with `LIMIT 1`.
- **The bound is in the select list, not the `WHERE`.** With `key > $1 AND key <= $2`, a generic plan (`force_generic_plan`, Postgres 16, 20,000 rows) estimated a small range and planned `Bitmap Index Scan` → `Sort` on every page: the quadratic shape again. As `(key <= $2) AS within` in the select list, the page keeps `Limit → Index Only Scan, Index Cond: (id > $1)`. A page drops the rows past the bound and is short, which ends the walk.
- **The key index's own operators, written schema-qualified.** `<`, `>` and `<=` are read from the primary key index's opclass (`pg_index.indclass` → `pg_amop` strategies 1, 5, 2) and written `OPERATOR(ext.<)`. Before, a `citext` in a schema off the search path planned `Sort ← Seq Scan, Filter: (id)::text > …` on every page; now `Index Only Scan using …_pkey, Index Cond: (id OPERATOR(ext.>) $1)` (and `Index Only Scan Backward` for the bound). A role that may not use the operators' schema gets the bare names with `USING <`: correct, but each page sorts the rest of the table, warned once per index with the `GRANT USAGE` that fixes it. Guard: `keyset`'s `a_key_whose_operators_are_off_the_search_path_is_walked_through_its_index` (`pg_stat_user_tables`: 52 index scans, no sequential scan; with the bare operators it fails).
- **Connections.** A page parked in `Walk` holds its connection while a live build takes another: two, not one. Stage 4's pool is `builds + 2`, one of the two for this page.
- **A root keyed by another column than its table's key** is backfilled with nothing, where the cursor wrote a tombstone per row. Start is refused for such a root anyway (`root_primary_key_mismatch`); `wal`'s `a_root_primary_key_other_than_the_tables_writes_no_document` and the 0.18 upgrade page say so.
- **Missing e2e, added:** `keyset_backfill`'s `a_live_rebuild_lands_after_the_held_page_write_it_overtook`. A page's document is built before a write to its row; the sink holds backfill writes 500 ms, with `writes = 4`. The live rebuild waits behind the page's write, starting only once it has landed, and its document is the one that stays.
- **Test fixes:** `flusso-daemon`'s `a_backfill_failing_at_the_same_page_doubles_its_backoff` hung, because its builder mapped no index, so nothing asked for a backfill. It now maps `users`, and its waits fail after ten minutes of paused time. `cargo fmt` on `review.rs`.
