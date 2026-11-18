# Stage 8b design, part 1: resuming an interrupted backfill

Track A, prefix `s8b:`. ADR 0012 stage 8b is "per-index restart and backfill resume". Roadmap issue #3 asks for the resume half: "checkpoint a keyset cursor … and resume from it instead of restarting the scope". This note designs the resume half only. Per-index restart waits for 8a's index tasks.

**Today**, a backfill cut short by a crash, a stop, a restart of the process or of the ingest engine is requested again at the next stage, and starts again from the first key. That is idempotent, but a 6-hour backfill that dies at hour 5 costs 11.

**After this**, each `(index, sink)` lane records how far it has durably got. The next walk starts from the least of the requesting lanes' points and gives each lane only what it still needs.

It is shaped to move into 8a's index task as it is: everything is per index, and the walk stays track A's `Walk`.

## 1. What "durably got to" means

**The resume point of a lane** `(index, sink)` is the greatest key K such that every document the walk produced for a key ≤ K has **landed** on that sink. Landed means its write returned and it was not rejected under `stop`.

### Why a lane can't read it off `Progress`

Snapshot items carry no position. `LaneItem::Batch` is built with `position: None` in `publish_backfill` (`libs/2-engine/src/ingest/commit.rs:314`), and `Progress` registers only positioned items. So the marks need a carrier of their own.

### The carrier: a mark on each snapshot item

A snapshot item carries the greatest key of its index in that batch, as an opaque **walk mark** (`stream::WalkMark`, section 5).

- **The keys are in walk order.** A batch's keys come off the walk's pages in key order (`Snapshot::take_keys`, `libs/2-engine/src/ingest/snapshot.rs:170`), so the last id of an index in a batch is that item's greatest key.
- **The ids, not the envelopes.** The ids are in the build job (`start_backfill`, `commit.rs:131`), and the mark is taken from them. Envelopes carry ids as strings, not keys.
- **Items arrive in key order.** Builds publish in grant order (`ingest/builds.rs`, the reorder slots), so a lane receives one walk's items with increasing marks.

### Landed out of order: the contiguous prefix

With `writes > 1` and merged writes, items land out of order. Admission (`writes.rs`'s `pick`) lets two backfill items of one lane fly at once whenever they share no document. So the resume point follows a **contiguous-prefix rule**, as the fences do for positions (`sink_engine/fences.rs`).

**A per-lane `Walked` tracker** in the sink engine:

- **It queues** the marks of the lane's snapshot items in lane order (the run's `seq`, `writes.rs` `Queued.seq`).
- **It sets a flag** on an item when its write returns with the item landed. The flag is set in the code that acknowledges landed parts in `Run::landed` (`writes.rs`, after `stopping_rejections`).
- **It advances** over flagged items from the front, and the newest mark passed over is the lane's landed point.

What doesn't count, and what the queue copes with:

- **A skipped item is never landed.** Under a stop, `skip_backfill` acknowledges queued snapshot items unwritten (`writes.rs:659`). They are not flagged and the prefix stops at them, so a stop never checkpoints past what is in the index.
- **A failed or `stop`-rejected part is never landed.** It is redelivered and comes back first in the next run.
- **Delivery order is lane order within a run.** The channel redelivers due items first, in lane order (`stream-channel`, issue #395). An item is released only on a failure, which ends the run. So the queue needs nothing across runs: a run starts empty, and what earlier runs landed has already been recorded, or is redone (section 2).
- **Live items on the same lane** are not walk items and don't touch the tracker.

## 2. Where it is stored, by whom, how often

**In the sink.** The `Sink` port gets the checkpoint as opaque text, and the OpenSearch sink keeps it in the index's meta document (`generations.rs`'s `put_meta`/`get_meta`, `:113`/`:126`). That document already holds `active_generation` and `seeded`.

```rust
// libs/1-ports/sink/src/sink.rs
/// What staging reads of an index: seeded, or where an unfinished backfill got to.
pub enum Seeding { Seeded, Unseeded { resume: Option<WalkMark> } }
async fn seeding(&self, index: &IndexName) -> Result<Seeding>   // default: from is_seeded, resume None
async fn record_walked(&self, index: &IndexName, mark: &WalkMark) -> Result<()>  // default no-op
async fn forget_walked(&self, index: &IndexName) -> Result<()>   // default no-op
```

- **`seeding` costs no request.** It is read from the same meta document `is_seeded` reads now (`sink_impl.rs:325`). Staging calls it instead of `is_seeded` (`sink_engine.rs:431`).
- **The meta document gains a field:** `walked: { mark, generation }`.
  - It is written with a partial `_update` that never touches `seeded`.
  - It is read only when `seeded` is false and `generation == active_generation`.

**Every whole-document write drops `walked`, which is wanted**, except one:

