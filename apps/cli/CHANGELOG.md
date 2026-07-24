# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.12.1](https://github.com/alias2k/flusso/compare/flusso-cli-v0.12.0...flusso-cli-v0.12.1) - 2026-07-24

### Other

- update Cargo.lock dependencies

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
