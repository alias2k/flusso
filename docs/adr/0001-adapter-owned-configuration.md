---
status: accepted
---

# Adapters own their configuration; the kernel carries an opaque options tree

Every port entry in `flusso.toml` (the source, the stream, each sink) is loaded as `{ kind, options }`, where `options` is a small kernel-owned value tree with ordered keys. The adapter for that kind defines the typed config struct and deserializes the tree with unknown fields rejected. The config crate never names an adapter, and the kernel never holds an adapter's settings. Decided in #130 after the third adapter (NATS, #128) would have paid the four-place touch again: a kernel struct, a TOML entity, a conversion arm, and a composition-root match.

## Considered options

- **Kernel-typed configs (status quo).** Fully typed lock, strict at load time, but every adapter's settings live two layers below the adapter and the assembly layer must know every backend.
- **One config crate per adapter between kernel and adapter.** Same ownership fix with a crate per adapter to publish and version.
- **Adapter-owned with a neutral tree (chosen).** Two-place touch per adapter; format-agnostic by construction; strictness moves, it does not disappear.

## Consequences

- `load` no longer means "fully validated". The composition root's `Backends::validate` instantiates every adapter config, without connecting, before any network call or lock write. `flusso build` and `check --offline` still catch a typo in a sink option.
- The lock stays byte-stable: options serialize through ordered maps, and a port entry is written as its `type` plus the options flattened beside it, the same shape the TOML uses. Every previously accepted `flusso.toml` still loads; the lock itself is re-versioned once (ADR 0005).
- The env-override convention `<NAME>_<KIND>_<FIELD>` is a kernel helper every adapter applies to its `Secret` fields, so the user-facing contract does not vary by adapter. The singleton ports use their table name: `SOURCE_POSTGRES_CONNECTION_URL`, `STREAM_CHANNEL_…`. `DATABASE_URL` is removed rather than deprecated: there is no user base to protect yet.
- Knobs that were CLI flags but belong to one adapter (`slot`, `publication`, `manage_publication`, `pretty`, the channel capacity) become options of that adapter; the flags and their `FLUSSO_*` variables stay as overrides applied in the composition root, so the daemon's options carry nothing adapter-specific.
- Adapters describe their options from one declaration, `#[derive(AdapterConfig)]` (kind, port, schema via schemars, an example per field, the override variables). The CLI renders three things from the registered descriptions: the complete editor JSON schema, committed in the config crate and drift-guarded; the option tables the Reference pages include; and the descriptions the designer renders its forms from. The hand-enumerated sink fields and the config half of the drift test go away, and an option's description has exactly one home, its doc comment.
