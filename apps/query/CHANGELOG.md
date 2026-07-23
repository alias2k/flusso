# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.11.0](https://github.com/alias2k/flusso/compare/flusso-query-v0.10.1...flusso-query-v0.11.0) - 2026-07-23

### Other

- document the visual schema designer

## [0.10.1](https://github.com/alias2k/flusso/compare/flusso-query-v0.10.0...flusso-query-v0.10.1) - 2026-06-26

### Fixed

- *(query)* dispatch combined-search hits by generation-suffixed _index

### Other

- *(query)* allow unused dev-deps in the lib test target
- *(query)* correct combined-search dispatch — hits carry the generation index
- *(query)* add live combined-search e2e against a real OpenSearch sink
- *(query)* cover generation-suffixed _index dispatch in combined search
- lead README with the tagline, move AI disclosure below it
- readability pass — front-load lookups, de-densify
- consistency pass — fix factual errors and terminology drift
- rewrite all docs to the prose style guide
- clarify pre-commit hook formats the whole workspace
- add dev workflow tooling
- split into an mdBook manual + per-crate READMEs

## [0.10.0](https://github.com/alias2k/flusso/compare/flusso-query-v0.9.2...flusso-query-v0.10.0) - 2026-06-25

### Added

- *(query)* log get_one found/not-found outcome
- *(query)* log per-slot msearch outcomes and failures
- *(query)* surface shard failures and timeouts as partial results
- *(query)* log the OpenSearch query body in request traces

### Fixed

- *(query)* export ShardStats from the crate root

### Other

- chore(fmt)

## [0.9.2](https://github.com/alias2k/flusso/compare/flusso-query-v0.9.1...flusso-query-v0.9.2) - 2026-06-24

### Added

- *(query)* typed map-key sort with language fallback

### Fixed

- *(query)* drop field-only sort modifiers on a map-key script sort

### Other

- *(query)* chainable map-key sort through the normal .by()
- record map-key fallback sort (issue #58)

## [0.9.0](https://github.com/alias2k/flusso/compare/flusso-query-v0.8.0...flusso-query-v0.9.0) - 2026-06-23

### Added

- *(query)* SortBuilder + nesting-aware Sortable sort surface
- *(query)* path metadata + FlussoDocument/FlussoIndex trait split

### Other

- *(query)* rustfmt + fix intra-doc links after trait split
- *(query)* cover nesting-aware sort + SortBuilder (refs #49)

## [0.8.0](https://github.com/alias2k/flusso/compare/flusso-query-v0.7.0...flusso-query-v0.8.0) - 2026-06-23

### Fixed

- *(query)* per-type numeric kinds + kind-based values for every kind ([#45](https://github.com/alias2k/flusso/pull/45))

## [0.6.0](https://github.com/alias2k/flusso/compare/flusso-query-v0.5.0...flusso-query-v0.6.0) - 2026-06-22

### Added

- *(query)* [**breaking**] compile-time gating of subfield accessors
- *(query)* add a prelude module
- *(query)* [**breaking**] typed MinimumShouldMatch over stringly-typed option
- *(query)* [**breaking**] typed geo distance (Distance + DistanceUnit)
- *(query)* [**breaking**] enum params for geo + sort options
- *(query)* sort sugar for Text, Bool, and geo distance
- *(query)* [**breaking**] unify geo predicates under the within family
- *(query)* [**breaking**] closed enum params over stringly-typed options
- *(query)* [**breaking**] typed Date inputs via FlussoValue<kind::Date>
- *(query)* [**breaking**] rename in_ to any_of and extend to Date/Text

### Other

- *(query)* drop the prelude module
- *(query)* rustfmt the enum-param edits

## [0.5.0](https://github.com/alias2k/flusso/compare/flusso-query-v0.4.0...flusso-query-v0.5.0) - 2026-06-22

### Added

- *(query)* complete the map matrix with NumberMap and DateMap
- *(query)* typed map handles and the FlussoMap marker trait

## [0.4.0](https://github.com/alias2k/flusso/compare/flusso-query-v0.3.2...flusso-query-v0.4.0) - 2026-06-19

### Added

- runtime index prefix to namespace all indices ([#24](https://github.com/alias2k/flusso/pull/24))

### Added

- `Client::index_prefix`: apply a runtime index prefix to every request, so one
  consumer binary can read any prefixed deployment.

## [0.3.2](https://github.com/alias2k/flusso/compare/flusso-query-v0.3.1...flusso-query-v0.3.2) - 2026-06-19

### Added

- *(query)* typed subfield accessors on string handles (§1c)
- *(query)* uuid cargo feature — Uuid is a keyword value (§1a)
- *(query)* search-level controls + highlight builder (§6)
- *(query)* standalone query types (§4)
- *(query)* bool minimum_should_match + compound queries (§2)
- *(query)* builder-returning leaf queries with options (§0/§3/§5)

### Other

- document the completed query surface (§1b) + CHANGELOGs

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
