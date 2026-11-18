# Stage 6 design: self-describing positions and `Progress`

Still one lane per sink. Stage 7 adds lanes; nothing here assumes there is one.

## 1. `Position` (kernel)

Today `Position(pub u64)`, `#[serde(transparent)]` (`libs/0-kernel/src/envelope.rs:37-39`).

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Position {
    pub seq: u64,             // the order; compared first, so Ord is by seq
    pub fence_before: u64,    // the source's own stamps, opaque to the engine
    pub confirm_before: u64,
    pub confirm_through: u64,
}
Position::new(seq)            // no stamps: mocks, a source that cannot confirm
Position::stamped(seq, fence_before, confirm_before, confirm_through)
```

- **Wire: unchanged.** `Envelope.position` is still written as `seq`, the decimal string of
  `position.seq` (`seq_string`, `envelope.rs:137-160`). Reading it back gives `Position::new(seq)`.
  A consumer of an emitting sink sees nothing new.
- **Rust API: breaking.** `Position(n)` → `Position::new(n)`, `.0` → `.seq`. Listed on the upgrade
  page (the kernel and the `ChangeCapture` port are public crates).
- `Batch` keeps its one `position` (the batch's last change). It is the item's key in `Progress`
  (unique per lane: batches cover disjoint ranges). `Progress` learns the batch's oldest position at
  publish, from the ingest engine, so the lane item needs no new field until stage 7.

## 2. The capture (Postgres) keeps no map

Today `Positions` holds `entries: BTreeMap<seq, commit lsn>` (`ack.rs:89-95`), filled by
`register` on every emitted change (`stream.rs:363`) and dropped by `confirm` (`ack.rs:210-225`).

New `Inner`: `next_seq`, `horizon`, `confirmed_lsn`. Nothing per change.

**Stamping**, in `DecodeState::next_change` (`stream.rs:359-369`), under the one lock it already
takes per change (`register`):

- `fence_before = lsn − 1` (the change's commit LSN, as `reached` computes today, `ack.rs:116`);
- `confirm_before = horizon` before the change: the end of the last commit fully emitted, or a
  keepalive / filtered commit read while nothing was buffered. Every change of one transaction gets
  the same value, because the horizon moves only when a commit's last change is emitted;
- `confirm_through = lsn` when this is its commit's last change (known at emission: the visibility
  hold keeps whole transactions in `pending`, `stream.rs:364`), else `confirm_before`;
- the last change of a commit moves the horizon to `lsn`, in the same critical section that bumps
  `next_seq`, so `p.seq + 1 == next_seq` and the horizon are always read consistently.

**The port** (`libs/1-ports/source/src/cdc/capture.rs`): `confirm(&self, Landed)` and
`reached(&self, Landed) -> Option<SourcePosition>`, with

```rust
pub enum Landed { Before(Position), Through(Position), Idle }
```

| `Landed` | `confirm` sets the resume point to | `reached` answers |
| --- | --- | --- |
| `Before(o)` | `o.confirm_before` | `o.fence_before` |
| `Through(p)`, `p.seq + 1 == next_seq` | the horizon | the horizon |
| `Through(p)`, a later change emitted | `p.confirm_through` | `p.confirm_through` |
| `Idle`, `next_seq == FIRST_SEQ` (nothing ever emitted) | the horizon | the horizon |
| `Idle`, something emitted | unchanged | the confirmed LSN |

`confirm` keeps `max(confirmed_lsn, target)`, so it is monotonic, never below `start_lsn`, and wakes
`confirmed_moved` only when it moved (`ack.rs:222`, unchanged). `reached` takes the max with the
confirmed LSN where it answers the horizon, as today (`ack.rs:115`).

**Why each guard still holds:**

- *Confirmation stops at the end of the last covered commit.* `Before(o)` confirms
  `o.confirm_before`, the end of a commit fully emitted before `o`'s, or WAL read with nothing
  buffered before `o`'s `Begin`. Both are at or before the start of `o`'s commit record, so Postgres
  never skips `o`'s transaction. `Through(p)` with a later change emitted confirms
  `p.confirm_through`: `p`'s own commit end only if `p` was that commit's last change.
- *A split transaction.* Its changes share `confirm_before`; only the last one's `confirm_through`
  is the commit. `Before(second)` → the commit before; `Through(first)` with `second` emitted →
  `first.confirm_through` = the commit before. So nothing confirms or fences the commit until its
  last change has landed (`a_split_transaction_is_not_confirmed_until_its_last_change`,
  `reached_holds_a_split_transaction_back`).
- *A fence stops strictly below the next outstanding change.* `Before(o)` → `o`'s commit − 1, as
  today's `next_outstanding(..).saturating_sub(1)` (`ack.rs:113-118`). Lower than today in one case:
  `Through(p)` with a later change emitted but not yet routed answers `p.confirm_through`, not
  "short of the next commit", because the engine no longer reports which emitted change is next.
  Under-claiming only delays a waiter until that change is routed and landed.
- *The horizon with nothing buffered (#111, #204, #205).* Keepalives and filtered commits still call
  `read_through` only while `open_txn` and `pending` are empty (`stream.rs:813-835`, unchanged).
  `read_through` no longer confirms by itself (`ack.rs:186-188` goes): the engine's one-second tick
  always calls `confirm`, and with nothing outstanding that is `Through(p)` → horizon, or `Idle` →
  horizon in a process that never emitted. Idle WAL is released within a tick plus the client's
  status interval: the same as today's `CONFIRM_BOUND`.
- *An emitted change not yet routed.* It is in no lane, so it is not outstanding; the `next_seq`
  test keeps `Through(p)` from passing it, and `Idle` stops following the horizon once anything was
  emitted (a fresh process can emit change 0 with the horizon past it before the engine routes it).
- *Monotonic across a reopened stream.* `Positions::new(start_lsn, first_seq)` still continues
  `next_seq` from the previous stream (`capture.rs:497-510`), so a `Landed` built from the previous
  stream's positions can't name a change of the new one, and the `next_seq` test stays exact across
  an ingest restart. A `Through(p)` from the old stream may confirm `p.confirm_through` on the new
  one: every change through `p` landed, so that is safe (today's map refused it only because it had
  forgotten `p`). `confirm`'s max keeps the resume point from going back to the new stream's start.

## 3. `Progress` (engine, `libs/2-engine/src/progress.rs`)

One per deployment, created by the daemon, handed to the ingest engine, every sink engine and the
fence. Outlives every task, like `Status`.

```text
Progress
├─ routed: Mutex<Option<Position>>          newest published position     written: ingest
└─ lanes: BTreeMap<SinkName, Lane>          fixed at construction
     Lane
     ├─ registered: Mutex<Registered>        written: ingest, at publish
     │    unlanded:     VecDeque<(key, oldest)>   in publish (= position) order
     │    unsearchable: VecDeque<(key, oldest)>
     └─ reports: Mutex<Reports>              written: that lane's sink engine
          landed:              BTreeSet<key>   items whose write returned and was accepted
          searchable_through:  Option<key>     in lane order, from the sink's `Fences`
