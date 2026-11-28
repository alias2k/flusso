# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.19.0](https://github.com/alias2k/flusso/compare/flusso-cli-v0.18.0...flusso-cli-v0.19.0) - 2026-11-28

### Added

- *(cli)* [**breaking**] two OpenSearch sinks on one cluster are refused
- *(daemon)* every reattach's gap is kept exactly as a list, and a gap end the source can't read is settled at a start (gap list, m4)
- *(daemon)* reattach --accept-gap keeps its gap and the fence refuses inside it; records read again once the slot is held (O16, S3, O29)
- *(daemon)* /status shows rebuilds and detached lanes, and a reindex of a rebuild starting or running answers 409 (O20)
- *(engine)* detach a lane at its next refused probe, discard what it holds, and stage it detached
- *(cli)* check --fix rewrites a lock the config can repair
- *(cli)* [**breaking**] check compares flusso.lock with what the config compiles to
- *(cli)* [**breaking**] check --format json is the human report's content and nothing more
- *(cli)* [**breaking**] check only checks; flusso show config|mapping holds the detail
- *(cli)* run --strict refuses every schema error; the others are logged and counted
- *(cli)* check prints errors last, with a summary line, coloured
- *(engine)* a builder serves a parked lane's fresh build on a substitute lane
- *(daemon)* [**breaking**] keep an index's last error per kind
- *(cli)* export flusso.index.down, backfill_retrying and restarts{kind}
- *(cli)* flusso.index.* per index and flusso.reader.waits{bound}
- *(cli)* [**breaking**] GET /indexes/<name>/status and POST /indexes/<name>/reindex; POST /reindex is gone
- *(cli)* a stop cut short by its deadline names each index left with documents read and not built
- *(config)* [concurrency] max_pending_documents, --max-pending-documents, FLUSSO_MAX_PENDING_DOCUMENTS
- *(cli)* [concurrency] max_waiting_per_index and max_waiting, with flags and FLUSSO_* variables
- *(engine)* share the memory budget per index, for many reservers
- *(engine)* changes_committed counts what every lane of a sink landed
- *(engine)* [**breaking**] keep each index's pending documents in the progress
- *(config)* a [backoff] table sets the engines' retry schedule
- *(alerts)* FlussoLaneParked, and a lane stuck restaging in flusso.lane.parked
- *(daemon)* /status names each lane's health; flusso.lane.parked and flusso.lane.probes

### Fixed

- *(cli)* pin that run binds its listeners before asking any cluster
- *(sink-opensearch)* reading the cluster uuid fails as a read
- *(cli)* ask the clusters at once after binding, and name the sinks a reindex skipped
- *(cli)* find two OpenSearch sinks on one cluster by the cluster's uuid
- *(cli)* two OpenSearch sinks on one cluster are a finding of check's report
- *(daemon)* /status and the log say an unknown gap end settles by itself at the next read while the slot is held, and what an operator can do meanwhile (m-g)
- *(engine)* a gap end is settled only in a read while this process holds the slot, the marker's gap write is conditional, and a reattach reuses the open gap a failed delete left and reads the record again before answering (S-a, S-b)
- *(daemon)* a reattach answers the engine's verdict, a detached record on a sink switched to backfill = false is put back live only, and refusals name what works (S4, m5, m6)
- *(engine)* a reindex of a rebuild waiting on its seed's retire block is refused with 409, naming the block (S2, O28)
- *(config)* a lock with CRLF line endings keeps its indexes' names
- *(cli)* a stale CRLF lock names its line endings as the cause
- *(cli)* show writes every flusso value as plain JSON
- *(cli)* a lock that can't be read is never headlined as missing
- *(source)* a time-of-day format over a date or timestamp is an error
- *(config)* a lock's decode error quotes no value from the lock
- *(cli)* the lock advice names the config and --offline, so it runs as printed
- *(cli)* a failure after check --fix says the lock was rewritten
- *(kernel)* append the ISO pattern only where a format can't read what flusso writes there
- *(source-postgres)* prepare is quiet only about the publication gaps the start reported
- *(cli)* the lock advice names the path check compared
- *(config)* a lock that fails to parse names the line, never its text
- *(cli)* a lock holding the same config in other bytes is no error
- *(source)* [**breaking**] drop the timestamp_format error; show mapping notes the appended pattern
- *(cli)* check's exit follows its summary, and offline gaps are null
- *(cli)* judge schema errors and capture gaps in one verdict, reported once
- *(source)* [**breaking**] a namesake pair stored unchecked or read by a script is judged by what holds it
- *(cli)* [**breaking**] a gap in capture flusso run won't close is an error
- *(source)* [**breaking**] namesakes whose documents differ from the mapping are an error
- *(source)* required over a view's column is a warning that says why
- *(source)* [**breaking**] a timestamp format OpenSearch refuses is an error
- *(cli)* each schema error a start goes on with is read once, by whoever reads stderr
- *(cli)* the publication line reads well for one table and for none
- *(cli)* an offline check that passes says the database checks did not run
- *(cli)* show mapping names a disabled index as disabled and still shows it
- *(cli)* show config masks every literal secret whole
- *(source)* [**breaking**] an index that differs from its schema is an error
- *(engine)* name the index pacing the reader, track enabled indexes only
- *(engine,daemon,cli)* working is about progress, the backlog age is a lag, a run's end clears its state
- *(cli)* an unset max_waiting rises to max_waiting_per_index, and the other way round
- *(engine)* the share bounds an index's live bytes, and nothing per index passes the limit
- *(engine)* a backfill whose target was created anew no longer seeds it
- *(cli)* the stop message says a refused lane is refused, from the one name /status gives
- *(cli)* one judge for [backoff], and an unset cap rises with the first wait
- *(daemon)* /status names a refused lane `refused`, as the metric's `why` does

