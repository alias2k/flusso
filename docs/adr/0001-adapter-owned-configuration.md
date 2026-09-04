---
status: proposed
---

# Adapters own their configuration; the kernel carries an opaque options tree

Every port entry in `flusso.toml` (the source, the stream, each sink) is loaded as `{ kind, options }`, where `options` is a small kernel-owned value tree with ordered keys. The adapter for that kind defines the typed config struct and deserializes the tree with unknown fields rejected. The config crate never names an adapter, and the kernel never holds an adapter's settings. Decided in #130 after the third adapter (NATS, #128) would have paid the four-place touch again: a kernel struct, a TOML entity, a conversion arm, and a composition-root match.

## Considered options

- **Kernel-typed configs (status quo).** Fully typed lock, strict at load time, but every adapter's settings live two layers below the adapter and the assembly layer must know every backend.
- **One config crate per adapter between kernel and adapter.** Same ownership fix with a crate per adapter to publish and version.
- **Adapter-owned with a neutral tree (chosen).** Two-place touch per adapter; format-agnostic by construction; strictness moves, it does not disappear.

## Consequences

- `load` no longer means "fully validated". The composition root's `Backends::validate` instantiates every adapter config, without connecting, before any network call or lock write. `flusso build` and `check --offline` still catch a typo in a sink option.
- The lock stays byte-stable: options serialize through ordered maps. The format freeze (#109) holds because every previously accepted file still loads.
- The env-override convention `<NAME>_<KIND>_<FIELD>` is a kernel helper every adapter applies, so the user-facing contract does not vary by adapter. `DATABASE_URL` is removed rather than deprecated: there is no user base to protect yet.
- Adapters describe their options with a schema derivation. The CLI assembles the editor JSON schema from registered adapters and the designer builds its forms from the same description, so the hand-enumerated sink fields and the sink half of the drift test go away.
