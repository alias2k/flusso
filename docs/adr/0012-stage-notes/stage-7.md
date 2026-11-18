# Stage 7 design: per-`(index, sink)` lanes, fences and caught-up

Today a sink has one lane carrying every index's items (`stream::Stream::lane(&SinkName)`,
`libs/1-ports/stream/src/items.rs:103`). After this stage the stream has one lane per
`(index, sink)`, the sink engine (still one task per sink) reads all of its sink's lanes, and
fences, caught-up, backfill requests and `/status` are per lane. One ingest task still builds every
index (stage 8a splits it).

## 1. The lane key

```rust
// libs/1-ports/stream/src/items.rs
pub struct LaneKey { pub index: IndexName, pub sink: SinkName }   // Ord: index, then sink
trait Stream {
    fn lane(&self, key: &LaneKey) -> Result<Lane>;          // unknown key: QueueError::UnknownLane
    fn lanes(&self, sink: &SinkName) -> Vec<LaneKey>;       // a sink engine's lanes, index order
    fn requests(&self) -> Result<RequestLane>;
    fn is_idle(&self) -> bool;
}
```

- **Which lanes exist:** every enabled index × every sink, fixed when the stream is built (the
  daemon passes them: `Backends::stream(config, &[LaneKey])`, `libs/3-daemon/src/backends.rs:71`,
  `apps/cli/src/backends.rs:79`). A disabled index has no lane.
- `Progress` is keyed by `LaneKey` instead of `SinkName` (`libs/2-engine/src/progress.rs`); nothing
  else in it changes.

## 2. Publishing: one build, one item per lane it touches

