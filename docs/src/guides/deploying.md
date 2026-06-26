# Deploying flusso with Docker

Ship flusso as the smallest possible image — without compiling the binary yourself, and without dragging your whole repo into the build context. For the image's internals (targets, base, non-root user) see the [`Dockerfile`](https://github.com/alias2k/flusso/blob/main/Dockerfile); for Kubernetes see the [Helm chart](https://github.com/alias2k/flusso/blob/main/deploy/helm/flusso/README.md).

## Pick a recipe

The key idea: **you never compile the binary, only a `flusso.lock`** — see [The one idea](#the-one-idea). Then pick by where the lock gets built:

| Situation | Recipe |
| --- | --- |
| Already have a `flusso.lock`, or a flat config | [A — bake your own lock](#recipe-a-bake-your-own-lock-smallest-simplest) (smallest, simplest) |
| Schemas scattered across a monorepo; want a hermetic in-Docker build | [B — build the lock inside Docker](#recipe-b-build-the-lock-inside-docker) |
| Want CI to build the lock and ship one file | [C — build the lock in CI](#recipe-c-build-the-lock-in-ci-ship-one-file) |
| Keep a flusso-only ignore file off everyone else's builds | [Scoping the `.dockerignore`](#scoping-the-dockerignore) |
| Wondering why `COPY *.schema.yml` won't do | [Why `COPY` alone can't do it](#why-copy-alone-cant-do-it) |

## The one idea

Two different "compilations" — conflating them is what makes Docker feel heavy:

1. **The `flusso` binary** — a full Rust build. *Our* job, published once per
   release as a registry image. **You never compile it.** Pull
   `ghcr.io/OWNER/flusso:VERSION` and build *from* it.
2. **A `flusso.lock`** — `flusso build` inlines your `flusso.toml` + every
   referenced `*.schema.yml` into one portable, self-contained file. No DB, no
   toolchain, no secrets baked in.

So however your schemas are laid out — even scattered across a monorepo next to
the services they describe — that layout only has to exist *where you run
`flusso build`*, never inside the image. Get a lock, ship the lock, run the lock.

> ℹ️ **Info** — Schema paths in `flusso.toml` resolve **relative to the config
> file's directory**, with no globbing; each `[[index]]` names its `schema = "…"`
> explicitly. That's the only rule the recipes below respect — when you compile the
> lock, the referenced files must exist at those paths.

## Recipe A: bake your own lock (smallest, simplest)

If you already have a `flusso.lock` (see Recipe C) — or you only have a flat
config — this is the whole thing. Build *from* the published image and copy one
file in:

```dockerfile
# syntax=docker/dockerfile:1
FROM ghcr.io/OWNER/flusso:0.1.0
COPY flusso.lock /app/flusso.lock
# ENTRYPOINT/CMD are inherited: `flusso run --public-address 0.0.0.0:9464`
# loads /app/flusso.lock by default.
```

```sh
docker build -t myorg/search:1.0 .
```

The image is the published base + one file. No Rust, no schema layout, a build
context of a few KB. Secrets (`DATABASE_URL`, `<SINK>_OPENSEARCH_URL`) come from
the environment at run time, so the lock is safe to commit and to bake in.

Don't want to rebuild an image at all? Mount the lock instead:

```sh
docker run --rm -e DATABASE_URL=… -e PRIMARY_OPENSEARCH_URL=… \
  -v "$PWD/flusso.lock:/app/flusso.lock" -p 9464:9464 \
  ghcr.io/OWNER/flusso:0.1.0
```

## Recipe B: build the lock inside Docker

Want the compile to happen *inside* Docker (hermetic, reproducible), and your
schemas are scattered across the repo? Compile the lock in a builder stage, then
copy **only the lock** into the final image. The trick that keeps the build
context tiny without enumerating folders is an allowlist ignore file (see
[Scoping the `.dockerignore`](#scoping-the-dockerignore)):

`flusso.Dockerfile`:

```dockerfile
# syntax=docker/dockerfile:1
FROM ghcr.io/OWNER/flusso:0.1.0 AS lock
WORKDIR /src
COPY . .                                   # context is already pruned to toml + *.schema.yml,
                                           # with their real paths preserved → flusso.toml resolves
RUN flusso build --config flusso.toml --out /app/flusso.lock

FROM ghcr.io/OWNER/flusso:0.1.0
COPY --from=lock /app/flusso.lock /app/flusso.lock
```

`flusso.Dockerfile.dockerignore`:

```gitignore
*
!flusso.toml
!**/*.schema.yml
```

```sh
docker build -f flusso.Dockerfile -t myorg/search:1.0 .
```

`COPY . .` is what preserves the scattered directory structure (so the relative
`schema = "…"` paths resolve); the ignore file is what keeps the multi-gigabyte
monorepo out of the build context. They're complementary — neither does the job
alone. Add a new schema anywhere in the tree and it just works, no Dockerfile
edit.

## Recipe C: build the lock in CI, ship one file

The lowest-friction option for a monorepo: compile the lock on the host or in CI,
where the repo is checked out and every relative path already resolves, then feed
the single artifact to [Recipe A](#recipe-a-bake-your-own-lock-smallest-simplest).

```sh
flusso build --config flusso.toml --out flusso.lock   # inlines all scattered schemas
```

Commit `flusso.lock`, or publish it as a CI artifact, and the image build never
sees a schema file — there's no pattern to match, no context to prune, no tree to
preserve. Just one file. (`flusso build` needs the `flusso` binary; in CI, run it
from the published image: `docker run --rm -v "$PWD:/src" -w /src
ghcr.io/OWNER/flusso:0.1.0 build --config flusso.toml --out flusso.lock`.)

## Scoping the `.dockerignore`

A root `.dockerignore` applies to *every* build in the repo, which you usually
don't want when flusso is one service among many. BuildKit (you're on it — every
recipe here starts with `# syntax=docker/dockerfile:1`) lets you scope an ignore
file to **one Dockerfile**: place `<dockerfile-name>.dockerignore` next to it, and
it takes precedence over the root `.dockerignore` for that build only.

```
flusso.Dockerfile
flusso.Dockerfile.dockerignore     # used only when building flusso.Dockerfile
.dockerignore                      # everyone else's default, untouched
```

So the allowlist in [Recipe B](#recipe-b-build-the-lock-inside-docker) lives in
`flusso.Dockerfile.dockerignore` and affects nothing else in the repo.

> [!NOTE]
> Per-Dockerfile ignore files are a **BuildKit** feature — honored by
> `docker build`, `docker buildx`, and `docker compose`. A legacy
> (non-BuildKit) builder silently falls back to the root `.dockerignore`.

## Why `COPY` alone can't do it

It's tempting to skip the ignore file and just `COPY` the schemas by pattern.
That doesn't work, for two reasons:

- **No recursive glob.** Docker's `COPY` uses `filepath.Match`, where `*` does
  not cross `/`. There's no `**`, so you can't express "every `*.schema.yml` at
  any depth."
- **It flattens.** When a wildcard matches several files they all land directly
  in the destination — the source directory structure is *not* preserved. That
  breaks the relative `schema = "…"` paths immediately.

The filtering has to happen at the **context** layer (`.dockerignore`), not the
`COPY` layer. `COPY . .` then preserves the tree; the scoped ignore file keeps the
context small. Hence Recipe B.
