# flusso-config

The configuration layer in one crate: parse `flusso.toml` and `*.schema.yml`, assemble the validated `Config`, compile the deterministic `flusso.lock`.

[`load`] takes the path to a `flusso.toml`, reads the source and sinks from it, resolves and
parses every `*.schema.yml` the file references, and hands back a single [`Config`].

The two file parsers are modules of this crate ([`toml`] and [`yaml`]); the kernel vocabulary
sits underneath. Downstream code depends only on this crate and reaches the kernel types
through its re-exports.

The crate also owns the compiled artifact: [`compile`] wraps a loaded [`Config`] in a
versioned envelope and [`write`](fn@write)/[`load_compiled`] serialize it as `flusso.lock` —
deterministic, generated-only TOML (same inputs, identical bytes), so a committed lock is
reviewable in a diff. The file formats are frozen for the major: a `flusso.toml`,
`*.schema.yml`, or `flusso.lock` valid on an earlier release keeps loading on every later
one, enforced by the golden-lock and compat-corpus tests in `tests/`.

# Example

```no_run
let config = config::load("flusso.toml")?;

for (name, index) in &config.indexes {
    println!("{name}: table {} ({} fields)", index.schema.table, index.schema.fields.len());
}
# Ok::<(), config::LoadError>(())
```