A built live batch holds envelopes of several indexes. At publish (`ingest/commit.rs`'s
`publish_live`, track C's file) the envelopes are grouped by index, in build order, each group an
`Arc<[Envelope]>` (one copy of the envelope list per index, the documents themselves are moved, not
cloned), and each group goes to the lanes `(index, s)` of every sink `s`.

- **An item's position** is the batch's last position, the same on every lane: a lane gets at most one
  item per batch and batches cover disjoint ranges, so the key stays unique per lane.
- **What it registers:** `Progress::published(lanes_that_got_an_item, oldest, last)`: the batch's
  oldest position, conservative for an index whose envelopes came only from later changes of the batch
  (it holds back nothing outside the batch). Stage 8a's pending sets give each index its own oldest.
- **An index with no envelope in the batch gets no item.** Its lanes are not named in `published`, so
  they have nothing outstanding for the batch: an index with no traffic pins nothing
  (`a_lane_with_nothing_routed_to_it_pins_nothing`), and one with an unlanded item still pins
  (`Before(its oldest)`).
- **A batch with no envelope at all publishes nothing** and still calls `published([], oldest, last)`,
  which moves `routed`: its changes are landed once routed (ADR §3). The `wal_release` empty-batch
  guard is rewritten to "nothing is published, and the slot still moves".
- **`changes`** (the source changes a batch was built from, for `changes_committed`) goes on each
  sink's first item of the batch, `0` on the others; a sink that gets no item from a batch is told by
  `Observer::on_changes_settled(sink, changes)`, so `changes_in_flight` keeps meaning captured minus
  settled.
- **Backfill:** a key page belongs to one index (`DocumentBuilder::keys(index, …)`), so its batch goes
  to `(index, s)` for each requesting sink `s`, and `SnapshotComplete` to the same lanes. Snapshot
  items stay unpositioned and unregistered.

**The seam I need from the publish path** (`ingest/commit.rs`, `ingest/snapshot.rs`; I can make these
edits myself if track C agrees, they are local):

```rust
type Lanes = BTreeMap<LaneKey, Box<dyn Producer<LaneItem>>>;      // ingest.rs:83, was by SinkName
async fn publish<'a>(&self, items: impl IntoIterator<Item = (&'a LaneKey, &'a Box<dyn Producer<LaneItem>>, LaneItem)>,
                     work: Work, started: Instant) -> Result<()>;  // unchanged but for the key type
// publish_live: group `envelopes` by index; one item per (index, sink); then
self.progress.published(keys_published, oldest, last);           // also when keys_published is empty
// publish_backfill: the page's index → (index, s) for s in requested sinks of that index
// finish_snapshot: LaneItem::SnapshotComplete { index } to (index, s)
```

## 3. Lane bound (`[stream] capacity`)

Each lane holds `max(1, capacity / indexes)` items (superseded: see §10), so a sink's lanes hold about `capacity` items
together, as its one lane did. `capacity` keeps its meaning ("items waiting per sink"), is deprecated
by the ADR (the byte budget of stage 5 replaces it) and still loads; `config-toml.md` and
`reference/stream-channel.md` say so. The sink engine's delivered items stay outside the bound
(§4).

## 4. The sink engine over several lanes

One task per sink (`sink_engine/writes.rs`'s `Run`), up to `writes` writes in flight in total.

- **Per lane:** a consumer, a window of delivered unfinished items, live/backfill counts, the
  barrier flag and its `SnapshotComplete` barrier. Admission is per lane: a document hash is of
  `(index, id)`, so items of two lanes never share a document, and an item is compared only with its
  own lane's earlier items and its own lane's writes in flight.
- **Taking from a lane:** only to use it, as today, now per lane: while a write slot is free and the
  lane has nothing admissible waiting, or, with only backfill waiting on the lane, to find a live item
  behind it, up to `LOOKAHEAD_PER_LANE` (8) waiting. Delivered items per sink: at most
  `writes + 8 × lanes`.
- **Selection across lanes:** live before backfill across all lanes; every fourth contested grant
  goes to backfill (unchanged rule, sink-wide counter); among candidates of one kind, lanes take
  turns (a cursor that moves past the lane just granted), so one busy index can't starve another.
  Within a lane, its earliest admissible item.
- **`SnapshotComplete`** is a barrier on its own lane only: `mark_seeded(index)` once everything
  before it on that lane is written; other lanes go on.
- **A reindex** of an index is staged once its lane has nothing in flight; that lane takes nothing
  meanwhile, the others go on.
- **A stop** skips the waiting backfill items of every lane and finishes the live ones (unchanged,
  per lane).
- **Failure:** a failed write still ends the run for the whole sink (stage 9 parks a lane). The run
  records which lanes failed. The next run puts a **barrier on those lanes only**: each writes its
  front item alone until one write on it lands; other lanes write freely. `advanced()` (the supervisor's
  progress signal) counts a landing on a failed lane's front, or any landing when the failure named no
  lane (a transport error), so a healthy sibling index landing doesn't reset the backoff of a poison
  item. A fresh engine counts every lane as failed (a previous process may have stopped anywhere).
- **Ordered sinks** (stdout): one write at a time. Across lanes, the next item is the live lane front
  with the lowest position; backfill fronts get every fourth turn when live waits, else go in lane
  order. Per lane, exact lane order, so per document the order a reader sees is the build order.
  Backfill items carry no position, so between indexes a reader sees no global order for them.
- **`Sink::write` keeps its signature** (the ADR lists an `index` argument for this stage): every
  item is one index's now, so a sink can read it off the envelopes, and adding the argument would touch
  every sink and test double (track A is in the OpenSearch sink). Deferred to when a sink needs it;
  the upgrade page says so.

## 5. Caught up and visibility, per lane

- A write is **caught up** when nothing else of its lane is queued or waiting. `Sink::write`'s
  contract stays sink-wide (it makes every write that returned before it searchable), which is at
  least what the lane needs.
- `Fences` (`sink_engine/fences.rs`) is per lane for positions (applied/searchable in lane order, the
  marks it feeds to `Progress`), with the returned-writes counter shared by the sink, since a
  caught-up write's coverage is sink-wide.
- **A `Deferred` refresh runs when its lane empties**: an idle lane with a written position not yet
  searchable gets an empty caught-up write (today: when the engine goes idle).

## 6. Backfill requests, per index

```rust
Request::Backfill { index: IndexName, sink: SinkName }   // was { sink, indexes }
```

- Staging sends one request per unseeded index; the sink engine's `outstanding` set is unchanged.
- The ingest engine coalesces by index (`ingest/snapshot.rs`'s `start_snapshot`, a pattern change),
  publishes each index's pages to `(index, s)` of the sinks that asked for it, and acknowledges each
  request after its lane's `SnapshotComplete`.

## 7. Fences per `(index, sink)`

- **Daemon** (`libs/3-daemon/src/fence.rs`): `FenceRequest` gains `indexes: Vec<IndexName>` (empty:
  every index). The fence waits on the lanes `indexes × (sink or every sink)`, taking the lowest
  answer, kept monotonic per lane. A name that is no enabled index is `FenceError::UnknownIndex`
  (404, like an unknown sink); a disabled index is unknown here, so the client gets `FenceRefused`.
- **HTTP** (`apps/cli/src/http/mod.rs:312`): `/wait?position=…&index=a&index=b`, repeatable; the
  answer echoes `indexes`. An old client sends none and waits on every index, as today.
- **Client** (`sdk/query/src/fence.rs`): `fence_before(level, indexes)`. A root search or get passes
  its root's index; a combined search and `_msearch` the union of their roots; a cursor walk its
  request's indexes. **A raw request** (`msearch` over hand-written lines) passes none, so it waits on
  every index. The once-per-level cache (`passed: AtomicU8`, `fence.rs:137`) becomes a small map
  `(level, index or every) → passed`, shared by a fenced client's clones; a union fence that passes
  marks each of its indexes, a pass on every index covers any index. "A fence never falls back to a
  stale read" and the restart-riding loop (#377) are unchanged: only the query string changes.
- **A stalled sibling index** on the same sink never blocks a fence on a healthy index (new guard,
  the per-index analogue of `a_stalled_sibling_blocks_an_all_sinks_fence_but_not_one_on_the_healthy_sink`).
- An index mid-reindex is still not covered (#6). A lane re-staging after `index_not_found` answering
  `503` is stage 9.
- **Compatibility:** an old daemon ignores `index` (it waits on every index); a new client against it
  is correct, only slower to pass. Breaking for embedders: `FenceRequest`, `Fence::wait`.

## 8. Observability

- `/status`: `sinks.<sink>.indexes.<index>` gains `outstanding` (items routed and not landed) and
  `applied_position` / `searchable_position` per lane. Per-sink totals stay.
- Metrics: `flusso.lane.outstanding{index,sink}` (gauge read from `Progress` at scrape); the write
  timings and counts gain `index` beside `sink`. `on_stage` keeps its meaning.
- `Observer::on_position_applied` / `on_position_searchable` gain the lane's index.

## 9. Order of work

1. Stream port + channel adapter (`LaneKey`, `lanes(sink)`, per-lane bound), `Progress` keys.
2. Sink engine over several lanes (window, admission, selection, barrier, snapshot, reindex, stop,
   ordered), `Fences` per lane.
3. Requests per index.
4. The publish split (seam above, with track C).
5. Fence, HTTP, status, metrics.
6. Client and its cache; query-train docs.

Tests red first at each step: the channel per lane (a skipped lane pins nothing, redelivery per lane),
the sink engine over several lanes (per-lane order, cross-lane concurrency up to `writes`, fairness,
a stalled index doesn't stall its sibling's writes or fences, `SnapshotComplete` per lane), the
seeded schedule test over several indexes, fence unit tests per index, HTTP, client (stub daemon); e2e
`daemon` `fence` (a per-index case), `flusso-cli` `fence_restart`/`shutdown`, `engine`
`pipeline`/`wal_release`/`backfill`/`keyset_backfill`, `dev/query-e2e`.

## 10. As built (where it differs from the design above)

- **Reindex stays sink-wide** (§4): staged once the sink has no write in flight, as before. Per lane
  needs a lane to hold back while the others write, which is stage 9's parking; doing it here would
  build half of that.
- **Look-ahead is 32 per lane**, not 8 (§4): the stage-4 value per lane, so a single-index deployment
  behaves as before. Delivered items per sink: at most `writes + 32 × lanes`.
- **Capacity** (§3): each lane holds `max(1, capacity / the sink's lanes)` items. Track C then
  deprecated `[stream] capacity` (f7863ce3): ignored with a warning, every lane bounded by a
  1,048,576-item backstop; memory is bounded by the stage-5 budget.
- **`/status`** (§8): the per-lane block is `sinks.<sink>.lanes.<index>` (`applied_position`,
  `searchable_position`, `outstanding`), beside the unchanged `sinks.<sink>.indexes.<index>` state
  string, so no existing field changes type.
- **Metrics** (§8): `flusso.lane.outstanding{index,sink}` is in. The write timings keep `sink` only;
  an `index` label is deferred (a write is one lane's, so it can be added in the sink engine's
  observer calls when needed).
- **Client** (§7): `Client::wait_on(position, indexes)` added; `MsearchBundle::indexes` with a default
  of none (a fence on every index), so a hand-written bundle stays correct.

## 11. For stage 8a and stage 9

- **8a (an ingest task per index):** `Progress::published` / `skipped` are already per lane and
  per call, so per-index ingest tasks each register their own lanes. The only cross-index piece left
  is `routed` (the newest position routed, one `Mutex<Option<Position>>`) and confirmation's minimum
  over lanes: two tasks registering out of seq order is fine for marks (each lane's own order holds)
  but `routed` must stay the max. A batch's `oldest` is still the batch's, conservative per index.
- **9 (park a lane):** `Blame::Lanes` already names the failed lanes and the barrier is per lane;
  parking = keep the run alive and stop granting that lane, instead of ending the run. A parked
  lane's unlanded items pin confirmation (the slot) and its own fences only. Reindex per lane can
  come with it.
