# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.18.0](https://github.com/alias2k/flusso/compare/flusso-cli-v0.17.0...flusso-cli-v0.18.0) - 2026-11-14

### Added

- *(source)* [**breaking**] validate array columns against the catalog ([#276](https://github.com/alias2k/flusso/pull/276))
- *(cli)* --ca-file for the indexes and reindex admin client

### Fixed

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
