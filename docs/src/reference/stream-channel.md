# Stream: channel

The `[stream]` table with `type = "channel"`: the in-process stream between the source side and the sinks. It is the default, so the table may be omitted entirely.

{{#include generated/stream-channel.md}}

The stream is one lane per `(index, sink)` plus a request lane. What the lanes hold is bounded in bytes by the ingest engine's memory budget, [`[concurrency] max_built_mib`](config-toml.md#concurrency): a batch is counted until every lane acknowledged it, so a sink that stops acknowledging fills the budget and the ingest engine waits. The slowest sink paces ingest and, through the replication slot's confirmation, WAL retention. Every delivered item stays in flight until the sink engine acks it after its write, and an unacked item is redelivered to a restarted engine. Each lane also keeps a fixed backstop of 1,048,576 items, which the budget's 1 KiB floor per batch keeps from binding.

`capacity` is **deprecated**: it still loads (as does `--queue-capacity` / `FLUSSO_QUEUE_CAPACITY`), flusso warns at start, and it is ignored.

## Example

```toml
[stream]
type = "channel"
capacity = 4096
```
