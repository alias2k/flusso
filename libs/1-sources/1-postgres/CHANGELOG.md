# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Fixed

- Keyed document lookup now casts each key/foreign-key operand to its column's
  catalog type (`$n::<type>`), so a `uuid` (or `date`/`timestamptz`/…) primary or
  foreign key no longer fails backfill and live capture with
  `operator does not exist: uuid = text`. Previously only filter operands were
  cast; keys were bound as `text`.

## [0.3.1](https://github.com/alias2k/flusso/compare/flusso-sources-postgres-v0.3.0...flusso-sources-postgres-v0.3.1) - 2026-06-19

### Added

- improve upon claude skills

## [0.3.0](https://github.com/alias2k/flusso/compare/flusso-sources-postgres-v0.2.0...flusso-sources-postgres-v0.3.0) - 2026-06-18

### Added

- add the ids field support

## [0.2.0](https://github.com/alias2k/flusso/compare/flusso-sources-postgres-v0.1.1...flusso-sources-postgres-v0.2.0) - 2026-06-18

### Added

- add the publication management

### Other

- code hygene, removed useless comments

## [0.1.1](https://github.com/alias2k/flusso/compare/flusso-sources-postgres-v0.1.0...flusso-sources-postgres-v0.1.1) - 2026-06-17

### Added

- change the "run" command to follow the "cargo" pattern of updating the "lock" file

### Other

- Merge pull request #9 from alias2k/feature/run-command-rewamp
