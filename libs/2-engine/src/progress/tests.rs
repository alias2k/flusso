use std::sync::atomic::AtomicBool;
use std::thread;

use super::*;
use source::document::DocumentId;

fn sink(name: &str) -> LaneKey {
    LaneKey::new(
        kernel::IndexName::try_new("users").unwrap(),
        kernel::SinkName::try_new(name).unwrap(),
    )
}

fn p(seq: u64) -> Position {
    Position::new(seq)
}

/// Register a batch on each of `lanes`, as each lane's send succeeds.
fn publish(progress: &Progress, lanes: &[&LaneKey], oldest: u64, last: u64) {
    for lane in lanes {
        progress.published(lane, ItemKey::new(last), p(oldest), p(last));
    }
}

#[test]
fn nothing_routed_is_idle() {
    let progress = Progress::new([sink("a")]);
    assert_eq!(progress.landed(), Landed::Idle);
    assert_eq!(
        progress.lane(&sink("a"), Level::Applied),
        Some(Landed::Idle)
    );
    assert_eq!(
        progress.lane(&sink("a"), Level::Searchable),
        Some(Landed::Idle)
    );
    assert_eq!(progress.lane(&sink("unknown"), Level::Applied), None);
}

#[test]
fn a_routed_item_holds_everything_from_its_oldest_change() {
    let a = sink("a");
    let progress = Progress::new([a.clone()]);
    progress.published(&a, ItemKey::new(3), p(1), p(3));
    assert_eq!(progress.landed(), Landed::Before(p(1)));

    progress.applied(&a, ItemKey::new(3));
    assert_eq!(progress.landed(), Landed::Through(p(3)));
}

/// Two items of one lane that share their newest position (stage 8a's
/// takes skip ids still building, so a later item can end on the same
/// change) are landed one at a time: landing the first leaves the second
/// outstanding.
#[test]
fn two_items_sharing_their_newest_position_land_one_at_a_time() {
    let a = sink("a");
    let progress = Progress::new([a.clone()]);
    let first = progress.next_key(&a.index).unwrap();
    let second = progress.next_key(&a.index).unwrap();
    progress.published(&a, first, p(1), p(5));
    progress.published(&a, second, p(3), p(5));

    progress.applied(&a, first);
    assert_eq!(
        progress.landed(),
        Landed::Before(p(3)),
        "only the first item landed; the second still holds its oldest change"
    );
}

/// Each index numbers its items on its own, from zero, one more each time,
/// never again for the life of the progress; an index with no lane has none.
#[test]
fn keys_number_each_indexs_items_on_their_own() {
    let users = sink("a");
    let orders = LaneKey::new(
        kernel::IndexName::try_new("orders").unwrap(),
        kernel::SinkName::try_new("a").unwrap(),
    );
    let progress = Progress::new([users.clone(), orders.clone()]);
    assert_eq!(progress.next_key(&users.index), Some(ItemKey::new(0)));
    assert_eq!(progress.next_key(&users.index), Some(ItemKey::new(1)));
    assert_eq!(
        progress.next_key(&orders.index),
        Some(ItemKey::new(0)),
        "another index counts on its own"
    );
    assert_eq!(progress.next_key(&users.index), Some(ItemKey::new(2)));
    assert_eq!(
        progress.next_key(&kernel::IndexName::try_new("unknown").unwrap()),
        None
    );
}

#[test]
fn a_mark_covers_every_item_through_it_and_the_oldest_uncovered_decides() {
    let a = sink("a");
    let progress = Progress::new([a.clone()]);
    publish(&progress, &[&a], 1, 2);
    publish(&progress, &[&a], 3, 4);
    publish(&progress, &[&a], 5, 6);

    progress.applied(&a, ItemKey::new(2));
    assert_eq!(progress.landed(), Landed::Before(p(3)));
    progress.applied(&a, ItemKey::new(6));
    assert_eq!(progress.landed(), Landed::Through(p(6)));
}

