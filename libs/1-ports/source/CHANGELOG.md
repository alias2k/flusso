# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.19.0](https://github.com/alias2k/flusso/compare/flusso-source-v0.18.0...flusso-source-v0.19.0) - 2026-11-28

### Added

- *(source)* a stream opened past Rereads::open vouches for nothing (m-1)
- *(engine)* a builder serves a parked lane's fresh build on a substitute lane

### Fixed

- *(cli)* a lock that can't be read is never headlined as missing
- *(source)* a date field over a timestamp column says its format refuses it
- *(source)* a time-of-day format over a date or timestamp is an error
- *(source)* a script that names root can read a level, said as such
- *(source)* a script namesake under an input-only parent is not said to run
- *(source)* [**breaking**] drop the timestamp_format error; show mapping notes the appended pattern
- *(source)* [**breaking**] a namesake pair stored unchecked or read by a script is judged by what holds it
- *(source)* [**breaking**] namesakes whose documents differ from the mapping are an error
- *(source)* required over a view's column is a warning that says why
- *(source)* [**breaking**] a timestamp format OpenSearch refuses is an error
- *(source)* [**breaking**] an index that differs from its schema is an error

### Other

- *(source)* readable_wait keeps its doc; streams_opened says it fails open (fifth re-check m-1, m-2)
- *(daemon)* one capture per process in the world, and its write oracle on the world's own state (fifth re-check, CI red)
- *(source-postgres)* spike S5, the source confirms what no lane needs
- *(cli)* the rewritten line names the lock once; rename a stale test helper
- *(source)* ask whether a table tracks nullability once per table
- *(engine)* a lane item carries its build's own ids, not a key per envelope
- *(source)* state the position contract the engine relies on

## [0.18.0](https://github.com/alias2k/flusso/compare/flusso-source-v0.17.0...flusso-source-v0.18.0) - 2026-11-19

### Added

