# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.19.0](https://github.com/alias2k/flusso/compare/flusso-engine-v0.18.0...flusso-engine-v0.19.0) - 2026-11-28

### Added

- *(engine)* [**breaking**] stop when another process changed the records, instead of reconciling
- *(source)* a stream opened past Rereads::open vouches for nothing (m-1)
- *(daemon)* every reattach's gap is kept exactly as a list, and a gap end the source can't read is settled at a start (gap list, m4)
- *(daemon)* reattach --accept-gap keeps its gap and the fence refuses inside it; records read again once the slot is held (O16, S3, O29)
- *(daemon)* /status shows rebuilds and detached lanes, and a reindex of a rebuild starting or running answers 409 (O20)
- *(engine)* a reindex of a detached index rebuilds it to the seed, a start attaches a rebuild or demotes it, and O20(c)
- *(engine)* attach a detached lane by quiescing its index's builder, and publish nothing to a detached lane
- *(engine)* Progress attaches a detached lane for a newer attempt, answering through what was routed
- *(engine)* detach a lane at its next refused probe, discard what it holds, and stage it detached
- *(engine)* a detached lane leaves the minimum, and with none attached the source is free
- *(engine)* read the detach records at the top of every staging step
- *(engine)* a reindex unsticks a parked, blocked or restaging lane
- *(engine)* a refused lane writes a fresh build of its item in the item's place
- *(engine)* a builder serves a parked lane's fresh build on a substitute lane
- *(engine)* report the kind of each index failure and restart
- *(engine)* restart one index's builder alone on its own failure
- *(daemon)* /status's ingest.indexes.<name>: documents waiting, backlog age, exemption, hold, walk, working
- *(engine)* what an operator reads of each index: exemption, holds, walk, working, and the reader's wait bound
- *(engine)* walks at once, each key read on a task of its own
- *(engine)* an index held at its cap is exempt and stops no other index's reading
- *(engine)* the walk token and one idle clock over every builder
- *(engine)* a builder per index, with persistent gate waiters, the fold and the document cap
- *(engine)* a dispatcher beside the reader and the builder, polled together as N futures
- *(engine)* count committed changes by a frontier per index, and answer through what was routed
- *(engine)* a reader task reads and marks ahead of one builder, up to a lead
- *(engine)* waiting lines for build permits, and a key-read permit
- *(engine)* share the memory budget per index, for many reservers
- *(engine)* changes_committed counts what every lane of a sink landed
- *(engine)* [**breaking**] keep each index's pending documents in the progress
- *(config)* a [backoff] table sets the engines' retry schedule
- *(engine)* [**breaking**] a lane parks alone on a refusal or a write block, and restages alone on a missing target

### Fixed

