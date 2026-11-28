# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.19.0](https://github.com/alias2k/flusso/compare/flusso-daemon-v0.18.0...flusso-daemon-v0.19.0) - 2026-11-28

### Added

- *(engine)* [**breaking**] stop when another process changed the records, instead of reconciling
- *(source)* a stream opened past Rereads::open vouches for nothing (m-1)
- *(daemon)* every reattach's gap is kept exactly as a list, and a gap end the source can't read is settled at a start (gap list, m4)
- *(daemon)* reattach --accept-gap keeps its gap and the fence refuses inside it; records read again once the slot is held (O16, S3, O29)
- *(daemon)* /status shows rebuilds and detached lanes, and a reindex of a rebuild starting or running answers 409 (O20)
- *(engine)* a reindex of a detached index rebuilds it to the seed, a start attaches a rebuild or demotes it, and O20(c)
- *(engine)* attach a detached lane by quiescing its index's builder, and publish nothing to a detached lane
- *(engine)* detach a lane at its next refused probe, discard what it holds, and stage it detached
- *(daemon)* [**breaking**] keep an index's last error per kind
- *(daemon)* show a walk a live failure cut as waiting, and cut an index's error at 4 KiB
- *(daemon)* show a down index and a walk retrying in /status
- *(daemon)* hand [backoff] to the ingest engine for per-index restarts
- *(cli)* [**breaking**] GET /indexes/<name>/status and POST /indexes/<name>/reindex; POST /reindex is gone
- *(daemon)* /status's ingest.indexes.<name>: documents waiting, backlog age, exemption, hold, walk, working
- *(cli)* a stop cut short by its deadline names each index left with documents read and not built
- *(config)* [concurrency] max_pending_documents, --max-pending-documents, FLUSSO_MAX_PENDING_DOCUMENTS
- *(cli)* [concurrency] max_waiting_per_index and max_waiting, with flags and FLUSSO_* variables
- *(engine)* share the memory budget per index, for many reservers
- *(engine)* changes_committed counts what every lane of a sink landed
- *(engine)* [**breaking**] keep each index's pending documents in the progress
- *(config)* a [backoff] table sets the engines' retry schedule
- *(daemon)* /status names each lane's health; flusso.lane.parked and flusso.lane.probes
- *(engine)* [**breaking**] a lane parks alone on a refusal or a write block, and restages alone on a missing target

### Fixed