| Writer | Effect |
| --- | --- |
| `mark_seeded` (`sink_impl.rs:405`) | Drops it: the backfill is done. |
| `reindex` (`:442`) | Drops it: a new generation means a new walk. |
| `ensure_index`'s retracted marker (`:136`) | Drops it: the generation is gone, so the work is too. |
| `ensure_index`'s plain rewrite of an **unseeded** index's meta on every start (`:134-137`) | **Must not drop it.** It changes to write only when the generation is created (`create_index` → `true`) or the meta is missing. A generation created now holds nothing, so dropping its checkpoint is right. |

**Who writes it.** The sink engine, when a lane's landed point has moved and the lane's last record is `RECORD_EVERY` old.

- `RECORD_EVERY` is 10 s, fixed: a crash redoes at most about 10 s of landed work, plus what was in flight.
- The record runs as a flight on its lane, like a seed (`Outcome::Record`). It takes no write slot: it is one small update.
- The lane's `SnapshotComplete` waits for it, so `mark_seeded`'s whole-document write always comes after any record.
- **It is never ahead of landed.** It records only a prefix mark, read after the writes returned.

**What it costs:**

| Case | Cost |
| --- | --- |
| `reference`, a backfill of about 3.5 s | 0 extra requests per backfill. 10 s never passes, and `SnapshotComplete` supersedes the record. |
| A 6-hour backfill | 2,160 updates per index, one every 10 s. |
| Stateless sinks (`backfill = false`, stdout) | The defaults. Nothing is stored, and nothing changes. |

## 3. When a checkpoint is valid, and why resuming is correct

### The claim

After a restart, a lane resumes the walk after its mark K. When the walk ends, every document of that index on that sink is at least as new as its row's state at the start of the slot's stream, and every later change is delivered live.

### The argument

The argument rests on two facts:

- The walk began after the slot existed: `prepare` runs before any snapshot, an order the daemon enforces.
- A resumed process streams from the slot's `confirmed_flush_lsn` C.

Take a row r with key ≤ K. Its document landed before the crash, built at some time t ≥ the slot's creation. For every change p to r after t:

- **p ≤ C:** confirmation passes p only once every lane landed it (`Progress::landed`, ADR stage 6). So the live build for p, which read r at or after p, has landed. Admission and grant order keep a document's builds landing in build order, so the newest build of r is in the sink.
- **p > C:** the resumed stream replays p and r is rebuilt. Nothing for r is needed from the walk.

The same holds on every lane, because each lane's mark is its own landed prefix.

### The cases that rest on it

- **A delete below K** during the downtime arrives as a live tombstone.
- **A key change across K.** It is a delete of the old key (live) and an insert of the new one (live, and maybe read again by the walk: idempotent).
- **8a's dropped pages.** A page drops ids pending live (ADR §1). Their live build is registered on every lane, so confirmation stays before it until it lands: covered by p ≤ C.

### The upper bound: read again, not stored

A resumed walk reads `last_key` afresh when it starts, as any walk does (`Walk::step`, `snapshot.rs:250`).

- **Why it's safe:** a key between K and the new bound is a row that exists now, so building it is correct, and a later change to it is in the stream. The walk only needs to reach every row whose key it hasn't covered, and a fresh bound reaches at least as far as the old one.
- **Why it's simpler:** nothing more is stored. The cost is building the rows inserted above the old bound, which the stream already delivered: idempotent.

### Valid only while every part of it still matches

A checkpoint is discarded, so the walk starts from the first key, when any of these doesn't match:

