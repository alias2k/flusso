# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.4.0](https://github.com/alias2k/flusso/compare/flusso-sinks-core-v0.3.2...flusso-sinks-core-v0.4.0) - 2026-06-19

### Other

- move inline unit tests into sibling tests.rs modules
- *(core)* typed canonical value vocabulary between source and sink

### Changed

- `to_json` renders the typed `GenericValue` variants to the JSON a sink ingests:
  temporals as ISO strings, a UUID as its hyphenated string, and `bytea` as
  base64 (what an OpenSearch `binary` field wants). The base64 lives here, at the
  sink boundary — core's value vocabulary stays format-agnostic.

## [0.3.1](https://github.com/alias2k/flusso/compare/flusso-sinks-core-v0.3.0...flusso-sinks-core-v0.3.1) - 2026-06-19

### Added

- improve upon claude skills

## [0.2.0](https://github.com/alias2k/flusso/compare/flusso-sinks-core-v0.1.1...flusso-sinks-core-v0.2.0) - 2026-06-18

### Added

- add the publication management

## [0.1.1](https://github.com/alias2k/flusso/compare/flusso-sinks-core-v0.1.0...flusso-sinks-core-v0.1.1) - 2026-06-17

### Added

- change the "run" command to follow the "cargo" pattern of updating the "lock" file

### Other

- Merge pull request #9 from alias2k/feature/run-command-rewamp
