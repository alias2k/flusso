# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.19.0](https://github.com/alias2k/flusso/compare/flusso-source-postgres-v0.18.0...flusso-source-postgres-v0.19.0) - 2026-11-28

### Added

- *(source)* a stream opened past Rereads::open vouches for nothing (m-1)

### Fixed

- *(source-postgres)* prepare is quiet only about the publication gaps the start reported
- *(cli)* judge schema errors and capture gaps in one verdict, reported once
- *(source)* required over a view's column is a warning that says why
- *(source)* [**breaking**] an index that differs from its schema is an error

### Other

- spikes S1, S3 and S4 for the detach probe, against live OpenSearch and Postgres
- *(daemon)* one capture per process in the world, and its write oracle on the world's own state (fifth re-check, CI red)
- *(source-postgres)* spike S5, the source confirms what no lane needs

## [0.18.0](https://github.com/alias2k/flusso/compare/flusso-source-postgres-v0.17.0...flusso-source-postgres-v0.18.0) - 2026-11-19

### Added

- *(engine,daemon,source-postgres)* [**breaking**] a stop drains what is in flight before the run ends
- *(bench)* review-ab --digest and --plans over the cases roots ([#347](https://github.com/alias2k/flusso/pull/347))
- *(bench)* review-ab shows each side's range; a wide composite-key root and a never-analysed root
- *(bench)* just review-ab compares a branch with its base on the Docker-backed benches
- *(source)* warn on a timestamp format that refuses the ISO form
- *(design)* author array columns ([#276](https://github.com/alias2k/flusso/pull/276))
- *(source)* [**breaking**] validate array columns against the catalog ([#276](https://github.com/alias2k/flusso/pull/276))
- *(source-postgres)* clean up an array column's value as the document is decoded ([#276](https://github.com/alias2k/flusso/pull/276))
- *(source-postgres)* per-element transforms on an array column ([#276](https://github.com/alias2k/flusso/pull/276))
- *(kernel)* a column may hold an array of its type ([#276](https://github.com/alias2k/flusso/pull/276))

### Fixed

- *(engine)* a snapshot row is its own document, not a change to resolve
- *(engine,source-postgres)* a stop that can't confirm its final position fails; a deferred sink refreshes past skipped batches
- *(source-postgres)* a stop right after the start doesn't wait for a status update
- *(cli)* bound the stop, honour a second signal and SIGHUP, follow the parent
- *(script,source-postgres)* a declared float in --input is a float; jsonb zero padding is not digits
- *(source-postgres)* a float reads as Postgres casts to float4; exact reads pinned in flusso-query
- *(source-postgres)* an untyped number is a decimal to a script at every magnitude
- *(source-postgres)* an ids of float keys reads each key as an f32
- *(source-postgres)* read and write every number exactly
- *(source-postgres)* date a snapshot by its read, not by when the stream took it
- *(source-postgres)* a snapshot task that can't miss a commit; bounded reads; one hold clock
- *(source-postgres)* [**breaking**] emit a change only once its transaction is visible
- *(bench)* a plan check that cannot read its input is a setup error ([#347](https://github.com/alias2k/flusso/pull/347))
- *(bench)* the review harness is a bench target of its own ([#347](https://github.com/alias2k/flusso/pull/347))
- *(test)* the guard rejects the postgres import, not call shapes
- *(test)* every test and bench starts Postgres through dev/test-postgres
- *(source-postgres)* [**breaking**] a filter value is never cut or rounded to its column's size
- *(bench)* review-ab only adopts a pre-marker cache with a real stamp and nothing foreign; --clean-results apart from --clean
- *(bench)* review-ab's spread narrows with more rounds; --clean deletes only its own cache; owner-labelled containers; cargo config in the stamp
- *(bench)* review-ab re-runs a flagged case before calling it slower, alternates sides, and runs one at a time
- *(source-postgres)* a batch whose prepared statement changed result type reruns once
- *(source-postgres)* write a batch's keys as an array constructor, padded to a power of two
- *(source-postgres)* a shared key whose document reads only its key orders nothing
- *(source-postgres)* choose a shared key's row by its primary key or the columns the document reads
- *(source-postgres)* a batch by key reruns by position when its cached key type is stale
- *(source-postgres)* a name key pairs by position, since its cast cuts a requested key
- *(source-postgres)* pair a batch's positions in a full join, which never runs as a nested loop
- *(source-postgres)* the source's own sessions run without JIT
- *(source-postgres)* build the batch at the root's own level and pair positions one level out
- *(source-postgres)* name the batched build's key list so no table's column meets it
- *(source-postgres)* a batched build's root probe returns one row, so it is never JIT-compiled
- *(source)* say what an unreadable root key type costs now
- *(source-postgres)* read a key type's name as the qualified identifier it is
- *(source-postgres)* a role that may not run has_schema_privilege still reads its key types
- *(source-postgres)* Postgres decides which row answers which requested key
- *(source-postgres)* mirror real, boolean, time and uuid casts in the batched build's matching
- *(source-postgres)* a key of a domain over a type the role may not use keeps indexing
- *(source-postgres)* the batched build serves the ids a single build serves, across key kinds
- *(source-postgres)* a numeric key equal to its row's is built, not tombstoned
- *(source-postgres)* a key is never cast to a size, and a resize refreshes the cached types
- *(source-postgres)* statements survive a column type change
- *(source-postgres)* only a declared decimal changes what the sink receives
- *(source-postgres)* [**breaking**] write every numeric digit as a JSON number
- *(source-postgres)* a refused link probe is cached like a verdict ([#324](https://github.com/alias2k/flusso/pull/324))
- *(source)* a check the database would not let run is reported, not dropped ([#324](https://github.com/alias2k/flusso/pull/324))
- *(source)* [**breaking**] a schema that can never index a document stops the start (#316, #309)
- *(source-postgres)* a Relation that omits a column is no evidence its type changed ([#298](https://github.com/alias2k/flusso/pull/298))
- *(source-postgres)* a table's Relation message drops its cached column types ([#298](https://github.com/alias2k/flusso/pull/298))
- *(source-postgres)* a failed document query forgets its tables' column types ([#298](https://github.com/alias2k/flusso/pull/298))
- *(source-postgres)* wrap an array of a bytea domain; keep the session SET as a backstop ([#298](https://github.com/alias2k/flusso/pull/298))
- *(source-postgres)* a binary field holds the column's bytes whatever bytea_output is ([#298](https://github.com/alias2k/flusso/pull/298))
- *(source-postgres)* say a lossy ids key may be stored altered; check prints a skipped probe (#299, #313)
- *(source-postgres)* probe a type pair once, in one round trip; pin exact ids pairs (#299, #313)
- *(source-postgres)* check reports a link or ids element the key can't take (#299, #312, #313)
- *(query-derive)* refuse by name only what can't be a string type of your own
- *(query-derive)* say only what the derive promises; make the boundary test cheap
- *(source-postgres)* resolve each change as it is buffered again ([#295](https://github.com/alias2k/flusso/pull/295))
- *(source-postgres)* compare a resolved value as the join compares its column ([#295](https://github.com/alias2k/flusso/pull/295))
- *(source-postgres)* parse years past 9999, so their ids keep main's form ([#295](https://github.com/alias2k/flusso/pull/295))
- *(source-postgres)* read a batch's root keys back in one query per index ([#295](https://github.com/alias2k/flusso/pull/295))
- *(source-postgres)* set no startup options, so PgBouncer passes ([#295](https://github.com/alias2k/flusso/pull/295))
- *(source-postgres)* a link names the root's own document, however it spells the key ([#295](https://github.com/alias2k/flusso/pull/295))
- *(source-postgres)* read a key back as the text the WAL sends ([#295](https://github.com/alias2k/flusso/pull/295))
- *(source-postgres)* refuse a jsonb or json root key ([#295](https://github.com/alias2k/flusso/pull/295))
- *(source-postgres)* read every key type the same way in backfill, WAL and build
- *(source-postgres)* read bytea as hex whatever the database's bytea_output
- *(source-postgres)* a constant builds the value the schema holds
- *(source-postgres)* index a binary column as its bytes, not Postgres hex text
- *(cli)* script run reports a document that fails to build as the failure
- *(source)* a domain over an array or jsonb holds an array
- review findings on array columns ([#276](https://github.com/alias2k/flusso/pull/276))
- *(source-postgres)* a transform probe the role cannot run is not a verdict
- *(source)* [**breaking**] settle the root-key kinds; probe transforms without a table
- *(source)* report four schemas check accepted that cannot work

### Other

- comments say what the code does, not which review or stage changed it (review L4)
- a mark records the comparison operator the walk used, so one taken with the bare `<` is never resumed with the qualified one; compare_keys' doc says an unplaced lane starts from the first key
- the source marks a walk's point and resumes after it in its own key order (walk_mark, resume_after, compare_keys)
- changes are resolved on a connection of their own (step 2)
- review follow-ups: no watermark left, wal_idle's half named, the argument per lane
- a walk ends at the greatest key it found, read through the key index's own operators
- READMEs and a doc link no longer name the stream's watermark
- rustfmt the review harness
- wal_idle confirms Idle once a second, as the engine's tick does
- the source confirms and fences from Landed; the Postgres capture keeps no map
- Position carries its seq and the source's stamps (touches all constructors)
- a backfill pages each index's root keys on the builder pool
- *(source-postgres)* an untyped whole-number double is the integer it reads as
- *(source-postgres)* name the untyped number rule the reader follows
- *(source-postgres)* a commit arriving as the snapshot task stops is still checked
- *(source-postgres)* keep a snapshot fresh while transactions await a check
- *(source-postgres)* read the visibility snapshot on its own task
- *(source-postgres)* measure a small batch's plan on one connection
- *(source-postgres)* a small batch's plan test refuses a server that counts no plans
- *(source-postgres)* a small batch's cached plan is measured the same way on every major
- trailing spaces on varchar(n), an array of a sized domain, rewrapped lines
- *(source-postgres)* a fitting filter value still matches its row for every sized type
- *(source-postgres)* a filter value a domain's CHECK refuses matches nothing
- *(source-postgres)* a domain filter compares as written for a role that may not probe schemas
- *(source-postgres)* a filter value that does not fit its column is compared as written
- *(source-postgres)* is_partitioned takes the name in the form each caller holds
- *(source-postgres)* the differential stores three uuid rows, so a key mix-up shows there too
- *(source-postgres)* a batched statement takes its shape whole, and the proptests draw both key forms
- *(source-postgres)* one query answers whether a table is partitioned
- *(source-postgres)* pruning is checked past the small-batch size, and plans are read once logged
- *(source-postgres)* the differential reaches both ways a batch sends its keys, and partitioned roots
- *(source-postgres)* a small batch's plan count is bounded per connection, not per call
- *(source-postgres)* large batches on an unpartitioned root pass their keys as array parameters
- *(source-postgres)* a batch survives its key column changing from text to name
- *(source-postgres)* a small batch reuses its plan, and a generic plan still prunes partitions
- *(source-postgres)* a shared key whose document reads only its key still builds
- *(source-postgres)* which connections set jit = off, and what it costs and gains
- *(source-postgres)* pass a batch's keys as one array parameter per kind
- *(source-postgres)* choosing a shared key's row reads no column the document does not read
- *(source-postgres)* a batch by key with a stale type cache still finds its rows
- *(source-postgres)* the differential covers name keys and keys past 63 bytes
- *(source-postgres)* pair a batch by key where the key's = is identity
- *(source-postgres)* a batched build pairs positions without a nested loop
- *(source-postgres)* the batched build's scope, the builder's jit setting and duplicate keys
- *(source-postgres)* the batched build's chunk and key check no longer describe an IN list
- *(source-postgres)* a raw filter reads the root row's columns in a batched build
- *(source-postgres)* a batched build on a partitioned root is not JIT-compiled
- *(source-postgres)* number the batched build's keys with unnest WITH ORDINALITY
- *(source-postgres)* a generated build/build_many differential, a revoked privilege probe, offset-less timestamptz text, quoted key type names
- *(source-postgres)* both build paths agree for real, uuid, bool and time keys too
- *(source-postgres)* both build paths agree across key kinds; a domain over a type the role may not use
- *(source-postgres)* a BigDecimal key binds as text and matches its row by its text
- *(source-postgres)* keep resolve's happy path as on main; one mismatch message for both query kinds
- *(source-postgres)* pin resized keys, numeric keys of another scale and the resolve mismatch message
- *(source-postgres)* match a batched build's rows to ids in one function
- *(source-postgres)* widen the differential corpus to 7605 cases
- *(source-postgres)* a wide document's first failure is the first declared
- only a declared decimal may change what the sink receives
- a numeric keeps every digit as a JSON number; a non-finite one fails the document
- the resolve bench's walked path, the per-path root-link answer and the generation
- *(source-postgres)* look up an already-warned unread table without a clone or an exclusive lock
- *(source-postgres)* a change on an unread table warns once per table
- *(source-postgres)* find each path once per builder and keep its root-link answer
- *(source-postgres)* count the catalog lookups and path searches resolve makes per change
- *(source-postgres)* bench a path walked from the key, and an unread table after its warning
- *(source-postgres)* seed the self-ancestor set only when the root table has paths
- *(source-postgres)* read root keys back only for a change that has links
- *(source-postgres)* bench resolving the changes that need no query, in process
- *(source-postgres)* a refused link probe is asked once per type pair ([#324](https://github.com/alias2k/flusso/pull/324))
- *(source)* a check the database would not let run is reported ([#324](https://github.com/alias2k/flusso/pull/324))
- *(source-postgres)* a Relation that omits a cached column keeps the cached types ([#298](https://github.com/alias2k/flusso/pull/298))
- *(source-postgres)* a link or ids element of a type the key can't take (#299, #312, #313)
- warn on a timestamp format that refuses the ISO form; pin tuned mappings
- *(query-derive)* typed-map shapes the review found still compile and fail ([#294](https://github.com/alias2k/flusso/pull/294))
- *(source-postgres)* restore the recorded pgoutput fixture ([#295](https://github.com/alias2k/flusso/pull/295))
- *(source-postgres)* a jsonb or "char" root key is refused ([#295](https://github.com/alias2k/flusso/pull/295))
- *(source-postgres)* every key type in every relation position ([#295](https://github.com/alias2k/flusso/pull/295))
- *(source-postgres)* array columns against a live Postgres ([#276](https://github.com/alias2k/flusso/pull/276))

## [0.17.0](https://github.com/alias2k/flusso/compare/flusso-source-postgres-v0.16.0...flusso-source-postgres-v0.17.0) - 2026-11-12

### Added

- *(source-postgres)* set REPLICA IDENTITY FULL on every table an index reads
- *(source)* validate_indexes checks every table and column a relation names
- *(engine)* a document that fails to build is stopped on or skipped by its index's policy
- *(schema)* an aggregate's column can be an arithmetic expression
- *(kernel)* script fields and input-only fields in the schema model
- *(source-postgres)* apply on: pairs in the document query and the read-column sets
- *(config)* on: column pairs on joins, aggregates and via hops
- *(schema)* aggregates across several hops with via, and the exists op
- *(source-postgres)* positions answer how far a sink has reached in WAL terms
- *(source-postgres)* resolution skips what an update's changed columns can't affect
- *(source-postgres)* an update with a pre-image carries its post-image
- *(kernel)* subfields and indexed opt-outs on columns and joins, hashed only when off

### Fixed

- *(source-postgres)* resolve a hop from the key when it carries the link
- *(source-postgres)* word the key gap for live writes; skip intermediate partitioned tables
- *(source-postgres)* emit a change without its key and never address a document by it
- *(source-postgres)* set a partition whose replica identity leaves out the primary key
- *(run)* refuse to start only on what cannot work as written
- *(source-postgres)* keep a USING INDEX identity only when it carries the primary key
- *(source-postgres)* keep a USING INDEX replica identity that carries every link
- *(check)* verify the soft-delete marker, and make run refuse to start on an error diagnostic
- *(source-postgres)* hand a confirmation to the replication client at once and report it every second
- *(source-postgres)* leave a link-free partitioned table alone, one info line for unset tables; check's verdict says where it won't set
- *(source-postgres)* a zero divisor in an aggregate expression nulls the row instead of failing the build
- review — report script failures under both policies, ids in print context, debug log of the failing document, shared build-by-id, field docs
- review findings for via and exists
- review findings for the fence
- *(source)* the soft-delete field resolves depth-first, as on main
- *(source)* a raw filter anywhere in an index turns the skip off for it
- *(source-postgres)* a partitioned table's update carries no post-image
- *(source-postgres)* a root-table change also rebuilds the documents that embed it ([#167](https://github.com/alias2k/flusso/pull/167))
- *(source-postgres)* set each partition's replica identity in its own transaction
- *(source-postgres)* read publication membership from the catalog
- *(source-postgres)* ensure the publication before the slot, and add a lone partition's parent
- *(source-postgres)* warn once when a change arrives for a table no index reads
- *(source-postgres)* set replica identity across a partitioned table's tree
- *(source-postgres)* publish partitioned tables via their root

### Other

- Merge remote-tracking branch 'origin/main' into chore/pgwire-replication-0-5
- *(source-postgres)* the URL's application_name does not rename the replication connection; reword the oversized-message hint
- *(source-postgres)* upgrade pgwire-replication to 0.5
- Merge pull request #222 from alias2k/fix/validate-soft-delete-and-run
- say a kept identity carries the primary key wherever it says the link
- say the root manual is the newest release and /main/ tracks main
- *(source-postgres)* name the one select case that loses the worker's outcome
- Merge remote-tracking branch 'origin/main' into feat/script-fields
- REPLICA IDENTITY FULL on every table an index reads — what, cost, opt-out, the 0.17 upgrade step
- REPLICA IDENTITY FULL by default on the root and a belongs_to target
- *(source-postgres)* check reports a typo in every relation position
- on: pairs in the joins and aggregates references, the guides, the plugin skill and CLAUDE.md
- crate READMEs for fences (source, sinks, engine, daemon, flusso-query)
- the unread-column fixtures set the indexed and subfields flags
- an update that changes no read column rebuilds nothing, and needs REPLICA IDENTITY FULL
- *(engine,source-postgres)* assert the row's own document and guard self-join cycles
- *(source-postgres)* review fixes for the self-join resolve ([#167](https://github.com/alias2k/flusso/pull/167))
- a join back at the root table rebuilds the documents that embed the row ([#167](https://github.com/alias2k/flusso/pull/167))
- *(source-postgres)* a root-table change resolves its self-referencing parents ([#167](https://github.com/alias2k/flusso/pull/167))
- partitioned tables publish via their root
- *(source-postgres)* partitioned tables end to end

## [0.16.0](https://github.com/alias2k/flusso/compare/flusso-source-postgres-v0.15.1...flusso-source-postgres-v0.16.0) - 2026-10-09

### Added

- *(kernel)* distinct on aggregates and filters on a through junction
- *(source-postgres)* set REPLICA IDENTITY FULL on child tables that need it
- *(source-postgres)* add manage_replica_identity (default on) and its --manage-replica-identity override
- *(source)* report tables whose replica identity drops the parent link
- *(source)* carry the pre-image on a change and resolve the old parent from it
- *(bench)* in-process PR-gating benches — engine loop over mocks, pgoutput decode over a recorded fixture, sink render
- *(engine)* [**breaking**] build-once ingest engine and one sink engine per sink over a bidirectional stream
- *(cli)* generate the editor schema and Reference tables from adapter descriptions
- *(config)* [**breaking**] adapters own their configuration; port entries are type + options
- *(query-derive)* [**breaking**] deprecate path= instead of deleting it
- improve upon claude skills
- add the publication management
- change the "run" command to follow the "cargo" pattern of updating the "lock" file
- DEPLOY.md for docker recipie
- license, security, coc, contributing and github templates
- rename flusso-search to flusso-query
- move files to correctly reflect dependencies
- create alias pointing to the latest index
- add belong_to
- start client, add geo
- rename config.toml to flusso.toml and flusso.bin to flusso.lock
- add compile functionality
- better opensearch defaults
- improve env vars handling and readme about sinks and sources
- backfill and renaming
- rebranding to storno
- update deps
- documentation

### Fixed

- address review — destructured Hash, empty junction filters keep the hash, stale junction filters cleared in the designer
- *(source-postgres)* aggregates over through honour distinct, junction filters, and ids filters
- *(source-postgres)* a USING INDEX identity must carry the primary key to count
- *(source-postgres)* set replica identity before the slot; never alter a partitioned table
- *(source-postgres)* address review — precise warn-once, safe keying under USING INDEX
- *(source-postgres)* key changes by the catalog primary key and decode the pre-image
- *(engine)* max_delay caps a burst inside the drain; max_changes is non-zero
- *(engine)* [**breaking**] review fixes — positions monotonic across streams, per-sink envelope stamping, supervisor loop

### Other

- *(source-postgres)* e2e for distinct and junction filters over a junction entity
- flusso sets child tables' replica identity itself
- e2e for ensure_pre_image and an auto-managed pipeline
- child tables need their parent link in the replica identity
- e2e for deleted and re-parented children and replica-identity gaps
- *(bench)* pipeline bench is attribution only — drop its backfill group, trim the Docker-backed timings
- *(libs)* describe the two engines and the ticketed channel ledger in the crate docs
- describe the kernel / ports / adapters layout and adapter-owned config
- *(arch)* [**breaking**] lay out kernel / ports / adapters / engine / daemon and sdk/
- root README as a pitch, flusso-query README as a landing; track the Start here pages
- the lock is deterministic TOML; formats frozen for the major
- source TLS — configuration guide section, README, crate README, plugin skills
- document the visual schema designer
- lead README with the tagline, move AI disclosure below it
- consistency pass — fix factual errors and terminology drift
- rewrite all docs to the prose style guide
- clarify pre-commit hook formats the whole workspace
- add dev workflow tooling
- split into an mdBook manual + per-crate READMEs
- Merge pull request #9 from alias2k/feature/run-command-rewamp
- preparation for going public
- documentation
- add requirements section
- add SCHEMA.md
- readme
- readme and cleanup
- README

## [0.14.0](https://github.com/alias2k/flusso/compare/flusso-sources-postgres-v0.13.0...flusso-sources-postgres-v0.14.0) - 2026-08-14

### Added

- *(query-derive)* [**breaking**] deprecate path= instead of deleting it

## [0.13.0](https://github.com/alias2k/flusso/compare/flusso-sources-postgres-v0.12.0...flusso-sources-postgres-v0.13.0) - 2026-07-24

### Added

- *(core)* carry declared enum order on Column + Mapping.enum_order

## [0.11.0](https://github.com/alias2k/flusso/compare/flusso-sources-postgres-v0.10.1...flusso-sources-postgres-v0.11.0) - 2026-07-23

### Added

- *(design)* sample-document preview from a live row
- *(sources-postgres)* implement SchemaIntrospection over pg_catalog

### Fixed

- *(design)* unknown column is a diagnostic, not 'database not reachable' + theme the search bar

### Other

- consolidate per-project .gitignore into a single root file
- document the visual schema designer

## [0.10.1](https://github.com/alias2k/flusso/compare/flusso-sources-postgres-v0.10.0...flusso-sources-postgres-v0.10.1) - 2026-06-26

### Other

- lead README with the tagline, move AI disclosure below it
- consistency pass — fix factual errors and terminology drift
- rewrite all docs to the prose style guide
- clarify pre-commit hook formats the whole workspace
- add dev workflow tooling
- split into an mdBook manual + per-crate READMEs

## [0.9.1](https://github.com/alias2k/flusso/compare/flusso-sources-postgres-v0.9.0...flusso-sources-postgres-v0.9.1) - 2026-06-24

### Fixed

- *(sources-postgres)* decode WAL keys to match the read-back variant

## [0.7.0](https://github.com/alias2k/flusso/compare/flusso-sources-postgres-v0.6.0...flusso-sources-postgres-v0.7.0) - 2026-06-22

### Fixed

- *(schema)* honor `required` on to-one joins instead of forcing nullable ([#37](https://github.com/alias2k/flusso/pull/37))

## [0.5.0](https://github.com/alias2k/flusso/compare/flusso-sources-postgres-v0.4.0...flusso-sources-postgres-v0.5.0) - 2026-06-22

### Added

- *(sources-postgres)* decode map columns as passthrough JSON

### Fixed

- add chunking to postgres

## [0.4.0](https://github.com/alias2k/flusso/compare/flusso-sources-postgres-v0.3.2...flusso-sources-postgres-v0.4.0) - 2026-06-19

### Added

- *(postgres)* coerce the document body to typed values by schema

### Fixed

- *(postgres)* cast keyed-lookup operands to their catalog type

### Other

- *(postgres)* update config_coverage e2e for typed values
- move inline unit tests into sibling tests.rs modules
- *(core)* typed canonical value vocabulary between source and sink
- *(postgres)* uuid-key e2e for build/build_many/resolve

### Fixed

- Keyed document lookup now casts each key/foreign-key operand to its column's
  catalog type (`$n::<type>`), so a `uuid` (or `date`/`timestamptz`/…) primary or
  foreign key no longer fails backfill and live capture with
  `operator does not exist: uuid = text`. Previously only filter operands were
  cast; keys were bound as `text`.

### Changed

- Decode each column into its typed canonical `GenericValue` variant
  (`uuid`→`Uuid`, `date`→`Date`, `int4`→`Int`, `int8`→`BigInt`, `bytea`→`Bytes`,
  …) instead of collapsing temporals/uuids to strings, and bind those typed
  values to Postgres natively. The batched lookup selects its key column raw (not
  `to_json`-wrapped) so the returned key matches the requested typed key exactly.
- The document **body** (assembled server-side as JSON) is now coerced to typed
  canonical values by each field's declared `FlussoType` — a `date` field reaches
  the sink as a `Date`, a `uuid` as a `Uuid`, a `bigint` as a `BigInt`, joins and
  aggregates recursively — so the whole document is typed end-to-end, not just its
  keys. A value that doesn't parse falls back to its untyped JSON shape rather
  than failing the document.

## [0.3.1](https://github.com/alias2k/flusso/compare/flusso-sources-postgres-v0.3.0...flusso-sources-postgres-v0.3.1) - 2026-06-19

### Added

- improve upon claude skills

## [0.3.0](https://github.com/alias2k/flusso/compare/flusso-sources-postgres-v0.2.0...flusso-sources-postgres-v0.3.0) - 2026-06-18

### Added

- add the ids field support

## [0.2.0](https://github.com/alias2k/flusso/compare/flusso-sources-postgres-v0.1.1...flusso-sources-postgres-v0.2.0) - 2026-06-18

### Added

- add the publication management

### Other

- code hygene, removed useless comments

## [0.1.1](https://github.com/alias2k/flusso/compare/flusso-sources-postgres-v0.1.0...flusso-sources-postgres-v0.1.1) - 2026-06-17

### Added

- change the "run" command to follow the "cargo" pattern of updating the "lock" file

### Other

- Merge pull request #9 from alias2k/feature/run-command-rewamp
