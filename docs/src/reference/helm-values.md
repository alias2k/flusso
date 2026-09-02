# Helm chart values

Every value the chart at `deploy/helm/flusso/` accepts, with its default. `values.yaml` in the chart carries the same list with comments.

| Value | Default | Meaning |
| --- | --- | --- |
| `image.repository` | `alias2k/flusso` | Docker Hub; `ghcr.io/alias2k/flusso` mirrors it. |
| `image.tag` | chart `appVersion` | |
| `image.pullPolicy` | `IfNotPresent` | |
| `imagePullSecrets` | `[]` | |
| `nameOverride`, `fullnameOverride` | `""` | |
| `replicaCount` | `1` | Fixed. flusso consumes one replication slot; the chart fails if raised. The Deployment uses the `Recreate` strategy. |
| `config.create` | `true` | Render `flussoToml` and `schemas` into a ConfigMap and run `--config <mountPath>/flusso.toml`. |
| `config.existingConfigMap` | `""` | Mount your own ConfigMap instead. It must hold `flusso.toml` and the schemas. |
| `config.mountPath` | `/config` | |
| `config.flussoToml` | a two-line sample | The config. Keep URLs as `{ env = "VAR" }` references. |
| `config.schemas` | `{}` | Filename to contents, mounted beside `flusso.toml`. |
| `config.create: false` with no ConfigMap | | Run nothing extra; the image must carry `/app/flusso.lock`. |
| `run.slot` | `flusso` | Passed as `--slot`. |
| `run.publication` | `flusso` | Passed as `--publication`. |
| `run.skipBackfill` | `false` | Passed as `--skip-backfill`. |
| `run.queueCapacity` | `1024` | Passed as `--queue-capacity`. |
| `run.lagPollSecs` | `15` | Passed as `--lag-poll-secs`. |
| `run.extraArgs` | `[]` | Raw args appended verbatim. |
| `http.port` | `9464` | Public surface, bound to `0.0.0.0` and exposed by the Service. |
| `http.privatePort` | `9465` | Private surface, bound to localhost only. |
| `secrets.create` | `false` | Manage a Secret from `secrets.data`. |
| `secrets.existingSecret` | `""` | Use a Secret you manage. |
| `secrets.data` | `{}` | Key to value; each becomes an env var through `envFrom`. |
| `env` | `{RUST_LOG: info}` | Plain environment variables. |
| `envFrom` | `[]` | Extra ConfigMaps or Secrets. |
| `service.type` | `ClusterIP` | |
| `service.port` | `9464` | |
| `service.annotations` | `{}` | |
| `metrics.serviceMonitor.enabled` | `false` | Create a Prometheus Operator `ServiceMonitor`. |
| `metrics.serviceMonitor.namespace` | release namespace | |
| `metrics.serviceMonitor.interval` | `30s` | |
| `metrics.serviceMonitor.scrapeTimeout` | `""` | |
| `metrics.serviceMonitor.labels`, `relabelings`, `metricRelabelings` | `{}`, `[]`, `[]` | |
| `serviceAccount.create`, `name`, `annotations` | `true`, `""`, `{}` | |
| `livenessProbe` | `GET /healthz` on `http`, 5 s delay, 10 s period | |
| `readinessProbe` | `GET /readyz` on `http`, 5 s delay, 10 s period, `failureThreshold: 30` | The threshold rides out a long backfill. |
| `resources` | `{}` | |
| `podAnnotations`, `podLabels` | `{}` | |
| `podSecurityContext` | non-root, uid/gid 65532 | |
| `securityContext` | no privilege escalation, read-only root filesystem, all capabilities dropped | |
| `nodeSelector`, `tolerations`, `affinity` | `{}`, `[]`, `{}` | |

The how-to is [Deploy with Helm](../deploy/helm.md).
