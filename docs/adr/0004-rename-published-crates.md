---
status: proposed
---

# Rename the published crates to the kernel / ports / adapters / engine / daemon vocabulary

The library crates are renamed on crates.io to match the seams they implement (`flusso-kernel`, `flusso-source`, `flusso-stream`, `flusso-sink`, `flusso-config`, `flusso-engine`, `flusso-daemon`, and `flusso-<port>-<technology>` for adapters), and the three config crates collapse into one. The old names (`flusso-schema-core` and the rest) stay on crates.io at their last 0.15 release and are never published again. Decided in #130: the workspace is 0.x with no external consumers, and keeping the old package names would leave a permanent gap between what the docs say and what crates.io shows.

## Considered options

- **Move directories and extern names only.** Free, but crates.io keeps names the documentation no longer uses, forever.
- **Rename now (chosen).** One-time cost while nobody external depends on the crates.

## Consequences

- The layered directory layout becomes `libs/0-kernel`, `libs/1-ports/{source,stream,sink}`, `libs/1-config`, `libs/2-adapters/*`, `libs/2-engine`, `libs/3-daemon`. Consumer-facing libraries (the query crates) move to `sdk/`; the designer stays in `apps/`.
- Release trains are unchanged in number: libs, cli, query. The version groups in the release configuration and the CI path filters are updated for the new paths.
- Old crate names get a final README pointing at their successors; nothing else is published under them.
