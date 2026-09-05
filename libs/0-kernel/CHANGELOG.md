# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.16.0](https://github.com/alias2k/flusso/compare/flusso-kernel-v0.15.1...flusso-kernel-v0.16.0) - 2026-09-05

### Added

- *(engine)* [**breaking**] build-once ingest engine and one sink engine per sink over a bidirectional stream
- *(cli)* generate the editor schema and Reference tables from adapter descriptions
- *(config)* [**breaking**] adapters own their configuration; port entries are type + options
- *(kernel)* #[derive(AdapterConfig)] in flusso-kernel-derive
- *(kernel)* [**breaking**] options tree, AdapterConfig, Secret as written, lock format 3
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

- *(engine)* share published envelopes via Arc, sink stamps own nam
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

## [0.14.0](https://github.com/alias2k/flusso/compare/flusso-schema-core-v0.13.0...flusso-schema-core-v0.14.0) - 2026-08-14

### Added

- *(query-derive)* [**breaking**] deprecate path= instead of deleting it

## [0.13.0](https://github.com/alias2k/flusso/compare/flusso-schema-core-v0.12.0...flusso-schema-core-v0.13.0) - 2026-07-24

### Added

- *(core)* carry declared enum order on Column + Mapping.enum_order

## [0.11.0](https://github.com/alias2k/flusso/compare/flusso-schema-core-v0.10.1...flusso-schema-core-v0.11.0) - 2026-07-23

### Other

- document the visual schema designer

## [0.10.1](https://github.com/alias2k/flusso/compare/flusso-schema-core-v0.10.0...flusso-schema-core-v0.10.1) - 2026-06-26

### Other

- lead README with the tagline, move AI disclosure below it
- consistency pass — fix factual errors and terminology drift
- rewrite all docs to the prose style guide
- clarify pre-commit hook formats the whole workspace
- add dev workflow tooling
- split into an mdBook manual + per-crate READMEs

## [0.8.0](https://github.com/alias2k/flusso/compare/flusso-schema-core-v0.7.0...flusso-schema-core-v0.8.0) - 2026-06-23

### Fixed

- *(query)* per-type numeric kinds + kind-based values for every kind ([#45](https://github.com/alias2k/flusso/pull/45))

## [0.7.0](https://github.com/alias2k/flusso/compare/flusso-schema-core-v0.6.0...flusso-schema-core-v0.7.0) - 2026-06-22

### Fixed

- *(schema)* reject `doc_id` instead of silently ignoring it ([#38](https://github.com/alias2k/flusso/pull/38))
- *(schema)* honor `required` on to-one joins instead of forcing nullable ([#37](https://github.com/alias2k/flusso/pull/37))

## [0.5.0](https://github.com/alias2k/flusso/compare/flusso-schema-core-v0.4.0...flusso-schema-core-v0.5.0) - 2026-06-22

### Added

- *(core)* add first-class map type to the vocabulary

## [0.4.0](https://github.com/alias2k/flusso/compare/flusso-schema-core-v0.3.2...flusso-schema-core-v0.4.0) - 2026-06-19

### Added

- runtime index prefix to namespace all indices ([#24](https://github.com/alias2k/flusso/pull/24))

### Other

- move inline unit tests into sibling tests.rs modules
- *(core)* typed canonical value vocabulary between source and sink

### Added

- `validate_index_prefix` for the deployment-wide index prefix.

### Changed

- **`GenericValue` is now a fully-typed canonical value vocabulary** — the
  middle type between a source and a sink. Numerics are split by width
  (`SmallInt`/`Int`/`BigInt`, `Float`/`Double`, `Decimal`), temporals are split
  (`Date`/`Time`/`Timestamp`/`TimestampTz`), and `Uuid`/`Bytes` are first-class,
  so no semantic type is erased to a string in transit. Serde is now the
  **derived, format-agnostic** representation (externally tagged), so a value
  round-trips losslessly through any queue encoding — in as a `GenericValue`,
  out as the same `GenericValue`. Core picks no wire format.

## [0.3.1](https://github.com/alias2k/flusso/compare/flusso-schema-core-v0.3.0...flusso-schema-core-v0.3.1) - 2026-06-19

### Added

- improve upon claude skills

## [0.3.0](https://github.com/alias2k/flusso/compare/flusso-schema-core-v0.2.0...flusso-schema-core-v0.3.0) - 2026-06-18

### Added

- add the ids field support

## [0.2.0](https://github.com/alias2k/flusso/compare/flusso-schema-core-v0.1.1...flusso-schema-core-v0.2.0) - 2026-06-18

### Added

- add the publication management

### Other

- code hygene, removed useless comments

## [0.1.1](https://github.com/alias2k/flusso/compare/flusso-schema-core-v0.1.0...flusso-schema-core-v0.1.1) - 2026-06-17

### Added

- change the "run" command to follow the "cargo" pattern of updating the "lock" file

### Other

- Merge pull request #9 from alias2k/feature/run-command-rewamp
