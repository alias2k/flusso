# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.18.0](https://github.com/alias2k/flusso/compare/flusso-config-v0.17.0...flusso-config-v0.18.0) - 2026-11-14

### Added

- *(config)* array: true on a scalar, enum, custom or json column ([#276](https://github.com/alias2k/flusso/pull/276))
- *(kernel)* a column may hold an array of its type ([#276](https://github.com/alias2k/flusso/pull/276))
- *(sink-opensearch)* trust a private CA with ca_file; never retry a refused certificate

### Fixed

- *(config)* an array default's refused element reads "an object element"
- review findings on array columns ([#276](https://github.com/alias2k/flusso/pull/276))

### Other

- *(config)* the golden fixture names each root field once

## [0.17.0](https://github.com/alias2k/flusso/compare/flusso-config-v0.16.0...flusso-config-v0.17.0) - 2026-11-12

### Added

- *(query)* [**breaking**] move every HTTP client to reqwest 0.13
- *(schema)* an aggregate's column can be an arithmetic expression
- *(config)* parse script, array and emit in *.schema.yml
- *(kernel)* script fields and input-only fields in the schema model
- *(config)* on: column pairs on joins, aggregates and via hops
- *(schema)* aggregates across several hops with via, and the exists op
- *(config)* subfields and indexed siblings in *.schema.yml
- *(kernel)* subfields and indexed opt-outs on columns and joins, hashed only when off
- *(cli)* editor schema and Reference table for per-index sink options
- *(config)* per-index sink options under [index.sinks.<name>]

### Fixed

- *(source-postgres)* keep a USING INDEX identity only when it carries the primary key
- *(source-postgres)* keep a USING INDEX replica identity that carries every link
- *(config)* an expression's sum/min/max needs a numeric value_type
- *(kernel)* every accepted expression re-parses from its canonical text
- review findings for on: pairs
- *(config)* keep ConversionError small enough for clippy
- *(config)* via rejections name the aggregate field
- *(config)* say subfields can be turned off only on text, keyword or uuid
- *(kernel)* [**breaking**] a map carries its keys' plain-field mapping and a new content hash
- review fixes for index settings (owned blocks/hidden, shared schema hoisting)

### Other

- say the root manual is the newest release and /main/ tracks main
- Merge remote-tracking branch 'origin/main' into feat/script-fields
- REPLICA IDENTITY FULL on every table an index reads — what, cost, opt-out, the 0.17 upgrade step
- *(config)* point the entity docs at FieldKind
- read-your-writes in the manual (GET /wait, the status positions, the query chapter)
- *(sink)* index settings need a restart before the reindex, and some weaken guarantees
- *(config)* index settings never change the content hash

## [0.16.0](https://github.com/alias2k/flusso/compare/flusso-config-v0.15.1...flusso-config-v0.16.0) - 2026-10-09

### Added

- *(config)* parse distinct and through.filters on aggregates and many_to_many
- *(kernel)* distinct on aggregates and filters on a through junction
- *(source-postgres)* add manage_replica_identity (default on) and its --manage-replica-identity override
- *(config)* a [batch] table for the ingest engine's batching
- *(engine)* [**breaking**] build-once ingest engine and one sink engine per sink over a bidirectional stream
- *(cli)* generate the editor schema and Reference tables from adapter descriptions
- *(config)* [**breaking**] adapters own their configuration; port entries are type + options
- *(kernel)* [**breaking**] options tree, AdapterConfig, Secret as written, lock format 3
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

- *(engine)* max_delay caps a burst inside the drain; max_changes is non-zero

### Other

- *(config)* pin distinct and through.filters in the golden lock
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
