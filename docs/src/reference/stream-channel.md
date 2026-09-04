# Stream: channel

The `[stream]` table with `type = "channel"`: the in-process stream between the source side and the sinks. It is the default, so the table may be omitted entirely.

{{#include generated/stream-channel.md}}

The stream is one bounded lane per sink plus a request lane, each holding `capacity` items. A full lane makes the ingest engine wait, so the slowest sink paces ingest and, through the slot watermark, WAL retention; a larger `capacity` absorbs bursts at the cost of memory. Every delivered item stays in flight until the sink engine acks it after its flush, and an unacked item is redelivered to a restarted engine. `--queue-capacity` / `FLUSSO_QUEUE_CAPACITY` override the key at run time.

## Example

```toml
[stream]
type = "channel"
capacity = 4096
```
