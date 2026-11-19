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

## Stopping

A pod stop is a graceful drain: flusso lands what is in flight, confirms it to Postgres, then exits, within `run.shutdownTimeoutSecs` (30 s). The pod's `terminationGracePeriodSeconds` is 45 s so Kubernetes doesn't kill the drain; raise both together, the grace period always above the timeout (the chart refuses otherwise). What a stop does is in the manual's [Stopping](https://alias2k.github.io/flusso/reference/cli.html#stopping).

## Alerts

With the Prometheus Operator installed, `prometheusRule.enabled: true` creates a `PrometheusRule` with flusso's alerts, the same rules as the dev stack's `dev/prometheus/rules/flusso.rules.yml`:

- `FlussoLaneStalled`: a lane (one index into one sink) has landed nothing for `laneStallSeconds` (600), or the slot keeps more than `laneWalBehindBytes` (1 GiB) of WAL behind a lane that has landed nothing for `laneWalStallSeconds` (60), for `for` (5m). Runbook: [A lane is stalled](https://alias2k.github.io/flusso/operate/stalled-lane.html).
- `FlussoLaneParked`: flusso stopped writing a lane for `for` (5m): `why` `refused` (a document refused under `on_error = "stop"`), `blocked` (the index refuses writes) or `restaging` (a target staging can't bring back). It also covers a refused backfill page, which never ages a lane into `FlussoLaneStalled`. Runbook: [A lane is stalled](https://alias2k.github.io/flusso/operate/stalled-lane.html).
- `FlussoVisibilityWait`: a commit held invisible for over 30 s (a synchronous standby not answering).

`prometheusRule.labels` adds labels to match your Prometheus's `ruleSelector`. A process that crash-loops never ages a lane into `FlussoLaneStalled` (the age starts at 0 in each process): alert on the pod's restarts in your own rules.

## Values

`values.yaml` documents every key. The manual lists them with defaults in [Helm chart values](https://alias2k.github.io/flusso/reference/helm-values.html); the walkthrough, including how config and secrets flow and how to reach the private surface, is [Deploy with Helm](https://alias2k.github.io/flusso/deploy/helm.html).