| What | How it is checked |
| --- | --- |
| **The slot** | Under `Continuity::Fresh` the changes since the checkpoint are lost. The sink engine calls `forget_walked` for every **unseeded** index while staging (`sink_engine.rs:298`, beside `rebuild_stale_seeds`), before `prepare` creates the slot (the daemon's order). A crash between the two comes back `Fresh` again. Seeded indexes are rebuilt by `reindex`, which drops it anyway. |
| **The content hash** | The meta document's id is the hash alias, so a new hash finds no checkpoint. |
| **The index prefix** | `flusso_meta` and the hash alias are prefixed, so a new prefix finds nothing. |
| **The generation** | Stored beside the mark and compared with `active_generation`. A reindex writes the next generation (`sink_impl.rs:442`), and the stored one no longer matches. |
| **The generation index deleted** | `ensure_index` recreates it, and creating it drops `walked` (section 2). The #123 retraction is the seeded case of the same. |
| **`flusso_meta` deleted** | No document, no checkpoint: from the first key. |
| **The root key's column or type** | The mark holds the column name, the base type (`ColumnMeta.key_type`) and the value. The source decodes it (`DocumentBuilder::resume_after`, section 5) and answers `None` when either differs. |

## 4. Several sinks, one walk

Requests coalesce into one walk fanned to every requester (`collect_requests`, `snapshot.rs:31`). After a restart, sink A may be at K₁, sink B at K₂, sink C fresh (no mark), and D seeded (no request).

**The walk is cut into segments at the marks, in key order.**

1. **Order the marks once.** Rust can't order keys as Postgres does (collations, `citext`, the opclass of stage 3), so the source does it: `DocumentBuilder::order_keys(index, &[RowKey]) -> Vec<usize>`. On Postgres that is one statement: `unnest($1::<bound>[]::<key type>[]) … ORDER BY k USING OPERATOR(s.<)`, with stage 3's operators. It runs only when two or more requesters hold marks.
2. **Walk from the least mark**, with no mark meaning from the first key, in segments. For marks ordered m₁ < m₂ < …, the walk reads `(start, m₁]`, then `(m₁, m₂]`, and so on up to `last_key`. Each segment is a run of `keys(index, after, through, limit)` with `through` set to the next mark. Stage 3 already bounds a page that way (`query::key_page_query`'s `within`).
3. **Publish each item only to the lanes whose mark is below the segment.** A lane at mark mᵢ gets items from segment i+1 on.

**Why filter rather than send everything:** sending a lane the range it already holds is idempotent, but it costs that range's writes again. Sink B at K₂ = 90% of a 6-hour table would rewrite 90% of it. Filtering costs one statement per walk and a set membership per published item.

Equal marks share a segment. A segment ends where a lane starts, so no item is cut in two: the batches of `take_keys` are filled per segment, and a mark is always a batch boundary of the earlier walk.

## 5. The request protocol, and the port changes

**The request carries the resume point.** `Request::Backfill { index, sink, after: Option<WalkMark> }` (`items.rs:127`).

- The sink engine fills `after` from `seeding()` at staging, so the ingest side never asks a sink anything.
- 8a's dispatcher forwards the request unchanged.

**`WalkMark`** is opaque to the stream and the sink: `pub struct WalkMark(Arc<str>)` in `libs/1-ports/stream`.

- **What it holds:** the source's own text, `{ "column", "key_type", "value" }`, with `value` in `GenericValue`'s lossless serde.
- **Who reads it:** only the source. `DocumentBuilder::walk_mark(index, &RowKey) -> Option<WalkMark>` writes one, and `DocumentBuilder::resume_after(index, &WalkMark) -> Option<RowKey>` reads one back, answering `None` (with a warning) when it no longer fits.
- **Who passes it:** the engine, untouched.

**`Batch`** gains `walked: Option<WalkMark>`: `None` on live items, and on a snapshot item the batch's last key of that index.

**The engine:**

- `Snapshot.requested` becomes per lane, with each lane's mark.
- `Walk` gains its segment bounds. `Walk::new(documents, [index])` keeps working for 8a, which moves the walk into the index task as it is.

**What stays as it is:**

- **The `outstanding` dedup** (`sink_engine.rs:132`): an index stays outstanding until its `SnapshotComplete`.
- **Re-staging after a sink engine's own restart** asks nothing again (`stage`'s `already_requested`, `:302`). The lane redelivers what was unacknowledged, and the walk in the ingest engine goes on.
- **An ingest engine restart within one process** gets the same redelivered request, carrying the mark read at staging, which may be old. It resumes from that mark, still correct, and redoes what landed since.
  - Resuming from "the last key it published" is 8a/8b's index-task restart, which keeps the walk's state in memory across a task restart (ADR §6).
  - This design leaves it a seam: `Snapshot`'s per-lane start.

**A reindex control** goes through `SinkEngine::reindex` → `sink.reindex` (`sink_engine.rs:564`).

- `sink.reindex` writes the next generation, which drops `walked` durably **before** the new request goes out with `after: None`.
- A crash between the two leaves an unseeded new generation with no checkpoint: the next start walks from the first key.
- A reindex arriving while the index's snapshot is outstanding is ignored as today (`:576`).

**The stream-port change lands first, in one small commit, announced to B and C.** It adds the field to `Batch` and to `Request::Backfill`, and fixes every construction site: about 24 `Batch { … }` literals, mostly in engine tests, and 5 `Request::Backfill { … }` sites, each given `walked: None` / `after: None`. It needs no behaviour change.

## 6. Operator surface

| Where | What |
| --- | --- |
| Log, on resume | `info!`: "resuming the backfill of {index} on {sink} after key {key text} ({n} rows already landed per the estimate)". On a discarded checkpoint, `warn!` with the reason (generation, key type, fresh slot). |
| `/status` per lane | `backfill: { resumed: bool, landed_documents, estimated_rows }` while one runs. `estimated_rows` comes from `DocumentBuilder::estimate_rows(index)` (Postgres: `pg_class.reltuples` of the root, read once when the walk starts, approximate, `null` when unknown). The percentage is left to the reader, since `reltuples` can be stale. |
| Metrics | `flusso.backfill.landed{index,sink}` (counter, documents) and `flusso.backfill.checkpoints{index,sink}` (records written). |
| Docs | `docs/src/operate/` gets a backfill section: what resumes and what restarts from the first key, from the table in section 3. Also the `reindex.md` note, the `sink-opensearch.md` meta document fields, the 0.18 upgrade page ("an interrupted backfill now resumes"), `http.md`'s `/status` fields, `metrics.md`, and CLAUDE.md. |
| `--skip-backfill` | No request, so the checkpoint is left as it is. A later start without the flag resumes from it. That is correct: the skipped run's live changes landed before they were confirmed, the same argument as in section 3. |

## 7. Failure modes

| What | What happens |
| --- | --- |
| A record fails to write | `warn!`, once per lane per minute. The backfill goes on, and the next due record tries again. It never fails the run: a missing record only costs redone work. |
| The checkpoint can't be read at staging | `seeding()` fails as `is_seeded` fails today: a staging error, and the daemon retries staging. To make this softer, an unreadable `walked` alone (the document read fine) is treated as no checkpoint, with a warning. |
| A checkpoint that is corrupt, foreign, or of another generation or key type | Discarded with a warning, and the walk starts from the first key (section 3). |
| A lane's mark the source can't place (key type changed) | `resume_after` answers `None`. That lane starts from the first key, and the other lanes keep theirs. |

## 8. Tests, written first

**Unit tests:**

- `Walked` (the tracker): out-of-order landing never moves the mark past an unlanded item; a skipped item stops it; a new run starts empty. Plus a seeded random test that lands items in random order against a model.
- The OpenSearch sink's `seeding` and `walked` against a stub document: generation mismatch, `seeded`, a missing field, malformed JSON.
- Postgres: `walk_mark` / `resume_after` round-trip every key type of the `keyset` e2e, and answer `None` on another type or column.

**Engine, over mocks, paused time:**

- `a_backfill_resumes_after_the_mark_its_sink_recorded` (a restart of the whole engine, with the mock sink's stored mark).
- `a_mark_is_never_recorded_past_an_item_still_in_flight` (seeded: `writes = 4`, random landing order).
- `two_sinks_at_different_marks_each_get_only_what_they_lack` (A at K₁, B at K₂, C fresh, D seeded: per lane, which keys were written).
- `a_fresh_source_forgets_every_unseeded_mark_before_preparing`.
- `a_reindex_drops_the_mark_before_requesting`.
- `a_stop_records_nothing_past_the_skipped_items`.
- `an_ingest_restart_resumes_from_the_requests_mark`.

**OpenSearch e2e** (`seeding` binary):

- `walked` round-trips.
- It is dropped by `mark_seeded`, by `reindex`, and by a recreated generation.
- It is kept across `ensure_index` of an existing unseeded generation.

**The e2e that matters:** `flusso-cli`'s `shutdown` binary gets `random_kills_during_a_backfill_resume_and_converge` (seeded, `FLUSSO_KILL_SEED`).

- The real binary over 50,000 rows, with concurrent writers.
- `SIGKILL` at random points of the backfill, then a restart, up to five times.
- At the end the index equals the table.
- The total number of documents built (the sum of `flusso.build` counts over the runs) is below a full restart's (the same kills with resume turned off by a test switch). The test reports both.

## 9. What I need from the other tracks

- **B (8a):**
  - `ingest/snapshot.rs` stays mine until 8a's step 4 moves the walk. I need nothing frozen beyond that.
  - When the walk moves into the index task, it must carry `Snapshot`'s per-lane start and the segment bounds as they are.
  - `Batch.walked` must be set on every snapshot item the index task publishes.
- **B (sink engine):** `writes.rs` gains `Walked` per lane and an `Outcome::Record` flight beside `Outcome::Seed`. Same shape as the seeds.
- **C:** nothing (`commit.rs`'s `publish_backfill` gets the mark and the per-lane filter: about 20 lines).
- **Order:**
  1. The port commit (section 5).
  2. The sink side: tracker and records.
  3. The source side: mark and ordering.
  4. The engine side: segments and filter.
  5. The e2e.

## Open points for the review

1. `RECORD_EVERY` fixed at 10 s, or an option? I'd keep it fixed until someone needs it.
2. The tracker needs no state across runs (section 1). Correct only while an item is released only on a failure that ends the run. True today (`writes.rs`'s failure path); a future partial release would need the tracker on `SinkEngine`.
3. Segments need `order_keys` only with two or more marked requesters. The alternative is separate walks per distinct mark: simpler, but it builds the overlap once per sink.
