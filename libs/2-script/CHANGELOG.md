# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.18.0](https://github.com/alias2k/flusso/compare/flusso-script-v0.17.0...flusso-script-v0.18.0) - 2026-11-19

### Added

- *(script)* [**breaking**] a real date type in script fields ([#364](https://github.com/alias2k/flusso/pull/364))

### Fixed

- *(script,source-postgres)* a declared float in --input is a float; jsonb zero padding is not digits
- *(source-postgres)* a float reads as Postgres casts to float4; exact reads pinned in flusso-query
- *(source-postgres)* an untyped number is a decimal to a script at every magnitude
- *(source-postgres)* read and write every number exactly
- *(script)* a thrown date is named as scripts name it
- *(script)* named zones reach every year chrono does; sort keeps Rhai's one-element rule ([#364](https://github.com/alias2k/flusso/pull/364))
- *(script)* [**breaking**] no silent date comparisons; months as Postgres age(); zones from jiff ([#364](https://github.com/alias2k/flusso/pull/364))
- *(script)* the nearest decimal past 28 digits; a zero decimal result has no sign
- *(script)* a declared decimal reaches a script as main read it, unless held exactly
- *(script)* a decimal script result is written with every digit
- *(source-postgres)* only a declared decimal changes what the sink receives
- *(source-postgres)* [**breaking**] write every numeric digit as a JSON number

### Other

- the source marks a walk's point and resumes after it in its own key order (walk_mark, resume_after, compare_keys)
- a walk ends at the greatest key it found, read through the key index's own operators
- a backfill pages each index's root keys on the builder pool
- *(script)* calendar edges in named zones, sort parity with Rhai, the upgrade page read as written ([#364](https://github.com/alias2k/flusso/pull/364))
- *(script)* refuse cross-kind date comparisons through Dynamic fallbacks ([#364](https://github.com/alias2k/flusso/pull/364))
- *(script)* pin the review's silent answers, months as age(), and the upgrade replacements ([#364](https://github.com/alias2k/flusso/pull/364))
- *(script)* point the schema skill at dates in a script; fix a private doc link ([#364](https://github.com/alias2k/flusso/pull/364))
- *(script)* typed dates in scripts, failing first ([#364](https://github.com/alias2k/flusso/pull/364))
- a script's decimal holds about 28 significant digits, not 28 fractional ones
- *(script)* a non-decimal value pays nothing for the decimal path
- *(script)* no precision cliff past 28 digits; a zero decimal result has no sign
- *(script)* a declared decimal reaches a script as main gave it
- *(script)* a decimal script result keeps its digits; a tiny decimal is a decimal
- only a declared decimal may change what the sink receives

## [0.17.0](https://github.com/alias2k/flusso/compare/flusso-script-v0.16.0...flusso-script-v0.17.0) - 2026-11-12

### Added

- *(script)* the flusso-script crate: compile, run and strip, behind a DocumentBuilder decorator
- *(query-derive)* [**breaking**] deprecate path= instead of deleting it
- improve upon claude skills
- add the publication management
- change the "run" command to follow the "cargo" pattern of updating the "lock" file
- DEPLOY.md for docker recipie
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

### Fixed

- review — report script failures under both policies, ids in print context, debug log of the failing document, shared build-by-id, field docs

### Other

- say the root manual is the newest release and /main/ tracks main
- an exhaustive script-fields guide — the Rhai you need, tested recipes, debugging, production
- script fields and input-only fields — reference, how-to, CLI, metrics, /status, glossary, CLAUDE.md
- root README as a pitch, flusso-query README as a landing; track the Start here pages
- the lock is deterministic TOML; formats frozen for the major
- source TLS — configuration guide section, README, crate README, plugin skills
- document the visual schema designer
- lead README with the tagline, move AI disclosure below it
- consistency pass — fix factual errors and terminology drift
- rewrite all docs to the prose style guide
- clarify pre-commit hook formats the whole workspace
- add dev workflow tooling
- split into an mdBook manual + per-crate READMEs
- Merge pull request #9 from alias2k/feature/run-command-rewamp
- preparation for going public
- documentation
- add requirements section
- add SCHEMA.md
- readme
- readme and cleanup
- README
