# flusso-schema-config-toml

Parses `flusso.toml` into neutral [`ConfigToml`] entities — the **parse** stage only, no assembly, no secret resolution.

## At a glance

| Concern | This crate | Elsewhere |
| --- | --- | --- |
| Deserialize the file 1:1, reject unknown fields | ✅ [`ConfigToml`] | |
| Lift entities → assembled `Config` | | `schema` crate (`From<ConfigToml>`) |
| Resolve `{ env = "VAR" }` secrets | | the environment that runs the pipeline |

## What it parses

A `flusso.toml` declares the Postgres source, the sinks documents are written
to, and the indexes to build. [`ConfigToml`] deserializes the file verbatim,
rejecting unknown fields, into entity types that mirror the file 1:1 and
reference only the `schema-core` vocabulary.

Lifting these entities into the assembled `Config` is a composition step that
lives in the `schema` crate (`From<ConfigToml>`), so this parser sits at the
bottom layer and never depends on `Config`.

## Secrets stay deferred

Secrets are **not** resolved here. Any string value may be given literally or
as `{ env = "VAR" }`; the entities carry that choice through unchanged so the
value can be read in the environment that runs the pipeline.

> 💡 **Did you know** — because secrets defer, a compiled `flusso.lock` carries
> no secret it wasn't handed literally — only the `{ env = "VAR" }` reference.
