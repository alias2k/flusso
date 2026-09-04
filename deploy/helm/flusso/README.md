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
    connection_url = { env = "SOURCE_POSTGRES_CONNECTION_URL" }

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
    SOURCE_POSTGRES_CONNECTION_URL: postgres://flusso:…@pg:5432/app
    PRIMARY_OPENSEARCH_URL: https://opensearch:9200
    FLUSSO_ADMIN_PASSWORD: change-me
```

## Values

`values.yaml` documents every key. The manual lists them with defaults in [Helm chart values](https://alias2k.github.io/flusso/reference/helm-values.html); the walkthrough, including how config and secrets flow and how to reach the private surface, is [Deploy with Helm](https://alias2k.github.io/flusso/deploy/helm.html).
