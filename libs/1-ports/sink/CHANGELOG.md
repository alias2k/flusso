# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.16.0](https://github.com/alias2k/flusso/compare/flusso-sink-v0.15.1...flusso-sink-v0.16.0) - 2026-09-05

### Added

- *(engine)* [**breaking**] build-once ingest engine and one sink engine per sink over a bidirectional stream
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

## [0.14.0](https://github.com/alias2k/flusso/compare/flusso-sinks-core-v0.13.0...flusso-sinks-core-v0.14.0) - 2026-08-14

### Added

- *(query-derive)* [**breaking**] deprecate path= instead of deleting it

## [0.11.0](https://github.com/alias2k/flusso/compare/flusso-sinks-core-v0.10.1...flusso-sinks-core-v0.11.0) - 2026-07-23

### Other

- document the visual schema designer

## [0.10.1](https://github.com/alias2k/flusso/compare/flusso-sinks-core-v0.10.0...flusso-sinks-core-v0.10.1) - 2026-06-26

### Other

- lead README with the tagline, move AI disclosure below it
- consistency pass — fix factual errors and terminology drift
- rewrite all docs to the prose style guide
- clarify pre-commit hook formats the whole workspace
- add dev workflow tooling
- split into an mdBook manual + per-crate READMEs

## [0.4.0](https://github.com/alias2k/flusso/compare/flusso-sinks-core-v0.3.2...flusso-sinks-core-v0.4.0) - 2026-06-19

### Other

- move inline unit tests into sibling tests.rs modules
- *(core)* typed canonical value vocabulary between source and sink

### Changed

- `to_json` renders the typed `GenericValue` variants to the JSON a sink ingests:
  temporals as ISO strings, a UUID as its hyphenated string, and `bytea` as
  base64 (what an OpenSearch `binary` field wants). The base64 lives here, at the
  sink boundary — core's value vocabulary stays format-agnostic.

## [0.3.1](https://github.com/alias2k/flusso/compare/flusso-sinks-core-v0.3.0...flusso-sinks-core-v0.3.1) - 2026-06-19

### Added

- improve upon claude skills

## [0.2.0](https://github.com/alias2k/flusso/compare/flusso-sinks-core-v0.1.1...flusso-sinks-core-v0.2.0) - 2026-06-18

### Added

- add the publication management

## [0.1.1](https://github.com/alias2k/flusso/compare/flusso-sinks-core-v0.1.0...flusso-sinks-core-v0.1.1) - 2026-06-17

### Added

- change the "run" command to follow the "cargo" pattern of updating the "lock" file

### Other

- Merge pull request #9 from alias2k/feature/run-command-rewamp