#[test]
fn a_stalled_lane_pins_confirmation_and_its_own_fence_only() {
    let (fast, stalled) = (sink("fast"), sink("stalled"));
    let progress = Progress::new([fast.clone(), stalled.clone()]);
    publish(&progress, &[&fast, &stalled], 1, 2);
    publish(&progress, &[&fast, &stalled], 3, 4);
    progress.applied(&fast, ItemKey::new(4));
    progress.searchable(&fast, ItemKey::new(4));

    assert_eq!(progress.landed(), Landed::Before(p(1)));
    assert_eq!(
        progress.lane(&fast, Level::Applied),
        Some(Landed::Through(p(4)))
    );
    assert_eq!(
        progress.lane(&fast, Level::Searchable),
        Some(Landed::Through(p(4)))
    );
    assert_eq!(
        progress.lane(&stalled, Level::Applied),
        Some(Landed::Before(p(1)))
    );
    assert!(progress.is_clear(&fast, Level::Applied));
    assert!(!progress.is_clear(&stalled, Level::Applied));
}

/// A full sibling lane: the batch is registered on the healthy lane as its
/// send succeeds, and the healthy lane reaches it, while the sibling, which
/// hasn't taken it, holds confirmation back at what it has.
#[test]
fn a_healthy_lane_reaches_what_it_landed_while_a_sibling_lane_is_full() {
    let (healthy, full) = (sink("healthy"), sink("full"));
    let progress = Progress::new([healthy.clone(), full.clone()]);
    publish(&progress, &[&healthy, &full], 1, 1);
    progress.applied(&healthy, ItemKey::new(1));
    progress.applied(&full, ItemKey::new(1));
    publish(&progress, &[&healthy], 2, 2);
    progress.applied(&healthy, ItemKey::new(2));

    assert_eq!(
        progress.lane(&healthy, Level::Applied),
        Some(Landed::Through(p(2)))
    );
    assert_eq!(
        progress.lane(&full, Level::Applied),
        Some(Landed::Through(p(1))),
        "the batch isn't registered on it: it answers what it holds"
    );
    assert_eq!(
        progress.landed(),
        Landed::Through(p(1)),
        "confirmation follows the lane that covers least"
    );
}

#[test]
fn a_skipped_lane_pins_nothing() {
    let (busy, idle) = (sink("busy"), sink("idle"));
    let progress = Progress::new([busy.clone(), idle.clone()]);
    progress.published(&busy, ItemKey::new(2), p(1), p(2));
    progress.skipped(&idle, p(2));
    assert_eq!(progress.landed(), Landed::Before(p(1)));
    assert_eq!(
        progress.lane(&idle, Level::Applied),
        Some(Landed::Through(p(2))),
        "it needs nothing of the batch"
    );
    progress.applied(&busy, ItemKey::new(2));
    assert_eq!(progress.landed(), Landed::Through(p(2)));
}

#[test]
fn a_landing_reported_before_its_registration_still_counts() {
    let a = sink("a");
    let progress = Progress::new([a.clone()]);
    progress.applied(&a, ItemKey::new(2));
    progress.searchable(&a, ItemKey::new(2));
    assert_eq!(
        progress.landed(),
        Landed::Idle,
        "a mark alone claims nothing"
    );
    progress.published(&a, ItemKey::new(2), p(1), p(2));
    assert_eq!(progress.landed(), Landed::Through(p(2)));
    assert_eq!(
        progress.lane(&a, Level::Searchable),
        Some(Landed::Through(p(2)))
    );
}

#[test]
fn searchable_trails_applied() {
    let a = sink("a");
    let progress = Progress::new([a.clone()]);
    publish(&progress, &[&a], 1, 2);
    publish(&progress, &[&a], 3, 4);
    progress.applied(&a, ItemKey::new(4));
    assert_eq!(
        progress.lane(&a, Level::Applied),
        Some(Landed::Through(p(4)))
    );
    assert_eq!(
        progress.lane(&a, Level::Searchable),
        Some(Landed::Before(p(1)))
    );
    progress.searchable(&a, ItemKey::new(2));
    assert_eq!(
        progress.lane(&a, Level::Searchable),
        Some(Landed::Before(p(3)))
    );
    progress.searchable(&a, ItemKey::new(4));
    assert_eq!(
        progress.lane(&a, Level::Searchable),
        Some(Landed::Through(p(4)))
    );
    assert_eq!(
        progress.landed(),
        Landed::Through(p(4)),
        "confirmation never waits on a refresh"
    );
}