### Other

- *(cli)* say what the cluster check can't tell apart
- *(cli)* the reattach's Failed and timed-out verdicts answer 503 at the edge (m-b)
- *(config)* a CRLF flusso.toml compiles to the config of its LF twin
- *(config)* a CRLF lock holds the config of its LF twin
- *(cli)* the rewritten line names the lock once; rename a stale test helper
- *(cli)* check --fix writes nothing when the config fails to validate
- *(cli)* a stale lock differing by a value of the same length
- *(cli)* a plain start over a capture gap reports it once and sets the gauge
- *(cli)* each check_report test writes its own project
- *(cli)* drop the coverage branch the one-line section made unreachable
- *(cli)* a strict refusal creates no slot or publication and leaves the replica identity
- check only checks; show holds the mapping; wrong-index errors and --strict
- *(cli)* each stretch of a reader wait under one bound is one observation
- *(cli)* the index gauges' descriptions say what they measure
- *(engine)* drop the change counts nothing reads
- *(engine)* [**breaking**] key lane items per index, not by their position
- *(cli)* an open lane has no flusso_lane_parked series
- *(cli)* a stop with a write-blocked lane exits 1 at the deadline naming the lane, and a restart converges
- rustfmt
- FlussoLaneParked fires for a parked lane, a refused backfill page and a lane stuck restaging; restaging is exported in flusso.lane.parked

## [0.18.0](https://github.com/alias2k/flusso/compare/flusso-cli-v0.17.0...flusso-cli-v0.18.0) - 2026-11-19

### Added

