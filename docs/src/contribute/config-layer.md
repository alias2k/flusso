# The config layer

Configuration loads in two stages, parse then convert, secrets are deferred to run time, and the file formats are frozen for the major with two tests enforcing it.

## The model

`schema::load(path)` is the front door: it reads `flusso.toml`, resolves and parses every referenced `*.schema.yml`, and returns one validated `Config`. Each file parser works in two stages.

1. **Parse.** `serde` deserializes into permissive *entity* types that mirror the file one to one; unknown fields are rejected. This is all the parser crates (`schema-config-toml`, `schema-index-yaml`) do.
2. **Convert.** Entities are lifted into the model and the rules the format can't express are applied: identifier validation, join key arity, aggregate operand rules, declared-type placement, filter shapes. For `*.schema.yml` this lives in `schema-index-yaml`. For `flusso.toml` the conversion is a composition step, so it lives in the `schema` crate next to `Config`, keeping the TOML parser free of `Config`.

The two hand-curated JSON Schemas for editor completion live inside the parser crates that own them (`config.schema.json`, `index.schema.yml`) so they ship in the published crate, are re-exported from `schema`, and are emitted by `flusso schema`. A drift test compares their enumerable sets (type keys, siblings, enum tokens, sink fields) against the parsers.

## Secrets are deferred

A `{ env = "VAR" }` reference becomes a `Secret` and is read in the environment that runs the pipeline. A compiled `flusso.lock` therefore carries no secret it wasn't given literally. Resolution happens in the CLI's `Backends` implementation, the one place that knows the running environment.

## Type-first fields

Each field is `- <type>: <name>`; the type key's value is the document key and siblings are what that type allows. Parsing is in `schema-index-yaml`'s `entities/field.rs`; the core model is `FieldSource` with `Join.kind: JoinKind`, and reverse resolution per kind is in the Postgres source's `document/resolve.rs`. An `enum`'s `variants` land on `Column.enum_order`, not on `FlussoType::Enum`, so `value_type: enum` keeps working and the lock round-trips. A `map`'s value kind rides `Mapping.map_values`, the only thing distinguishing it from a plain `object`.

## The lock

`compile` runs the load pipeline and wraps the result in a `Compiled { format_version, config }` envelope; `write` serializes it as deterministic TOML with a fixed header; `load_compiled` reads it back. Byte-stable: same inputs, identical bytes, nothing derived from the producing binary. Format version 2; version 1 was the pre-freeze MessagePack and is rejected with a regenerate hint.

## The freeze

Any `flusso.toml`, `*.schema.yml`, or `flusso.lock` a release in the major accepts must keep loading on every later one. Backwards only; `deny_unknown_fields` stays; deprecate, don't remove. Additive lock changes use `#[serde(default)]` and need no version bump; a bump obliges a reader for every prior version.

Two guards in `libs/2-schema/tests/`:

| Test | Guards |
| --- | --- |
| `golden_lock.rs` | byte-pins the serialized shape against a maximal fixture; re-bless with `FLUSSO_BLESS=1` after reviewing the diff |
| `compat.rs` | walks the immutable per-release corpus in `tests/compat/`; never edit a snapshot, fix the change |

## Where this shows up

- [flusso.lock](../reference/lock.md) for the user-facing contract.
- [Identifiers and validation](../reference/identifiers.md) for the rules convert applies.
