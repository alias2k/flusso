# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.19.0](https://github.com/alias2k/flusso/compare/flusso-stream-channel-v0.18.0...flusso-stream-channel-v0.19.0) - 2026-11-28

### Added

- *(engine)* attach a detached lane by quiescing its index's builder, and publish nothing to a detached lane
- *(engine)* a builder serves a parked lane's fresh build on a substitute lane

### Fixed

- *(stream-channel)* wake a waiting recv when a delivery is released
- *(engine)* a backfill whose target was created anew no longer seeds it

### Other

- *(stream)* a returned delivery comes back in lane order, before anything new
- *(engine)* drop the change counts nothing reads
- *(engine)* [**breaking**] key lane items per index, not by their position

## [0.18.0](https://github.com/alias2k/flusso/compare/flusso-stream-channel-v0.17.0...flusso-stream-channel-v0.18.0) - 2026-11-19

### Added

- *(engine)* write several lane items at once per sink (#386 stage 2)

### Fixed

- *(daemon)* reset an engine's restart backoff after a run that made progress (#395 review)
- *(sinks)* resend items OpenSearch refuses as busy, and release a lane delivery by its ticket ([#395](https://github.com/alias2k/flusso/pull/395))

### Other

- the default memory budget goes back to 512 MiB
- the default memory budget is 64 MiB, not 512
- a deprecated [stream] capacity or --queue-capacity validates, warns once and is ignored
- Batch.walked and Request::Backfill.after (touches all constructors)
- a configured lane holds the backstop, not the backstop times the sinks (review 11); search.rs doc order (review 9)
- [stream] capacity is deprecated: accepted, warned about, ignored; lanes keep a 1,048,576-item backstop
- a published batch carries its charge until its last copy is dropped
- one lane per (index, sink): stream, Progress, sink engine, publish, requests, fence
- READMEs and a doc link no longer name the stream's watermark
- the engines confirm and fence from one Progress; the stream keeps no watermark
- Position carries its seq and the source's stamps (touches all constructors)
- *(stream-channel)* is_empty reads no lock; an acknowledged handle takes none on drop ([#408](https://github.com/alias2k/flusso/pull/408))
- the sink's real bound, the measured numbers, the catch-up refresh cost and the API changes (#408 review 1 findings 3-4, review 2 findings 1, 3, 5, 6)
- concurrent writes keep per-document order, the watermark and live-first (#386 stage 2)
- an item refused as busy is resent, and a release redelivers only its ticket ([#395](https://github.com/alias2k/flusso/pull/395))

## [0.17.0](https://github.com/alias2k/flusso/compare/flusso-stream-channel-v0.16.0...flusso-stream-channel-v0.17.0) - 2026-11-12

### Other

- say the root manual is the newest release and /main/ tracks main

## [0.16.0](https://github.com/alias2k/flusso/compare/flusso-stream-channel-v0.15.1...flusso-stream-channel-v0.16.0) - 2026-10-09

### Added

- *(engine)* [**breaking**] build-once ingest engine and one sink engine per sink over a bidirectional stream
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

### Fixed

- *(engine)* [**breaking**] review fixes — positions monotonic across streams, per-sink envelope stamping, supervisor loop

### Other

- *(engine)* share published envelopes via Arc, sink stamps own nam
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

## [0.14.0](https://github.com/alias2k/flusso/compare/flusso-queue-channel-v0.13.0...flusso-queue-channel-v0.14.0) - 2026-08-14

### Added

- *(query-derive)* [**breaking**] deprecate path= instead of deleting it

## [0.11.0](https://github.com/alias2k/flusso/compare/flusso-queue-channel-v0.10.1...flusso-queue-channel-v0.11.0) - 2026-07-23

### Other

- document the visual schema designer

## [0.10.1](https://github.com/alias2k/flusso/compare/flusso-queue-channel-v0.10.0...flusso-queue-channel-v0.10.1) - 2026-06-26

### Other

- lead README with the tagline, move AI disclosure below it
- consistency pass — fix factual errors and terminology drift
- rewrite all docs to the prose style guide
- clarify pre-commit hook formats the whole workspace
- add dev workflow tooling
- split into an mdBook manual + per-crate READMEs

## [0.4.0](https://github.com/alias2k/flusso/compare/flusso-queue-channel-v0.3.2...flusso-queue-channel-v0.4.0) - 2026-06-19

### Other

- move inline unit tests into sibling tests.rs modules

## [0.3.1](https://github.com/alias2k/flusso/compare/flusso-queue-channel-v0.3.0...flusso-queue-channel-v0.3.1) - 2026-06-19

### Added

- improve upon claude skills

## [0.2.0](https://github.com/alias2k/flusso/compare/flusso-queue-channel-v0.1.1...flusso-queue-channel-v0.2.0) - 2026-06-18

### Added

- add the publication management

## [0.1.1](https://github.com/alias2k/flusso/compare/flusso-queue-channel-v0.1.0...flusso-queue-channel-v0.1.1) - 2026-06-17

### Added

- change the "run" command to follow the "cargo" pattern of updating the "lock" file

### Other

- Merge pull request #9 from alias2k/feature/run-command-rewamp
