# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.9.0](https://github.com/alias2k/flusso/compare/flusso-query-derive-v0.8.0...flusso-query-derive-v0.9.0) - 2026-06-23

### Added

- *(query)* SortBuilder + nesting-aware Sortable sort surface
- *(query)* path metadata + FlussoDocument/FlussoIndex trait split

### Other

- *(query)* rustfmt + fix intra-doc links after trait split
- *(query)* cover nesting-aware sort + SortBuilder (refs #49)

## [0.8.0](https://github.com/alias2k/flusso/compare/flusso-query-derive-v0.7.0...flusso-query-derive-v0.8.0) - 2026-06-23

### Fixed

- *(query)* per-type numeric kinds + kind-based values for every kind ([#45](https://github.com/alias2k/flusso/pull/45))

## [0.6.0](https://github.com/alias2k/flusso/compare/flusso-query-derive-v0.5.0...flusso-query-derive-v0.6.0) - 2026-06-22

### Added

- *(query)* [**breaking**] compile-time gating of subfield accessors
- *(query)* [**breaking**] typed geo distance (Distance + DistanceUnit)
- *(query)* [**breaking**] closed enum params over stringly-typed options

### Other

- *(query)* rustfmt the enum-param edits

## [0.5.0](https://github.com/alias2k/flusso/compare/flusso-query-derive-v0.4.0...flusso-query-derive-v0.5.0) - 2026-06-22

### Added

- *(query)* complete the map matrix with NumberMap and DateMap
- *(query-derive)* generate map handles and validate map doc types

## [0.3.2](https://github.com/alias2k/flusso/compare/flusso-query-derive-v0.3.1...flusso-query-derive-v0.3.2) - 2026-06-19

### Other

- document the completed query surface (§1b) + CHANGELOGs
- *(query-derive)* issue #19 acceptance test (§1b fixture)

### Added

- Acceptance test for the completed typed query surface (issue #19): a realistic
  projection with a `Uuid` keyword field (no `#[flusso(skip)]`), an enum keyword
  field, subfield targeting, `minimum_should_match`, and null-aware sort builds
  with zero `Search::raw` / `Json::raw`. The derive needs no change — it already
  defers a `FlussoValue<Keyword>` bound for `Uuid`, satisfied by `flusso-query`'s
  `uuid` feature.

## [0.3.1](https://github.com/alias2k/flusso/compare/flusso-query-derive-v0.3.0...flusso-query-derive-v0.3.1) - 2026-06-19

### Added

- improve upon claude skills

## [0.3.0](https://github.com/alias2k/flusso/compare/flusso-query-derive-v0.2.0...flusso-query-derive-v0.3.0) - 2026-06-18

### Added

- add the ids field support

## [0.2.0](https://github.com/alias2k/flusso/compare/flusso-query-derive-v0.1.1...flusso-query-derive-v0.2.0) - 2026-06-18

### Added

- add the publication management

### Other

- code hygene, removed useless comments

## [0.1.1](https://github.com/alias2k/flusso/compare/flusso-query-derive-v0.1.0...flusso-query-derive-v0.1.1) - 2026-06-17

### Added

- change the "run" command to follow the "cargo" pattern of updating the "lock" file

### Other

- Merge pull request #9 from alias2k/feature/run-command-rewamp
- release v0.1.0

## [0.1.0](https://github.com/alias2k/flusso/releases/tag/flusso-query-derive-v0.1.0) - 2026-06-17

### Added

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

- preparation for going public
- documentation
- add requirements section
- add SCHEMA.md
- readme
- readme and cleanup
- README
