---
status: accepted
---

# The lock is re-versioned to format 3 and the freeze restarts at 0.16

`flusso.lock` moves to `format_version = 3`, in which every port entry is stored as its `type` plus its options flattened beside it. A format 2 lock is rejected with a hint to regenerate it from the `flusso.toml`; nothing translates it. The `flusso.toml` and `*.schema.yml` formats are unchanged and keep loading. Decided in #130 together with ADR 0001: the format 2 lock stored each adapter's settings as kernel-typed structs (`source_type`, `connection.parts`, `tls`, externally tagged sinks), which is exactly the ownership the reshuffle removes.

## Considered options

- **A frozen legacy decoder in the config crate.** Keeps every format 2 lock loading, but the config crate would name postgres, opensearch, and stdout in a shim that can never be extended, and the translation into each adapter's TOML shape is adapter knowledge.
- **Adapters accept both shapes through serde aliases.** Spreads the old lock's shape into every adapter's config type for the life of the major.
- **Re-version and reject (chosen).** One regenerate step for a lock that is always derivable from the config beside it, and no adapter name leaks into the config crate.

## Consequences

- The #109 freeze holds for the two user-authored formats and restarts for the lock at the release that ships format 3. The compat corpus keeps the v0.15 `flusso.toml` and schemas, drops the v0.15 lock, and gains a v0.16 snapshot; its README states where the lock freeze starts.
- A deployment that runs `--locked` off a committed lock must rebuild that lock once when it upgrades. `flusso run` without `--locked` does it automatically.
