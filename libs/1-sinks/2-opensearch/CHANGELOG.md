# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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
