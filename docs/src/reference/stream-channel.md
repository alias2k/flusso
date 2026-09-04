# Stream: channel

The `[stream]` table with `type = "channel"`: the in-process stream between the source side and the sinks. It is the default, so the table may be omitted entirely.

{{#include generated/stream-channel.md}}

The channel is a bounded buffer. When it is full, capture waits, so the slowest sink paces ingest; a larger `capacity` absorbs bursts at the cost of memory. `--queue-capacity` / `FLUSSO_QUEUE_CAPACITY` override the key at run time.

## Example

```toml
[stream]
type = "channel"
capacity = 4096
```
