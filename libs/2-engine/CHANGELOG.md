# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.19.0](https://github.com/alias2k/flusso/compare/flusso-engine-v0.18.0...flusso-engine-v0.19.0) - 2026-11-21

### Added

- *(engine)* a reader task reads and marks ahead of one builder, up to a lead
- *(engine)* waiting lines for build permits, and a key-read permit
- *(engine)* share the memory budget per index, for many reservers
- *(engine)* changes_committed counts what every lane of a sink landed
- *(engine)* [**breaking**] keep each index's pending documents in the progress
- *(config)* a [backoff] table sets the engines' retry schedule
- *(engine)* [**breaking**] a lane parks alone on a refusal or a write block, and restages alone on a missing target

### Fixed

- *(engine)* review round 4 of slice 6a: one drain before every batch, one word for the handshake, fair budget
- *(engine)* review round 2 of slice 6a: a grant takes every run handed over, a hand-over beside a build walks nothing
- *(engine)* batches cut as main cut them, captured counted read on every path, the reader awaited on an error
- *(engine)* the gate reserves only at the generation it chose the waiter at
- *(engine)* a waiter's standing is read from the budget's current generation, never a stale mark
- *(engine)* a waiter refused for being behind is eligible again when one ahead leaves
- *(engine)* an index holding no live bytes always gets one live batch
- *(engine)* the share bounds an index's live bytes, and nothing per index passes the limit
- *(engine)* live keeps room beside an index's backfill, and a held index is one budget wait
- *(engine)* a skipped step always replaces a lane's newest entry; steps_kept reads with the unlanded count
- *(engine)* a lane keeps a bounded record of steps while it is parked
- *(engine)* a batch carries its own run's newest change, no floor
- *(engine)* pending builds by step, main's admission and settle order
- *(engine)* a live lane item without a key fails the sink engine
- *(engine)* only a lane being staged again holds its SnapshotComplete
- *(engine)* a held walk keeps nothing, and a live item's release holds no walk
- *(engine,sink-opensearch)* cut a backfill by its target's identity, not one staging's word
- *(engine)* a backfill whose target was created anew no longer seeds it
- *(daemon)* a probe past every date reads as the latest one, never a panic
- *(daemon)* /status names a refused lane `refused`, as the metric's `why` does
- *(engine)* a park between two stagings keeps the staging's own backoff; the staging warning says when
- *(engine)* a staging that comes due inside a grant starts at once, not at the next wake-up
- *(engine)* let go of a part's batch before acknowledging it
- *(engine)* a lane staged again keeps its backoff until one of its writes lands
- *(engine)* a parked lane's waiting backfill doesn't end the other lanes' merge wait

### Other

- *(engine)* an ended future polled again fails its test; the budget wake under block_on stated
- *(engine)* the reader and the builder as two futures of one task
- *(engine)* a grant pass alone is one hold of the gate's lock; one scan per index
- *(engine)* pin where a parked index's backlog stops the reading, and that rule 5 is never blocked
- *(engine)* drop the change counts nothing reads
- *(source)* state the position contract the engine relies on
- *(engine)* a live batch's position is a Position, not an Option
- *(engine)* a replayed batch claims no change another lane landed
- *(engine)* the review of stage 8a slice 2, red first
- *(engine)* a document being built holds confirmation in the progress
- *(engine)* two indexes into two sinks each key their own items
- *(engine)* fences keep live items in key order, observers hear positions
- *(engine)* [**breaking**] key lane items per index, not by their position
- *(engine)* two lane items sharing a position land one at a time
- *(engine)* a parked lane's completion runs once a stop skipped the page in front
- the target identity rule in CLAUDE.md, the ticket's process assumption
- *(engine)* a cut survives every staging attempt that fails after it
- *(engine)* a target deleted mid-backfill is seeded only after a full backfill
- *(engine)* a park between two stagings keeps the staging's own backoff
- *(engine)* a lane whose write landed after its staging is staged again at once next time
- *(engine)* pipeline's stop branches assert the lane parked on its own refusal (512 bytes, NaN)
- FlussoLaneParked fires for a parked lane, a refused backfill page and a lane stuck restaging; restaging is exported in flusso.lane.parked
- *(engine)* a part's bytes are back when its acknowledgement returns
- *(engine)* a parked lane's waiting backfill doesn't end another lane's merge wait
- *(engine)* a lane is staged again only once its own writes in flight returned
- *(engine)* a missing target staging can't cure is retried with a backoff, not in a loop
- *(engine)* a lane parks alone on a refusal or a write block, and restages alone on a missing target

