# flusso Helm chart

Deploys [flusso](../../../README.md) — which keeps OpenSearch in sync with
Postgres from declarative config — as a single Kubernetes Deployment.

> **One instance only.** flusso consumes a single Postgres logical replication
> slot, so the chart pins `replicas: 1` (and **fails** if you set `replicaCount`
> higher) and uses the `Recreate` rollout strategy so a new pod never overlaps
> the old one on the slot. Postgres and OpenSearch are **not** deployed by this
> chart — point flusso at your existing clusters.

## Install

```sh
helm install flusso ./deploy/helm/flusso \
  --namespace flusso --create-namespace \
  --set image.repository=ghcr.io/OWNER/flusso \
  --set secrets.create=true \
  --set-string secrets.data.DATABASE_URL='postgres://user:pass@pg:5432/app' \
  --set-string secrets.data.PRIMARY_OPENSEARCH_URL='https://opensearch:9200' \
  -f my-values.yaml
```

A typical `my-values.yaml` supplies the config and schemas:

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
  schemas:
    users.schema.yml: |
      root: public.users
      fields:
        - keyword: email
```

## How config and secrets flow

- **Config** (`config.*`): with `config.create=true` the chart renders
  `flussoToml` and every entry under `schemas` into a ConfigMap, mounts it at
  `config.mountPath` (default `/config`), and runs `flusso run --config
  /config/flusso.toml`. Schema `schema =` paths resolve relative to that file —
  i.e. against the `schemas` keys. Use `config.existingConfigMap` to bring your
  own, or set `config.create=false` with no ConfigMap when the image already
  carries a baked `/app/flusso.lock`. In the `--config` modes `run` recompiles
  the lock from the config on each start; the chart points `--lock` at a writable
  `lock-state` emptyDir (the root filesystem is read-only), so the ConfigMap
  remains the source of truth and the recompiled lock is ephemeral.
- **Secrets** (`secrets.*`): connection/sink URLs in the config should be
  `{ env = "VAR" }` references. The matching env vars come from a Secret mounted
  via `envFrom` — either managed here (`secrets.create=true` + `secrets.data`)
  or your own (`secrets.existingSecret`). So no secret ever lands in the
  ConfigMap.

## Configuration is also available via env vars

Every `flusso` CLI flag also reads a `FLUSSO_*` environment variable (the flag
wins when both are set). The chart sets flags explicitly under `run.*`, but you
can equally drive them through `env`/`secrets` — e.g. `FLUSSO_SLOT`,
`FLUSSO_PUBLICATION`, `FLUSSO_HTTP_ADDR`, `FLUSSO_SKIP_BACKFILL`.

## Metrics

The pod serves Prometheus metrics at `:{{ http.port }}/metrics`. With the
Prometheus Operator installed, set `metrics.serviceMonitor.enabled=true` to
scrape it. For plain Prometheus, scrape the Service directly.

## Key values

| Key | Default | Description |
| --- | --- | --- |
| `image.repository` / `image.tag` | `ghcr.io/OWNER/flusso` / chart appVersion | Image to run. |
| `config.create` / `config.flussoToml` / `config.schemas` | `true` / sample / `{}` | Render config into a ConfigMap. |
| `config.existingConfigMap` | `""` | Use an existing config ConfigMap instead. |
| `secrets.create` / `secrets.data` | `false` / `{}` | Manage a Secret of env vars. |
| `secrets.existingSecret` | `""` | Use an existing Secret instead. |
| `run.slot` / `run.publication` | `flusso` / `flusso` | Replication slot / publication. |
| `run.skipBackfill` | `false` | Resume live capture only. |
| `run.queueCapacity` / `run.lagPollSecs` | `1024` / `15` | Queue size / lag poll interval. |
| `http.port` | `9464` | Operational HTTP surface port. |
| `metrics.serviceMonitor.enabled` | `false` | Create a Prometheus Operator ServiceMonitor. |
| `resources` / `nodeSelector` / `tolerations` / `affinity` | `{}` / … | Standard scheduling knobs. |

See [`values.yaml`](values.yaml) for the full list.
