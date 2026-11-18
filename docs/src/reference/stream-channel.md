# Stream: channel

The `[stream]` table with `type = "channel"`: the in-process stream between the source side and the sinks. It is the default, so the table may be omitted entirely.

{{#include generated/stream-channel.md}}

The stream is one bounded lane per sink plus a request lane, each holding `capacity` items. A full lane makes the ingest engine wait, so the slowest sink paces ingest and, through the replication slot's confirmation, WAL retention; a larger `capacity` absorbs bursts at the cost of memory. Every delivered item stays in flight until the sink engine acks it after its write, and an unacked item is redelivered to a restarted engine. Delivered items sit outside the bound: a sink engine takes at most [`writes`](config-toml.md#concurrency) + 32 of them, so each sink holds up to `capacity` + `writes` + 32 batches (about `capacity` + `writes` with live traffic alone; the 32 are taken only to look past blocked items or a backfill). `--queue-capacity` / `FLUSSO_QUEUE_CAPACITY` override the key at run time.

## Example

```toml
[stream]
type = "channel"
capacity = 4096
```
