# Dependency updates

Dependabot proposes updates once a week in a handful of grouped PRs, and a RustSec advisory check runs weekly and on every change to the manifests or the lock.

## The model

Two pieces, both under `.github/`:

- **`dependabot.yml`** opens the update PRs. It reads the file from `main` only, so a change to it takes effect after merge.
- **`workflows/audit.yml`** runs `cargo deny check advisories --all-features` over the graph flusso actually builds. It isn't a required check: a new advisory never blocks an unrelated merge.

## What the bot opens

Every Monday, at most five open PRs per ecosystem, each titled `chore(deps): …`:

| Ecosystem | Grouped PR | On its own |
| --- | --- | --- |
| cargo (root workspace) | `cargo-compatible`: every semver-compatible update, touching only `Cargo.lock` | each breaking update, rewriting its requirement in `Cargo.toml` (`"0.12.*"` becomes `"0.13.*"`) |
| cargo | `opentelemetry`: the `opentelemetry*` crates and `tracing-opentelemetry`, any update type, so they move in lockstep | — |
| npm (`apps/design/frontend`) | `npm-compatible`: minor and patch | each major |
| GitHub Actions | `github-actions`: everything | — |

Cargo's own rules decide "compatible": a 0.x minor (`0.8` to `0.9`) is breaking and gets its own PR, like a major.

Left out on purpose:

- **flusso's own crates** — they're path dependencies, and their minor-precision requirements keep the release trains decoupled (see [Releasing](releasing.md#why-the-trains-stay-decoupled)). Dependabot skips path dependencies; an `ignore` on `flusso-*` keeps it that way.
- **`rust-toolchain.toml`** — pinned because the trybuild UI tests compare exact compiler diagnostics. Bump it by hand and re-bless.
- **ESLint 10** (`eslint`, `@eslint/js`) — `eslint-plugin-react` and `eslint-plugin-jsx-a11y` don't support it yet. Drop the `ignore` once they do.
- **The fuzz crate** — its own workspace with no committed lock, so every fuzz run already resolves the newest compatible versions.

## Before merging a bump

- **cargo** — CI is the gate. A breaking update usually needs code changes pushed onto the bot's branch.
- **npm** — the `designer SPA` job fails until the committed `apps/design/dist/` matches a fresh build. Check out the bot's branch, rebuild, and commit the output:

  ```sh
  cd apps/design/frontend && npm ci && npm run build
  git add ../dist && git commit -m "chore(design): rebuild the SPA"
  ```

  Once someone else pushes to its branch, Dependabot stops rebasing that PR; rebase it by hand if `main` moves.

- **GitHub Actions** — read the release notes when an action jumps a major version.

> ℹ️ **Info** — a merged bump doesn't publish anything. release-plz releases only when its release PR is merged. It does notice the change, though: a breaking update changes the dependency requirements of every crate that uses it, and a lock-only update changes what `flusso-cli` ships. Either way the affected trains show up in the next release PR.

## When the advisory check goes red

The scheduled run failing is the signal; nothing opens an issue for it. Read the job log, then for each advisory:

1. **A fixed version exists** — run `cargo update -p <crate>` (or bump the requirement for a breaking fix) in a `fix(deps): …` PR. That PR re-runs the check.
2. **No fix yet, and it affects flusso** — open an issue and track the upstream.
3. **It doesn't apply** (the vulnerable code path is never reached) — add an `ignore` for its ID under `[advisories]` in a `deny.toml` at the repo root, with a comment saying why. The comment is the audit trail.

cargo-deny resolves features, so a crate that sits in `Cargo.lock` only through an optional dependency nothing enables isn't reported at all.

## Where this shows up

- [Releasing](releasing.md) — the trains and why internal requirements stay minor-precision.
- [Testing](testing.md#ci-order) — the rest of CI.
