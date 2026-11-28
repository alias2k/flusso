# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.19.0](https://github.com/alias2k/flusso/compare/flusso-sink-opensearch-v0.18.0...flusso-sink-opensearch-v0.19.0) - 2026-11-28

### Added

- *(engine)* [**breaking**] stop when another process changed the records, instead of reconciling
- *(daemon)* every reattach's gap is kept exactly as a list, and a gap end the source can't read is settled at a start (gap list, m4)
- *(daemon)* reattach --accept-gap keeps its gap and the fence refuses inside it; records read again once the slot is held (O16, S3, O29)
- *(sink)* begin a rebuild, demote it, and delete a finished rebuild's record at the seed
- *(sink)* record a detach, marker first, every write conditional on what was read
- *(engine)* read the detach records at the top of every staging step
- *(sinks)* a closed index parks its lane blocked, and staging it no longer fails the sink
- *(engine)* [**breaking**] a lane parks alone on a refusal or a write block, and restages alone on a missing target

### Fixed

- *(sink-opensearch)* every meta-index read waits for a shard not ready and fails as a read
- *(sink-opensearch)* H's wait stays under the client timeout, fails as a read, and only on a shard not ready
- *(sink-opensearch)* refresh the generation reads see once per start, seeded or not
- *(sink-opensearch)* record a generation's uuid only on the marker naming it
- a seed drops a gap only on a generation created after it
- *(sink-opensearch)* reading the cluster uuid fails as a read
- *(sink-opensearch)* the minors of round 13's review
- *(sink-opensearch)* a marker rewrite waits for a meta index still recovering
- *(sink-opensearch)* a demotion carries no details of another generation
- *(sink-opensearch)* a marker carrying detaching_from never reads as on serving, and keeps every gap
- *(sink-opensearch)* the meta documents' plans as pure functions; only a seed drops a gap
- *(sink-opensearch)* a detach over a detached record keeps where it began
- *(cli)* find two OpenSearch sinks on one cluster by the cluster's uuid
- *(engine)* record-keeping sinks write only stamped, and nothing goes out unread
- *(sink-opensearch)* a detach keeps the marker's gaps and gives them back
- *(engine)* judge detach records by who wrote them
- *(daemon)* /status and the log say an unknown gap end settles by itself at the next read while the slot is held, and what an operator can do meanwhile (m-g)
- *(sink-opensearch)* a gap the seed marker keeps that can't be read is a gap with unknown ends, never no gap (m-c)
- *(engine)* a gap end is settled only in a read while this process holds the slot, the marker's gap write is conditional, and a reattach reuses the open gap a failed delete left and reads the record again before answering (S-a, S-b)
- *(sink-opensearch)* a put-back restores the marker's checkpoint too, byte for byte
- *(engine)* a live-only record never reads as a finished rebuild, nor one with no known serving copy
- *(engine)* read each block of a seed's refusal on its own, and say what a delete costs
- *(engine)* a seed refused by a blocked old generation parks its lane
- *(engine)* an unreadable detach marker stages as before, and every _mget entry answers for its id
- *(sink-opensearch)* listing generations asks for no hidden indexes
- *(sink-opensearch)* a generation is created only after its meta doc says unseeded
- *(engine,sink-opensearch)* cut a backfill by its target's identity, not one staging's word
- *(engine)* a backfill whose target was created anew no longer seeds it

### Other