- *(config)* cap [concurrency] writes at 64 (#408 review 1, finding 3)
- *(engine)* write several lane items at once per sink (#386 stage 2)
- *(engine)* [**breaking**] stage timings account for each task's whole time (#399 review)
- per-stage timings in /metrics and the bench's attribution table ([#399](https://github.com/alias2k/flusso/pull/399))
- *(query)* [**breaking**] a fenced read rides out a restart of flusso
- *(dev)* the visibility wait on the dashboard and as an alert; name it beside slot lag
- *(script)* [**breaking**] a real date type in script fields ([#364](https://github.com/alias2k/flusso/pull/364))
- *(source)* [**breaking**] validate array columns against the catalog ([#276](https://github.com/alias2k/flusso/pull/276))
- *(cli)* --ca-file for the indexes and reindex admin client

### Fixed

- *(daemon)* [**breaking**] Phase and ControlError are non_exhaustive
- *(cli)* build the stop on Windows; check the CLI on Windows and macOS in CI
- *(cli)* [**breaking**] force a stop only on the same sender twice quickly; keep nohup's ignored SIGHUP
- *(cli)* [**breaking**] drain on stop under a 30 s deadline; a repeated signal is one stop
- *(cli)* bound the stop, honour a second signal and SIGHUP, follow the parent
- *(query)* pace, bound and classify fence retries (#377 review)
- *(source-postgres)* read and write every number exactly
- *(cli)* check judges the source server as run does
- *(source-postgres)* [**breaking**] emit a change only once its transaction is visible
- *(test)* the guard rejects the postgres import, not call shapes
- *(test)* every test and bench starts Postgres through dev/test-postgres
- *(script)* a decimal script result is written with every digit
- *(source-postgres)* [**breaking**] write every numeric digit as a JSON number
- *(source)* a check the database would not let run is reported, not dropped ([#324](https://github.com/alias2k/flusso/pull/324))
- *(cli)* a refusal stops the start only when every sink refuses ([#316](https://github.com/alias2k/flusso/pull/316))
- *(source)* [**breaking**] a schema that can never index a document stops the start (#316, #309)
- *(source-postgres)* a table's Relation message drops its cached column types ([#298](https://github.com/alias2k/flusso/pull/298))
- *(source-postgres)* say a lossy ids key may be stored altered; check prints a skipped probe (#299, #313)
- *(cli)* an ids element_type that holds no key prints and starts ([#299](https://github.com/alias2k/flusso/pull/299))
- *(source-postgres)* check reports a link or ids element the key can't take (#299, #312, #313)
- *(source-postgres)* read every key type the same way in backfill, WAL and build
- *(design,source)* a name applies when the press that left its box ends; key lists by identity
- *(source,design)* duplicate-name outcomes for constants and binary; one-line node header; palette returns focus
- *(design)* refuse a field name its level already has; warn about duplicates; read typed table and column names as flusso does
- *(cli)* script run reports a document that fails to build as the failure
- *(source)* a domain over an array or jsonb holds an array
- *(source)* [**breaking**] settle the root-key kinds; probe transforms without a table
- *(source)* report four schemas check accepted that cannot work

### Other

- count how each budget wait ends: flusso.memory.wait{work,wake}
- e2e: a backfill killed mid-way resumes and reads fewer key-page rows than a full one; stage notes as built
- /status backfill_checkpoints, flusso.backfill.checkpoints, docs
- /wait answers 503 backfilling or rebuilding, naming the lanes
- changes are resolved on a connection of their own (step 2)
- flusso.sink.refreshes{sink}: refresh requests a sink sent; dev/bench reports them per phase
- compile fix: backends.rs test Batch takes walked (3bf541c4 crossed 73cfe0d7)
- run warns when max_built_mib reaches the cgroup limit or host memory
- a deprecated [stream] capacity or --queue-capacity validates, warns once and is ignored
- the memory series in metrics.md's Prometheus table, pinned by an export test
- e2e: a fenced read with an index prefix waits on the logical index (review: not verified)
- tests: /status carries each lane's stall and the pinning lane; the gauges export them
- stall age and WAL behind per lane, and the lane that pins the slot
- [stream] capacity is deprecated: accepted, warned about, ignored; lanes keep a 1,048,576-item backstop
- flusso.lane.outstanding{index,sink}: items routed to a lane and not landed
- /status reports each lane; fence and /wait tests per index
- one lane per (index, sink): stream, Progress, sink engine, publish, requests, fence
- [concurrency] max_built_mib (16..=1048576, default 512), the daemon's budget, /status memory, flusso.memory metrics, the designer's field
- the engines confirm and fence from one Progress; the stream keeps no watermark
- --builds / FLUSSO_BUILDS, the daemon's DEFAULT_BUILDS, the builder pool at builds + 2, the designer's field
- the source confirms and fences from Landed; the Postgres capture keeps no map
- a backfill pages each index's root keys on the builder pool
- *(sink)* [**breaking**] Sink::write replaces apply + flush (#386 stage 2)
- *(cli)* random stops under writes converge after a restart (seeded)
- exit 0 means the final position was confirmed; --shutdown-timeout-secs 0 is the one exception
- the help, manual and demo compose describe the drain and the force rule
- *(cli)* check fails where run refuses the source server
- *(source-postgres)* a wide document's first failure is the first declared
- *(agents)* add the coordinator skill and guard its paths
- *(source)* a check the database would not let run is reported ([#324](https://github.com/alias2k/flusso/pull/324))
- *(cli)* a refusal stops the start only when every sink refuses ([#316](https://github.com/alias2k/flusso/pull/316))
- Merge pull request #321 from alias2k/fix/type-pairs-that-fail-at-runtime-299
- *(cli)* check installs no logger of its own ([#299](https://github.com/alias2k/flusso/pull/299))
- *(cli)* the duplicate-names matrix deletes only its own pair's indexes
- *(cli)* an ids element_type that holds no key prints and starts ([#299](https://github.com/alias2k/flusso/pull/299))
- warn on a timestamp format that refuses the ISO form; pin tuned mappings
- Merge pull request #249 from alias2k/fix/check-validation-gaps

## [0.17.0](https://github.com/alias2k/flusso/compare/flusso-cli-v0.16.0...flusso-cli-v0.17.0) - 2026-11-12

### Added

- *(query)* [**breaking**] move every HTTP client to reqwest 0.13
- *(cli)* run and check ask every table read for its whole old row
- *(cli)* check prints each diagnostic's field path
- *(design)* preview reports script compile errors; the sample runs the scripts on a chosen row
- *(cli)* run scripts in every build, compile them on build/check/run, loud failure log + metric, flusso script run
- *(cli)* GET /wait holds a request until a commit is readable
- *(cli)* editor schema and Reference table for per-index sink options
- *(cli)* validate per-index sink options and hand them to the OpenSearch sink

### Fixed

- *(source-postgres)* word the key gap for live writes; skip intermediate partitioned tables
- *(source-postgres)* set a partition whose replica identity leaves out the primary key
- *(run)* refuse to start only on what cannot work as written
- *(source-postgres)* keep a USING INDEX replica identity that carries every link
- *(check)* verify the soft-delete marker, and make run refuse to start on an error diagnostic
- *(source-postgres)* leave a link-free partitioned table alone, one info line for unset tables; check's verdict says where it won't set
- review — report script failures under both policies, ids in print context, debug log of the failing document, shared build-by-id, field docs
- *(cli)* check prints an unindexed field as one object (not indexed)
- *(cli)* adopt flusso-kernel, flusso-config, flusso-sink-opensearch 0.17.0 (stored-but-not-indexed content)
- *(cli)* [**breaking**] adopt flusso-kernel, flusso-sink-opensearch 0.17.0 (typed map keys)
- *(cli)* check shows a map as map<values>
- *(design)* validate per-index sink options before connecting
- review fixes for index settings (owned blocks/hidden, shared schema hoisting)
- *(cli)* report partitioned tables streamed under their partitions' names

### Other

- *(deps)* OpenTelemetry 0.33 and tracing-opentelemetry 0.34
- Merge pull request #222 from alias2k/fix/validate-soft-delete-and-run
- say the root manual is the newest release and /main/ tracks main
- Merge remote-tracking branch 'origin/main' into feat/script-fields
- flusso replaces a replica identity set on purpose; what FULL costs other consumers; check footnote names its group
- script fields and input-only fields — reference, how-to, CLI, metrics, /status, glossary, CLAUDE.md
- *(cli)* per-index settings skip disabled indexes and key by logical name
- *(cli)* box the OpenSearch sink config variant

## [0.16.0](https://github.com/alias2k/flusso/compare/flusso-cli-v0.15.1...flusso-cli-v0.16.0) - 2026-10-09

### Added

- *(source-postgres)* set REPLICA IDENTITY FULL on child tables that need it
- *(source-postgres)* add manage_replica_identity (default on) and its --manage-replica-identity override
- *(cli)* wire pre-image columns into the source and report replica identity in check
- *(cli)* [batch] reaches the ingest engine; --batch-max-changes / --batch-max-delay-ms
- *(engine)* [**breaking**] build-once ingest engine and one sink engine per sink over a bidirectional stream
- *(design)* render source, stream, and sink forms from adapter descriptions
- *(cli)* generate the editor schema and Reference tables from adapter descriptions
- *(config)* [**breaking**] adapters own their configuration; port entries are type + options
- *(cli)* flusso run skips the lock rewrite when nothing changed
- *(cli,design)* wire source TLS through both composition roots

### Fixed

- *(source-postgres)* a USING INDEX identity must carry the primary key to count
- *(source-postgres)* set replica identity before the slot; never alter a partitioned table
- *(source-postgres)* address review — precise warn-once, safe keying under USING INDEX
- *(engine)* max_delay caps a burst inside the drain; max_changes is non-zero
- *(engine)* [**breaking**] review fixes — positions monotonic across streams, per-sink envelope stamping, supervisor loop

### Other

- *(engine)* share published envelopes via Arc, sink stamps own nam
- describe the kernel / ports / adapters layout and adapter-owned config
- *(arch)* [**breaking**] lay out kernel / ports / adapters / engine / daemon and sdk/
- READMEs as landings, repoint every deep link, serve the book at the Pages root, add the docs CI job
- root README as a pitch, flusso-query README as a landing; track the Start here pages
- *(cli)* guard the agent docs against dangling repo paths
- *(release)* [**breaking**] three release trains — per-crate versions + version groups
- the lock is deterministic TOML; formats frozen for the major
- source TLS — configuration guide section, README, crate README, plugin skills

## [0.14.0](https://github.com/alias2k/flusso/compare/flusso-cli-v0.13.0...flusso-cli-v0.14.0) - 2026-08-14

### Added

- *(query-derive)* [**breaking**] deprecate path= instead of deleting it

## [0.11.0](https://github.com/alias2k/flusso/compare/flusso-cli-v0.10.1...flusso-cli-v0.11.0) - 2026-07-23

### Added

- *(design)* install stderr logging so the listening URL prints
- *(design)* open the designer in a browser on start
- *(design)* React SPA + flusso design CLI subcommand

### Other

- *(cli)* put the designer behind a default-on feature; Docker omits it
- document the visual schema designer

## [0.10.1](https://github.com/alias2k/flusso/compare/flusso-cli-v0.10.0...flusso-cli-v0.10.1) - 2026-06-26

### Other

- lead README with the tagline, move AI disclosure below it
- consistency pass — fix factual errors and terminology drift
- rewrite all docs to the prose style guide
- clarify pre-commit hook formats the whole workspace
- add dev workflow tooling
- split into an mdBook manual + per-crate READMEs

## [0.9.2](https://github.com/alias2k/flusso/compare/flusso-cli-v0.9.1...flusso-cli-v0.9.2) - 2026-06-24

### Added

- *(cli)* select OTLP transport (gRPC vs HTTP) via OTEL_EXPORTER_OTLP_PROTOCOL

## [0.4.0](https://github.com/alias2k/flusso/compare/flusso-cli-v0.3.2...flusso-cli-v0.4.0) - 2026-06-19

### Added

- runtime index prefix to namespace all indices ([#24](https://github.com/alias2k/flusso/pull/24))

### Other

- move inline unit tests into sibling tests.rs modules

### Added

- `--index-prefix` / `FLUSSO_INDEX_PREFIX` to namespace every index per deployment.

## [0.3.1](https://github.com/alias2k/flusso/compare/flusso-cli-v0.3.0...flusso-cli-v0.3.1) - 2026-06-19

### Added

- improve upon claude skills

## [0.2.0](https://github.com/alias2k/flusso/compare/flusso-cli-v0.1.1...flusso-cli-v0.2.0) - 2026-06-18

### Added

- add the publication management

### Other

- code hygene, removed useless comments

## [0.1.1](https://github.com/alias2k/flusso/compare/flusso-cli-v0.1.0...flusso-cli-v0.1.1) - 2026-06-17

### Added

- change the "run" command to follow the "cargo" pattern of updating the "lock" file

### Other

- Merge pull request #9 from alias2k/feature/run-command-rewamp
- release v0.1.0

## [0.1.0](https://github.com/alias2k/flusso/releases/tag/flusso-cli-v0.1.0) - 2026-06-17

### Added

- license, security, coc, contributing and github templates
- wire to CLI
- on-demand reindex — alias-over-generations sink + restart trigger
- split the operational HTTP surface into public + private with Basic auth
- rename flusso-search to flusso-query
- move files to correctly reflect dependencies
- more checks in CI
- decoupled the config from the rest
- add on failure behavior
- abstract backends
- create alias pointing to the latest index
- add belong_to
- add ENV VARS to cli that works with the params, helm
- move otel from daemon to cli
- add deamon crate, add metrics export, add prometheus and grafana, add http health endpoint
- flusso cli schema command
- start client, add geo
- rename config.toml to flusso.toml and flusso.bin to flusso.lock
- rename compile to build
- add compile functionality
- better opensearch defaults
- improve env vars handling and readme about sinks and sources
- add tracing
- add opensearch support
- backfill and renaming
- add backfill
- rebranding to storno
- update deps
- some e2e tests
- documentation
- project structure and start with schema

### Fixed

- clippy and fmt

### Other

- preparation for going public
- documentation
- add requirements section
- code cleanup and improvements
- split cli into multiple files
- rename check.rs to correctly named print.rs
- add SCHEMA.md
- readme
- readme and cleanup
- README