/// A redelivered item lands again under a lower mark than one already
/// reported: the mark never moves back.
#[test]
fn a_mark_never_moves_back() {
    let a = sink("a");
    let progress = Progress::new([a.clone()]);
    publish(&progress, &[&a], 1, 2);
    publish(&progress, &[&a], 3, 4);
    progress.applied(&a, ItemKey::new(4));
    progress.applied(&a, ItemKey::new(2));
    assert_eq!(progress.landed(), Landed::Through(p(4)));
    publish(&progress, &[&a], 5, 6);
    assert_eq!(progress.landed(), Landed::Before(p(5)));
}

/// Several lanes take the least answer.
#[test]
fn several_lanes_answer_their_least_and_an_unknown_lane_none() {
    let (a, b) = (sink("a"), sink("b"));
    let progress = Progress::new([a.clone(), b.clone()]);
    publish(&progress, &[&a, &b], 1, 1);
    publish(&progress, &[&a, &b], 2, 3);
    progress.applied(&a, ItemKey::new(3));
    progress.applied(&b, ItemKey::new(1));
    assert_eq!(
        progress.over([&a, &b], Level::Applied),
        Some(Landed::Before(p(2)))
    );
    assert_eq!(
        progress.over([&a], Level::Applied),
        Some(Landed::Through(p(3)))
    );
    assert_eq!(progress.over([&a, &sink("c")], Level::Applied), None);
    assert_eq!(progress.over(std::iter::empty(), Level::Applied), None);
    assert_eq!(progress.outstanding(&b), 1);
    assert_eq!(progress.outstanding(&a), 0);
}

#[test]
fn every_registration_calls_the_listener() {
    let a = sink("a");
    let progress = Progress::new([a.clone()]);
    let calls = Arc::new(AtomicU64::new(0));
    let counted = Arc::clone(&calls);
    progress.on_registered(Arc::new(move || {
        counted.fetch_add(1, Ordering::SeqCst);
    }));
    progress.published(&a, ItemKey::new(1), p(1), p(1));
    progress.skipped(&a, p(2));
    assert_eq!(calls.load(Ordering::SeqCst), 2);
}

/// A tiny deterministic generator, so a failing seed replays.
struct Rng(u64);

impl Rng {
    fn below(&mut self, n: usize) -> usize {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        usize::try_from(self.0 % n as u64).unwrap()
    }
}

#[derive(Debug)]
struct Batch {
    oldest: u64,
    last: u64,
    /// Which lanes get an item (the others are skipped).
    lanes: Vec<usize>,
}

/// The seq just past what an answer covers.
fn reach(landed: Landed) -> u64 {
    match landed {
        Landed::Before(oldest) => oldest.seq,
        Landed::Through(last) => last.seq + 1,
        Landed::Idle => 0,
    }
}

