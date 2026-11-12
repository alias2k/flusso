# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.17.0](https://github.com/alias2k/flusso/compare/flusso-sink-opensearch-v0.16.0...flusso-sink-opensearch-v0.17.0) - 2026-11-12

### Added

- *(query)* [**breaking**] move every HTTP client to reqwest 0.13
- *(sink)* every key of a keyword map with subfields gets an exact .keyword subfield
- *(sink-opensearch)* report flush visibility; refresh deferred indexes when caught up
- *(sink-opensearch)* map an unindexed field as a disabled object, honour subfields: false
- *(kernel)* subfields and indexed opt-outs on columns and joins, hashed only when off
- *(sink-opensearch)* index_settings, sink-wide and per index

### Fixed

- *(sink-opensearch)* report a transport error with its cause
- *(sink-opensearch)* the exact map-key subfield follows the same subfield rule as the query derive
- review findings for the fence
- *(sink-opensearch)* map every key of a map like a plain field of its values type
- review fixes for index settings (owned blocks/hidden, shared schema hoisting)
- *(sink-opensearch)* write seeded indexes through the hash alias with require_alias; reindex is a no-op while a rebuild is pending

### Other

- *(sink-opensearch)* one sequential https test, safe under cargo test
- *(sink-opensearch)* pin the sink and flusso-query over https
- say the root manual is the newest release and /main/ tracks main
- Merge pull request #193 from alias2k/feat/keyword-map-exact
- exact operators across a keyword map's keys, the .keyword subfield, and the map key glossary entry
- crate READMEs for fences (source, sinks, engine, daemon, flusso-query)
- read-your-writes in the manual (GET /wait, the status positions, the query chapter)
- *(sink-opensearch)* an explicit empty fields option leaves no subfields
- keep content without indexing it (subfields: false, indexed: false)
- dotted map keys, the field budget and the rebuild's stricter values on upgrade
- *(kernel)* Mapping::of_type is the one place a map key's mapping is built
- map keys are mapped, queried and sorted like a plain field of their values kind
- *(sink-opensearch)* live e2e for typed map keys, coercion and rejections
- *(sink)* index settings need a restart before the reindex, and some weaken guarantees
- build every OpensearchConfig literal with index_settings
- index settings, sink-wide and per index
- *(sink-opensearch)* correct mid-backfill recovery advice and the prefixed auto_create_index pattern
- *(sink-opensearch)* address review — one home for the missing-target story, targets map, logical_names helper
- *(sink-opensearch)* writes through the hash alias, a generation deleted while running, reindex while rebuilding
- *(sink-opensearch)* e2e for a generation deleted mid-run, a second reindex, and orphan sweep

## [0.16.0](https://github.com/alias2k/flusso/compare/flusso-sink-opensearch-v0.15.1...flusso-sink-opensearch-v0.16.0) - 2026-10-09

### Added

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

## [0.14.0](https://github.com/alias2k/flusso/compare/flusso-sinks-opensearch-v0.13.0...flusso-sinks-opensearch-v0.14.0) - 2026-08-14

### Added

- *(query-derive)* [**breaking**] deprecate path= instead of deleting it

## [0.13.0](https://github.com/alias2k/flusso/compare/flusso-sinks-opensearch-v0.12.0...flusso-sinks-opensearch-v0.13.0) - 2026-07-24

### Added

- *(opensearch)* prebake enum sort rank into a .sort subfield
- *(core)* carry declared enum order on Column + Mapping.enum_order

## [0.11.0](https://github.com/alias2k/flusso/compare/flusso-sinks-opensearch-v0.10.1...flusso-sinks-opensearch-v0.11.0) - 2026-07-23

### Other

- document the visual schema designer

## [0.10.1](https://github.com/alias2k/flusso/compare/flusso-sinks-opensearch-v0.10.0...flusso-sinks-opensearch-v0.10.1) - 2026-06-26

### Other

- lead README with the tagline, move AI disclosure below it
- consistency pass — fix factual errors and terminology drift
- rewrite all docs to the prose style guide
- clarify pre-commit hook formats the whole workspace
- add dev workflow tooling
- split into an mdBook manual + per-crate READMEs

## [0.8.0](https://github.com/alias2k/flusso/compare/flusso-sinks-opensearch-v0.7.0...flusso-sinks-opensearch-v0.8.0) - 2026-06-23

### Fixed

- *(query)* per-type numeric kinds + kind-based values for every kind ([#45](https://github.com/alias2k/flusso/pull/45))

## [0.5.0](https://github.com/alias2k/flusso/compare/flusso-sinks-opensearch-v0.4.0...flusso-sinks-opensearch-v0.5.0) - 2026-06-22

### Other

- *(sinks-opensearch)* cover map fields rendering as dynamic objects

## [0.4.0](https://github.com/alias2k/flusso/compare/flusso-sinks-opensearch-v0.3.2...flusso-sinks-opensearch-v0.4.0) - 2026-06-19

### Added

- runtime index prefix to namespace all indices ([#24](https://github.com/alias2k/flusso/pull/24))

### Other

- move inline unit tests into sibling tests.rs modules
- *(core)* typed canonical value vocabulary between source and sink
- *(opensearch)* e2e coverage for the index prefix

### Added

- `with_index_prefix`: prepend a prefix to every index, alias, and the meta index.

## [0.3.1](https://github.com/alias2k/flusso/compare/flusso-sinks-opensearch-v0.3.0...flusso-sinks-opensearch-v0.3.1) - 2026-06-19

### Added

- improve upon claude skills

## [0.3.0](https://github.com/alias2k/flusso/compare/flusso-sinks-opensearch-v0.2.0...flusso-sinks-opensearch-v0.3.0) - 2026-06-18

### Added

- add the ids field support

## [0.2.0](https://github.com/alias2k/flusso/compare/flusso-sinks-opensearch-v0.1.1...flusso-sinks-opensearch-v0.2.0) - 2026-06-18

### Added

- add the publication management

### Other

- code hygene, removed useless comments

## [0.1.1](https://github.com/alias2k/flusso/compare/flusso-sinks-opensearch-v0.1.0...flusso-sinks-opensearch-v0.1.1) - 2026-06-17

### Added

- change the "run" command to follow the "cargo" pattern of updating the "lock" file

### Other

- Merge pull request #9 from alias2k/feature/run-command-rewamp