```

- **Publish** (ingest, once per batch): after every lane accepted the item, lock each lane's
  `registered` and push `(key, oldest)` onto both queues, then set `routed = last`. Registering
  *after* the items are on the lanes is safe: until then `routed` is older and the `next_seq` test
  keeps `Through(routed)` short of the new changes. A sink that lands an item before it is
  registered leaves its key in `landed`, where the registration finds it.
- **Landed / searchable** (sink engine, once per item, one lock): `write_returned` inserts the key
  into `landed` and raises `searchable_through`, in one call. The timer that makes positions
  searchable later takes the same lock once.
- **Reading a case** (ingest confirm per commit and tick, the fence on demand): lock `routed` and
  copy it *first*, then each lane: lock `registered`, then `reports`; pop `unlanded` fronts found in
  `landed` and `unsearchable` fronts at or below `searchable_through`; drop `landed` keys that can no
  longer match (below the front, or at or below the newest registered key when the queue is empty).
  Reading `routed` before the lanes is what makes `Through` safe: a batch registered after the read
  can't be counted as landed under the older `routed`.
- **The case** over a set of lanes: the minimum `oldest` of the fronts → `Before(o)`; none and a
  `routed` → `Through(routed)`; neither → `Idle`. Confirmation takes it over every lane at the
  landed level; a fence over one lane (or all) at the landed (*applied*) or searchable level. The
  searchable front is the lower of the two queues' fronts.

**Contention.** The ingest engine's publish touches `registered` and `routed`; a sink engine's report
touches only its lane's `reports`. They never take the same lock. Both contend only with a reader,
and the regular reader (confirm) runs on the ingest task itself.

**Per-item lock / atomic counts** (Mutex lock = 1; `Notify::notify_waiters` = 1, it locks):

| Path | Today | Stage 6 |
| --- | ---: | ---: |
| capture, per emitted change | 1 (`register`) | 1 (stamp, same lock) |
| ingest publish, per batch | 0 beyond the channel | lanes + 1 (`registered` per lane, `routed`) |
| ingest confirm, per commit / tick | lanes (`watermark`: a ledger lock each) + 2 (`open_positions`, `Positions`) | 1 + 2 × lanes + 2 |
| channel `recv`, per item | 1 ledger (records `before`) | 1 ledger (no position bookkeeping) |
| sink write returned, per item | `Fences` 1 + fence `record_applied` (1 + 2 × `reached` at 2 each) + `notify` 1 + `record_searchable` 1 + `notify` 1 = 9 | `Fences` 1 + `reports` 1 + `notify` 1 (+1 if searchable moved) = 3–4 |
| channel ack, per item | 1 ledger | 1 ledger |

The fence observer no longer translates a position per item: it only wakes waiters, and a waiter
computes its case when it checks.

## 4. Fences (daemon, `libs/3-daemon/src/fence.rs`)

- `Fence::new(source, progress, sinks)`. `reached(sink, level)` = `source.reached(progress.case(
  lane, level))`, kept as the max ever seen per sink and level, so a reopened stream never walks a
  sink back (today's `applied_reached` / `searchable_reached`, `fence.rs:186-200`).
- `record_applied` / `record_searchable` and the `unsearchable` translation queue go: the position
  carries its own stamps, so nothing has to be translated "while the source still holds it".
- `FenceObserver::on_position_applied` / `on_position_searchable` only wake waiters. The events keep
  their meaning (in-order, per sink) for other observers.
- Searchable with nothing unsearchable follows the applied case, as today (`fence.rs:268-273`).

## 5. Callers

| Caller | Change |
| --- | --- |
| `kernel::Position` constructors (≈150, mostly tests) | `Position::new(n)`, `.seq` — one mechanical commit |
| `source::cdc::ChangeCapture` | `confirm(Landed)`, `reached(Landed)`; `Landed` beside `SourcePosition` |
| Postgres `ack.rs`, `stream.rs` (`next_change`), `capture.rs` (`confirm`, `reached`) | §2 |
| mock captures (engine, daemon, CLI tests) | the new signatures; mocks answer from `Landed` |
| `stream::Stream::watermark` | removed; `is_idle` stays (the drain still waits for snapshot items) until stage 7 |
| `stream-channel` | the ledger's `delivered` / `before` go; tests of the watermark move to `Progress` |
| `IngestEngine` | `with_progress`; registers at publish (`commit.rs`, after `publish`); `confirm.rs` confirms `progress.landed()` after each commit (skipping a value already sent) and **always** on the tick; `advances` compares `reached(landed)` |
| `engine::drain_lanes` | takes `&Progress`; confirms as it moves, always once more at the end |
| `SinkEngine` | `with_progress`; `write_returned` reports landed + searchable; `report_searchable` too |
| daemon `lib.rs` / `supervise.rs` | one `Progress`, handed to both engines, the fence and the drain |

## 6. For stage 7

- A lane key becomes `(index, sink)`; `Progress::new` takes the set of lanes, `published` the lanes
  the batch went to. A lane a batch skips has nothing outstanding for it, so it passes.
- With more than one item per lane sharing an `oldest`, the queues already allow it (they hold
  `(key, oldest)` and pop by key); keys must stay unique per lane.
- `Stream::is_idle` can move to `Progress` once snapshot items are tracked there too.