/// Random interleavings of registering (per lane, in batch order per lane:
/// an item or a skip), and marks moving on each lane (also past items not
/// registered yet), against a model: every answer must equal the model's.
#[test]
fn every_interleaving_answers_as_the_model_does() {
    let names: Vec<LaneKey> = ["a", "b", "c"].iter().map(|n| sink(n)).collect();
    for seed in 1..=300u64 {
        let mut rng = Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1);
        let lanes = 1 + rng.below(3);
        let progress = Progress::new(names.iter().take(lanes).cloned());
        let mut batches: Vec<Batch> = Vec::new();
        // Per lane: how many batches are registered on it, and how many of
        // its items are written and searchable, in lane order.
        let mut registered = vec![0usize; lanes];
        let mut applied = vec![0usize; lanes];
        let mut searchable = vec![0usize; lanes];
        let mut next = 1u64;
        for step in 0..200 {
            let items = |batches: &[Batch], l: usize| -> Vec<u64> {
                batches
                    .iter()
                    .filter(|b| b.lanes.contains(&l))
                    .map(|b| b.last)
                    .collect()
            };
            match rng.below(5) {
                0 => {
                    let size = 1 + rng.below(3) as u64;
                    let on: Vec<usize> = (0..lanes).filter(|_| rng.below(4) != 0).collect();
                    batches.push(Batch {
                        oldest: next,
                        last: next + size - 1,
                        lanes: on,
                    });
                    next += size;
                }
                1 => {
                    let l = rng.below(lanes);
                    if let Some(batch) = batches.get(registered[l]) {
                        if batch.lanes.contains(&l) {
                            progress.published(
                                &names[l],
                                ItemKey::new(batch.last),
                                p(batch.oldest),
                                p(batch.last),
                            );
                        } else {
                            progress.skipped(&names[l], p(batch.last));
                        }
                        registered[l] += 1;
                    }
                }
                2 | 3 => {
                    let l = rng.below(lanes);
                    let keys = items(&batches, l);
                    if applied[l] < keys.len() {
                        applied[l] += 1 + rng.below(keys.len() - applied[l]);
                        progress.applied(&names[l], ItemKey::new(keys[applied[l] - 1]));
                    }
                }
                _ => {
                    let l = rng.below(lanes);
                    let keys = items(&batches, l);
                    if searchable[l] < applied[l] {
                        searchable[l] += 1 + rng.below(applied[l] - searchable[l]);
                        progress.searchable(&names[l], ItemKey::new(keys[searchable[l] - 1]));
                    }
                }
            }

            let answer = |l: usize, level: Level| -> Landed {
                let done = match level {
                    Level::Applied => applied[l],
                    Level::Searchable => searchable[l].min(applied[l]),
                };
                let mine: Vec<&Batch> = batches[..registered[l]]
                    .iter()
                    .filter(|b| b.lanes.contains(&l))
                    .collect();
                match mine.get(done) {
                    Some(batch) => Landed::Before(p(batch.oldest)),
                    None => batches[..registered[l]]
                        .last()
                        .map_or(Landed::Idle, |b| Landed::Through(p(b.last))),
                }
            };
            for (l, name) in names.iter().enumerate().take(lanes) {
                for level in [Level::Applied, Level::Searchable] {
                    assert_eq!(
                        progress.lane(name, level),
                        Some(answer(l, level)),
                        "seed {seed}, step {step}, lane {name}, {level:?}: {batches:?}"
                    );
                }
            }
            let least = (0..lanes)
                .map(|l| reach(answer(l, Level::Applied)))
                .min()
                .unwrap_or(0);
            assert_eq!(
                reach(progress.landed()),
                least,
                "seed {seed}, step {step}: {batches:?}, registered {registered:?}, applied {applied:?}"
            );
        }
    }
}

