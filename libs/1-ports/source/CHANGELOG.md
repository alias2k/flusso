# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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
