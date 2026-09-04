# The config layer

Configuration loads in two stages, parse then convert; each port table is handed to its adapter for strict validation before anything connects; secrets are deferred to run time; and the file formats are frozen for the major with two tests enforcing it.

## The model

`config::load(path)` is the front door: it reads `flusso.toml`, resolves and parses every referenced `*.schema.yml`, and returns one `Config`. Each file parser is a module of the `config` crate and works in two stages.

1. **Parse.** `serde` deserializes into permissive *entity* types that mirror the file one to one; unknown top-level fields are rejected. The port tables (`[source]`, `[stream]`, `[sinks.<name>]`) parse into the kernel's `PortEntry`: the `type` plus every other key as an uninterpreted `Options` tree. This is all the `toml` and `yaml` modules do.
2. **Convert.** Entities are lifted into the model and the rules the format can't express are applied: identifier validation, join key arity, aggregate operand rules, declared-type placement, filter shapes. For `*.schema.yml` this lives in the `yaml` module. For `flusso.toml` the conversion is a composition step, so it lives next to `Config`; the port entries pass through untouched and an omitted `[stream]` becomes the channel with its defaults.

## Adapters validate their own options

The config crate never learns what an option means. The adapter for each `type` owns a config struct with `#[derive(AdapterConfig)]` (`deny_unknown_fields` is required by the derive), and the CLI's adapter registry deserializes each entry's options into it. `Backends::validate` runs that for every entry right after loading, in `build`, `check` (offline too), `run`, and the designer, so a misspelled sink option is a config error and never a silent no-op. The typed struct is also where the adapter resolves its secrets, with the `<ENTRY>_<TYPE>_<KEY>` override variables.

The editor schema for `flusso.toml` is **generated** from the same declarations: `flusso schema config` derives the base from the config entities and splices in one alternative per registered adapter. The result is committed at `libs/1-config/config.schema.json` (so it ships in the crate and Pages publishes it per release) and a CLI test fails when it drifts; `just schema-gen` refreshes it together with the Reference option tables under `docs/src/reference/generated/`. The `*.schema.yml` schema stays hand-curated in the same crate, with a drift test over its enumerable sets (type keys, siblings, enum tokens).

## Secrets are deferred

A `{ env = "VAR" }` reference becomes a `Secret` and is read in the environment that runs the pipeline. A compiled `flusso.lock` therefore carries no secret it wasn't given literally. Resolution happens in each adapter's config type, called from the CLI's `Backends` implementation, the one place that knows the running environment.

## Type-first fields

Each field is `- <type>: <name>`; the type key's value is the document key and siblings are what that type allows. Parsing is in the `yaml` module's `entities/field.rs`; the core model is `FieldSource` with `Join.kind: JoinKind`, and reverse resolution per kind is in the Postgres source's `document/resolve.rs`. An `enum`'s `variants` land on `Column.enum_order`, not on `FlussoType::Enum`, so `value_type: enum` keeps working and the lock round-trips. A `map`'s value kind rides `Mapping.map_values`, the only thing distinguishing it from a plain `object`.

## The lock

`compile` runs the load pipeline and wraps the result in a `Compiled { format_version, config }` envelope; `write` serializes it as deterministic TOML with a fixed header; `load_compiled` reads it back. Byte-stable: same inputs, identical bytes, nothing derived from the producing binary. Each port entry is written as its `type` plus its options with sorted keys, the shape the TOML uses. Format version 3; version 2 (kernel-typed adapter settings, before 0.16) and version 1 (the pre-freeze MessagePack) are rejected with a regenerate hint, and the version is checked before the body is decoded so the hint is what the user sees.

## The freeze

Any `flusso.toml` or `*.schema.yml` a release in the major accepts must keep loading on every later one, and so must any `flusso.lock` from 0.16 on. Backwards only; `deny_unknown_fields` stays; deprecate, don't remove. Additive lock changes use `#[serde(default)]` and need no version bump; a bump obliges a regenerate step for every user, which is why the lock was re-versioned exactly once (ADR 0005).

Two guards in `libs/1-config/tests/`:

| Test | Guards |
| --- | --- |
| `golden_lock.rs` | byte-pins the serialized shape against a maximal fixture; re-bless with `FLUSSO_BLESS=1` after reviewing the diff |
| `compat.rs` | walks the immutable per-release corpus in `tests/compat/`; never edit a snapshot, fix the change. The `v0.15` snapshot keeps only its user-authored files; the lock guarantee starts with `v0.16` |

## Where this shows up

- [flusso.lock](../reference/lock.md) for the user-facing contract.
- [Identifiers and validation](../reference/identifiers.md) for the rules convert applies.
- [Environment variables](../reference/environment.md#config-values) for the override rule.
