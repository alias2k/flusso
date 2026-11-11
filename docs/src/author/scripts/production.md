# Run scripts in production

What a script costs, what happens when one fails on a live row, how to hear about it, and how to get the affected documents right again.

## Pick the failure policy first

A script that fails on a document fails that document, and the index's [`on_error`](../../reference/index-and-on-error.md#on_error) decides what that means. The failure happens on the ingest side, before any sink sees the document.

| `on_error` | A failing document | Good for |
| --- | --- | --- |
| `"stop"` (default) | stops the ingest engine; the daemon restarts it with backoff, and the change fails again until the script or the row is fixed. The slot doesn't advance, so lag and WAL grow. | indexes where a wrong or missing document is worse than stale ones |
| `"skip"` | is published to no sink; every sink keeps the version it already has. Sync carries on. | most indexes: one odd row shouldn't stall the rest |

```toml
[[index]]
name = "products"
schema = "products.schema.yml"
enabled = true
on_error = "skip"
```

The same policy governs a document the destination rejects ([Handle rejected documents](../../operate/rejected-documents.md)), so choose it for both.

> ⚠️ **Warning** — Under `skip` a failing document isn't deleted and isn't updated: the index keeps serving its previous version, or nothing if it never had one. Stale is quiet; set up the alert below.

## Hear about it

Every failure, under either policy, reaches three places:

- **The log**, at `error`, with the index, the document id, the field, the message, and the script line with a caret. One field failing on a million rows during a backfill logs once a minute, carrying the count it held back. The document itself goes to `debug` only (`RUST_LOG=info,script=debug`), since it may hold personal data.
- **The metric** [`flusso_script_failures_total{index,field,policy}`](../../reference/metrics.md):

  ```promql
  sum by (index, field) (increase(flusso_script_failures_total[10m])) > 0
  ```

- **`/status`**, under [`script_failures`](../../reference/http.md#get-status): a count per index and the most recent failure, with its id, field, message, line and column.

The dev stack's Grafana dashboard has a **Script failures** panel built on the metric.

## Fix it and catch up

| What was wrong | Do | What rebuilds |
| --- | --- | --- |
| the script | fix it and redeploy | everything: a script is part of the [content hash](../../reference/sink-opensearch.md#index-naming), so the index is rebuilt into a new generation, and the old one serves until it's done |
| the data, in a few rows | fix the rows | each fixed row's document, through the normal change stream |
| the data, in many rows | fix them, then [reindex](../../operate/reindex.md) | the whole index |

Under `stop`, the engine resumes on its own once a fix lands: the failing change is retried after every restart. Under `skip`, a skipped document stays stale until its row changes again (or a reindex rebuilds it).

Reproduce a failure from the log with the id it names: `flusso script run --index products --id <id>`. See [Test and debug a script](debugging.md).

## What a script costs

Scripts run on the ingest side, once per document, on the CPU of the flusso process. Measured on a laptop over 1,000 documents (the `scripted` bench, run on every PR):

| Shape | Per document |
| --- | --- |
| three root-level scripts reading `this` | about 8 µs |
| one script on each of 10 `has_many` elements, reading `this` and `root` | about 23 µs |

That's small beside the database round trip a build already makes, but it adds up on wide arrays. What drives it:

- **Each level a script sits on is converted once per document.** A script inside a `has_many` of 500 elements converts 500 objects. Prefer one root-level script over the array to one script per element when the result is per document anyway.
- **`root` is built only when some script of the index reads it**, so don't mention `root` where `this` will do.
- **Input-only fields are still fetched.** They cost the query, not the index.
- **The sandbox caps work** at 1,000,000 operations per call. A script that hits it fails its document; it never stalls the pipeline.

## Changing scripts safely

- **Run `flusso check --offline` in CI.** It compiles every script, so a syntax error never reaches a deploy.
- **Keep a few fixture documents** and run `flusso script run --input` over them in CI; it exits non-zero when a script fails. Add a fixture for every production failure you fix.
- **Every edit rebuilds the index.** Batch script changes together, and expect a backfill's worth of load when one ships.
- **A new `required: true` script on an existing index** will fail on every row it can't produce a value for; start it as `required: false` until the data proves it.
- **Upgrading flusso** keeps scripts behaving the same as far as its tests can tell: the Rhai version is pinned per release, and every recipe of this guide runs in the test suite. Re-run your fixtures after an upgrade all the same.

## Related

- [Script fields](../../reference/scripts.md#failures): the exact failure semantics.
- [Watch it run](../../operate/watch-it-run.md): the rest of the signals.
- [Handle rejected documents](../../operate/rejected-documents.md).
