# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.11.0](https://github.com/alias2k/flusso/compare/flusso-schema-v0.10.1...flusso-schema-v0.11.0) - 2026-07-23

### Added

- *(schema)* structured index-yaml parse errors with typo suggestions

### Fixed

- *(config)* omit an empty [server] table and order flusso.toml sections

### Other

- document the visual schema designer

## [0.10.1](https://github.com/alias2k/flusso/compare/flusso-schema-v0.10.0...flusso-schema-v0.10.1) - 2026-06-26

### Other

- lead README with the tagline, move AI disclosure below it
- consistency pass — fix factual errors and terminology drift
- rewrite all docs to the prose style guide
- clarify pre-commit hook formats the whole workspace
- fmt
- add dev workflow tooling
- split into an mdBook manual + per-crate READMEs

## [0.7.0](https://github.com/alias2k/flusso/compare/flusso-schema-v0.6.0...flusso-schema-v0.7.0) - 2026-06-22

### Fixed

- *(schema)* reject non-scalar `default` instead of silently dropping it ([#39](https://github.com/alias2k/flusso/pull/39))
- *(schema)* reject `doc_id` instead of silently ignoring it ([#38](https://github.com/alias2k/flusso/pull/38))
- *(schema)* honor `required` on to-one joins instead of forcing nullable ([#37](https://github.com/alias2k/flusso/pull/37))

### Other

- *(fmt)* formatted tests

## [0.5.0](https://github.com/alias2k/flusso/compare/flusso-schema-v0.4.0...flusso-schema-v0.5.0) - 2026-06-22

### Added

- *(schema)* parse and convert the map: field type

## [0.4.0](https://github.com/alias2k/flusso/compare/flusso-schema-v0.3.2...flusso-schema-v0.4.0) - 2026-06-19

### Added

- runtime index prefix to namespace all indices ([#24](https://github.com/alias2k/flusso/pull/24))

### Other

- *(core)* typed canonical value vocabulary between source and sink

### Added

- Deployment-wide `prefix` on `Config` (the index prefix).

## [0.3.1](https://github.com/alias2k/flusso/compare/flusso-schema-v0.3.0...flusso-schema-v0.3.1) - 2026-06-19

### Added

- improve upon claude skills

## [0.3.0](https://github.com/alias2k/flusso/compare/flusso-schema-v0.2.0...flusso-schema-v0.3.0) - 2026-06-18

### Added

- add the ids field support

## [0.2.0](https://github.com/alias2k/flusso/compare/flusso-schema-v0.1.1...flusso-schema-v0.2.0) - 2026-06-18

### Added

- add the publication management

### Other

- code hygene, removed useless comments

## [0.1.1](https://github.com/alias2k/flusso/compare/flusso-schema-v0.1.0...flusso-schema-v0.1.1) - 2026-06-17

### Added

- change the "run" command to follow the "cargo" pattern of updating the "lock" file

### Other

- Merge pull request #9 from alias2k/feature/run-command-rewamp
