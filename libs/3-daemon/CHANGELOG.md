# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.18.0](https://github.com/alias2k/flusso/compare/flusso-daemon-v0.17.0...flusso-daemon-v0.18.0) - 2026-11-19

### Added

- *(engine)* write several lane items at once per sink (#386 stage 2)
- *(engine)* [**breaking**] stage timings account for each task's whole time (#399 review)
- per-stage timings in /metrics and the bench's attribution table ([#399](https://github.com/alias2k/flusso/pull/399))
- *(engine)* stage a sink's indexes concurrently, and validate indexes concurrently
- *(engine,daemon,source-postgres)* [**breaking**] a stop drains what is in flight before the run ends
- *(kernel)* a column may hold an array of its type ([#276](https://github.com/alias2k/flusso/pull/276))
- *(sink-opensearch)* trust a private CA with ca_file; never retry a refused certificate

### Fixed

- *(engine)* [**breaking**] progress is the oldest unfinished item landing, not any acknowledgement (#408 review 2, finding 3)
- *(engine)* a replay under new stream positions is not progress
- *(engine)* only an advance of the source is ingest progress; a failed apply drops its batch's buffer (#395 review 3)
- *(daemon)* reset an engine's restart backoff after a run that made progress (#395 review)
- *(daemon)* [**breaking**] Phase and ControlError are non_exhaustive
- *(engine)* a stopping sink acknowledges queued snapshot batches unapplied
- *(source-postgres)* [**breaking**] emit a change only once its transaction is visible
- *(test)* every test and bench starts Postgres through dev/test-postgres
- *(design)* refuse a field name its level already has; warn about duplicates; read typed table and column names as flusso does

### Other

- the default memory budget goes back to 512 MiB
- a live item ready while a write is out waits for it, bounded, and goes out merged
- count how each budget wait ends: flusso.memory.wait{work,wake}
- the default memory budget is 64 MiB, not 512
- comments say what the code does, not which review or stage changed it (review L4)
- a panicking build never hangs the deployment; what a first-poll panic does, documented (review L1)
- a restart in the middle of a rebuild says rebuilding, vouching for nothing
- a restarting sink engine keeps its lane's stall age
- pinned_by is the lane Progress::landed is decided by
- e2e: a first seed's index already holds the live write; only the fence's answer is asserted
- /status backfill_checkpoints, flusso.backfill.checkpoints, docs
- e2e: a fence holds through a first seed, a reindex and a dropped-slot rebuild
- a fence vouches only for what reads of each lane see
- changes are resolved on a connection of their own (step 2)
- tests: /status carries each lane's stall and the pinning lane; the gauges export them
- e2e: a stalled index's lane ages and pins the slot while its sibling writes; CLAUDE.md
- stall age and WAL behind per lane, and the lane that pins the slot
- the sink engine merges ready items into one write
- fence e2e: a stalled index on a sink blocks only the fences that include it
- flusso.lane.outstanding{index,sink}: items routed to a lane and not landed
- /status reports each lane; fence and /wait tests per index
- one lane per (index, sink): stream, Progress, sink engine, publish, requests, fence
- [concurrency] max_built_mib (16..=1048576, default 512), the daemon's budget, /status memory, flusso.memory metrics, the designer's field
- review follow-ups: no watermark left, wal_idle's half named, the argument per lane
- an engine takes the Progress it shares; none is built without one
- each lane is registered as its own send succeeds and answers from its own newest
- tests: a full sibling lane holds back a healthy lane's progress and fence (red)
- measured on reference: about +15% backfill at 2-4 builds, sink-bound beyond; RSS up; one allocation per batch more
- rustfmt
- a sink engine raises its lane's marks without a lock
- a walk ends at the greatest key it found, read through the key index's own operators
- the failing-page backoff test declares its index, and every backoff wait has a deadline
- the engines confirm and fence from one Progress; the stream keeps no watermark
- --builds / FLUSSO_BUILDS, the daemon's DEFAULT_BUILDS, the builder pool at builds + 2, the designer's field
- the source confirms and fences from Landed; the Postgres capture keeps no map
- Position carries its seq and the source's stamps (touches all constructors)
- a backfill pages each index's root keys on the builder pool
- the sink's real bound, the measured numbers, the catch-up refresh cost and the API changes (#408 review 1 findings 3-4, review 2 findings 1, 3, 5, 6)
- after a failed run the oldest item is written alone until it lands (#408 review 1, finding 2)
- *(sink)* [**breaking**] Sink::write replaces apply + flush (#386 stage 2)
- a replayed change or backfill page is no progress, and a failed apply leaves nothing buffered (#395 review 3)
- *(daemon)* a restart after a run that made progress waits one second again
- *(engine,daemon)* hold a commit only once Postgres makes commits wait
- *(engine,daemon)* pin read-after-visible in its own e2e binary
- *(engine,daemon)* a commit read before it is visible is indexed as committed

## [0.17.0](https://github.com/alias2k/flusso/compare/flusso-daemon-v0.16.0...flusso-daemon-v0.17.0) - 2026-11-12

### Added

- *(query)* [**breaking**] move every HTTP client to reqwest 0.13
- *(daemon)* the ingest engine gets the failure policies; /status counts script failures per index
- *(kernel)* script fields and input-only fields in the schema model
- *(daemon)* a fence waits until a sink has reached a source position
- *(source-postgres)* resolution skips what an update's changed columns can't affect

### Fixed

- review — report script failures under both policies, ids in print context, debug log of the failing document, shared build-by-id, field docs
- review findings for the fence

### Other

- say the root manual is the newest release and /main/ tracks main
- *(daemon)* the fence e2e sets the indexed and subfields flags from #187
- crate READMEs for fences (source, sinks, engine, daemon, flusso-query)
- *(daemon)* live fence e2e over real Postgres and OpenSearch
- build every OpensearchConfig literal with index_settings

## [0.16.0](https://github.com/alias2k/flusso/compare/flusso-daemon-v0.15.1...flusso-daemon-v0.16.0) - 2026-10-09

### Added

- *(source)* carry the pre-image on a change and resolve the old parent from it
- *(cli)* [batch] reaches the ingest engine; --batch-max-changes / --batch-max-delay-ms
- *(config)* a [batch] table for the ingest engine's batching
- *(engine)* [**breaking**] build-once ingest engine and one sink engine per sink over a bidirectional stream
- *(config)* [**breaking**] adapters own their configuration; port entries are type + options
- *(sources,engine)* report source continuity and rebuild seeds it invalidates

### Fixed

- *(engine)* max_delay caps a burst inside the drain; max_changes is non-zero
- *(engine)* [**breaking**] review fixes — positions monotonic across streams, per-sink envelope stamping, supervisor loop
- *(engine)* stage stale-seed rebuilds before the resume point exists

### Other

- *(daemon)* run the ingest engine on its own task; publish to lanes concurrently
- *(libs)* describe the two engines and the ticketed channel ledger in the crate docs
- *(arch)* [**breaking**] lay out kernel / ports / adapters / engine / daemon and sdk/
- root README as a pitch, flusso-query README as a landing; track the Start here pages
- *(release)* [**breaking**] three release trains — per-crate versions + version groups
- the lock is deterministic TOML; formats frozen for the major
- *(daemon)* carry the new Source.tls field in the backendless config
- source TLS — configuration guide section, README, crate README, plugin skills

## [0.14.0](https://github.com/alias2k/flusso/compare/flusso-daemon-v0.13.0...flusso-daemon-v0.14.0) - 2026-08-14

### Added

- *(query-derive)* [**breaking**] deprecate path= instead of deleting it

## [0.11.0](https://github.com/alias2k/flusso/compare/flusso-daemon-v0.10.1...flusso-daemon-v0.11.0) - 2026-07-23

### Other

- document the visual schema designer

## [0.10.1](https://github.com/alias2k/flusso/compare/flusso-daemon-v0.10.0...flusso-daemon-v0.10.1) - 2026-06-26

### Other

- lead README with the tagline, move AI disclosure below it
- consistency pass — fix factual errors and terminology drift
- rewrite all docs to the prose style guide
- clarify pre-commit hook formats the whole workspace
- add dev workflow tooling
- split into an mdBook manual + per-crate READMEs

## [0.4.0](https://github.com/alias2k/flusso/compare/flusso-daemon-v0.3.2...flusso-daemon-v0.4.0) - 2026-06-19

### Added

- runtime index prefix to namespace all indices ([#24](https://github.com/alias2k/flusso/pull/24))

### Other

- move inline unit tests into sibling tests.rs modules
- *(core)* typed canonical value vocabulary between source and sink

## [0.3.1](https://github.com/alias2k/flusso/compare/flusso-daemon-v0.3.0...flusso-daemon-v0.3.1) - 2026-06-19

### Added

- improve upon claude skills

## [0.2.0](https://github.com/alias2k/flusso/compare/flusso-daemon-v0.1.1...flusso-daemon-v0.2.0) - 2026-06-18

### Added

- add the publication management

### Other

- code hygene, removed useless comments

## [0.1.1](https://github.com/alias2k/flusso/compare/flusso-daemon-v0.1.0...flusso-daemon-v0.1.1) - 2026-06-17

### Added

- change the "run" command to follow the "cargo" pattern of updating the "lock" file

### Other

- Merge pull request #9 from alias2k/feature/run-command-rewamp
- release v0.1.0

## [0.1.0](https://github.com/alias2k/flusso/releases/tag/flusso-daemon-v0.1.0) - 2026-06-17

### Added

- license, security, coc, contributing and github templates
- on-demand reindex — alias-over-generations sink + restart trigger
- split the operational HTTP surface into public + private with Basic auth
- rename flusso-search to flusso-query
- move files to correctly reflect dependencies
- more checks in CI
- add on failure behavior
- tests
- abstract backends
- create alias pointing to the latest index
- add belong_to
- move otel from daemon to cli
- add deamon crate, add metrics export, add prometheus and grafana, add http health endpoint
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

- daemon compilation

### Other

- preparation for going public
- documentation
- add requirements section
- add SCHEMA.md
- readme
- readme and cleanup
- README
