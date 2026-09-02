# Compile and run

Turn `flusso.toml` and its schemas into a running pipeline, either straight from the source files or through a committed `flusso.lock`.

## When to use this

You have a validated config and want to run it, locally or in a deployment. The lock is optional for a dev machine and the recommended unit to ship; see [flusso.lock](../reference/lock.md).

## Steps

1. **Run from source.** With `flusso.toml` in the working directory (or given with `--config`), `run` recompiles it, rewrites `flusso.lock` if the bytes changed, and runs.

   ```sh
   flusso run --config flusso.toml --public-address 0.0.0.0:9464
   ```

   flusso checks the slot, ensures every index exists with its mapping, backfills unseeded indexes, then follows live changes. `/readyz` returns `200` once backfill begins.

2. **Commit the lock.** The rewritten `flusso.lock` is deterministic TOML. Commit it; a config change shows as a readable diff.

3. **Ship the lock, run it locked.** In a deployment, copy or mount only the lock and pin it:

   ```sh
   flusso run --locked --lock /app/flusso.lock
   ```

   Nothing is recompiled or rewritten. Secrets resolve from the environment here. Or build the lock explicitly in CI:

   ```sh
   flusso build --config flusso.toml --out flusso.lock
   ```

4. **Restart without a backfill.** Once seeded, a restart skips the backfill on its own because the sink remembers. `--skip-backfill` forces that even when the sink disagrees; it exists for recovery, not routine use.

5. **Stop.** `SIGINT` or `SIGTERM` drains in-flight batches and flushes telemetry. Unconfirmed changes are redelivered on the next start; that's the at-least-once guarantee doing its job.

## Options and variations

- **Configure from the environment.** Every flag has a `FLUSSO_*` twin: `FLUSSO_CONFIG`, `FLUSSO_PUBLIC_ADDRESS`, `FLUSSO_INDEX_PREFIX`. See [Environment variables](../reference/environment.md#cli-flags).
- **Where it listens.** Both surfaces default to localhost. A container needs the public one on `0.0.0.0`; the private one should stay on localhost. Defaults and ports are in [HTTP endpoints](../reference/http.md).
- **Change the admin password** before exposing anything: `--admin-password` / `FLUSSO_ADMIN_PASSWORD`. flusso warns on every start while it's the default.
- **Logs** go to stderr; `RUST_LOG` filters them; `FLUSSO_LOG_FORMAT=json` structures them.
- **Queue depth.** `--queue-capacity` bounds changes buffered between capture and the sink. Capture blocks when it's full; that's back-pressure, not a fault.

## Related

- [Ship with Docker](docker.md) and [Deploy with Helm](helm.md) for the two packaging routes.
- [CLI](../reference/cli.md#run) for every `run` flag.
