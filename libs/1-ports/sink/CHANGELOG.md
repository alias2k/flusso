# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.19.0](https://github.com/alias2k/flusso/compare/flusso-sink-v0.18.0...flusso-sink-v0.19.0) - 2026-11-28

### Added

- *(engine)* [**breaking**] stop when another process changed the records, instead of reconciling
- *(daemon)* every reattach's gap is kept exactly as a list, and a gap end the source can't read is settled at a start (gap list, m4)
- *(daemon)* reattach --accept-gap keeps its gap and the fence refuses inside it; records read again once the slot is held (O16, S3, O29)
- *(sink)* begin a rebuild, demote it, and delete a finished rebuild's record at the seed
- *(sink)* record a detach, marker first, every write conditional on what was read

### Fixed

- *(sink-opensearch)* reading the cluster uuid fails as a read
- *(engine)* record-keeping sinks write only stamped, and nothing goes out unread
- *(engine)* judge detach records by who wrote them
- *(engine)* a gap end is settled only in a read while this process holds the slot, the marker's gap write is conditional, and a reattach reuses the open gap a failed delete left and reads the record again before answering (S-a, S-b)
- *(engine)* a live-only record never reads as a finished rebuild, nor one with no known serving copy
- *(engine)* read each block of a seed's refusal on its own, and say what a delete costs
- *(engine)* a seed refused by a blocked old generation parks its lane
- *(engine)* an unreadable detach marker stages as before, and every _mget entry answers for its id
- *(engine,sink-opensearch)* cut a backfill by its target's identity, not one staging's word
- *(engine)* a backfill whose target was created anew no longer seeds it

### Other

- *(daemon)* a rebuild begun again under the same attempts is a different rebuild
- *(engine)* a seed refused by a blocked old generation parks its lane
- *(engine)* pin the review's cases for reading the detach records
- *(engine)* staging reads the detach records first, failing until it does
- *(sink)* an ordered sink's order is per lane; a parked lane is overtaken
- *(engine)* a lane parks alone on a refusal or a write block, and restages alone on a missing target

## [0.18.0](https://github.com/alias2k/flusso/compare/flusso-sink-v0.17.0...flusso-sink-v0.18.0) - 2026-11-19

### Added

- *(engine)* [**breaking**] stage timings account for each task's whole time (#399 review)
- per-stage timings in /metrics and the bench's attribution table ([#399](https://github.com/alias2k/flusso/pull/399))
- *(engine)* stage a sink's indexes concurrently, and validate indexes concurrently

### Fixed

- *(engine)* only an advance of the source is ingest progress; a failed apply drops its batch's buffer (#395 review 3)
- *(source-postgres)* read and write every number exactly
- *(source-postgres)* only a declared decimal changes what the sink receives
- *(source-postgres)* [**breaking**] write every numeric digit as a JSON number
- *(source)* a camelCase format name reads the ISO form; the date table follows the T form ([#304](https://github.com/alias2k/flusso/pull/304))
- *(sink)* [**breaking**] write a naive timestamp as ISO 8601 with a T

### Other

- a restart in the middle of a rebuild says rebuilding, vouching for nothing
- flusso.sink.refreshes{sink}: refresh requests a sink sent; dev/bench reports them per phase
- the sink keeps a backfill checkpoint: Sink::seeding, record_walked, forget_walked; OpenSearch keeps it in the meta doc
- caught up per lane: a caught-up write covers its quiet lanes' indexes, and a lane going quiet makes its own writes searchable (review 4, 5)
- a lane's SnapshotComplete seeds its index as a flight holding a write slot
- tests: ready items merge into one write (red); Sink::write_parts and write_limit
- *(engine)* an ordered sink gets its items in lane order, one at a time (#408 review 2, finding 4)
- *(sink)* [**breaking**] Sink::write replaces apply + flush (#386 stage 2)
- only a declared decimal may change what the sink receives
- a decimal keeps every digit; a non-finite number fails its document
- *(query)* a decimal past Decimal's range is written as a string (#306, #327)
- a naive timestamp must reach OpenSearch as ISO 8601
- *(query-derive)* null map values, date and constant kinds, named non-string types ([#294](https://github.com/alias2k/flusso/pull/294))
- *(query-derive)* typed-map shapes the review found still compile and fail ([#294](https://github.com/alias2k/flusso/pull/294))

## [0.17.0](https://github.com/alias2k/flusso/compare/flusso-sink-v0.16.0...flusso-sink-v0.17.0) - 2026-11-12

### Added

- *(sink)* [**breaking**] a flush reports when its documents become searchable

### Fixed

- review findings for the fence

### Other

- say the root manual is the newest release and /main/ tracks main
- crate READMEs for fences (source, sinks, engine, daemon, flusso-query)
- *(sink-opensearch)* writes through the hash alias, a generation deleted while running, reindex while rebuilding

## [0.16.0](https://github.com/alias2k/flusso/compare/flusso-sink-v0.15.1...flusso-sink-v0.16.0) - 2026-10-09

### Added

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
