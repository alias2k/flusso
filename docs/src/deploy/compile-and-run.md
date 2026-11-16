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

5. **Stop.** `SIGTERM`, `SIGINT` or `SIGHUP` drops the pipeline, closes the HTTP surfaces and flushes telemetry, within 5 seconds by default; a second signal exits at once. Changes that weren't confirmed are read again on the next start: that's the at-least-once guarantee doing its job. Signals, deadline and exit codes are in [Stopping](../reference/cli.md#stopping).

## Options and variations

- **Configure from the environment.** Every flag has a `FLUSSO_*` twin: `FLUSSO_CONFIG`, `FLUSSO_PUBLIC_ADDRESS`, `FLUSSO_INDEX_PREFIX`. See [Environment variables](../reference/environment.md#cli-flags).
- **Where it listens.** Both surfaces default to localhost. A container needs the public one on `0.0.0.0`; the private one should stay on localhost. Defaults and ports are in [HTTP endpoints](../reference/http.md).
- **Change the admin password** before exposing anything: `--admin-password` / `FLUSSO_ADMIN_PASSWORD`. flusso warns on every start while it's the default.
- **Logs** go to stderr; `RUST_LOG` filters them; `FLUSSO_LOG_FORMAT=json` structures them.
- **Queue depth.** `[stream] capacity` (or `--queue-capacity`) bounds changes buffered between capture and the sink. Capture blocks when it's full; that's back-pressure, not a fault. See [Stream: channel](../reference/stream-channel.md).

## Run under a task runner or supervisor

One flusso per replication slot: a second one on the same addresses fails at start, and on other addresses waits for the slot. So whatever starts flusso must also stop it.

- **systemd, Docker, Kubernetes** send `SIGTERM` and wait before `SIGKILL`: 90 s, 10 s, 30 s by default (the Helm chart keeps Kubernetes' default). All are longer than the 5 s [stop deadline](../reference/cli.md#stopping), so flusso exits on its own. Shorten a grace period below it only together with `--shutdown-timeout-secs`.
- **A task runner on a dev machine** (moon, `just`, a `Procfile` runner) starts flusso as a child and may die without stopping it: killed, crashed, or taken down with the terminal around a laptop's sleep. Its flusso then keeps running, holding port 9464 and the slot, and the next start reports `another flusso (pid <pid>) is already running there`. Add `--exit-with-parent` to the task's command, or set `FLUSSO_EXIT_WITH_PARENT=true`, and flusso stops when its parent goes:

  ```sh
  flusso run --exit-with-parent
  ```

  It follows its direct parent. Keep `flusso run` the task's only command, so the runner's shell hands over to flusso; in a wrapper script, start it with `exec flusso run …`. A shell that stays around in between survives a runner that is killed, and flusso would follow the shell.
- **A flusso left over** from before: `/status` on its public address names its `pid`; `kill <pid>` stops it, and `kill` again stops it at once.

## Related

- [Ship with Docker](docker.md) and [Deploy with Helm](helm.md) for the two packaging routes.
- [CLI](../reference/cli.md#run) for every `run` flag, and [Stopping](../reference/cli.md#stopping) for what a stop does.
