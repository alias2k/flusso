# flusso-schema-config-toml

Parse `flusso.toml` into neutral [`ConfigToml`] entities.

A config file declares the Postgres source, the sinks documents are written
to, and the indexes to build. This crate handles only the **parse** stage:
[`ConfigToml`] deserializes the file verbatim, rejecting unknown fields, into
entity types that mirror the file 1:1 and reference only the `schema-core`
vocabulary. Lifting these entities into the assembled `Config` is a
composition step that lives in the `schema` crate (`From<ConfigToml>`), so
this parser sits at the bottom layer and never depends on `Config`.

Secrets are **not** resolved here. Any string value may be given literally or
as `{ env = "VAR" }`; the entities carry that choice through unchanged so the
value can be read in the environment that runs the pipeline.
