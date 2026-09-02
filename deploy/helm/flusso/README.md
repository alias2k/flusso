# flusso Helm chart

Deploys [flusso](https://alias2k.github.io/flusso/), OpenSearch kept in sync with Postgres from declarative config, as a single Kubernetes Deployment. Postgres and OpenSearch are external.

> ⚠️ **Warning** — One instance only. flusso consumes a single replication slot, so the chart pins `replicas: 1`, fails if `replicaCount` is raised, and uses the `Recreate` strategy so a new pod never overlaps the old one.

## Install

```sh
helm install flusso ./deploy/helm/flusso \
  --namespace flusso --create-namespace -f my-values.yaml
```

A minimal `my-values.yaml` supplies the config inline and the secrets as env vars:

```yaml
config:
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
secrets:
  create: true
  data:
    DATABASE_URL: postgres://flusso:…@pg:5432/app
    PRIMARY_OPENSEARCH_URL: https://opensearch:9200
    FLUSSO_ADMIN_PASSWORD: change-me
```

## Key values

| Key | Default | Meaning |
| --- | --- | --- |
| `image.repository` / `image.tag` | `alias2k/flusso` / chart `appVersion` | Docker Hub; `ghcr.io/alias2k/flusso` mirrors it |
| `config.create` / `config.flussoToml` / `config.schemas` | `true` / sample / `{}` | render the config into a ConfigMap mounted at `/config` |
| `config.existingConfigMap` | `""` | bring your own |
| `secrets.create` / `secrets.data` / `secrets.existingSecret` | `false` / `{}` / `""` | the env-var Secret, mounted with `envFrom` |
| `run.slot` / `run.publication` / `run.skipBackfill` / `run.queueCapacity` / `run.lagPollSecs` / `run.extraArgs` | `flusso` / `flusso` / `false` / `1024` / `15` / `[]` | `flusso run` flags |
| `http.port` / `http.privatePort` | `9464` / `9465` | public surface, Service-exposed / private surface, localhost only |
| `metrics.serviceMonitor.enabled` | `false` | Prometheus Operator ServiceMonitor |
| `resources`, `nodeSelector`, `tolerations`, `affinity` | `{}` | scheduling |

`values.yaml` documents every key. The full walkthrough, including how config and secrets flow and how to reach the private surface, is the manual's [Deploy with Helm](https://alias2k.github.io/flusso/deploy/helm.html).