/// Threads: one ingest registering every batch on every lane in a random
/// lane order (skipping some), one sink per lane landing its items in order,
/// and readers asking `landed()` all along. Whatever an answer covers, every
/// lane had landed by then: it is checked right after against what each sink
/// says it landed (which only grows, so the check can only miss a bug, never
/// invent one).
#[test]
fn concurrent_registration_and_landing_never_answer_past_a_lane() {
    const LANES: usize = 3;
    const BATCHES: u64 = 3_000;
    let names: Vec<LaneKey> = (0..LANES).map(|l| sink(&format!("s{l}"))).collect();
    let progress = Arc::new(Progress::new(names.iter().cloned()));
    // Per lane: the queue of items sent to it, and the newest seq its sink
    // landed with everything the lane needs before it (`u64::MAX`: none).
    let queues: Arc<Vec<Mutex<VecDeque<u64>>>> =
        Arc::new((0..LANES).map(|_| Mutex::new(VecDeque::new())).collect());
    let landed: Arc<Vec<AtomicU64>> = Arc::new((0..LANES).map(|_| AtomicU64::new(0)).collect());
    let done = Arc::new(AtomicBool::new(false));

    let ingest = {
        let (progress, queues, names) = (Arc::clone(&progress), Arc::clone(&queues), names.clone());
        thread::spawn(move || {
            let mut rng = Rng(0x5EED);
            for seq in 0..BATCHES {
                let start = rng.below(LANES);
                for offset in 0..LANES {
                    let l = (start + offset) % LANES;
                    if rng.below(5) == 0 {
                        progress.skipped(&names[l], p(seq));
                    } else {
                        queues[l].lock().unwrap().push_back(seq);
                        progress.published(&names[l], ItemKey::new(seq), p(seq), p(seq));
                    }
                }
            }
        })
    };
    let sinks: Vec<_> = (0..LANES)
        .map(|l| {
            let (progress, queues, landed, done, name) = (
                Arc::clone(&progress),
                Arc::clone(&queues),
                Arc::clone(&landed),
                Arc::clone(&done),
                names[l].clone(),
            );
            thread::spawn(move || {
                while !done.load(Ordering::Acquire) {
                    let item = queues[l].lock().unwrap().pop_front();
                    match item {
                        Some(seq) => {
                            landed[l].store(seq + 1, Ordering::Release);
                            progress.applied(&name, ItemKey::new(seq));
                        }
                        None => thread::yield_now(),
                    }
                }
            })
        })
        .collect();
    let readers: Vec<_> = (0..2)
        .map(|_| {
            let (progress, queues, landed, done) = (
                Arc::clone(&progress),
                Arc::clone(&queues),
                Arc::clone(&landed),
                Arc::clone(&done),
            );
            thread::spawn(move || {
                while !done.load(Ordering::Acquire) {
                    let covered = reach(progress.landed());
                    for l in 0..LANES {
                        // Every item of the lane below `covered` was landed.
                        let queued = queues[l].lock().unwrap().front().copied();
                        if let Some(waiting) = queued {
                            assert!(
                                waiting >= covered || landed[l].load(Ordering::Acquire) > waiting,
                                "an answer covering {covered} passed lane {l}'s item {waiting}"
                            );
                        }
                    }
                }
            })
        })
        .collect();

    ingest.join().unwrap();
    while queues.iter().any(|queue| !queue.lock().unwrap().is_empty()) {
        thread::yield_now();
    }
    done.store(true, Ordering::Release);
    for handle in sinks.into_iter().chain(readers) {
        handle.join().unwrap();
    }
    assert_eq!(progress.landed(), Landed::Through(p(BATCHES - 1)));
}

fn id(index: &str, n: i64) -> DocumentId {
    DocumentId {
        index: kernel::IndexName::try_new(index).unwrap(),
        key: source::RowKey(vec![(
            kernel::ColumnName::try_new("id").unwrap(),
            kernel::GenericValue::BigInt(n),
        )]),
    }
}

fn nothing_held(_: &source::RowKey) -> bool {
    false
}

/// A document marked pending holds its lanes before its change while it
/// waits and while it builds; once its item is registered the item holds
/// them, and once the item lands nothing does.
#[test]
fn a_marked_id_pins_confirmation_until_its_build_lands() {
    let a = sink("a");
    let progress = Progress::new([a.clone()]);
    assert_eq!(progress.mark([(id("users", 1), p(3))]), 1);
    assert_eq!(progress.waiting(), 1);
    assert_eq!(progress.landed(), Landed::Before(p(3)));

    let taken = progress.take(&a.index, usize::MAX, &nothing_held).unwrap();
    assert_eq!(progress.waiting(), 0);
    assert_eq!(progress.landed(), Landed::Before(p(3)), "building");

    let key = progress.next_key(&a.index).unwrap();
    progress.published_build(
        &a,
        key,
        taken.oldest,
        p(3),
        taken.token,
        progress.close_step(),
    );
    progress.release(&a.index, taken.token);
    assert_eq!(progress.landed(), Landed::Before(p(3)), "on the lane");
    progress.applied(&a, key);
    assert_eq!(progress.landed(), Landed::Through(p(3)));
}

/// A build stops holding a lane back once its item is registered there: a
/// full sibling lane of the same index holds back only itself.
#[test]
fn a_build_holds_a_lane_only_until_its_item_is_registered_there() {
    let (healthy, full) = (sink("healthy"), sink("full"));
    let progress = Progress::new([healthy.clone(), full.clone()]);
    progress.mark([(id("users", 1), p(1))]);
    let taken = progress
        .take(&healthy.index, usize::MAX, &nothing_held)
        .unwrap();
    let key = progress.next_key(&healthy.index).unwrap();
    progress.published_build(
        &healthy,
        key,
        taken.oldest,
        p(1),
        taken.token,
        progress.close_step(),
    );
    progress.applied(&healthy, key);

    assert_eq!(
        progress.lane(&healthy, Level::Applied),
        Some(Landed::Through(p(1)))
    );
    assert_eq!(
        progress.lane(&full, Level::Applied),
        Some(Landed::Before(p(1))),
        "the item isn't on it yet: the build still holds it"
    );
    assert_eq!(progress.landed(), Landed::Before(p(1)));
}

