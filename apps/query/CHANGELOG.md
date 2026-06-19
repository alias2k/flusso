# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Complete the typed query surface (issue #19): leaf operators now return
  per-query builders carrying that query's options plus the universal `boost` /
  `name`, composing via `AsQuery`. Adds `case_insensitive` / `fuzziness` /
  `slop` / `operator` / … on the existing query types; `minimum_should_match`
  and the compound/scoring queries `constant_score` / `dis_max` /
  `function_score` / `boosting`; the standalone queries `ids` / `query_string` /
  `simple_query_string` / `combined_fields` / `match_bool_prefix` / `script` /
  `script_score` / `distance_feature` / `rank_feature` / `more_like_this`; sort
  features (`Sort::score()` / `missing` / `mode` / nested / script); and the
  search-level controls `min_score` / `track_total_hits` / `track_scores` /
  `search_after` / `collapse` / `post_filter` / `highlight`.
- `uuid` cargo feature: `uuid::Uuid` is a valid `keyword` value, so id /
  foreign-key fields need no `#[flusso(skip)]`.
- Typed subfield accessors on string handles (`Text::keyword()` /
  `keyword_lowercase()`, `Keyword::text()` / `keyword_lowercase()`), so exact /
  wildcard / case-insensitive-sort / full-text targeting needs no string path.

## [0.3.1](https://github.com/alias2k/flusso/compare/flusso-query-v0.3.0...flusso-query-v0.3.1) - 2026-06-19

### Added

- improve upon claude skills

## [0.2.0](https://github.com/alias2k/flusso/compare/flusso-query-v0.1.1...flusso-query-v0.2.0) - 2026-06-18

### Added

- add the publication management

### Other

- code hygene, removed useless comments

## [0.1.1](https://github.com/alias2k/flusso/compare/flusso-query-v0.1.0...flusso-query-v0.1.1) - 2026-06-17

### Added

- change the "run" command to follow the "cargo" pattern of updating the "lock" file

### Other

- Merge pull request #9 from alias2k/feature/run-command-rewamp
- release v0.1.0

## [0.1.0](https://github.com/alias2k/flusso/releases/tag/flusso-query-v0.1.0) - 2026-06-17

### Added

- correct workflows for deployment, code quality corrections
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
