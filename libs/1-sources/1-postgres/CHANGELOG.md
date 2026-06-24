# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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