- spikes S1, S3 and S4 for the detach probe, against live OpenSearch and Postgres
- *(sink-opensearch)* every meta read waits for a shard not ready, then fails as a read
- *(sink-opensearch)* contains_key in MemoryMeta::ensure (clippy)
- *(sink-opensearch)* m-C, a start refreshes the generation reads see whatever its seeded flag
- *(sink-opensearch)* m-B's model test reaches a demotion that carries no uuid
- *(sink-opensearch)* m-B, a uuid recorded on a marker naming another generation
- B-1, a seed dropping a gap on a generation that still holds its ghosts
- *(sink-opensearch)* F, D and E on a real cluster
- *(sink-opensearch)* a live-only demotion after a detach during a reindex keeps the serving generation
- *(engine)* guard the restart arm and every gated write's fresh read
- *(daemon)* a rebuild begun again under the same attempts is a different rebuild
- *(engine)* the review's cases for a seed parked on a blocked old generation
- *(sink-opensearch)* a seed past a read-only generation waits for the block
- *(sink-opensearch)* spike B1, what OpenSearch answers past a read-only generation
- *(sink-opensearch)* a seed past a blocked or closed old generation deletes it
- *(sink-opensearch)* strip the prefix from a name listed after a comma
- *(engine)* pin the review's cases for reading the detach records
- *(engine)* staging reads the detach records first, failing until it does
- *(sinks)* a closed index is blocked, never rejected, and parks its lane until opened or reindexed
- *(sink-opensearch)* a generation deleted or replaced mid-backfill, case by case
- *(sink-opensearch)* a seeded generation recreated by a failed staging is never served seeded
- *(sink-opensearch)* an auto-created generation is never seeded
- *(sink-opensearch)* staging a generation deleted mid-backfill says it is new
- LaneHealthView.state documented, Chunk below the imports, LaneSnapshot's new fields on the upgrade page
- *(engine)* a lane parks alone on a refusal or a write block, and restages alone on a missing target

## [0.18.0](https://github.com/alias2k/flusso/compare/flusso-sink-opensearch-v0.17.0...flusso-sink-opensearch-v0.18.0) - 2026-11-19

### Added

- *(engine)* [**breaking**] stage timings account for each task's whole time (#399 review)
- per-stage timings in /metrics and the bench's attribution table ([#399](https://github.com/alias2k/flusso/pull/399))
- *(engine)* stage a sink's indexes concurrently, and validate indexes concurrently
- *(sink-opensearch)* [**breaking**] warn when ca_file is set with tls_verify off; say when the file is read
- *(sink-opensearch)* trust a private CA with ca_file; never retry a refused certificate

### Fixed

