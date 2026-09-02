# Ship with Docker

Build the smallest possible flusso image from the published base and a `flusso.lock`, without compiling Rust or copying a schema tree into the build context.

## When to use this

You're deploying flusso as a container. Two things get "compiled" and conflating them is what makes Docker feel heavy: the **binary**, a full Rust build published once per release as `alias2k/flusso`, and the **lock**, which `flusso build` produces from your config in a second. You never build the binary. Get a lock, ship the lock, run the lock.

## Steps

1. **Pick an image tag.** `alias2k/flusso:X.Y.Z` (exact, immutable, for production), `X.Y` (follows patches), `latest`, or `sha-<short>` (per commit). `ghcr.io/alias2k/flusso` is an identical mirror.

2. **Build the lock where the paths resolve.** Schema paths in `flusso.toml` are relative to that file, with no globbing, so the lock must be compiled where the referenced files exist. Locally, in CI, or from the image itself:

   ```sh
   docker run --rm -v "$PWD:/src" -w /src alias2k/flusso:X.Y.Z \
     build --config flusso.toml --out flusso.lock
   ```

   Commit it or publish it as a CI artifact. It carries no secrets.

3. **Bake it into a child image.**

   ```dockerfile
   # syntax=docker/dockerfile:1
   FROM alias2k/flusso:X.Y.Z
   COPY flusso.lock /app/flusso.lock
   ```

   The inherited entrypoint is `flusso run --public-address 0.0.0.0:9464`, which loads `/app/flusso.lock` when no `flusso.toml` is present. The build context is one file.

4. **Run it with secrets from the environment.**

   ```sh
   docker run --rm -p 9464:9464 \
     -e DATABASE_URL=… -e PRIMARY_OPENSEARCH_URL=… -e OS_PASSWORD=… \
     myorg/search:1.0
   ```

5. **Verify.** `curl localhost:9464/readyz` returns `200` once backfill starts; `/status` shows the phase and per-index state.

## Options and variations

- **Mount instead of bake.** `-v "$PWD/flusso.lock:/app/flusso.lock"` on the base image, no child image at all.
- **Mount the config instead.** `-v "$PWD:/config" alias2k/flusso:X.Y.Z run --config /config/flusso.toml` recompiles on start, like `cargo run`. The lock is then written next to the config, so the mount must be writable.
- **Compile the lock inside Docker** when schemas are scattered across a monorepo and you want a hermetic build. A builder stage runs `flusso build` over a context pruned to just the config files by a per-Dockerfile ignore file:

  `flusso.Dockerfile`:

  ```dockerfile
  # syntax=docker/dockerfile:1
  FROM alias2k/flusso:X.Y.Z AS lock
  WORKDIR /src
  COPY . .
  RUN flusso build --config flusso.toml --out /app/flusso.lock

  FROM alias2k/flusso:X.Y.Z
  COPY --from=lock /app/flusso.lock /app/flusso.lock
  ```

  `flusso.Dockerfile.dockerignore`:

  ```gitignore
  *
  !flusso.toml
  !**/*.schema.yml
  ```

  `COPY . .` preserves the directory structure so relative paths resolve; the ignore file keeps the rest of the repo out of the context. BuildKit honors `<name>.dockerignore` beside its Dockerfile, so the allowlist affects nothing else. A `COPY *.schema.yml` can't do this: Docker's `COPY` has no `**` and flattens matches into one directory.

- **The image itself** is `debian:bookworm-slim` plus `ca-certificates`, runs as uid 65532, exposes 9464, and is built with `--no-default-features`, so it has no `flusso design`.

## Related

- [flusso.lock](../reference/lock.md) for what the lock is and how `run` treats it.
- [Deploy with Helm](helm.md) when the target is Kubernetes.