/// While an index's pending documents are older than every lane's own
/// items, they are what pins the resume point, named by their index; a
/// lane's own oldest item counts only what is on the lane.
#[test]
fn a_backlog_pinning_confirmation_is_named_by_its_index() {
    let users = sink("a");
    let orders = LaneKey::new(
        kernel::IndexName::try_new("orders").unwrap(),
        kernel::SinkName::try_new("a").unwrap(),
    );
    let progress = Progress::new([users.clone(), orders.clone()]);
    progress.published(&orders, ItemKey::new(0), p(5), p(5));
    progress.mark([(id("users", 1), p(2))]);

    assert_eq!(progress.landed(), Landed::Before(p(2)));
    assert_eq!(
        progress.pinned_by(),
        Some(Pin::Backlog(users.index.clone()))
    );
    assert_eq!(progress.own_oldest(&users), None, "nothing on the lane");
    assert_eq!(progress.own_oldest(&orders), Some(p(5)));

    progress.published(&users, ItemKey::new(0), p(2), p(2));
    assert_eq!(
        progress.pinned_by(),
        Some(Pin::Lane(users.clone())),
        "a tie names the lane"
    );
}

/// An id of an index with no lane is dropped: no build would ever publish
/// it, so it would hold confirmation back for good.
#[test]
fn an_id_of_an_index_without_lanes_pins_nothing() {
    let a = sink("a");
    let progress = Progress::new([a.clone()]);
    assert_eq!(progress.mark([(id("ghost", 1), p(1))]), 0);
    assert_eq!(progress.waiting(), 0);
    assert_eq!(progress.landed(), Landed::Idle);
}

/// What a run left behind unpublished is forgotten when the next run
/// starts, every time it is asked: the replay marks it again.
#[test]
fn a_run_forgets_what_an_earlier_run_left_behind() {
    let a = sink("a");
    let progress = Progress::new([a.clone()]);
    progress.mark([(id("users", 1), p(1)), (id("users", 2), p(2))]);
    progress.take(&a.index, usize::MAX, &nothing_held).unwrap();
    progress.mark([(id("users", 3), p(3))]);
    assert_eq!(progress.landed(), Landed::Before(p(1)));
    progress.forget_all();
    progress.forget_all();
    assert_eq!(progress.waiting(), 0);
    assert_eq!(progress.landed(), Landed::Idle);
}

/// Register `taken`'s item on `lane` under `key`: the call the ingest
/// engine makes once that lane's send returned.
fn register(progress: &Progress, lane: &LaneKey, key: ItemKey, taken: &Taken, last: Position) {
    progress.published_build(
        lane,
        key,
        taken.oldest,
        last,
        taken.token,
        progress.close_step(),
    );
}

/// Each answer a registration moves is followed by a notification, so a
/// fence waiter on that lane wakes. Here the sink lands the item before its
/// registration (the ingest task resumed late) and a sibling lane hasn't
/// taken it: the last notification must see the lane's new answer.
#[test]
fn a_registration_that_moves_a_lanes_answer_notifies_after_it() {
    let (a, b) = (sink("a"), sink("b"));
    let progress = Arc::new(Progress::new([a.clone(), b.clone()]));
    let seen: Arc<Mutex<Vec<Option<Landed>>>> = Arc::default();
    let weak = Arc::downgrade(&progress);
    let log = Arc::clone(&seen);
    let lane = a.clone();
    progress.on_registered(Arc::new(move || {
        if let Some(progress) = weak.upgrade() {
            log.lock()
                .unwrap()
                .push(progress.lane(&lane, Level::Applied));
        }
    }));
    progress.mark([(id("users", 1), p(3))]);
    let taken = progress.take(&a.index, usize::MAX, &nothing_held).unwrap();
    let key = progress.next_key(&a.index).unwrap();
    progress.applied(&a, key);
    register(&progress, &a, key, &taken, p(3));

    assert_eq!(
        progress.lane(&a, Level::Applied),
        Some(Landed::Through(p(3)))
    );
    let seen = seen.lock().unwrap().clone();
    assert_eq!(
        seen.last().copied().flatten(),
        Some(Landed::Through(p(3))),
        "a waiter on `a` was woken after its answer moved: {seen:?}"
    );
}

