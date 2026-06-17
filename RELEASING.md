# Releasing flusso

flusso ships three ways from one release flow:

- **crates.io** — all 16 publishable crates (`cargo install flusso-cli`, or depend on `flusso-query`)
- **prebuilt binaries** — GitHub Release assets + installers (via `dist`)
- **Docker image** — `ghcr.io/alias2k/flusso` (via the `docker` workflow)

## How it fits together

```
push to main ──▶ release-plz opens a "release" PR (version bump + CHANGELOG)
                        │
                merge the release PR
                        │
   release-plz ─────────┼──────────────▶ publishes all crates to crates.io
                        │                 (dependency order, automatic)
                        └──▶ pushes per-crate tags, incl. `flusso-cli-v<version>`
                                          │
                        ┌─────────────────┴─────────────────┐
                docker workflow                      dist workflow
            (build & push ghcr image)        (build binaries + installers,
                                              attach to the GitHub Release)
```

Every crate shares one version (`Cargo.toml` `[workspace.package]`), so a release bumps them together.

## One-time setup

### crates.io
1. Create a [crates.io](https://crates.io) account, verify your email.
2. Generate an API token (Account Settings → API Tokens) scoped to publish-new + publish-update.
3. Add it as a secret named **`CARGO_REGISTRY_TOKEN`** in the **`release`** GitHub
   environment (Settings → Environments → release → Environment secrets), **not** as a
   plain repository secret. The `release` job declares `environment: release`, so it reads
   the token from there — and any environment protection rules (e.g. required reviewers) gate
   the publish. The ungated `release-pr` job needs no token.

### Environment gating
Both publish jobs run in the `release` environment: `release-plz`'s `release` job (crates.io)
and the `docker` job (GHCR image). Add **required reviewers** to the environment if you want a
human to approve each publish. The `release-pr` job and the CI workflow stay ungated.

### GHCR (Docker)
No secret needed — the `docker` workflow authenticates with the built-in `GITHUB_TOKEN`. After the
first push, set the `ghcr.io/alias2k/flusso` package to public (Package settings) if you want
anonymous `docker pull`.

### dist (prebuilt binaries) — generates its own workflow
`dist`'s release workflow is **machine-generated**; don't hand-edit it.

```sh
cargo install cargo-dist            # provides the `dist` binary
dist init                           # interactive; writes [workspace.metadata.dist] + .github/workflows/release.yml
dist generate                       # (re)generate the workflow after config changes
```

Recommended answers at `dist init`:
- **CI:** GitHub
- **Installers:** shell, powershell, homebrew (add `msi` if you want a Windows installer)
- **Targets:** `x86_64-unknown-linux-gnu`, `aarch64-unknown-linux-gnu`, `x86_64-apple-darwin`, `aarch64-apple-darwin`, `x86_64-pc-windows-msvc`

Then:
- **Ship only the CLI binary.** The workspace also has the `flusso-dev-search-api` example binary — exclude it by adding to `dev/search-api/Cargo.toml`:
  ```toml
  [package.metadata.dist]
  dist = false
  ```
- `dist` understands release-plz's `flusso-cli-v<version>` tag format, so it triggers on the same tag the `docker` workflow does. Commit the generated `release.yml` (CI checks it's not stale via `dist generate --check`).

## Cutting a release

1. Merge feature work to `main` using conventional commits (`feat:`, `fix:`, `feat!:`/`BREAKING CHANGE:` for bumps) — these drive the version + changelog.
2. release-plz opens/updates the **release PR**. Review the version bumps and `CHANGELOG.md`.
3. Merge the release PR. release-plz publishes to crates.io and pushes the tags; the `docker` and `dist` workflows fire off `flusso-cli-v<version>`.
4. Verify: crates on crates.io, the GitHub Release has binaries + installers, and `docker pull ghcr.io/alias2k/flusso:<version>` works.

### Pushing to main without publishing
A push to main only publishes when a version bump has landed (release-plz publishes just the crates
whose version isn't already on crates.io), so ordinary feature pushes publish nothing. To be explicit
— or to suppress a publish even when a bump is present — include **`[skip release]`** in the head
commit message; the `release` job is skipped entirely (and with it the downstream `docker`/`dist`
tag builds, which only fire off tags the `release` job pushes).

## First release — read once

- **Versions are permanent.** Once `0.1.0` is published it can be *yanked* but never reused. The first publish also **claims all 16 `flusso-*` names** for your account.
- **Publish order is handled** by release-plz (bottom-up: `flusso-schema-core` → parsers → `flusso-schema` → engine/sinks/sources/queue → `flusso-daemon` → `flusso-query-derive` → `flusso-query` → `flusso-cli`).
- **Prefer to do the very first publish by hand?** `cargo login`, then `cargo publish` each crate in the order above (`--dry-run` fully verifies only the leaf `flusso-schema-core`; each later crate can't dry-run until its deps are live). After that, let release-plz drive subsequent releases.
- **CI on the release PR:** the PR is opened with `GITHUB_TOKEN`, so CI won't run on it by default. Use a fine-grained PAT or the release-plz GitHub App if you want it to. See <https://release-plz.dev/docs/github>.
