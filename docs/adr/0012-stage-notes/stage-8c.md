# Stage 8c: stall age, WAL behind, and the alert

ADR 0012 §5: "a stalled lane pins the slot at its oldest outstanding position … `/status` and metrics give each lane's stall age and WAL behind; an alert fires past 10 minutes or 1 GiB". Dropping and rebuilding a stalled index stays with #394.

Stage 7 gave every `(index, sink)` its own lane and its own marks in `Progress` (`libs/2-engine/src/progress.rs`), so this stage needs no index tasks.

## 1. Stall age

**Definition.** How long a lane has gone without landing its oldest outstanding item. This is the ADR's rule: call a lane stalled when it has acknowledged nothing for a window, not when its backlog is high (a busy index is healthy).

- A lane with nothing outstanding has age 0: `Progress::lane(lane, Applied)` answers `Through` or `Idle`.
- A lane landing items steadily stays near 0, because its oldest outstanding item changes at every landing.
- Only a lane whose oldest outstanding item stays the same item grows an age.

**How, at no cost per item.** A daemon task, `stall.rs`, samples every lane once a second through the existing `Progress::lane(&key, Level::Applied)`. That is one lock per lane per second, beside the engines' own once-per-batch registrations. It keeps, per lane, the oldest outstanding position it last saw and when it first saw it.

- Age is now minus that moment while the same oldest is outstanding.
- The age resets when the oldest changes or the lane clears.
- Resolution is the sampling period, 1 s; the alert works in minutes.
- Nothing is timestamped on the hot path. No new accessor on `Progress`.

**Backfill items carry no position, so they are not in `Progress`.** A lane whose only outstanding items are a backfill's has age 0 here.

- That is honest for the slot: a backfill pins no WAL.
- A live item queued behind a stuck backfill item on the same lane is registered, so its age grows and the alert fires.
- A backfill stuck with no live traffic at all shows as the index staying `backfilling` in `/status` and `flusso.indexes.seeded` not moving. The runbook says so.

## 2. WAL behind, and which lane pins the slot

**WAL behind per lane.** The source already reports the slot's lag (`ChangeCapture::lag`, polled in `lag.rs` into `Status`), and turns a `Landed` into its own position (`ChangeCapture::reached`; for Postgres an LSN).

- A lane holds WAL from its own `reached` onward.
- The slot is confirmed to the least of them: `Progress::landed`.
- So a lane's WAL behind is `slot_lag − (reached(lane) − reached(landed))`, clamped at 0.
- The lane that decides `landed` (the least `reached`, among lanes with something outstanding) **pins the slot**, and its WAL behind is the slot's lag.
- A source without positions (`reached` answers `None`) reports the slot lag on the pinning lane and nothing per lane.

**Exposed:**

- `/status`: `sinks.<sink>.lanes.<index>.stall_seconds` and `.wal_behind_bytes`, beside stage 7's `outstanding`. Top level: `pinned_by: {index, sink}` or `null`.
- Metrics:
  - `flusso.lane.stall_age{index,sink}` (gauge, s);
  - `flusso.lane.wal_behind{index,sink}` (gauge, By);
  - `flusso.slot.pinned_by{index,sink}` (gauge, 1 on the pinning lane, absent otherwise).
  - Cardinality is the number of lanes, as `flusso.lane.outstanding` already is.

**No threshold in flusso's behaviour:** nothing changes when a lane is "stalled". The threshold lives in the alert, and in one log line.

## 3. The alert

**`dev/prometheus/rules/flusso.rules.yml`,** modelled on `FlussoVisibilityWait`:

```yaml
- alert: FlussoLaneStalled
  expr: flusso_lane_stall_age_seconds > 600 or flusso_lane_wal_behind_bytes > 1073741824
  for: 5m
  labels: { severity: page }
```

- The annotations name the index and sink, and link the runbook.
- 10 minutes or 1 GiB are the ADR's numbers.
- `for: 5m` keeps a lane that recovers within the window from paging.

**The Helm chart.** `templates/prometheusrule.yaml` renders a `PrometheusRule` with the same rule behind `prometheusRule.enabled` (default `false`), as `serviceMonitor` is, with `labels` to match the operator's selector and the two thresholds as values. `helm` isn't installed here, so a golden render of the template with the values set is compared in a test, unless `helm template` is available.

**Grafana.** Two panels on the dev dashboard: stall age by lane, and WAL behind by lane with the pinning lane marked.

## 4. Logging

- **`warn!` once a lane's stall age passes 10 minutes,** naming the index, sink, age and WAL behind. Then once every 10 minutes while it lasts, as the visibility hold's warning repeats.
- **`info!` when it lands again,** with how long it stalled.
- **Under 10 minutes nothing is logged:** a sink restarting with backoff already logs its own errors.

## 5. The runbook (`docs/src/operate/`)

A page "A lane is stalled":

- How to find the index and sink: `/status` `pinned_by`, the alert's labels, `flusso.lane.stall_age`.
- The usual causes, each with what to do:
  - a write block on the index (`index.blocks.write`): lift it;
  - a mapping rejection under `on_error = "stop"`: the log names the document; fix the data or the schema, or switch the index to `skip`;
  - OpenSearch down or refusing (429 for good): the sink's own errors;
  - a stuck backfill.
- Reindex or skip as the ways out.
- The honest statement: until #394, a stalled lane pins WAL for every index on the deployment, so `max_slot_wal_keep_size` is the deadline before the slot is invalidated and everything rebuilds.

## 6. Tests

- **`stall::tests`, on paused time, over a real `Progress`:**
  - an idle lane has age 0;
  - a lane with an item outstanding ages;
  - a lane that lands its oldest resets;
  - a lane landing steadily never passes the sampling period;
  - a backfill-only lane has age 0;
  - the pinning lane is the least `reached` among outstanding lanes;
  - WAL behind is clamped, and is the slot lag on the pinning lane;
  - the warn / info transitions, through a counting log layer or a returned event.
- **`/status` fields** in the daemon's status tests; the gauge in the CLI's metrics tests.
- **The Helm render**, golden.
- **One e2e:** real Postgres and OpenSearch, two indexes on one sink, `index.blocks.write` set on one index, live changes to both.
  - The blocked lane's age grows past a short test threshold, and `pinned_by` names it.
  - Before stage 9 (per-lane failure) a write error stops the whole sink run, so the sibling lane may stall too. The test asserts what holds today and states it.

## Files

- `libs/3-daemon/src/stall.rs` (new) and `status.rs`; `lag.rs` hands the lag to it.
- The CLI's `telemetry/metrics.rs` and `http/`.
- `deploy/helm/flusso/`, `dev/prometheus/`, `dev/grafana/`, `docs/src/operate/`, the Reference pages for `/status`, metrics and Helm values.