/// A build whose item reached some lanes and waits on another is that
/// lane's to answer for: it pins the slot by that lane and ages it.
#[test]
fn a_build_sent_to_some_lanes_and_blocked_on_one_pins_that_lane() {
    let (a, b) = (sink("a"), sink("b"));
    let progress = Progress::new([a.clone(), b.clone()]);
    progress.mark([(id("users", 1), p(3))]);
    let taken = progress.take(&a.index, usize::MAX, &nothing_held).unwrap();
    assert_eq!(
        progress.pinned_by(),
        Some(Pin::Backlog(a.index.clone())),
        "building: the backlog's"
    );
    progress.build_landed(&a.index, taken.token);
    progress.build_sending(&a.index, taken.token);
    let key = progress.next_key(&a.index).unwrap();
    register(&progress, &a, key, &taken, p(3));
    progress.applied(&a, key);

    assert_eq!(progress.pinned_by(), Some(Pin::Lane(b.clone())));
    assert_eq!(progress.own_oldest(&b), Some(p(3)));
}

/// On a tie across lanes, a lane's own item is named, not another index's
/// backlog, whatever the lanes' order.
#[test]
fn a_lane_wins_a_tie_with_another_indexs_backlog() {
    let users = sink("a");
    let orders = LaneKey::new(
        kernel::IndexName::try_new("orders").unwrap(),
        kernel::SinkName::try_new("a").unwrap(),
    );
    let progress = Progress::new([users.clone(), orders.clone()]);
    progress.published(&users, ItemKey::new(0), p(5), p(5));
    progress.mark([(id("orders", 1), p(5))]);
    assert_eq!(progress.pinned_by(), Some(Pin::Lane(users)));
}

mod committed {
    use super::*;

    fn lane(index: &str, sink: &str) -> LaneKey {
        LaneKey::new(
            kernel::IndexName::try_new(index).unwrap(),
            kernel::SinkName::try_new(sink).unwrap(),
        )
    }

    fn name(sink: &str) -> SinkName {
        SinkName::try_new(sink).unwrap()
    }

    /// Read `changes` and close their step.
    fn read(progress: &Progress, changes: u64) -> Step {
        for _ in 0..changes {
            progress.change_read();
        }
        progress.close_step()
    }

    /// An item of `step` on `lane`, registered; its key.
    fn route(progress: &Progress, lane: &LaneKey, step: Step) -> ItemKey {
        let key = progress.next_key(&lane.index).unwrap();
        progress.published_step(lane, key, p(1), p(1), step);
        key
    }

    #[test]
    fn nothing_read_is_nothing_committed() {
        let progress = Progress::new([lane("users", "a")]);
        assert_eq!(progress.committed(&name("a")), 0);
        assert_eq!(progress.changes_read(), 0);
    }

    /// A change counts once every lane of the sink has landed its item; a
    /// sibling sink's lanes don't hold it.
    #[test]
    fn a_change_counts_once_every_lane_of_its_sink_landed_it() {
        let (users_a, orders_a, users_b) =
            (lane("users", "a"), lane("orders", "a"), lane("users", "b"));
        let progress = Progress::new([users_a.clone(), orders_a.clone(), users_b.clone()]);
        let step = read(&progress, 2);
        let users = route(&progress, &users_a, step);
        progress.published_step(&users_b, users, p(1), p(2), step);
        let orders = route(&progress, &orders_a, step);

        progress.applied(&orders_a, orders);
        progress.applied(&users_b, users);
        assert_eq!(
            progress.committed(&name("a")),
            0,
            "users hasn't landed on a"
        );
        assert_eq!(progress.committed(&name("b")), 2);

        progress.applied(&users_a, users);
        assert_eq!(progress.committed(&name("a")), 2);
    }

