# flusso-schema

Load a `flusso` configuration into a validated model.

This is the front door to the configuration layer. [`load`] takes the path
to a `flusso.toml`, reads the source and sinks from it, resolves and parses
every index schema the file references, and hands back a single [`Config`].

The format-specific crates (`schema-config-toml`, `schema-index-yaml`) and
the core model (`schema-core`) sit underneath. Downstream code depends only
on this crate and reaches the core types through its re-exports.

# Example

```no_run
let config = schema::load("flusso.toml")?;

for (name, index) in &config.indexes {
    println!("{name}: table {} ({} fields)", index.schema.table, index.schema.fields.len());
}
# Ok::<(), schema::LoadError>(())
```

# 2-schema — config & schema loading

This group turns config **files** into the validated `Config`. `schema` (this crate)
is the front door: [`load`] reads a `flusso.toml` plus the `*.schema.yml` files it
references into one validated `Config`. The two nested parser crates each work in two
stages — parse (serde → neutral entities), then convert (entities → model). The
`flusso.toml` → `Config` conversion lives here in the `schema` crate (a composition
step), keeping the toml parser free of `Config`.

- [schema](.) — the front door: `load()` reads a `flusso.toml` + its `*.schema.yml` files into one validated `Config`, and re-exports the `schema-core` vocabulary.
- [schema-config-toml](1-config-toml) — parses `flusso.toml` → entities.
- [schema-index-yaml](1-index-yaml) — parses `*.schema.yml` → core types.

Part of [the flusso library crates](../README.md).
