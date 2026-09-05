# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.16.0](https://github.com/alias2k/flusso/compare/flusso-config-v0.15.1...flusso-config-v0.16.0) - 2026-09-05

### Added

- *(engine)* [**breaking**] build-once ingest engine and one sink engine per sink over a bidirectional stream
- *(cli)* generate the editor schema and Reference tables from adapter descriptions
- *(config)* [**breaking**] adapters own their configuration; port entries are type + options
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

### Other

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

## [0.14.0](https://github.com/alias2k/flusso/compare/flusso-schema-v0.13.0...flusso-schema-v0.14.0) - 2026-08-14

### Added

- *(query-derive)* [**breaking**] deprecate path= instead of deleting it

## [0.13.0](https://github.com/alias2k/flusso/compare/flusso-schema-v0.12.0...flusso-schema-v0.13.0) - 2026-07-24

### Added

- *(schema)* parse an optional `variants` order on the enum tag

### Other

- document declared enum ordering

## [0.11.0](https://github.com/alias2k/flusso/compare/flusso-schema-v0.10.1...flusso-schema-v0.11.0) - 2026-07-23

### Added

- *(schema)* structured index-yaml parse errors with typo suggestions

### Fixed

- *(config)* omit an empty [server] table and order flusso.toml sections

### Other

- document the visual schema designer

## [0.10.1](https://github.com/alias2k/flusso/compare/flusso-schema-v0.10.0...flusso-schema-v0.10.1) - 2026-06-26

### Other

- lead README with the tagline, move AI disclosure below it
- consistency pass — fix factual errors and terminology drift
- rewrite all docs to the prose style guide
- clarify pre-commit hook formats the whole workspace
- fmt
- add dev workflow tooling
- split into an mdBook manual + per-crate READMEs

## [0.7.0](https://github.com/alias2k/flusso/compare/flusso-schema-v0.6.0...flusso-schema-v0.7.0) - 2026-06-22

### Fixed

- *(schema)* reject non-scalar `default` instead of silently dropping it ([#39](https://github.com/alias2k/flusso/pull/39))
- *(schema)* reject `doc_id` instead of silently ignoring it ([#38](https://github.com/alias2k/flusso/pull/38))
- *(schema)* honor `required` on to-one joins instead of forcing nullable ([#37](https://github.com/alias2k/flusso/pull/37))

### Other

- *(fmt)* formatted tests

## [0.5.0](https://github.com/alias2k/flusso/compare/flusso-schema-v0.4.0...flusso-schema-v0.5.0) - 2026-06-22

### Added

- *(schema)* parse and convert the map: field type

## [0.4.0](https://github.com/alias2k/flusso/compare/flusso-schema-v0.3.2...flusso-schema-v0.4.0) - 2026-06-19

### Added

- runtime index prefix to namespace all indices ([#24](https://github.com/alias2k/flusso/pull/24))

### Other

- *(core)* typed canonical value vocabulary between source and sink

### Added

- Deployment-wide `prefix` on `Config` (the index prefix).

## [0.3.1](https://github.com/alias2k/flusso/compare/flusso-schema-v0.3.0...flusso-schema-v0.3.1) - 2026-06-19

### Added

- improve upon claude skills

## [0.3.0](https://github.com/alias2k/flusso/compare/flusso-schema-v0.2.0...flusso-schema-v0.3.0) - 2026-06-18

### Added

- add the ids field support

## [0.2.0](https://github.com/alias2k/flusso/compare/flusso-schema-v0.1.1...flusso-schema-v0.2.0) - 2026-06-18

### Added

- add the publication management

### Other

- code hygene, removed useless comments

## [0.1.1](https://github.com/alias2k/flusso/compare/flusso-schema-v0.1.0...flusso-schema-v0.1.1) - 2026-06-17

### Added

- change the "run" command to follow the "cargo" pattern of updating the "lock" file

### Other

- Merge pull request #9 from alias2k/feature/run-command-rewamp