## [0.18.0](https://github.com/alias2k/flusso/compare/flusso-engine-v0.17.0...flusso-engine-v0.18.0) - 2026-11-19

### Added

- *(engine)* an ordered sink gets lane order, one write at a time; stdout is one (#408 review 2, finding 4)
- *(engine)* write several lane items at once per sink (#386 stage 2)
- *(engine)* page carries each snapshot row's whole time; startup steps per bench start (#399 review 3)
- *(engine)* [**breaking**] stage timings account for each task's whole time (#399 review)
- per-stage timings in /metrics and the bench's attribution table ([#399](https://github.com/alias2k/flusso/pull/399))
- *(engine)* stage a sink's indexes concurrently, and validate indexes concurrently
- *(engine,daemon,source-postgres)* [**breaking**] a stop drains what is in flight before the run ends
- *(bench)* just review-ab compares a branch with its base on the Docker-backed benches
- *(kernel)* a column may hold an array of its type ([#276](https://github.com/alias2k/flusso/pull/276))
- *(sink-opensearch)* trust a private CA with ca_file; never retry a refused certificate

### Fixed

- *(engine)* the narrow wait polls control before a landed write
- *(engine)* a fresh engine writes its first item alone (#408 review 3)
- *(engine)* the lane's recv future is made only when its wait is polled (#408 review 3)
- *(engine)* a write landing after an older one failed is no progress (#408 review 3)
- *(engine)* [**breaking**] progress is the oldest unfinished item landing, not any acknowledgement (#408 review 2, finding 3)
- *(engine)* a pending reindex outlives a failed run (#408 review 2, finding 2)
- *(engine)* every error of a sink engine's run waits for the writes in flight (#408 review 1, finding 6)
- *(engine)* after a failed run, write the oldest redelivered item alone until it lands (#408 review 1, finding 2)
- *(engine)* start no index after a staging failure; stage a twice-named reindex once
- *(engine)* a replay under new stream positions is not progress
- *(engine)* only an advance of the source is ingest progress; a failed apply drops its batch's buffer (#395 review 3)
- *(daemon)* reset an engine's restart backoff after a run that made progress (#395 review)
- *(engine)* a snapshot row is its own document, not a change to resolve
- *(engine,source-postgres)* a stop that can't confirm its final position fails; a deferred sink refreshes past skipped batches
- *(engine)* a stopping sink acknowledges queued snapshot batches unapplied
- *(source-postgres)* a float reads as Postgres casts to float4; exact reads pinned in flusso-query
- *(source-postgres)* read and write every number exactly
- *(test)* the guard rejects the postgres import, not call shapes
- *(test)* every test and bench starts Postgres through dev/test-postgres
- *(bench)* review-ab re-runs a flagged case before calling it slower, alternates sides, and runs one at a time
- *(engine)* the pipeline bench requests keys of its columns' own type
- *(source-postgres)* [**breaking**] write every numeric digit as a JSON number
- *(source-postgres)* resolve each change as it is buffered again ([#295](https://github.com/alias2k/flusso/pull/295))
- *(source-postgres)* read a batch's root keys back in one query per index ([#295](https://github.com/alias2k/flusso/pull/295))
- *(source-postgres)* read a key back as the text the WAL sends ([#295](https://github.com/alias2k/flusso/pull/295))
- *(design)* refuse a field name its level already has; warn about duplicates; read typed table and column names as flusso does
- *(engine,sink-opensearch)* a rejected document's id is shown shortened; an empty-id delete is dropped
- *(kernel)* store a constant list mixing kinds unindexed, on a new index
- *(source-postgres)* a constant builds the value the schema holds
- *(source-postgres)* index a binary column as its bytes, not Postgres hex text
- *(source)* [**breaking**] settle the root-key kinds; probe transforms without a table

### Other

- *(engine)* pin the one-lane merge-wait guard and the locked refresh case
- *(engine)* the keyset backfill's settle wait outlasts a loaded machine (30 s failed at load 12, passes in 12 s quiet)
- a merge hold ends when its instant passes, so a failed run or a pending reindex never spins on it; the tour and the upgrade page say what the merge wait does
- test: a merge hold that outlives a failure or a pending reindex must not spin the run (red)
- the default memory budget goes back to 512 MiB
- a live item ready while a write is out waits for it, bounded, and goes out merged
- test: items ready while a write is out wait for it, bounded, and go out merged (red)
- count how each budget wait ends: flusso.memory.wait{work,wake}
- every position a lane writes ends searchable, whatever lands beside its refresh (M-1, M-2, L-b, L-c, L-d)
- test: what lands beside a lane's caught-up write (red: M-1, M-2, L-b, L-c)
- the default memory budget is 64 MiB, not 512
- a quiet lane's caught-up write is a flight, not awaited in the run loop
- test: a quiet lane's caught-up write beside another lane's refresh (red: the engine deadlocks)
- comments say what the code does, not which review or stage changed it (review L4)
- a panicking build never hangs the deployment; what a first-poll panic does, documented (review L1)
- a stop rejection is blamed on its own lane even when an acknowledgement fails (review L2)
- test: a stop rejection keeps its blame when an acknowledgement fails (red: Everyone; review L2)
- a guard on the order publish_live registers a lane's newest position (review M1)
- the 8a pending sets and Gate::acquire leave the shared branch (review M2)
- a restart in the middle of a rebuild says rebuilding, vouching for nothing
- Progress::pinned_by, the lane landed() is decided by
- a test drives Builds::affordable with a release right after its check
- a backfill walk resumes after its lanes' points, each lane sent only what it lacks
- the build gate: permits shared by every build, live first, every fourth contested grant to backfill, first in first out (step 3)
- changes are resolved on a connection of their own (step 2)
- a lane records the landed prefix of its backfill walk; staging resumes after it, forgets it on a fresh source
- Progress keeps a pending set per index: mark, route, take, publish_taken, give_back
- the sink engine announces a rebuild before the sink moves its writes
- flusso.sink.refreshes{sink}: refresh requests a sink sent; dev/bench reports them per phase
- the heap walk counts each envelope's copy of its index name
- each part of a merged write gives its bytes back once, at its own ack
- Batch.walked and Request::Backfill.after (touches all constructors)
- every byte comes back after an aborted engine, an abandoned snapshot and an ingest restart
- FLUSSO_E2E_BUDGET_KIB sets the engine e2e's memory budget
- a release between the check and the wish is never lost
- caught up per lane: a caught-up write covers its quiet lanes' indexes, and a lane going quiet makes its own writes searchable (review 4, 5)
- tests: a quiet lane makes its last writes searchable beside a stalled lane (red: the idle caught-up write waits for the whole sink)
- an ordered sink takes every lane's front before it picks by position (review 8)
- tests: an ordered sink writes live items across lanes in position order (red: 0, 2, 1)
- tests: both seeded schedules run over two indexes; the turns test starts past the first landings (review 6)
- a quiet lane always takes one item; one lane holds at most slots - 1 writes; lanes merge only while none has a write out (review 1, 2)
- an engine refuses a lane its Progress doesn't keep (review 10); tests: the review's three (red)
- a lane's SnapshotComplete seeds its index as a flight holding a write slot
- tests: a sink seeds several indexes at once, beside other lanes' writes (red)
- pick keeps the no-hash front pick unless another item could join it (WIP: measuring)
- a standing leak guard: every engine test and e2e counts what it builds, and nothing stays counted
- the sink engine merges ready items into one write
- tests: ready items merge into one write (red); Sink::write_parts and write_limit
- a published batch carries its charge until its last copy is dropped
- tests: a batch is split by index, a stalled index holds back no sibling, lanes take turns, SnapshotComplete per lane
- ingest tests: the Spy stream takes a lane key (compile fix for 035e7d90)
- tests: the seeded schedule on an open stream, and a stop while a closed batch waits for a build
- the engine's e2e run at builds = 4, what ships; FLUSSO_E2E_BUILDS selects another
- one lane per (index, sink): stream, Progress, sink engine, publish, requests, fence
- builds are aborted when the engine's run future is dropped
- test: a dropped engine aborts its running builds (red: they outlive it)
- the seeded schedules check lane order, and a seed replays alone
- [concurrency] max_built_mib (16..=1048576, default 512), the daemon's budget, /status memory, flusso.memory metrics, the designer's field
- builds reserve from the budget at grant and settle when they land
- review follow-ups: no watermark left, wal_idle's half named, the argument per lane
- an engine takes the Progress it shares; none is built without one
- the budget's accountant and the heap walk
- tests for the budget's accountant (red: the stub reserves nothing)
- each lane is registered as its own send succeeds and answers from its own newest
- tests: a full sibling lane holds back a healthy lane's progress and fence (red)
- a sink engine raises its lane's marks without a lock
- a walk ends at the greatest key it found, read through the key index's own operators
- a root keyed by another column than its table's key is backfilled with nothing
- a spawned live build is counted as landed at builds = 1 too
- tests: a page and a live batch of one document in either order; changes apart on an open stream (red at builds = 1)
- the engines confirm and fence from one Progress; the stream keeps no watermark
- rustfmt the keyset_backfill e2e
- up to builds batches build at once, published in grant order
- Progress: what every lane has landed, with no lock both sides take
- Progress tests (red: the stub routes nothing)
- the source confirms and fences from Landed; the Postgres capture keeps no map
- tests for concurrent builds (red: they hang on one build at a time)
- Position carries its seq and the source's stamps (touches all constructors)
- e2e for writes during a keyset backfill, a stop mid-walk, PgBouncer pages
- split ingest.rs into modules (no behaviour change)
- a backfill pages each index's root keys on the builder pool
- *(engine)* a narrower wait while writing, no Stop clone per turn, flights in a Vec ([#408](https://github.com/alias2k/flusso/pull/408))
- *(engine)* Flights under a replaced waker, a late wake and wakes from other threads
- *(engine)* two comments say what the code does (#408 review 5)
- a new process writes its oldest unconfirmed batch alone (#408 review 5)
- *(engine)* the run polls a write only when its own waker fired (#408 review 5, CI bench gate)
- *(engine)* one fences lock per landing; no fence scan when nothing can wait (#408 review 5)
- *(engine)* a fresh engine writes nothing past a persistent stop rejection (#408 review 3)
- *(engine)* a write landing after an older one failed is no progress (#408 review 3)
- *(engine)* a write failing at its first poll, a stop with every slot busy, the stop rejection split by when it is seen (#408 review 3, review 4 finding 3)
- *(engine)* tests of several writes at once land one warm-up write first (#408 review 3)
- the sink's real bound, the measured numbers, the catch-up refresh cost and the API changes (#408 review 1 findings 3-4, review 2 findings 1, 3, 5, 6)
- *(engine)* the sink engine's loop costs what one write at a time cost (#408 bench gate, review 2 finding 1)
- *(engine)* an ordered sink gets its items in lane order, one at a time (#408 review 2, finding 4)
- *(engine)* only the oldest unfinished item landing counts as progress (#408 review 2, finding 3)
- *(engine)* a reindex taken before a failed write is staged after the restart (#408 review 2, finding 2)
- *(engine)* failures in the seeded schedule test; skip and stop rejections beside writes in flight (#408 review 1, finding 5)
- *(engine)* a failed acknowledgement still waits for the writes in flight (#408 review 1, finding 6)
- after a failed run the oldest item is written alone until it lands (#408 review 1, finding 2)
- *(engine)* hash documents for admission only when a comparison is needed (#386 stage 2)
- *(engine)* admit by sorted document hashes, no per-document counting map (#386 stage 2)
- concurrent writes per sink, [concurrency] writes and the Sink::write port (#386 stage 2)
- concurrent writes keep per-document order, the watermark and live-first (#386 stage 2)
- *(sink)* [**breaking**] Sink::write replaces apply + flush (#386 stage 2)
- *(bench)* live_drain_held is the contended upper bound; what it catches; what the hold costs (#417 review)
- *(engine)* live_drain measures a sink that holds each batch's last reference
- *(engine)* after a staging failure no new index starts
- an item refused as busy is resent, and a release redelivers only its ticket ([#395](https://github.com/alias2k/flusso/pull/395))
- *(engine)* snapshot rows are not resolved, and what a faster backfill holds in memory
- *(engine)* a backfill of related indexes resolves nothing and builds each document once
- one intro and one stop section on the 0.18 upgrade page; a close that just fits is not a failure
- *(engine)* check the stop with one atomic load per live change
- *(engine,daemon)* hold a commit only once Postgres makes commits wait
- *(engine,daemon)* pin read-after-visible in its own e2e binary
- *(engine,daemon)* a commit read before it is visible is indexed as committed
- *(engine)* wait for the seed marker before checking the dropped generation
- *(engine)* resolve keeps finding parents after a key and its link become name
- *(source-postgres)* a generated build/build_many differential, a revoked privilege probe, offset-less timestamptz text, quoted key type names
- *(source-postgres)* both build paths agree across key kinds; a domain over a type the role may not use
- *(source-postgres)* pin resized keys, numeric keys of another scale and the resolve mismatch message
- *(engine)* pin column type changes mid-run on a pool with several connections
- only a declared decimal may change what the sink receives
- a numeric keeps every digit as a JSON number; a non-finite one fails the document
- *(source-postgres)* a type change is read by its new type without a restart ([#298](https://github.com/alias2k/flusso/pull/298))
- *(source-postgres)* a column changing type mid-run recovers after one restart ([#298](https://github.com/alias2k/flusso/pull/298))
- *(source-postgres)* an array of a bytea domain and a composite read as main does ([#298](https://github.com/alias2k/flusso/pull/298))
- *(source-postgres)* every bytea value reads the same under bytea_output escape ([#298](https://github.com/alias2k/flusso/pull/298))
- *(source-postgres)* a link or ids element of a type the key can't take (#299, #312, #313)
- warn on a timestamp format that refuses the ISO form; pin tuned mappings
- a naive timestamp must reach OpenSearch as ISO 8601
- *(source-postgres)* state what PgBouncer must allow; a refused setting retries ([#295](https://github.com/alias2k/flusso/pull/295))
- *(source-postgres)* a numeric link reads one root key back per change ([#295](https://github.com/alias2k/flusso/pull/295))
- *(source-postgres)* key types after review; PgBouncer; guard transaction mode ([#295](https://github.com/alias2k/flusso/pull/295))
- *(source-postgres)* a link of another type rebuilds what the join reads ([#295](https://github.com/alias2k/flusso/pull/295))
- *(source-postgres)* temporal keys past year 9999 keep main's ids ([#295](https://github.com/alias2k/flusso/pull/295))
- *(source-postgres)* a burst reads root keys back once per batch ([#295](https://github.com/alias2k/flusso/pull/295))
- *(source-postgres)* the pipeline runs through PgBouncer in session mode ([#295](https://github.com/alias2k/flusso/pull/295))
- *(source-postgres)* a link spelled differently rebuilds the root's own document ([#295](https://github.com/alias2k/flusso/pull/295))
- *(source-postgres)* keys under other session settings and out of range ([#295](https://github.com/alias2k/flusso/pull/295))
- *(source-postgres)* every key type in every relation position ([#295](https://github.com/alias2k/flusso/pull/295))
- *(sink-opensearch)* an id OpenSearch refuses fails the whole bulk request
- Merge pull request #249 from alias2k/fix/check-validation-gaps

## [0.17.0](https://github.com/alias2k/flusso/compare/flusso-engine-v0.16.0...flusso-engine-v0.17.0) - 2026-11-12

### Added

- *(query)* [**breaking**] move every HTTP client to reqwest 0.13
- *(engine)* a document that fails to build is stopped on or skipped by its index's policy
- *(schema)* an aggregate's column can be an arithmetic expression
- *(kernel)* script fields and input-only fields in the schema model
- *(config)* on: column pairs on joins, aggregates and via hops
- *(schema)* aggregates across several hops with via, and the exists op
- *(daemon)* a fence waits until a sink has reached a source position
- *(engine)* a sink engine reports its applied and searchable positions
- *(sink)* [**breaking**] a flush reports when its documents become searchable
- *(source-postgres)* resolution skips what an update's changed columns can't affect
- *(kernel)* subfields and indexed opt-outs on columns and joins, hashed only when off

### Fixed

- *(source-postgres)* resolve a hop from the key when it carries the link
- *(source-postgres)* keep a USING INDEX identity only when it carries the primary key
- *(source-postgres)* keep a USING INDEX replica identity that carries every link
- *(source-postgres)* a zero divisor in an aggregate expression nulls the row instead of failing the build
- review — report script failures under both policies, ids in print context, debug log of the failing document, shared build-by-id, field docs
- review findings for via and exists
- *(source)* a raw filter anywhere in an index turns the skip off for it

### Other

- say the root manual is the newest release and /main/ tracks main
- *(engine)* fold the last-flush guard into wal_release with a 5 s confirm bound
- *(engine)* the slot releases WAL within 5 s of the last flush
- *(engine)* exercise the minimum over lanes and split the wal_release bounds
- list the wal_release e2e and its guards
- *(engine)* guard that the replication slot releases WAL flusso has no use for
- Merge remote-tracking branch 'origin/main' into feat/script-fields
- REPLICA IDENTITY FULL by default on the root and a belongs_to target
- *(engine)* run_deployment keeps its doc comment
- *(engine)* aggregate expressions build Postgres arithmetic and rebuild from each operand
- *(engine)* on: pairs rebuild the parent from either side of a pair
- *(engine)* a self-reaching via and a via under a nested relation rebuild every ancestor
- *(engine)* a via aggregate rebuilds its parent from every hop
- *(engine)* a searchable flush with nothing waiting skips the queue and the timer
- crate READMEs for fences (source, sinks, engine, daemon, flusso-query)
- the unread-column fixtures set the indexed and subfields flags
- *(engine)* the recording sink records a batch at its flush
- *(engine)* seed the unread-column case before the slot so the backfill can't race it
- *(engine)* an unread-column update leaves the OpenSearch documents unwritten
- *(engine)* an update of unread columns rebuilds nothing, each read column still does
- *(engine)* prove the guide's root-filtered self-referencing tree end to end
- *(engine,source-postgres)* assert the row's own document and guard self-join cycles
- *(engine)* a self-referencing has_many rebuilds the parent end to end ([#167](https://github.com/alias2k/flusso/pull/167))
- build every OpensearchConfig literal with index_settings

## [0.16.0](https://github.com/alias2k/flusso/compare/flusso-engine-v0.15.1...flusso-engine-v0.16.0) - 2026-10-09

### Added

- *(kernel)* distinct on aggregates and filters on a through junction
- *(source)* carry the pre-image on a change and resolve the old parent from it
- *(engine)* commit a lone change on a quiet stream without waiting out max_delay
- *(bench)* in-process PR-gating benches — engine loop over mocks, pgoutput decode over a recorded fixture, sink render
- *(engine)* [**breaking**] build-once ingest engine and one sink engine per sink over a bidirectional stream
- *(config)* [**breaking**] adapters own their configuration; port entries are type + options
- *(sources,engine)* report source continuity and rebuild seeds it invalidates

### Fixed

- *(source-postgres)* set replica identity before the slot; never alter a partitioned table
- *(engine)* max_delay caps a burst inside the drain; max_changes is non-zero
- *(engine)* coalesce backfill requests by lane, not a fixed window
- *(engine)* [**breaking**] review fixes — positions monotonic across streams, per-sink envelope stamping, supervisor loop
- *(engine)* stage stale-seed rebuilds before the resume point exists

### Other

- e2e for ensure_pre_image and an auto-managed pipeline
- e2e for deleted and re-parented children and replica-identity gaps
- *(engine)* describe quiet-stream batching and max_delay's remaining roles
- *(engine)* a lone change on a quiet stream commits without waiting
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
