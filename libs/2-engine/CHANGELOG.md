# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.16.0](https://github.com/alias2k/flusso/compare/flusso-engine-v0.15.1...flusso-engine-v0.16.0) - 2026-09-05

### Added

- *(bench)* in-process PR-gating benches — engine loop over mocks, pgoutput decode over a recorded fixture, sink render
- *(engine)* [**breaking**] build-once ingest engine and one sink engine per sink over a bidirectional stream
- *(config)* [**breaking**] adapters own their configuration; port entries are type + options
- *(sources,engine)* report source continuity and rebuild seeds it invalidates

### Fixed

- *(engine)* coalesce backfill requests by lane, not a fixed window
- *(engine)* [**breaking**] review fixes — positions monotonic across streams, per-sink envelope stamping, supervisor loop
- *(engine)* stage stale-seed rebuilds before the resume point exists

### Other

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
