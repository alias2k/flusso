# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.18.0](https://github.com/alias2k/flusso/compare/flusso-engine-v0.17.0...flusso-engine-v0.18.0) - 2026-11-14

### Added

- *(kernel)* a column may hold an array of its type ([#276](https://github.com/alias2k/flusso/pull/276))
- *(sink-opensearch)* trust a private CA with ca_file; never retry a refused certificate

### Fixed

- *(source-postgres)* resolve each change as it is buffered again ([#295](https://github.com/alias2k/flusso/pull/295))
- *(source-postgres)* read a batch's root keys back in one query per index ([#295](https://github.com/alias2k/flusso/pull/295))
- *(source-postgres)* read a key back as the text the WAL sends ([#295](https://github.com/alias2k/flusso/pull/295))
- *(design)* refuse a field name its level already has; warn about duplicates; read typed table and column names as flusso does
- *(engine,sink-opensearch)* a rejected document's id is shown shortened; an empty-id delete is dropped
- *(kernel)* store a constant list mixing kinds unindexed, on a new index
- *(source-postgres)* a constant builds the value the schema holds
- *(source-postgres)* index a binary column as its bytes, not Postgres hex text
- *(source)* [**breaking**] settle the root-key kinds; probe transforms without a table

### Other

- *(source-postgres)* a type change is read by its new type without a restart ([#298](https://github.com/alias2k/flusso/pull/298))
- *(source-postgres)* a column changing type mid-run recovers after one restart ([#298](https://github.com/alias2k/flusso/pull/298))
- *(source-postgres)* an array of a bytea domain and a composite read as main does ([#298](https://github.com/alias2k/flusso/pull/298))
- *(source-postgres)* every bytea value reads the same under bytea_output escape ([#298](https://github.com/alias2k/flusso/pull/298))
- *(source-postgres)* a link or ids element of a type the key can't take (#299, #312, #313)
- warn on a timestamp format that refuses the ISO form; pin tuned mappings
- a naive timestamp must reach OpenSearch as ISO 8601
- *(source-postgres)* state what PgBouncer must allow; a refused setting retries ([#295](https://github.com/alias2k/flusso/pull/295))
- *(source-postgres)* a numeric link reads one root key back per change ([#295](https://github.com/alias2k/flusso/pull/295))
- *(source-postgres)* key types after review; PgBouncer; guard transaction mode ([#295](https://github.com/alias2k/flusso/pull/295))
- *(source-postgres)* a link of another type rebuilds what the join reads ([#295](https://github.com/alias2k/flusso/pull/295))
- *(source-postgres)* temporal keys past year 9999 keep main's ids ([#295](https://github.com/alias2k/flusso/pull/295))
- *(source-postgres)* a burst reads root keys back once per batch ([#295](https://github.com/alias2k/flusso/pull/295))
- *(source-postgres)* the pipeline runs through PgBouncer in session mode ([#295](https://github.com/alias2k/flusso/pull/295))
- *(source-postgres)* a link spelled differently rebuilds the root's own document ([#295](https://github.com/alias2k/flusso/pull/295))
- *(source-postgres)* keys under other session settings and out of range ([#295](https://github.com/alias2k/flusso/pull/295))
- *(source-postgres)* every key type in every relation position ([#295](https://github.com/alias2k/flusso/pull/295))
- *(sink-opensearch)* an id OpenSearch refuses fails the whole bulk request
- Merge pull request #249 from alias2k/fix/check-validation-gaps

## [0.17.0](https://github.com/alias2k/flusso/compare/flusso-engine-v0.16.0...flusso-engine-v0.17.0) - 2026-11-12

### Added

- *(query)* [**breaking**] move every HTTP client to reqwest 0.13
- *(engine)* a document that fails to build is stopped on or skipped by its index's policy
- *(schema)* an aggregate's column can be an arithmetic expression
- *(kernel)* script fields and input-only fields in the schema model
- *(config)* on: column pairs on joins, aggregates and via hops
- *(schema)* aggregates across several hops with via, and the exists op
- *(daemon)* a fence waits until a sink has reached a source position
- *(engine)* a sink engine reports its applied and searchable positions
- *(sink)* [**breaking**] a flush reports when its documents become searchable
- *(source-postgres)* resolution skips what an update's changed columns can't affect
- *(kernel)* subfields and indexed opt-outs on columns and joins, hashed only when off

### Fixed

- *(source-postgres)* resolve a hop from the key when it carries the link
- *(source-postgres)* keep a USING INDEX identity only when it carries the primary key
- *(source-postgres)* keep a USING INDEX replica identity that carries every link
- *(source-postgres)* a zero divisor in an aggregate expression nulls the row instead of failing the build
- review — report script failures under both policies, ids in print context, debug log of the failing document, shared build-by-id, field docs
- review findings for via and exists
- *(source)* a raw filter anywhere in an index turns the skip off for it

### Other

- say the root manual is the newest release and /main/ tracks main
- *(engine)* fold the last-flush guard into wal_release with a 5 s confirm bound
- *(engine)* the slot releases WAL within 5 s of the last flush
- *(engine)* exercise the minimum over lanes and split the wal_release bounds
- list the wal_release e2e and its guards
- *(engine)* guard that the replication slot releases WAL flusso has no use for
- Merge remote-tracking branch 'origin/main' into feat/script-fields
- REPLICA IDENTITY FULL by default on the root and a belongs_to target
- *(engine)* run_deployment keeps its doc comment
- *(engine)* aggregate expressions build Postgres arithmetic and rebuild from each operand
- *(engine)* on: pairs rebuild the parent from either side of a pair
- *(engine)* a self-reaching via and a via under a nested relation rebuild every ancestor
- *(engine)* a via aggregate rebuilds its parent from every hop
- *(engine)* a searchable flush with nothing waiting skips the queue and the timer
- crate READMEs for fences (source, sinks, engine, daemon, flusso-query)
- the unread-column fixtures set the indexed and subfields flags
- *(engine)* the recording sink records a batch at its flush
- *(engine)* seed the unread-column case before the slot so the backfill can't race it
- *(engine)* an unread-column update leaves the OpenSearch documents unwritten
- *(engine)* an update of unread columns rebuilds nothing, each read column still does
- *(engine)* prove the guide's root-filtered self-referencing tree end to end
- *(engine,source-postgres)* assert the row's own document and guard self-join cycles
- *(engine)* a self-referencing has_many rebuilds the parent end to end ([#167](https://github.com/alias2k/flusso/pull/167))
- build every OpensearchConfig literal with index_settings

## [0.16.0](https://github.com/alias2k/flusso/compare/flusso-engine-v0.15.1...flusso-engine-v0.16.0) - 2026-10-09

### Added

- *(kernel)* distinct on aggregates and filters on a through junction
- *(source)* carry the pre-image on a change and resolve the old parent from it
- *(engine)* commit a lone change on a quiet stream without waiting out max_delay
- *(bench)* in-process PR-gating benches — engine loop over mocks, pgoutput decode over a recorded fixture, sink render
- *(engine)* [**breaking**] build-once ingest engine and one sink engine per sink over a bidirectional stream
- *(config)* [**breaking**] adapters own their configuration; port entries are type + options
- *(sources,engine)* report source continuity and rebuild seeds it invalidates

### Fixed

- *(source-postgres)* set replica identity before the slot; never alter a partitioned table
- *(engine)* max_delay caps a burst inside the drain; max_changes is non-zero
- *(engine)* coalesce backfill requests by lane, not a fixed window
- *(engine)* [**breaking**] review fixes — positions monotonic across streams, per-sink envelope stamping, supervisor loop
- *(engine)* stage stale-seed rebuilds before the resume point exists

### Other

- e2e for ensure_pre_image and an auto-managed pipeline
- e2e for deleted and re-parented children and replica-identity gaps
- *(engine)* describe quiet-stream batching and max_delay's remaining roles
- *(engine)* a lone change on a quiet stream commits without waiting
- *(bench)* pipeline bench is attribution only — drop its backfill group, trim the Docker-backed timings
- *(engine)* share published envelopes via Arc, sink stamps own nam
- *(daemon)* run the ingest engine on its own task; publish to lanes concurrently
- *(libs)* describe the two engines and the ticketed channel ledger in the crate docs
- *(arch)* [**breaking**] lay out kernel / ports / adapters / engine / daemon and sdk/
- root README as a pitch, flusso-query README as a landing; track the Start here pages
- *(engine)* restart reseeds a deleted generation and rebuilds after a dropped slot
- a seed is only as good as the stream behind it
- *(release)* [**breaking**] three release trains — per-crate versions + version groups
- the lock is deterministic TOML; formats frozen for the major
- source TLS — configuration guide section, README, crate README, plugin skills

## [0.14.0](https://github.com/alias2k/flusso/compare/flusso-engine-v0.13.0...flusso-engine-v0.14.0) - 2026-08-14

### Added

- *(query-derive)* [**breaking**] deprecate path= instead of deleting it

## [0.13.0](https://github.com/alias2k/flusso/compare/flusso-engine-v0.12.0...flusso-engine-v0.13.0) - 2026-07-24

### Added

- *(core)* carry declared enum order on Column + Mapping.enum_order

## [0.11.0](https://github.com/alias2k/flusso/compare/flusso-engine-v0.10.1...flusso-engine-v0.11.0) - 2026-07-23

### Other

- document the visual schema designer

## [0.10.1](https://github.com/alias2k/flusso/compare/flusso-engine-v0.10.0...flusso-engine-v0.10.1) - 2026-06-26

### Other

- lead README with the tagline, move AI disclosure below it
- consistency pass — fix factual errors and terminology drift
- rewrite all docs to the prose style guide
- clarify pre-commit hook formats the whole workspace
- fmt
- add dev workflow tooling
- split into an mdBook manual + per-crate READMEs

## [0.9.1](https://github.com/alias2k/flusso/compare/flusso-engine-v0.9.0...flusso-engine-v0.9.1) - 2026-06-24

### Fixed

- *(sources-postgres)* decode WAL keys to match the read-back variant

### Other

- *(engine)* serialize OpenSearch-backed e2e so CI doesn't starve

## [0.7.0](https://github.com/alias2k/flusso/compare/flusso-engine-v0.6.0...flusso-engine-v0.7.0) - 2026-06-22

### Fixed

- *(schema)* honor `required` on to-one joins instead of forcing nullable ([#37](https://github.com/alias2k/flusso/pull/37))

## [0.4.0](https://github.com/alias2k/flusso/compare/flusso-engine-v0.3.2...flusso-engine-v0.4.0) - 2026-06-19

### Other

- *(core)* typed canonical value vocabulary between source and sink

## [0.3.1](https://github.com/alias2k/flusso/compare/flusso-engine-v0.3.0...flusso-engine-v0.3.1) - 2026-06-19

### Added

- improve upon claude skills

## [0.2.0](https://github.com/alias2k/flusso/compare/flusso-engine-v0.1.1...flusso-engine-v0.2.0) - 2026-06-18

### Added

- add the publication management

### Other

- code hygene, removed useless comments

## [0.1.1](https://github.com/alias2k/flusso/compare/flusso-engine-v0.1.0...flusso-engine-v0.1.1) - 2026-06-17

### Added

- change the "run" command to follow the "cargo" pattern of updating the "lock" file

### Other

- Merge pull request #9 from alias2k/feature/run-command-rewamp
