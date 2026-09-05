# Releasing

The workspace publishes to crates.io as three independent release trains driven by release-plz, with binaries and images cut from the CLI train's tag.

## The trains

| Train | Crates | Version |
| --- | --- | --- |
| libs | every `libs/*` crate | one shared version; it's one engine |
| cli | `flusso-cli`, `flusso-design` | one version; the SPA ships inside the binary |
| query | `flusso-query`, `flusso-query-derive` | one version; the derive is exact-pinned `=x.y.z`, serde-style |

Each crate owns its `version`; there is no shared `[workspace.package] version`. The `version_group`s in `release-plz.toml` keep a train in lockstep while letting trains diverge.

## Why the trains stay decoupled

Internal requirements in `[workspace.dependencies]` are **minor-precision** (`"0.15"`, never `"0.15.1"`). release-plz rewrites a dependent's requirement only when a new version stops matching, so a libs patch releases nothing but libs, while a libs 0.x minor (breaking) legitimately cascades a bump into the trains that depend on it. Never tighten those requirements to patch precision.

## Getting a libs fix into the binaries

The docker image and dist installers build from the `flusso-cli-v*` tag, in-tree, so they always carry main-tip libs. A libs release alone ships nothing. Land a `fix(cli): adopt <crate> <version> (<reason>)` commit appending a line to `apps/cli/ADOPTIONS.md`; release-plz counts it as a CLI change, bumps that train, and the tag drives the artifacts. The file is both the trigger and the audit trail.

## When publishing happens

Only when the release PR is merged, never on an ordinary push to main. release-plz opens the PR with version bumps and per-crate `CHANGELOG.md` entries from conventional commits; merging it publishes, tags, and creates the GitHub Release. `semver_check` surfaces accidental breakage as a note in the PR.

## Publish order

Bottom-up within a train, a dependency before its dependents: `flusso-kernel`, `flusso-kernel-derive`, the ports (`flusso-source`, `flusso-stream`, `flusso-sink`), `flusso-config`, the adapters, then `flusso-engine`, then `flusso-daemon`; apps on top (`flusso-design` before `flusso-cli`; `flusso-query-derive` before `flusso-query`). The pre-0.16 names (`flusso-schema-core` and the rest) stay on crates.io at their last release; `scripts/tombstone-crates.sh` publishes a final pointer README under each. `dev/search-api`, `dev/query-e2e`, and `dev/bench` are `publish = false`.

## Commit conventions

Conventional Commits with the crate as scope: `fix(engine): …`, `feat(query): …`, `docs(book): …`. The type and scope decide which train's changelog a commit lands in.

## Where this shows up

- The versioned editor schemas at `alias2k.github.io/flusso/schemas/` are assembled from the two parser crates' release tags, which move together because both sit in the libs train.