- *(sink-opensearch)* caught-up writes refresh one at a time (#408 review 1, finding 1)
- *(engine)* only an advance of the source is ingest progress; a failed apply drops its batch's buffer (#395 review 3)
- *(daemon)* reset an engine's restart backoff after a run that made progress (#395 review)
- *(sinks)* resend items OpenSearch refuses as busy, and release a lane delivery by its ticket ([#395](https://github.com/alias2k/flusso/pull/395))
- *(source-postgres)* [**breaking**] write every numeric digit as a JSON number
- *(engine,sink-opensearch)* a rejected document's id is shown shortened; an empty-id delete is dropped
- *(sink-opensearch)* reject an id OpenSearch refuses instead of failing the bulk

### Other

- a restart in the middle of a rebuild says rebuilding, vouching for nothing
- a first start goes on beside a plain index named like the alias; a generation without its meta doc is reused
- tests: a plain index named like the convenience alias, a first generation without its meta doc (red)
- flusso.sink.refreshes{sink}: refresh requests a sink sent; dev/bench reports them per phase
- the sink keeps a backfill checkpoint: Sink::seeding, record_walked, forget_walked; OpenSearch keeps it in the meta doc
- caught up per lane: a caught-up write covers its quiet lanes' indexes, and a lane going quiet makes its own writes searchable (review 4, 5)
- crash table for the new mark_seeded order; a first seed and each first-round request failing alone
- measured before and after; seeding indexes at once designed and pinned on the sink
- the sink engine merges ready items into one write
- rustfmt
- rustfmt mark_seeded
- cheaper seeding and generation creation (#410, #403)
- e2e for seeding and generation creation, request by request (#354, #403, #410)
- Position carries its seq and the source's stamps (touches all constructors)
- *(sink-opensearch)* each doc comment on its own item; every chunk of a caught-up write refreshes (#408 review 4, findings 1-2)
- *(sink-opensearch)* every chunk of a caught-up write refreshes, a multi-shard index is refreshed (#408 review 3, review 4 finding 2)
- *(sink-opensearch)* a caught-up write skips the refresh its own bulk already did (#408 review 2, finding 5)
- *(sink-opensearch)* a caught-up write over the one single-shard index left unrefreshed sends no extra refresh (#408 review 2, finding 5)
- *(sink-opensearch)* overlapping caught-up writes wait for an earlier refresh (#408 review 1, finding 1)
- *(sink-opensearch)* an empty write that isn't caught up reports Immediate (#386 stage 2)
- concurrent writes keep per-document order, the watermark and live-first (#386 stage 2)
- *(sink)* [**breaking**] Sink::write replaces apply + flush (#386 stage 2)
- a replayed change or backfill page is no progress, and a failed apply leaves nothing buffered (#395 review 3)
- an item refused as busy is resent, and a release redelivers only its ticket ([#395](https://github.com/alias2k/flusso/pull/395))
- *(sink-opensearch)* write a bulk action line without a value tree
- *(sink-opensearch)* an id OpenSearch refuses fails the whole bulk request

## [0.17.0](https://github.com/alias2k/flusso/compare/flusso-sink-opensearch-v0.16.0...flusso-sink-opensearch-v0.17.0) - 2026-11-12

### Added

- *(query)* [**breaking**] move every HTTP client to reqwest 0.13
- *(sink)* every key of a keyword map with subfields gets an exact .keyword subfield
- *(sink-opensearch)* report flush visibility; refresh deferred indexes when caught up
- *(sink-opensearch)* map an unindexed field as a disabled object, honour subfields: false
- *(kernel)* subfields and indexed opt-outs on columns and joins, hashed only when off
- *(sink-opensearch)* index_settings, sink-wide and per index

### Fixed

- *(sink-opensearch)* report a transport error with its cause
- *(sink-opensearch)* the exact map-key subfield follows the same subfield rule as the query derive
- review findings for the fence
- *(sink-opensearch)* map every key of a map like a plain field of its values type
- review fixes for index settings (owned blocks/hidden, shared schema hoisting)
- *(sink-opensearch)* write seeded indexes through the hash alias with require_alias; reindex is a no-op while a rebuild is pending

### Other

- *(sink-opensearch)* one sequential https test, safe under cargo test
- *(sink-opensearch)* pin the sink and flusso-query over https
- say the root manual is the newest release and /main/ tracks main
- Merge pull request #193 from alias2k/feat/keyword-map-exact
- exact operators across a keyword map's keys, the .keyword subfield, and the map key glossary entry
- crate READMEs for fences (source, sinks, engine, daemon, flusso-query)
- read-your-writes in the manual (GET /wait, the status positions, the query chapter)
- *(sink-opensearch)* an explicit empty fields option leaves no subfields
- keep content without indexing it (subfields: false, indexed: false)
- dotted map keys, the field budget and the rebuild's stricter values on upgrade
- *(kernel)* Mapping::of_type is the one place a map key's mapping is built
- map keys are mapped, queried and sorted like a plain field of their values kind
- *(sink-opensearch)* live e2e for typed map keys, coercion and rejections
- *(sink)* index settings need a restart before the reindex, and some weaken guarantees
- build every OpensearchConfig literal with index_settings
- index settings, sink-wide and per index
- *(sink-opensearch)* correct mid-backfill recovery advice and the prefixed auto_create_index pattern
- *(sink-opensearch)* address review — one home for the missing-target story, targets map, logical_names helper
- *(sink-opensearch)* writes through the hash alias, a generation deleted while running, reindex while rebuilding
- *(sink-opensearch)* e2e for a generation deleted mid-run, a second reindex, and orphan sweep

## [0.16.0](https://github.com/alias2k/flusso/compare/flusso-sink-opensearch-v0.15.1...flusso-sink-opensearch-v0.16.0) - 2026-10-09

### Added

- *(bench)* in-process PR-gating benches — engine loop over mocks, pgoutput decode over a recorded fixture, sink render
- *(engine)* [**breaking**] build-once ingest engine and one sink engine per sink over a bidirectional stream
- *(cli)* generate the editor schema and Reference tables from adapter descriptions
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

### Other

- *(bench)* pipeline bench is attribution only — drop its backfill group, trim the Docker-backed timings
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

## [0.14.0](https://github.com/alias2k/flusso/compare/flusso-sinks-opensearch-v0.13.0...flusso-sinks-opensearch-v0.14.0) - 2026-08-14

### Added

- *(query-derive)* [**breaking**] deprecate path= instead of deleting it

## [0.13.0](https://github.com/alias2k/flusso/compare/flusso-sinks-opensearch-v0.12.0...flusso-sinks-opensearch-v0.13.0) - 2026-07-24

### Added

- *(opensearch)* prebake enum sort rank into a .sort subfield
- *(core)* carry declared enum order on Column + Mapping.enum_order

## [0.11.0](https://github.com/alias2k/flusso/compare/flusso-sinks-opensearch-v0.10.1...flusso-sinks-opensearch-v0.11.0) - 2026-07-23

### Other

- document the visual schema designer

## [0.10.1](https://github.com/alias2k/flusso/compare/flusso-sinks-opensearch-v0.10.0...flusso-sinks-opensearch-v0.10.1) - 2026-06-26

### Other

- lead README with the tagline, move AI disclosure below it
- consistency pass — fix factual errors and terminology drift
- rewrite all docs to the prose style guide
- clarify pre-commit hook formats the whole workspace
- add dev workflow tooling
- split into an mdBook manual + per-crate READMEs

## [0.8.0](https://github.com/alias2k/flusso/compare/flusso-sinks-opensearch-v0.7.0...flusso-sinks-opensearch-v0.8.0) - 2026-06-23

### Fixed

- *(query)* per-type numeric kinds + kind-based values for every kind ([#45](https://github.com/alias2k/flusso/pull/45))

## [0.5.0](https://github.com/alias2k/flusso/compare/flusso-sinks-opensearch-v0.4.0...flusso-sinks-opensearch-v0.5.0) - 2026-06-22

### Other

- *(sinks-opensearch)* cover map fields rendering as dynamic objects

## [0.4.0](https://github.com/alias2k/flusso/compare/flusso-sinks-opensearch-v0.3.2...flusso-sinks-opensearch-v0.4.0) - 2026-06-19

### Added

- runtime index prefix to namespace all indices ([#24](https://github.com/alias2k/flusso/pull/24))

### Other

- move inline unit tests into sibling tests.rs modules
- *(core)* typed canonical value vocabulary between source and sink
- *(opensearch)* e2e coverage for the index prefix

### Added

- `with_index_prefix`: prepend a prefix to every index, alias, and the meta index.

## [0.3.1](https://github.com/alias2k/flusso/compare/flusso-sinks-opensearch-v0.3.0...flusso-sinks-opensearch-v0.3.1) - 2026-06-19

### Added

- improve upon claude skills

## [0.3.0](https://github.com/alias2k/flusso/compare/flusso-sinks-opensearch-v0.2.0...flusso-sinks-opensearch-v0.3.0) - 2026-06-18

### Added

- add the ids field support

## [0.2.0](https://github.com/alias2k/flusso/compare/flusso-sinks-opensearch-v0.1.1...flusso-sinks-opensearch-v0.2.0) - 2026-06-18

### Added

- add the publication management

### Other

- code hygene, removed useless comments

## [0.1.1](https://github.com/alias2k/flusso/compare/flusso-sinks-opensearch-v0.1.0...flusso-sinks-opensearch-v0.1.1) - 2026-06-17

### Added

- change the "run" command to follow the "cargo" pattern of updating the "lock" file

### Other

- Merge pull request #9 from alias2k/feature/run-command-rewamp