    /// A step a lane had nothing for counts once the lane's earlier items
    /// landed, and a lane still behind an item holds its sink's count for
    /// every later step, on every index.
    #[test]
    fn a_skipped_step_counts_after_the_lanes_earlier_items() {
        let (users, orders) = (lane("users", "a"), lane("orders", "a"));
        let progress = Progress::new([users.clone(), orders.clone()]);
        let first = read(&progress, 1);
        let held = route(&progress, &users, first);
        progress.skipped_step(&orders, p(1), first);
        let second = read(&progress, 1);
        progress.skipped_step(&users, p(2), second);
        let landed = route(&progress, &orders, second);
        progress.applied(&orders, landed);
        assert_eq!(progress.committed(&name("a")), 0);

        progress.applied(&users, held);
        assert_eq!(progress.committed(&name("a")), 2);
    }

    /// A redelivered item that had landed raises no mark further: it counts
    /// once.
    #[test]
    fn a_redelivered_item_counts_once() {
        let a = lane("users", "a");
        let progress = Progress::new([a.clone()]);
        let key = route(&progress, &a, read(&progress, 1));
        progress.applied(&a, key);
        progress.applied(&a, key);
        assert_eq!(progress.committed(&name("a")), 1);
    }

    /// A source that replays under the positions it had: the first run's
    /// read and its replay are two reads. A sink that landed the first read
    /// counts it then and the replay when that lands; a sink that hadn't
    /// counts the first read once its own item lands, never before.
    #[test]
    fn a_replayed_change_counts_once_per_read_on_each_sink() {
        let (on_a, on_b) = (lane("users", "a"), lane("users", "b"));
        let progress = Progress::new([on_a.clone(), on_b.clone()]);
        let first = read(&progress, 1);
        let old = route(&progress, &on_a, first);
        progress.published_step(&on_b, old, p(1), p(1), first);
        progress.applied(&on_a, old);
        assert_eq!(progress.committed(&name("a")), 1);
        assert_eq!(progress.committed(&name("b")), 0);

        // The engine run ends; the next one replays the change.
        progress.forget_all();
        let replay = read(&progress, 1);
        let new = route(&progress, &on_a, replay);
        progress.published_step(&on_b, new, p(1), p(1), replay);
        assert_eq!(
            progress.committed(&name("a")),
            1,
            "the replay hasn't landed"
        );
        assert_eq!(progress.committed(&name("b")), 0);

        progress.applied(&on_b, old);
        assert_eq!(progress.committed(&name("b")), 1, "b's first read landed");
        progress.applied(&on_a, new);
        progress.applied(&on_b, new);
        assert_eq!(progress.committed(&name("a")), 2);
        assert_eq!(progress.committed(&name("b")), 2);
        assert_eq!(progress.changes_read(), 2);
    }

    /// Reads an engine run never granted close a step at the next run's
    /// start that no lane is routed: they count with the next step routed,
    /// once it lands.
    #[test]
    fn reads_of_an_ended_run_count_with_the_next_step_routed() {
        let a = lane("users", "a");
        let progress = Progress::new([a.clone()]);
        progress.change_read();
        progress.change_read();
        progress.forget_all();
        progress.forget_all();
        let replay = read(&progress, 2);
        let key = route(&progress, &a, replay);
        assert_eq!(progress.committed(&name("a")), 0);

        progress.applied(&a, key);
        assert_eq!(progress.committed(&name("a")), 4);
    }

    /// A sink with no lane has every closed step.
    #[test]
    fn a_sink_with_no_lane_has_every_closed_step() {
        let progress = Progress::new([lane("users", "a")]);
        read(&progress, 3);
        progress.change_read();
        assert_eq!(progress.committed(&name("elsewhere")), 3);
    }

    /// Steps behind every lane are dropped as new ones close, so the record
    /// stays as long as the slowest lane is behind, not as the run is long.
    #[test]
    fn steps_every_lane_passed_are_dropped() {
        let a = lane("users", "a");
        let progress = Progress::new([a.clone()]);
        for _ in 0..100 {
            let key = route(&progress, &a, read(&progress, 1));
            progress.applied(&a, key);
        }
        read(&progress, 1);
        assert!(lock(&progress.registry).steps.closed.len() <= 2);
        assert_eq!(progress.committed(&name("a")), 100);
    }
}
