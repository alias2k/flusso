# Deploy with Helm

Install flusso on Kubernetes as a single-replica Deployment with the config in a ConfigMap, secrets from a Secret, a Service on the public surface, and optional Prometheus Operator scraping.

## When to use this

You run Kubernetes and Postgres and OpenSearch already exist; the chart deploys neither. It lives in the repository at `deploy/helm/flusso/`.

## Steps

1. **Write a values file** with the config and schemas inline. Keep connection and sink URLs as `{ env = "VAR" }` references so no secret lands in the ConfigMap. Schema paths resolve against the keys under `schemas`.

   ```yaml
   config:
     create: true
     flussoToml: |
       [source]
       type = "postgres"
       connection_url = { env = "DATABASE_URL" }

       [sinks.primary]
       type = "opensearch"
       url = { env = "PRIMARY_OPENSEARCH_URL" }

       [[index]]
       name = "users"
       schema = "users.schema.yml"
       enabled = true
     schemas:
       users.schema.yml: |
         version: 1
         table: users
         primary_key: id
         fields:
           - keyword: email
             required: true
   ```

2. **Provide the secrets.** Either let the chart manage a Secret, or point at yours. Both are mounted with `envFrom`, so each key becomes an environment variable.

   ```yaml
   secrets:
     create: true
     data:
       DATABASE_URL: postgres://flusso:…@pg:5432/app
       PRIMARY_OPENSEARCH_URL: https://opensearch:9200
       FLUSSO_ADMIN_PASSWORD: change-me
   ```

   or `secrets.existingSecret: my-flusso-secret`.

3. **Install.**

   ```sh
   helm install flusso ./deploy/helm/flusso \
     --namespace flusso --create-namespace -f my-values.yaml
   ```

   The chart renders a Deployment with `replicas: 1`, the `Recreate` strategy, a read-only root filesystem, and a non-root user.

4. **Verify.** The notes print the in-cluster URL. Port-forward and read the status:

   ```sh
   kubectl -n flusso port-forward svc/flusso 9464:9464
   curl localhost:9464/status
   ```

## Options and variations

| Value | Default | Meaning |
| --- | --- | --- |
| `image.repository` / `image.tag` | `alias2k/flusso` / chart `appVersion` | Docker Hub; `ghcr.io/alias2k/flusso` mirrors it. |
| `replicaCount` | `1` | Fixed. flusso consumes one replication slot; the chart fails if raised. |
| `config.create` / `config.flussoToml` / `config.schemas` | `true` / sample / `{}` | Render the config into a ConfigMap mounted at `config.mountPath` (`/config`). |
| `config.existingConfigMap` | `""` | Mount your own ConfigMap holding `flusso.toml` and the schemas. |
| `config.create: false` with no ConfigMap | | Run nothing extra; the image must carry `/app/flusso.lock`. |
| `secrets.create` / `secrets.data` / `secrets.existingSecret` | `false` / `{}` / `""` | The env-var Secret. |
| `env` / `envFrom` | `{RUST_LOG: info}` / `[]` | Plain variables (`RUST_LOG`, `OTEL_*`) and extra sources. |
| `run.slot` / `run.publication` | `flusso` / `flusso` | Passed as flags. |
| `run.skipBackfill` / `run.queueCapacity` / `run.lagPollSecs` | `false` / `1024` / `15` | Passed as flags. |
| `run.extraArgs` | `[]` | Raw args appended verbatim. |
| `http.port` / `http.privatePort` | `9464` / `9465` | Public surface, bound to `0.0.0.0` and exposed by the Service; private surface, bound to localhost only. |
| `service.type` / `service.port` | `ClusterIP` / `9464` | |
| `metrics.serviceMonitor.enabled` | `false` | Create a Prometheus Operator `ServiceMonitor` (plus `interval`, `labels`, relabelings). |
| `livenessProbe` / `readinessProbe` | `/healthz` / `/readyz` | On the public port. Readiness has `failureThreshold: 30` to ride out a long backfill. |
| `resources`, `nodeSelector`, `tolerations`, `affinity`, `podAnnotations` | `{}` | Standard knobs. |

- **The lock is ephemeral in `--config` mode.** `run` recompiles on every start and writes the lock to a writable `lock-state` emptyDir, so the ConfigMap stays the source of truth.
- **Reach the private surface** with `kubectl port-forward` on `http.privatePort`; it is never cluster-exposed. Set `FLUSSO_ADMIN_PASSWORD` in the Secret. See [Secure the private surface](../operate/private-surface.md).
- **Flags via env.** Anything under `run.*` can equally be set through `env` as `FLUSSO_*`.

## Related

- [Ship with Docker](docker.md) for the image the chart runs.
- [Watch it run](../operate/watch-it-run.md) for scraping the metrics the Service exposes.
