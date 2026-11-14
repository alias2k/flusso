# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.18.0](https://github.com/alias2k/flusso/compare/flusso-query-derive-v0.17.0...flusso-query-derive-v0.18.0) - 2026-11-14

### Added

- *(query-derive)* read an array column as a Vec or an array newtype ([#276](https://github.com/alias2k/flusso/pull/276))

### Fixed

- *(query-derive)* an Arc<str>/Rc<str> field without `rc` names the feature ([#306](https://github.com/alias2k/flusso/pull/306))
- *(query-derive)* string alternatives, Decimal by its kinds, a true message ([#306](https://github.com/alias2k/flusso/pull/306))
- *(query-derive)* a fragment accepts what a root accepts ([#306](https://github.com/alias2k/flusso/pull/306))
- *(source)* a camelCase format name reads the ISO form; the date table follows the T form ([#304](https://github.com/alias2k/flusso/pull/304))
- *(query-derive)* refuse by name only what can't be a string type of your own
- *(query-derive)* say only what the derive promises; make the boundary test cheap
- *(query-derive)* read null map values, refuse exhaustive enums without variants, name the tagged-newtype refusals
- *(query-derive)* close the typed-map holes the review found; pin root/fragment agreement
- *(query-derive)* check typed maps against what they read; pin the boundary
- *(kernel)* store a constant list mixing kinds unindexed, on a new index
- *(query-derive)* two refusals suggest a fix that works
- *(query-derive)* [**breaking**] refuse a flattened non-object and a map type that reads no object
- *(query-derive)* [**breaking**] a newtype reads an array only as one of the two array kinds
- *(query-derive)* a fragment takes what a root takes at a json array
- *(query-derive)* a newtype over an array newtype reads the array too
- *(query-derive)* say why an array newtype does not fit a single value
- *(query-derive)* an array newtype of values holds leaf values only
- review findings on array columns ([#276](https://github.com/alias2k/flusso/pull/276))
- *(query-derive)* a newtype over a primitive is a value; opaque in a fragment keeps the kind check at a level
- *(query-derive)* a value type is no sub-document at a nested or object field
- *(query-derive)* refuse an array of arrays at any depth of newtypes
- *(query-derive)* an array newtype needs a sub-document element
- *(query-derive)* [**breaking**] arrays of maps, newtypes over a Vec, hand-written values in fragments
- *(query-derive)* a field serde skips may still name its level with flusso(rename)
- *(query-derive)* [**breaking**] root and fragment agree on maps; reject a rename serde does not see
- *(query-derive)* a transparent named-field struct is a value newtype too
- *(query-derive)* match a fragment's opaque to a root's, and close the review gaps
- *(query-derive)* [**breaking**] reject the remaining unread attributes
- *(query-derive)* leave a flattened catch-all unchecked in a fragment; reject a rename serde contradicts
- *(query-derive)* [**breaking**] reject opaque, rename and scope_mod where they do nothing
- *(query-derive)* reject a second scope for one level; the flatten error names the way out
- *(query-derive)* [**breaking**] a scope attribute that renames nothing is a compile error

### Other

- *(query-derive)* no assert on a field type named Arc<str> or Rc<str> ([#306](https://github.com/alias2k/flusso/pull/306))
- *(query)* a decimal past Decimal's range is written as a string (#306, #327)
- *(query-derive)* pin string alternatives and foreign types at custom ([#306](https://github.com/alias2k/flusso/pull/306))
- *(query-derive)* review round of #306
- *(query-derive)* state and pin every trusted rule of the derive ([#306](https://github.com/alias2k/flusso/pull/306))
- *(query-derive)* a fragment accepts what a root accepts ([#306](https://github.com/alias2k/flusso/pull/306))
- *(query-derive)* string-reading map keys and tagged newtypes the derive refuses by name ([#294](https://github.com/alias2k/flusso/pull/294))
- *(query-derive)* null map values, date and constant kinds, named non-string types ([#294](https://github.com/alias2k/flusso/pull/294))
- *(query-derive)* typed-map shapes the review found still compile and fail ([#294](https://github.com/alias2k/flusso/pull/294))
- *(query-derive)* typed maps the derive does not check yet ([#294](https://github.com/alias2k/flusso/pull/294))
- *(query-derive)* drop the Box case, whose rustc notes differ between hosts

## [0.17.0](https://github.com/alias2k/flusso/compare/flusso-query-derive-v0.16.0...flusso-query-derive-v0.17.0) - 2026-11-12

### Added

- *(query)* the exact family on a keyword map with subfields, over each key's .keyword
- *(sink)* every key of a keyword map with subfields gets an exact .keyword subfield
- *(query)* [**breaking**] a map handle mirrors its values kind across all keys
- *(query-derive)* no handles for an unindexed field, NoSubfields for subfields: false
- *(query-derive)* stamp the subfield type-state on string map handles
- *(query)* [**breaking**] map keys have the plain-field surface; contains and a configurable cross-key search

### Fixed

- *(query-derive)* [**breaking**] reject scope on an indexed: false field
- *(query-derive)* a decimal map's keys take the decimal kind, like a plain decimal field
- *(kernel)* [**breaking**] a map carries its keys' plain-field mapping and a new content hash

### Other

- *(query-derive)* a subfields: false field has no subfield views
- say the root manual is the newest release and /main/ tracks main
- *(query-derive)* script fields get their declared handles; an input-only field is not in the index
- *(query)* an exists aggregate gets the Bool handle
- *(query-derive)* an unindexed field's stored shape is still checked
- keep content without indexing it (subfields: false, indexed: false)

## [0.16.0](https://github.com/alias2k/flusso/compare/flusso-query-derive-v0.15.1...flusso-query-derive-v0.16.0) - 2026-10-09

### Added

- *(config)* [**breaking**] adapters own their configuration; port entries are type + options

### Other

- *(arch)* [**breaking**] lay out kernel / ports / adapters / engine / daemon and sdk/
- root README as a pitch, flusso-query README as a landing; track the Start here pages
- the lock is deterministic TOML; formats frozen for the major
- source TLS — configuration guide section, README, crate README, plugin skills

## [0.15.1](https://github.com/alias2k/flusso/compare/flusso-query-derive-v0.15.0...flusso-query-derive-v0.15.1) - 2026-08-17

### Fixed

- *(query-derive)* finish the map-in-fragment story the derive fix started

### Other

- Merge branch 'main' into fix/query-derive-map-in-fragments

## [0.15.0](https://github.com/alias2k/flusso/compare/flusso-query-derive-v0.14.0...flusso-query-derive-v0.15.0) - 2026-08-17

### Fixed

- *(query-derive)* FlussoMap accepts any struct, and carries its value kind
- *(query-derive)* [**breaking**] generated scopes live in a module, not the caller's namespace

### Other

- *(query-derive)* [**breaking**] name the generated module flusso_<root>_query
- *(query-derive)* pin where an Option goes on a nested clause
- *(query-derive)* pin the namespace collision, and cover the dev example

## [0.14.0](https://github.com/alias2k/flusso/compare/flusso-query-derive-v0.13.0...flusso-query-derive-v0.14.0) - 2026-08-14

### Added

- *(query-derive)* [**breaking**] remove the old form outright — no deprecation tail
- *(query-derive)* [**breaking**] deprecate path= instead of deleting it
- *(query-derive)* name the Rust type in fragment messages; rename a namespace
- *(query-derive)* [**breaking**] rename the derive to FlussoRoot; accept transparent newtypes
- *(query-derive)* [**breaking**] the root generates the whole handle tree; path= is gone
- *(query-derive)* the root bakes the mapping and drives fragment checks
- *(query-derive)* FlussoFragment — a location-free, reusable document shape
- *(query-derive)* FlussoValue carries its kinds and variants as constants

### Other

- *(query)* the root/fragment model is the documented way
- *(query-derive)* cover the reuse case the feature exists for
- *(query-derive)* cover flattened field groups
- *(query)* [**breaking**] rename FlussoIndex to FlussoRoot, FlussoDocument trait to FlussoScope

## [0.13.0](https://github.com/alias2k/flusso/compare/flusso-query-derive-v0.12.0...flusso-query-derive-v0.13.0) - 2026-07-24

### Added

- *(query-derive)* emit the Enum handle for declared-order enums

## [0.11.0](https://github.com/alias2k/flusso/compare/flusso-query-derive-v0.10.1...flusso-query-derive-v0.11.0) - 2026-07-23

### Other

- document the visual schema designer

## [0.10.1](https://github.com/alias2k/flusso/compare/flusso-query-derive-v0.10.0...flusso-query-derive-v0.10.1) - 2026-06-26

### Other

- lead README with the tagline, move AI disclosure below it
- consistency pass — fix factual errors and terminology drift
- rewrite all docs to the prose style guide
- clarify pre-commit hook formats the whole workspace
- add dev workflow tooling
- split into an mdBook manual + per-crate READMEs

## [0.9.2](https://github.com/alias2k/flusso/compare/flusso-query-derive-v0.9.1...flusso-query-derive-v0.9.2) - 2026-06-24

### Other

- *(query)* chainable map-key sort through the normal .by()
- *(query)* cover map-key sort + reroute (issue #58)

## [0.9.0](https://github.com/alias2k/flusso/compare/flusso-query-derive-v0.8.0...flusso-query-derive-v0.9.0) - 2026-06-23

### Added

- *(query)* SortBuilder + nesting-aware Sortable sort surface
- *(query)* path metadata + FlussoDocument/FlussoIndex trait split

### Other

- *(query)* rustfmt + fix intra-doc links after trait split
- *(query)* cover nesting-aware sort + SortBuilder (refs #49)

## [0.8.0](https://github.com/alias2k/flusso/compare/flusso-query-derive-v0.7.0...flusso-query-derive-v0.8.0) - 2026-06-23

### Fixed

- *(query)* per-type numeric kinds + kind-based values for every kind ([#45](https://github.com/alias2k/flusso/pull/45))

## [0.6.0](https://github.com/alias2k/flusso/compare/flusso-query-derive-v0.5.0...flusso-query-derive-v0.6.0) - 2026-06-22

### Added

- *(query)* [**breaking**] compile-time gating of subfield accessors
- *(query)* [**breaking**] typed geo distance (Distance + DistanceUnit)
- *(query)* [**breaking**] closed enum params over stringly-typed options

### Other

- *(query)* rustfmt the enum-param edits

## [0.5.0](https://github.com/alias2k/flusso/compare/flusso-query-derive-v0.4.0...flusso-query-derive-v0.5.0) - 2026-06-22

### Added

- *(query)* complete the map matrix with NumberMap and DateMap
- *(query-derive)* generate map handles and validate map doc types

## [0.3.2](https://github.com/alias2k/flusso/compare/flusso-query-derive-v0.3.1...flusso-query-derive-v0.3.2) - 2026-06-19

### Other

- document the completed query surface (§1b) + CHANGELOGs
- *(query-derive)* issue #19 acceptance test (§1b fixture)

### Added

- Acceptance test for the completed typed query surface (issue #19): a realistic
  projection with a `Uuid` keyword field (no `#[flusso(skip)]`), an enum keyword
  field, subfield targeting, `minimum_should_match`, and null-aware sort builds
  with zero `Search::raw` / `Json::raw`. The derive needs no change — it already
  defers a `FlussoValue<Keyword>` bound for `Uuid`, satisfied by `flusso-query`'s
  `uuid` feature.

## [0.3.1](https://github.com/alias2k/flusso/compare/flusso-query-derive-v0.3.0...flusso-query-derive-v0.3.1) - 2026-06-19

### Added

- improve upon claude skills

## [0.3.0](https://github.com/alias2k/flusso/compare/flusso-query-derive-v0.2.0...flusso-query-derive-v0.3.0) - 2026-06-18

### Added

- add the ids field support

## [0.2.0](https://github.com/alias2k/flusso/compare/flusso-query-derive-v0.1.1...flusso-query-derive-v0.2.0) - 2026-06-18

### Added

- add the publication management

### Other

- code hygene, removed useless comments

## [0.1.1](https://github.com/alias2k/flusso/compare/flusso-query-derive-v0.1.0...flusso-query-derive-v0.1.1) - 2026-06-17

### Added

- change the "run" command to follow the "cargo" pattern of updating the "lock" file

### Other

- Merge pull request #9 from alias2k/feature/run-command-rewamp
- release v0.1.0

## [0.1.0](https://github.com/alias2k/flusso/releases/tag/flusso-query-derive-v0.1.0) - 2026-06-17

### Added

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