- *(sink-opensearch)* a marker carrying detaching_from never reads as on serving, and keeps every gap
- *(sink-opensearch)* a detach over a detached record keeps where it began
- *(engine)* record-keeping sinks write only stamped, and nothing goes out unread
- *(engine)* judge detach records by who wrote them
- *(engine)* a lane taken detached on a read from an earlier stream is read again
- *(engine)* every gated write stands on records read under the stream now held
- *(engine)* a decision that rewrites the records stands on a read under the stream now held
- *(engine)* the stop's message says only what holds of the confirmation
- *(engine)* a lane never drops a change no record covers
- *(engine)* no seed without the slot; plain writes as on main (fifth re-check B-1, S-1)
- *(daemon)* a detach heard again during a rebuild keeps it a rebuild
- *(engine)* a detached lane holds confirmation until read under the current stream
- *(engine)* another process's rebuild is put back to detached before its lane drops anything (B-2)
- *(engine)* reads of the detach records count per lane, never per sink: an index whose read failed stays unread while its siblings go on (B1''')
- *(daemon)* an unread lane vouches for nothing: the frozen answer is gone, and an opening wakes every sink engine to read at once (item 2)
- *(daemon)* /status and the log say an unknown gap end settles by itself at the next read while the slot is held, and what an operator can do meanwhile (m-g)
- *(daemon)* an unread lane's frozen answer advances at an opening only for a sink that read during the stream left; a property test over the fence gate (B1'')
- *(daemon)* a detach adopted while its sink is unread freezes at what the lane applied under the previous stream, never at the new stream's start (S-c)
- *(engine)* the live stream opens only through Rereads::open, which marks every sink unread and freezes what lanes applied under the previous stream before the new one can answer (B1', m-a)
- *(daemon)* a reattach answers the engine's verdict, a detached record on a sink switched to backfill = false is put back live only, and refusals name what works (S4, m5, m6)
- *(engine)* a reindex of a rebuild waiting on its seed's retire block is refused with 409, naming the block (S2, O28)
- *(daemon)* an unread sink masks only a lane serving its writes, keeps what it applied before the slot opened, and its read wakes waits (B1)
- *(engine)* a lane held detached stays so at a later staging, and its fence stays detached under a backfill ask
- *(daemon)* a stop before the first staging never lets the source prepare
- *(engine)* name the index pacing the reader, track enabled indexes only
- *(engine,daemon,cli)* working is about progress, the backlog age is a lag, a run's end clears its state
- *(cli)* an unset max_waiting rises to max_waiting_per_index, and the other way round
- *(engine)* the share bounds an index's live bytes, and nothing per index passes the limit
- *(engine)* a skipped step always replaces a lane's newest entry; steps_kept reads with the unlanded count
- *(engine)* a lane keeps a bounded record of steps while it is parked
- *(engine)* pending builds by step, main's admission and settle order
- *(engine,sink-opensearch)* cut a backfill by its target's identity, not one staging's word
- *(engine)* a backfill whose target was created anew no longer seeds it
- *(cli)* the stop message says a refused lane is refused, from the one name /status gives
- *(cli)* one judge for [backoff], and an unset cap rises with the first wait
- *(daemon)* a probe past every date reads as the latest one, never a panic
- *(daemon)* /status names a refused lane `refused`, as the metric's `why` does

### Other

- *(daemon)* a drop under a kept record loses only what the lane still held
- *(daemon)* the world's other process forgets a record a seed outlived
- *(daemon)* the hand-deletion exemption judges a freed generation by its uuid
- *(daemon)* the world's stand-in answers is_rebuilding, and judges deletes by what held the row
- B-1, a seed dropping a gap on a generation that still holds its ghosts
- *(daemon)* a record naming a generation a hand deletion freed is #492's
- *(daemon)* the world judges what reads see against its own ledger
- *(daemon)* the world's dev-dependency on the adapter is path only
- *(daemon)* the world's stand-in runs the OpenSearch sink's own plans
- *(daemon)* the world's other process reattaches only beside a marker on serving
- *(daemon,engine)* a live-only detach leaves a moved marker where it is
- *(daemon,engine)* the stand-ins model detaching_from and the read's put-back
- *(daemon)* the world's other process keeps the gaps on its detach
- *(daemon)* outlast the longest backoff before the world judges liveness
- *(daemon)* the world draws a rebuild finished and begun again often
- *(daemon)* the world judges liveness by what the records mean
- *(daemon)* the world reaches a read answered under an earlier stream
- *(daemon)* every own write in the world can land and fail, as on OpenSearch
- *(daemon)* the world's store and other process as OpenSearch and Postgres have them
- *(daemon)* a walk resumed over a kept gap starts from the first key
- *(daemon)* a backfill cut while the slot is away is asked again
- *(daemon)* both fence properties read PROPTEST_CASES
- *(daemon)* a rebuild begun again under the same attempts is a different rebuild
- *(daemon)* a rebuild whose record was replaced meanwhile is dropped by the run
- *(engine)* the sink engine holds its sink as one that can't erase
- *(daemon)* the world's other process finishes rebuilds and reattaches (seventh re-check S-3)
- *(daemon)* the reviewer's world seeds kept as their event lists
- *(daemon)* the gate property judges every record and draws each unheld case
- *(engine)* a gate checked before awaits lets an erasing request through once the stream ended (sixth re-check B-1)
- *(daemon)* the world ends streams with no opening begun, and detaches lanes itself (fifth re-check)
- *(daemon)* one capture per process in the world, and its write oracle on the world's own state (fifth re-check, CI red)
- *(daemon)* the world property writes for real, waits, and judges by what the test did (S-2)
- *(daemon)* the O29 gate's property through the real sink engines (two sinks, partial reads, restarts, drops, reattach, rebuild, both levels); it found a rebuild begun while unread vouching, fixed (item 3)
- *(engine)* the attach's R6 check asserted, the quiesce beside another builder, the fence after an attach, and step 3's review in the note
- *(source-postgres)* spike S5, the source confirms what no lane needs
- *(engine)* the review's cases for a seed parked on a blocked old generation
- *(daemon)* a stop asked inside the first staging starts no engine
- *(daemon)* a panicking build's deployment runs on and ends Ok at the stop
- *(engine)* drop the change counts nothing reads
- *(daemon)* changes_committed counts once every lane of a sink landed a change
- *(engine)* the review of stage 8a slice 2, red first
- *(engine)* [**breaking**] key lane items per index, not by their position
- *(daemon)* a restart after progress starts over at the configured first wait
- LaneHealthView.state documented, Chunk below the imports, LaneSnapshot's new fields on the upgrade page
- *(daemon)* the status document's lane block carries its health
- a parked lane in the manual, the 0.19 upgrade page, CLAUDE.md and stage 9 as built

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
