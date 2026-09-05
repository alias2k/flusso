# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.16.0](https://github.com/alias2k/flusso/compare/flusso-source-postgres-v0.15.1...flusso-source-postgres-v0.16.0) - 2026-09-05

### Added

- *(bench)* in-process PR-gating benches — engine loop over mocks, pgoutput decode over a recorded fixture, sink render
- *(engine)* [**breaking**] build-once ingest engine and one sink engine per sink over a bidirectional stream
- *(cli)* generate the editor schema and Reference tables from adapter descriptions
- *(config)* [**breaking**] adapters own their configuration; port entries are type + options
- *(query-derive)* [**breaking**] deprecate path= instead of deleting it
- improve upon claude skills
- add the publication management
- change the "run" command to follow the "cargo" pattern of updating the "lock" file
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

- *(engine)* [**breaking**] review fixes — positions monotonic across streams, per-sink envelope stamping, supervisor loop

### Other

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
