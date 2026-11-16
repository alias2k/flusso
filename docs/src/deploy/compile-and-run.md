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

5. **Stop.** `SIGTERM`, `SIGINT` or `SIGHUP` drains: nothing new is read, what is in flight lands on every sink and is confirmed to Postgres, then flusso exits `0`, within 30 seconds by default. A second signal exits at once; whatever wasn't confirmed is read again on the next start. The sequence, deadline and exit codes are in [Stopping](../reference/cli.md#stopping).

## Options and variations

- **Configure from the environment.** Every flag has a `FLUSSO_*` twin: `FLUSSO_CONFIG`, `FLUSSO_PUBLIC_ADDRESS`, `FLUSSO_INDEX_PREFIX`. See [Environment variables](../reference/environment.md#cli-flags).
- **Where it listens.** Both surfaces default to localhost. A container needs the public one on `0.0.0.0`; the private one should stay on localhost. Defaults and ports are in [HTTP endpoints](../reference/http.md).
- **Change the admin password** before exposing anything: `--admin-password` / `FLUSSO_ADMIN_PASSWORD`. flusso warns on every start while it's the default.
- **Logs** go to stderr; `RUST_LOG` filters them; `FLUSSO_LOG_FORMAT=json` structures them.
- **Queue depth.** `[stream] capacity` (or `--queue-capacity`) bounds changes buffered between capture and the sink. Capture blocks when it's full; that's back-pressure, not a fault. See [Stream: channel](../reference/stream-channel.md).

## Run under a task runner or supervisor

One flusso per replication slot: a second one on the same addresses fails at start, and on other addresses waits for the slot. So whatever starts flusso must also stop it, and give the [drain](../reference/cli.md#stopping) its 30 seconds before killing it.

| Supervisor | Waits after `SIGTERM` by default | Set |
| --- | --- | --- |
| Kubernetes | 30 s | `terminationGracePeriodSeconds` above the deadline. The Helm chart sets 45 (`terminationGracePeriodSeconds`) and refuses a value at or below `run.shutdownTimeoutSecs`. |
| Docker / compose | 10 s | `docker stop -t 35`, or `stop_grace_period: 35s` on the service. |
| systemd | 90 s | Nothing; `TimeoutStopSec` already exceeds it. |
| moon | 2 s | `killProcessThreshold`, see below. |

### Under moon or another task runner

A task runner on a dev machine starts flusso as its child and can die without stopping it: killed, crashed, or taken down with its terminal around a laptop's sleep. flusso then keeps running, holding port 9464 and the slot, and the next start reports `another flusso (pid <pid>) is already running there`. Two settings fix that:

- **`--exit-with-parent`** (or `FLUSSO_EXIT_WITH_PARENT=true`; `1` is refused): flusso stops, draining as usual, when the process that started it is gone.
- **The runner's kill delay above the deadline**, so a stop the runner asks for isn't cut short.

For moon, in the project's `moon.yml`:

```yaml
tasks:
  flusso:
    command: 'flusso run --exit-with-parent'
    options:
      persistent: true
```

and in `.moon/workspace.yml` (or `MOON_PIPELINE_KILL_PROCESS_THRESHOLD=35000` in the environment):

```yaml
pipeline:
  killProcessThreshold: 35000
```

moon runs the command as its own child, so flusso follows moon itself. In general flusso follows its **direct** parent: keep `flusso run` the task's only command, and in a wrapper script start it with `exec flusso run …`. A shell that stays in between survives a runner that is killed, and flusso would follow the shell. One Ctrl-C reaches flusso twice, from the terminal and forwarded by the runner: that is one stop, and it drains. Press it twice quickly to exit at once.

### A flusso left over

`/status` on its public address names the `pid` it reports (inside a container, its own namespace's). `kill <pid>` stops it gracefully; two `kill`s from the same shell within half a second stop it at once.

## Related

- [Ship with Docker](docker.md) and [Deploy with Helm](helm.md) for the two packaging routes.
- [CLI](../reference/cli.md#run) for every `run` flag, and [Stopping](../reference/cli.md#stopping) for what a stop does.
