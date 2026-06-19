# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.4.0](https://github.com/alias2k/flusso/compare/flusso-schema-core-v0.3.2...flusso-schema-core-v0.4.0) - 2026-06-19

### Added

- runtime index prefix to namespace all indices ([#24](https://github.com/alias2k/flusso/pull/24))

### Other

- move inline unit tests into sibling tests.rs modules
- *(core)* typed canonical value vocabulary between source and sink

### Added

- `validate_index_prefix` for the deployment-wide index prefix.

### Changed

- **`GenericValue` is now a fully-typed canonical value vocabulary** — the
  middle type between a source and a sink. Numerics are split by width
  (`SmallInt`/`Int`/`BigInt`, `Float`/`Double`, `Decimal`), temporals are split
  (`Date`/`Time`/`Timestamp`/`TimestampTz`), and `Uuid`/`Bytes` are first-class,
  so no semantic type is erased to a string in transit. Serde is now the
  **derived, format-agnostic** representation (externally tagged), so a value
  round-trips losslessly through any queue encoding — in as a `GenericValue`,
  out as the same `GenericValue`. Core picks no wire format.

## [0.3.1](https://github.com/alias2k/flusso/compare/flusso-schema-core-v0.3.0...flusso-schema-core-v0.3.1) - 2026-06-19

### Added

- improve upon claude skills

## [0.3.0](https://github.com/alias2k/flusso/compare/flusso-schema-core-v0.2.0...flusso-schema-core-v0.3.0) - 2026-06-18

### Added

- add the ids field support

## [0.2.0](https://github.com/alias2k/flusso/compare/flusso-schema-core-v0.1.1...flusso-schema-core-v0.2.0) - 2026-06-18

### Added

- add the publication management

### Other

- code hygene, removed useless comments

## [0.1.1](https://github.com/alias2k/flusso/compare/flusso-schema-core-v0.1.0...flusso-schema-core-v0.1.1) - 2026-06-17

### Added

- change the "run" command to follow the "cargo" pattern of updating the "lock" file

### Other

- Merge pull request #9 from alias2k/feature/run-command-rewamp