- *(engine)* stage a sink's indexes concurrently, and validate indexes concurrently
- *(engine,daemon,source-postgres)* [**breaking**] a stop drains what is in flight before the run ends
- *(source)* warn on a timestamp format that refuses the ISO form
- *(source)* [**breaking**] validate array columns against the catalog ([#276](https://github.com/alias2k/flusso/pull/276))
- *(kernel)* a column may hold an array of its type ([#276](https://github.com/alias2k/flusso/pull/276))

### Fixed

- *(engine)* a snapshot row is its own document, not a change to resolve
- *(source-postgres)* [**breaking**] emit a change only once its transaction is visible
- *(source)* say what an unreadable root key type costs now
- *(source-postgres)* Postgres decides which row answers which requested key
- *(source-postgres)* statements survive a column type change
- *(source-postgres)* [**breaking**] write every numeric digit as a JSON number
- *(source)* a check the database would not let run is reported, not dropped ([#324](https://github.com/alias2k/flusso/pull/324))
- *(source)* [**breaking**] a schema that can never index a document stops the start (#316, #309)
- *(source-postgres)* say a lossy ids key may be stored altered; check prints a skipped probe (#299, #313)
- *(source-postgres)* probe a type pair once, in one round trip; pin exact ids pairs (#299, #313)
- *(source-postgres)* check reports a link or ids element the key can't take (#299, #312, #313)
- *(source)* a camelCase format name reads the ISO form; the date table follows the T form ([#304](https://github.com/alias2k/flusso/pull/304))
- *(source-postgres)* resolve each change as it is buffered again ([#295](https://github.com/alias2k/flusso/pull/295))
- *(source-postgres)* read a batch's root keys back in one query per index ([#295](https://github.com/alias2k/flusso/pull/295))
- *(source-postgres)* read every key type the same way in backfill, WAL and build
- *(design,source)* a name applies when the press that left its box ends; key lists by identity
- *(source,design)* duplicate-name outcomes for constants and binary; one-line node header; palette returns focus
- *(design)* refuse a field name its level already has; warn about duplicates; read typed table and column names as flusso does
- *(source)* a domain over an array or jsonb holds an array
- review findings on array columns ([#276](https://github.com/alias2k/flusso/pull/276))
- *(source-postgres)* a transform probe the role cannot run is not a verdict
- *(source)* [**breaking**] settle the root-key kinds; probe transforms without a table
- *(source)* report four schemas check accepted that cannot work

### Other

- a mark records the comparison operator the walk used, so one taken with the bare `<` is never resumed with the qualified one; compare_keys' doc says an unplaced lane starts from the first key
- the source marks a walk's point and resumes after it in its own key order (walk_mark, resume_after, compare_keys)
- review follow-ups: no watermark left, wal_idle's half named, the argument per lane
- a walk ends at the greatest key it found, read through the key index's own operators
- the engines confirm and fence from one Progress; the stream keeps no watermark
- the source confirms and fences from Landed; the Postgres capture keeps no map
- a backfill pages each index's root keys on the builder pool
- *(engine)* snapshot rows are not resolved, and what a faster backfill holds in memory
- *(source)* a check the database would not let run is reported ([#324](https://github.com/alias2k/flusso/pull/324))
- *(source)* a camelCase format name is a name, not a literal pattern ([#304](https://github.com/alias2k/flusso/pull/304))
- warn on a timestamp format that refuses the ISO form; pin tuned mappings

## [0.17.0](https://github.com/alias2k/flusso/compare/flusso-source-v0.16.0...flusso-source-v0.17.0) - 2026-11-12

### Added

- *(source)* SourceSpec::pre_images and a two-severity PreImageReport
- *(source)* validate_indexes checks every table and column a relation names
- *(design)* preview reports script compile errors; the sample runs the scripts on a chosen row
- *(script)* the flusso-script crate: compile, run and strip, behind a DocumentBuilder decorator
- *(engine)* a document that fails to build is stopped on or skipped by its index's policy
- *(schema)* an aggregate's column can be an arithmetic expression
- *(kernel)* script fields and input-only fields in the schema model
- *(source-postgres)* apply on: pairs in the document query and the read-column sets
- *(config)* on: column pairs on joins, aggregates and via hops
- *(schema)* aggregates across several hops with via, and the exists op
- *(source)* a source position a client can wait on, and how far a stream position reaches
- *(source)* changes carry an after-image; the columns each path reads come from one place
- *(kernel)* subfields and indexed opt-outs on columns and joins, hashed only when off

### Fixed

- *(source-postgres)* resolve a hop from the key when it carries the link
- *(source-postgres)* word the key gap for live writes; skip intermediate partitioned tables
- *(source-postgres)* emit a change without its key and never address a document by it
- *(source-postgres)* set a partition whose replica identity leaves out the primary key
- *(run)* refuse to start only on what cannot work as written
- *(source-postgres)* keep a USING INDEX replica identity that carries every link
- *(check)* verify the soft-delete marker, and make run refuse to start on an error diagnostic
- review — report script failures under both policies, ids in print context, debug log of the failing document, shared build-by-id, field docs
- review findings for aggregate expressions
- review findings for on: pairs
- *(source)* the soft-delete field resolves depth-first, as on main
- *(source)* a raw filter anywhere in an index turns the skip off for it
- *(source)* a declared primary key counts as read
- *(source-postgres)* a root-table change also rebuilds the documents that embed it ([#167](https://github.com/alias2k/flusso/pull/167))
- *(source-postgres)* publish partitioned tables via their root

### Other

- Merge pull request #222 from alias2k/fix/validate-soft-delete-and-run
- say a kept identity carries the primary key wherever it says the link
- say the root manual is the newest release and /main/ tracks main
- Merge remote-tracking branch 'origin/main' into feat/script-fields
- *(source)* drop the stray blank line in the columns module doc list
- crate READMEs for fences (source, sinks, engine, daemon, flusso-query)
- the unread-column fixtures set the indexed and subfields flags
- an update that changes no read column rebuilds nothing, and needs REPLICA IDENTITY FULL
- *(source-postgres)* a root-table change resolves its self-referencing parents ([#167](https://github.com/alias2k/flusso/pull/167))

## [0.16.0](https://github.com/alias2k/flusso/compare/flusso-source-v0.15.1...flusso-source-v0.16.0) - 2026-10-09

### Added

- *(kernel)* distinct on aggregates and filters on a through junction
- *(source-postgres)* set REPLICA IDENTITY FULL on child tables that need it
- *(source)* report tables whose replica identity drops the parent link
- *(source)* carry the pre-image on a change and resolve the old parent from it
- *(engine)* [**breaking**] build-once ingest engine and one sink engine per sink over a bidirectional stream
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

- *(source-postgres)* a USING INDEX identity must carry the primary key to count
- *(source-postgres)* set replica identity before the slot; never alter a partitioned table
- *(source-postgres)* address review — precise warn-once, safe keying under USING INDEX

### Other

- child tables need their parent link in the replica identity
- *(libs)* describe the two engines and the ticketed channel ledger in the crate docs
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

## [0.14.0](https://github.com/alias2k/flusso/compare/flusso-sources-core-v0.13.0...flusso-sources-core-v0.14.0) - 2026-08-14

### Added

- *(query-derive)* [**breaking**] deprecate path= instead of deleting it

## [0.13.0](https://github.com/alias2k/flusso/compare/flusso-sources-core-v0.12.0...flusso-sources-core-v0.13.0) - 2026-07-24

### Added

- *(core)* carry declared enum order on Column + Mapping.enum_order

## [0.11.0](https://github.com/alias2k/flusso/compare/flusso-sources-core-v0.10.1...flusso-sources-core-v0.11.0) - 2026-07-23

### Added

- *(sources-core)* add SchemaIntrospection trait for catalog enumeration

### Fixed

- *(design)* unknown column is a diagnostic, not 'database not reachable' + theme the search bar

### Other

- document the visual schema designer

## [0.10.1](https://github.com/alias2k/flusso/compare/flusso-sources-core-v0.10.0...flusso-sources-core-v0.10.1) - 2026-06-26

### Other

- lead README with the tagline, move AI disclosure below it
- consistency pass — fix factual errors and terminology drift
- rewrite all docs to the prose style guide
- clarify pre-commit hook formats the whole workspace
- add dev workflow tooling
- split into an mdBook manual + per-crate READMEs

## [0.7.0](https://github.com/alias2k/flusso/compare/flusso-sources-core-v0.6.0...flusso-sources-core-v0.7.0) - 2026-06-22

### Fixed

- *(schema)* honor `required` on to-one joins instead of forcing nullable ([#37](https://github.com/alias2k/flusso/pull/37))

## [0.4.0](https://github.com/alias2k/flusso/compare/flusso-sources-core-v0.3.2...flusso-sources-core-v0.4.0) - 2026-06-19

### Other

- move inline unit tests into sibling tests.rs modules

## [0.3.1](https://github.com/alias2k/flusso/compare/flusso-sources-core-v0.3.0...flusso-sources-core-v0.3.1) - 2026-06-19

### Added

- improve upon claude skills

## [0.3.0](https://github.com/alias2k/flusso/compare/flusso-sources-core-v0.2.0...flusso-sources-core-v0.3.0) - 2026-06-18

### Added

- add the ids field support

## [0.2.0](https://github.com/alias2k/flusso/compare/flusso-sources-core-v0.1.1...flusso-sources-core-v0.2.0) - 2026-06-18

### Added

- add the publication management

### Other

- code hygene, removed useless comments

## [0.1.1](https://github.com/alias2k/flusso/compare/flusso-sources-core-v0.1.0...flusso-sources-core-v0.1.1) - 2026-06-17

### Added

- change the "run" command to follow the "cargo" pattern of updating the "lock" file

### Other

- Merge pull request #9 from alias2k/feature/run-command-rewamp