- a seed drops a gap only on a generation created after it
- *(engine)* a live-only record beside a moved marker is put back; a seeded rebuild is finished by its marker
- *(sink-opensearch)* a detach over a detached record keeps where it began
- *(engine)* record-keeping sinks write only stamped, and nothing goes out unread
- *(engine)* one answer for a failed detach's own record, and say why a gated write waits
- *(engine)* judge detach records by who wrote them
- *(engine)* a lane taken detached on a read from an earlier stream is read again
- *(engine)* every gated write stands on records read under the stream now held
- *(engine)* a decision that rewrites the records stands on a read under the stream now held
- *(engine)* a failed write whose read back shows it landed is taken as done
- *(engine)* a lane's own failed detach, read again, is left to its clear
- *(engine)* a rebuild restart whose detach failed writes nothing onto its record
- *(engine)* a read after a clean end is judged, not adopted
- *(engine)* the stop's message says only what holds of the confirmation
- *(engine)* an own write whose outcome is unknown is known as before or after it
- *(engine)* what this process wrote of a record is what it knows, version included
- *(engine)* a backfill resumes from its checkpoint only where no gap is kept
- *(engine)* a read that leaves a backfill cut asks it again
- *(engine)* an owed seed refused on a blocked generation parks its lane
- *(engine)* a rebuild is ours only on its own generation, in the run too
- *(engine)* a fresh gap cuts the backfill before it; a removed rebuild is over
- *(engine)* a lane never drops a change no record covers
- *(engine)* the parked seed's slot condition in its minimal form
- *(engine)* a parked seed refused for want of the slot waits for an opening
- *(engine)* a demotion or a detach waits for the slot
- *(engine)* a seed refused for want of the slot is owed, not walked again (sixth re-check S-1, B-2)
- *(engine)* every erasing request checks the slot right before it is sent (sixth re-check B-1)
- *(engine)* before its first opening a process that will open holds no slot either
- *(engine)* one decision from the detach records, for staging and the read again (fifth re-check B-3)
- *(engine)* a reattach, a reindex and a rebuild wait for the slot (O29, the narrowed rule)
- *(engine)* a failed detach's clear waits until the slot is held (fifth re-check, second report, B-2)
- *(engine)* no seed without the slot; plain writes as on main (fifth re-check B-1, S-1)
- *(engine)* a detached lane holds confirmation until read under the current stream
- *(engine)* whose rebuild a record is, its attempts say
- *(engine)* nothing is written or seeded while no stream is current
- *(engine)* another process's rebuild is put back to detached before its lane drops anything (B-2)
- *(engine)* a lane opens or is written only once read under the current stream (B-1)
- *(engine)* a reattach finishing with no gap open says it refuses the lane's whole past and what clears it (m-2)
- *(engine)* reads of the detach records count per lane, never per sink: an index whose read failed stays unread while its siblings go on (B1''')
- *(daemon)* an unread lane vouches for nothing: the frozen answer is gone, and an opening wakes every sink engine to read at once (item 2)
- *(engine)* a reattach finds a lane held detached with no record kept and finishes it; a delete and its re-read both failing say so (m-i)
- *(engine)* a gap kept with an unknown end beside a reattach's is settled after the attach while the slot is held (m-f)
- *(engine)* the attach takes its gap by its start, open or not, and only ever moves its end later (S-a')
- *(daemon)* an unread lane's frozen answer advances at an opening only for a sink that read during the stream left; a property test over the fence gate (B1'')
- *(engine)* a gap end is settled only in a read while this process holds the slot, the marker's gap write is conditional, and a reattach reuses the open gap a failed delete left and reads the record again before answering (S-a, S-b)
- *(engine)* the live stream opens only through Rereads::open, which marks every sink unread and freezes what lanes applied under the previous stream before the new one can answer (B1', m-a)
- *(engine)* a resumed rebuild serves rebuilding, a rebuild that can't begin is owed and retried, and another process's rebuild read again is tested (m1, m2, m3)
- *(engine)* every grant follows a read of the records once the slot opened again, the ordered sink's included; tested (S5)
- *(daemon)* a reattach answers the engine's verdict, a detached record on a sink switched to backfill = false is put back live only, and refusals name what works (S4, m5, m6)
- *(engine)* a reindex of a rebuild waiting on its seed's retire block is refused with 409, naming the block (S2, O28)
- *(engine)* a rebuild running in the process survives its sink's re-staging under --skip-backfill (S1)
- *(daemon)* an unread sink masks only a lane serving its writes, keeps what it applied before the slot opened, and its read wakes waits (B1)
- *(engine)* a live-only rebuilding record beside a marker on serving is detached again (O30)
- *(engine)* Attached is the exact boundary after a run death: a builder publishes an unannounced attach before any grant
- *(engine)* an advance after every lane was free is counted once a lane is attached again (m6)
- *(engine)* an unreadable record stages as before only beside a newer unseeded marker, and is detached again otherwise (O30)
- *(engine)* a lane held detached stays so at a later staging, and its fence stays detached under a backfill ask
- *(engine)* a staging during a run says itself that it found the index detached, and the run takes the lane so
- *(engine)* a detach again that the sink keeps no record of fails the staging step
- *(engine)* a detach record that landed although its call failed goes before the lane's next probe
- *(engine)* a live-only record never reads as a finished rebuild, nor one with no known serving copy
- *(engine)* a detached lane drains at once, not one item per idle tick
- *(kernel)* append the ISO pattern only where a format can't read what flusso writes there
- *(engine)* the park reason ends the destination's block text with one semicolon
- *(engine)* read each block of a seed's refusal on its own, and say what a delete costs
- *(engine)* a seed refused by a blocked old generation parks its lane
- *(engine)* an unreadable detach marker stages as before, and every _mget entry answers for its id
- *(engine)* an owed reindex's reason wins over a later left-out one, and the staging wording says what holds
- *(engine)* staging per index stops when the destination stops answering, and at a stop
- *(engine)* a walk checkpoint is recorded only for the backfill it belongs to
- *(engine)* an index left out by a staging keeps its backfill, and staging per index reaches every index
- *(engine)* a sink's staging goes on without an index it cannot ensure
- *(bench)* the engine bench spawns the ingest engine, as the daemon does
- *(engine)* a restarted ingest run with the stop asked still opens its stream
- *(engine)* a stop landing while staging ensures makes it ensure only
- *(daemon)* a stop before the first staging never lets the source prepare
- *(engine)* staging a run restarted during the stop only ensures
- *(engine)* a reindex staged inline as the stop comes opens no lane either
- *(engine)* a stop releases every restaging lane and finishes a staging under way as state only
- *(engine)* an index owed its reindex is announced rebuilding, also one never started
- *(engine)* a stop while a reindex is owed drains at once and stages nothing
- *(engine)* a reindex whose backfill cannot be asked for is owed, not lost
- *(engine)* a reindex whose staging fails is retried until it stages, not dropped
- *(engine)* a parked item's keys are matched by index and rendered id, and alike renderings give none
- *(engine)* held items stay until acknowledged, as a list, released at a stop, and a reindex that cannot stage fails alone
- *(engine)* no probe after a failed run, nothing seeded for no document, map before acknowledgement
- *(engine)* an F9 entry goes only when the lane's account of unacknowledged items says so
- *(engine)* one fresh build waits per lane, none is granted while stopping
- *(engine)* a substitute's documents stay superseded until no older item can come back
- *(engine)* restart a failed index its wait after the failure, and pin W2 while the walk reads keys
- *(engine)* a walk's retry waits for a running walk, and a failure is reported as it happens
- *(engine)* a failed walk is retried alone while the index's live work goes on
- *(engine)* per-index restart: holds, budget waits, backoff per kind, resume
- *(engine)* name the index pacing the reader, track enabled indexes only
- *(engine,daemon,cli)* working is about progress, the backlog age is a lag, a run's end clears its state
- *(engine)* a resumed walk's planning reads take the key-read permit; tests pin no read ahead, a waiting read's abort and each walk's bound
- *(engine)* rule 4 of the budget is per index: an index holding no backfill bytes gets one backfill batch
- *(engine)* the exemption's log lines are the engine's, checked on the tick for every index
- *(engine)* an exempt index grows only while another index is free, and the log names the current state
- *(engine)* an exempt index grows only while it is held and another index isn't
- *(engine)* a settling builder answers its turn at the gate while it waits on a build or the budget
- *(engine)* beside other builders a batch the budget refuses keeps its place in the line (G6 across builders)
- *(engine)* what a capped take leaves builds beside a running build while a full batch of it is left
- *(engine)* a folding builder's backlog take wakes the reader
- *(engine)* a builder blocked publishing holds no permit and no place in a line, and folds
- *(engine)* a builder leaves every gate line when it ends, and its snapshot's line at the stop
- *(engine)* a capped build reserves for the documents it takes; schedules resume from what was confirmed
- *(engine)* a lane's newest names only positions whose every document of its index was taken
- *(engine)* pin the builder's end and inbox rules, and keep the dispatcher's request read across its polls
- *(engine)* forget a run in one step, honour cut reads at once, and keep the restart cap per lane
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

- *(engine)* a rebuild over a gap waiting for the slot says why on its lane
- B-1, a seed dropping a gap on a generation that still holds its ghosts
- *(engine)* the seed's read back forgets a live-only rebuilding record
- *(daemon,engine)* a live-only detach leaves a moved marker where it is
- *(daemon,engine)* the stand-ins model detaching_from and the read's put-back
- *(engine)* guard the restart arm and every gated write's fresh read
- *(engine)* slot.rs says the seed's record delete is best effort, and how writes tell known
- *(engine)* Restaged::WaitsForSlot, not the stand-in's old name
- *(engine)* a single deployment's three regressions (eighth re-check S-1, S-2, S-3)
- *(adr)* after the seventh re-check, the window as it is, and the guard in CLAUDE.md
- *(engine)* the sink engine holds its sink as one that can't erase
- *(daemon)* the world's other process finishes rebuilds and reattaches (seventh re-check S-3)
- *(engine)* no lane drops a change no record covers (seventh re-check B-1)
- *(engine)* a parked seed met without the slot waits without spinning (seventh re-check B-2)
- *(engine)* every record, marker and generation write in one table
- *(engine)* a read again is due with no slot held; an owed rebuild waits for it
- *(engine)* a staging without the slot leaves another process's rebuild to the holder (sixth re-check S-2)
- *(engine)* a seed met without the slot lands once held, a parked lane included, with no new walk (sixth re-check B-2, S-1)
- *(engine)* a gate checked before awaits lets an erasing request through once the stream ended (sixth re-check B-1)
- *(engine)* the narrowed O29 rule and every record write's decision in the module docs
- *(engine)* a substitute arriving with an opening is written only once its lane is read (fifth re-check N1)
- *(engine)* pin the hold with two detached lanes and a failed read of a detached lane; drop take_attached's redundant guards (fifth re-check)
- *(daemon)* one capture per process in the world, and its write oracle on the world's own state (fifth re-check, CI red)
- *(engine)* a staging demotes another process's rebuild as the read again does (fifth re-check B-3)
- *(engine)* a failed detach's clear waits while no stream is held (fifth re-check, second report, B-2)
- *(engine)* a stream that ended seeds nothing over a detach; a stop after a failed opening drains (fifth re-check B-1, S-1)
- *(engine)* a step whose read spans an opening counts no read for it (S-1)
- *(engine)* the live() tripwire also catches calls through the trait's path, and says why it isn't a type-level token (m-1)
- *(engine)* the Rereads example names its lane's types (B1''')
- *(engine)* a guard that the live stream is opened only through Rereads::open (m-h)
- *(engine)* an ingest run's end releases the slot, a failed opening counts no read, and a re-staging while held settles a flagged gap
- *(engine)* the reattach kill matrix asserts where the fence freezes or refuses, and dies after Attached too (m-d)
- *(engine)* a reattach killed before each step comes back detached or with its gap; fails if the record goes first (S3)
- *(adr)* step 4 as built; a second reattach keeps the first gap's start
- *(engine)* a rebuild's backfill waits behind another sink's walk of the index (O27)
- *(engine)* the attach's R6 check asserted, the quiesce beside another builder, the fence after an attach, and step 3's review in the note
- *(engine)* R7 inside a shared walk, a backfill for a detached lane starts no walk, and Attached only once the progress counts the lane
- *(engine)* SinkControl is non-exhaustive, and two doc lines read as meant
- *(engine)* the item a lane parked on is still written after a detach the sink kept no record of
- *(engine)* the fence hears a detach before the lane leaves the minimum, in a run and at staging
- *(engine)* spike S5 reads every emitted change by its table
- *(engine)* spike S6, what the attach boundary must survive
- *(source-postgres)* spike S5, the source confirms what no lane needs
- *(engine)* pin which named date formats read what flusso writes
- *(engine)* a space-only date format indexes, reads both forms and prints the user's
- *(engine)* the review's cases for a seed parked on a blocked old generation
- *(sink-opensearch)* a seed past a read-only generation waits for the block
- *(engine)* a seed refused by a blocked old generation parks its lane
- *(engine)* pin the review's cases for reading the detach records
- *(engine)* staging reads the detach records first, failing until it does
- *(engine)* pin R5 on a live item finding its target gone, and on a walk cut with nothing asked again
- *(engine)* an index left out keeps its backfill, many indexes leave only the failing one out, and restages are bounded
- *(engine)* a restarted run's staging goes on without the index it cannot ensure
- Merge pull request #473 from alias2k/feat/lane-failure-4
- *(bench)* what the spawned engine bench reads, and the poller's real cap
- *(sinks)* a closed index is blocked, never rejected, and parks its lane until opened or reindexed
- *(engine)* a stop's two remaining waits, one backfill request per index as before, and keys that differ
- *(engine)* an open lane not ensured under the stop is left restaging and released
- *(engine)* the stop table covers the window, with where else an item can be, and every reindex path
- *(engine)* a restage out as the stop comes asks for nothing and opens nothing
- *(engine)* a retry's ask in progress at the stop completes unserved, and nothing else moves
- *(engine)* a restart forgets an owed reindex, a stop drains past one, and a build's ids are one index's
- *(engine)* a lane item carries its build's own ids, not a key per envelope
- *(engine)* a reindex unsticks a write-blocked index, live
- *(engine)* a document's key rides in its envelope, not in a vector of its own
- *(engine)* the random schedules fail a sink write after every substitute
- *(engine)* the parked item is never rewritten whole after its substitute
- *(engine)* the lane health module names the F9 map
- *(engine)* a row OpenSearch refuses unparks its lane once fixed or deleted
- *(engine)* a build of one index publishes its envelopes and keys as they are
- *(engine)* a walk blocked on a full lane holds no other walk's key reads; the keyset e2e with walks at once
- *(engine)* one index builds its holds from the moved charges, with no record lookup
- *(engine)* one index keeps no outstanding record, its hold the budget's charge alone
- *(engine)* a pending document costs 266 B held, 314 B at the peak
- *(engine)* the exemption end to end: the pending cap, D4, the stream's end and the stop with an exempt backlog, release, and stuck stretches in the schedules and the model
- *(engine)* stage 8a 6b-1c fix round 2: the grant check's cost, settle's turn, and what both fix rounds cost
- *(engine)* a grant check costs work bounded by what builds, not by what waits
- *(engine)* G4 across builders, a slice only for an index a change touched, a build error ending the run beside another builder
- *(engine)* the reader looks up a change's builder once per index, not per document
- *(engine)* stage 8a slice 6b-1c as built: ADR 0012 reads "a builder per index", the note, CLAUDE.md and the contributor page
- *(engine)* a builder per index: the document cap, two indexes, a stop with one stalled, a freed permit, the fold, the walk token
- *(engine)* drop slice 4's deferral, which a builder per index replaces
- *(engine)* the mid-forget test proves its hook ran; the cap's doc says per lane
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
