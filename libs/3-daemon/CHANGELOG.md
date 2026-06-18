# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.2.0](https://github.com/alias2k/flusso/compare/flusso-daemon-v0.1.1...flusso-daemon-v0.2.0) - 2026-06-18

### Added

- add the publication management

### Other

- code hygene, removed useless comments

## [0.1.1](https://github.com/alias2k/flusso/compare/flusso-daemon-v0.1.0...flusso-daemon-v0.1.1) - 2026-06-17

### Added

- change the "run" command to follow the "cargo" pattern of updating the "lock" file

### Other

- Merge pull request #9 from alias2k/feature/run-command-rewamp
- release v0.1.0

## [0.1.0](https://github.com/alias2k/flusso/releases/tag/flusso-daemon-v0.1.0) - 2026-06-17

### Added

- license, security, coc, contributing and github templates
- on-demand reindex — alias-over-generations sink + restart trigger
- split the operational HTTP surface into public + private with Basic auth
- rename flusso-search to flusso-query
- move files to correctly reflect dependencies
- more checks in CI
- add on failure behavior
- tests
- abstract backends
- create alias pointing to the latest index
- add belong_to
- move otel from daemon to cli
- add deamon crate, add metrics export, add prometheus and grafana, add http health endpoint
- start client, add geo
- rename config.toml to flusso.toml and flusso.bin to flusso.lock
- add compile functionality
- better opensearch defaults
- improve env vars handling and readme about sinks and sources
- backfill and renaming
- rebranding to storno
- update deps
- documentation

### Fixed

- daemon compilation

### Other

- preparation for going public
- documentation
- add requirements section
- add SCHEMA.md
- readme
- readme and cleanup
- README
