# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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
