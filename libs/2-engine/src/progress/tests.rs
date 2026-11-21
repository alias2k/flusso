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

    /// Steps a lane has passed are dropped as new ones are routed, so its
    /// record stays as long as it is behind, not as the run is long.
    #[test]
    fn steps_every_lane_passed_are_dropped() {
        let a = lane("users", "a");
        let progress = Progress::new([a.clone()]);
        for _ in 0..100 {
            let key = route(&progress, &a, read(&progress, 1));
            progress.applied(&a, key);
        }
        read(&progress, 1);
        assert!(progress.steps_kept(&a).unwrap().0 <= 2);
        // The last read built nothing and everything before it landed: it
        // counts (the catch-up, stage 8a slice 6b-1a).
        assert_eq!(progress.committed(&name("a")), 101);
    }
}

mod parked {
    use super::*;

    fn lane(index: &str) -> LaneKey {
        LaneKey::new(
            kernel::IndexName::try_new(index).unwrap(),
            kernel::SinkName::try_new("a").unwrap(),
        )
    }

    /// What the bound says a lane may keep: its unlanded items, plus two.
    fn within_bound(progress: &Progress, lane: &LaneKey) -> bool {
        let (kept, unlanded) = progress.steps_kept(lane).unwrap();
        kept <= unlanded + 2
    }

    /// `users` holds `parked` unlanded items (the first on the first step
    /// when `from_the_start`), then `grants` live grants pass it with
    /// documents for `orders` only, each landing at once. Every lane keeps
    /// a record bounded by its unlanded items, and once `users` lands the
    /// sink has every read.
    fn grants_past_a_parked_lane(parked: usize, from_the_start: bool, grants: u64) {
        let (users, orders) = (lane("users"), lane("orders"));
        let progress = Progress::new([users.clone(), orders.clone()]);
        let sink = SinkName::try_new("a").unwrap();
        let mut read = 0;
        let mut grant = |progress: &Progress, users_item: bool| {
            progress.change_read();
            read += 1;
            let step = progress.close_step();
            if users_item {
                let key = progress.next_key(&users.index).unwrap();
                progress.published_step(&users, key, p(read), p(read), step);
                progress.skipped_step(&orders, p(read), step);
            } else {
                let key = progress.next_key(&orders.index).unwrap();
                progress.published_step(&orders, key, p(read), p(read), step);
                progress.skipped_step(&users, p(read), step);
                progress.applied(&orders, key);
            }
        };
        if !from_the_start {
            grant(&progress, false);
        }
        for _ in 0..parked {
            grant(&progress, true);
            grant(&progress, false);
        }
        for _ in 0..grants {
            grant(&progress, false);
            assert!(
                within_bound(&progress, &users),
                "users keeps {:?}",
                progress.steps_kept(&users)
            );
            assert!(
                within_bound(&progress, &orders),
                "orders keeps {:?}",
                progress.steps_kept(&orders)
            );
        }
        assert_eq!(progress.outstanding(&users), parked);
        assert_eq!(progress.committed(&sink), u64::from(!from_the_start));

        let last = progress.next_key(&users.index).unwrap();
        progress.applied(&users, last);
        assert_eq!(progress.committed(&sink), progress.changes_read());
        assert!(within_bound(&progress, &users));
    }

    #[test]
    fn a_lane_parked_on_its_first_item_keeps_a_bounded_record() {
        grants_past_a_parked_lane(1, true, 10_000);
    }

    #[test]
    fn a_lane_parked_after_landing_keeps_a_bounded_record() {
        grants_past_a_parked_lane(1, false, 10_000);
    }

    #[test]
    fn a_lane_with_several_unlanded_items_keeps_twice_as_many() {
        grants_past_a_parked_lane(5, false, 10_000);
    }
}

/// The compacted record answers as a naive one that keeps every step: random
/// grants over two indexes on two sinks, each lane getting an item or a skip,
/// items landing in key order at random, an engine run ending now and then
/// with reads never granted. After every operation each sink's count equals
/// the model's, and each lane keeps within its bound. The model has the
/// frontier's rules (stage 8a slice 6b-1a): these reads mark nothing, so a
/// lane with nothing unlanded has every read (its catch-up), and a lane's
/// count never goes down.
#[test]
fn compaction_changes_no_count() {
    let sinks = ["a", "b"];
    let indexes = ["users", "orders"];
    let lanes: Vec<LaneKey> = indexes
        .iter()
        .flat_map(|index| {
            sinks.iter().map(move |sink| {
                LaneKey::new(
                    kernel::IndexName::try_new(*index).unwrap(),
                    SinkName::try_new(*sink).unwrap(),
                )
            })
        })
        .collect();
    for seed in 0..40_u64 {
        let mut state = seed
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        let mut next = |below: u64| {
            state = state
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            (state >> 33) % below
        };
        let progress = Progress::new(lanes.clone());
        // Per lane: every step routed (read total, item key) and how many
        // items have landed.
        let mut routed: Vec<Vec<(u64, Option<u64>)>> = vec![Vec::new(); lanes.len()];
        let mut keys: Vec<Vec<ItemKey>> = vec![Vec::new(); lanes.len()];
        let mut landed: Vec<usize> = vec![0; lanes.len()];
        let mut most: Vec<u64> = vec![0; lanes.len()];
        for _ in 0..400 {
            match next(10) {
                0 => {
                    progress.change_read();
                    progress.forget_all();
                }
                1..=5 => {
                    for _ in 0..next(3) {
                        progress.change_read();
                    }
                    let step = progress.close_step();
                    let read = progress.changes_read();
                    for index in &indexes {
                        let item = next(3) == 0;
                        let key = item.then(|| {
                            progress
                                .next_key(&kernel::IndexName::try_new(*index).unwrap())
                                .unwrap()
                        });
                        for (at, lane) in lanes.iter().enumerate() {
                            if lane.index.as_ref() != *index {
                                continue;
                            }
                            match key {
                                Some(key) => {
                                    progress.published_step(lane, key, p(read), p(read), step);
                                    keys[at].push(key);
                                    routed[at].push((read, Some(key.number())));
                                }
                                None => {
                                    progress.skipped_step(lane, p(read), step);
                                    routed[at].push((read, None));
                                }
                            }
                        }
                    }
                }
                _ => {
                    let at = next(lanes.len() as u64) as usize;
                    let lane = &lanes[at];
                    let left = keys[at].len() - landed[at];
                    if left > 0 {
                        landed[at] += 1 + next(left as u64) as usize;
                        progress.applied(lane, keys[at][landed[at] - 1]);
                    }
                }
            }
            for sink in sinks {
                let model = lanes
                    .iter()
                    .enumerate()
                    .filter(|(_, lane)| lane.sink.as_ref() == sink)
                    .map(|(at, _)| {
                        let first_unlanded = keys[at].get(landed[at]).map(|key| key.number());
                        let value = match first_unlanded {
                            None => progress.changes_read(),
                            Some(_) => routed[at]
                                .iter()
                                .take_while(|(_, key)| {
                                    first_unlanded
                                        .is_none_or(|unlanded| key.is_none_or(|key| key < unlanded))
                                })
                                .last()
                                .map_or(0, |(read, _)| *read),
                        };
                        most[at] = most[at].max(value);
                        most[at]
                    })
                    .min()
                    .unwrap();
                assert_eq!(
                    progress.committed(&SinkName::try_new(sink).unwrap()),
                    model,
                    "seed {seed}"
                );
            }
            for lane in &lanes {
                let (kept, unlanded) = progress.steps_kept(lane).unwrap();
                assert!(
                    kept <= unlanded + 2,
                    "seed {seed}: {lane} keeps {:?}",
                    progress.steps_kept(lane)
                );
            }
        }
    }
}

/// Stage 8a slice 6b-1a: each index's steps carry its own frontier (the
/// changes through which every one of its documents was taken), a lane
/// catches up to the read count while it and its index are idle, and the
/// count never goes down.
mod frontier {
    use super::*;

    fn lane(index: &str, sink: &str) -> LaneKey {
        LaneKey::new(
            kernel::IndexName::try_new(index).unwrap(),
            kernel::SinkName::try_new(sink).unwrap(),
        )
    }

    fn index(name: &str) -> IndexName {
        IndexName::try_new(name).unwrap()
    }

    fn name(sink: &str) -> SinkName {
        SinkName::try_new(sink).unwrap()
    }

    /// Mark `id` of `index` by the read `read`, at position `read`.
    fn mark(progress: &Progress, index: &str, id_: i64, read: u64) {
        progress.mark_reads([(id(index, id_), p(read), read)]);
    }

    /// Count reads up to `total`.
    fn read_through(progress: &Progress, total: u64) {
        while progress.changes_read() < total {
            progress.change_read();
        }
    }

    /// A take of up to `max` of `index`'s documents, its step closed with
    /// every other index of `others` at its frontier, its item published
    /// and the build released; the item's key, if it took something.
    fn grant(
        progress: &Progress,
        lanes: &[&LaneKey],
        taken_index: &str,
        max: usize,
        at: u64,
    ) -> Option<ItemKey> {
        let taken = progress.take(&index(taken_index), max, &nothing_held);
        let mut frontiers: Vec<(IndexName, u64)> = progress
            .indexes()
            .filter(|other| other.as_ref() != taken_index || taken.is_none())
            .map(|other| (other.clone(), progress.frontier(other)))
            .collect();
        if let Some(taken) = &taken {
            frontiers.push((index(taken_index), taken.frontier));
        }
        let steps = progress.close_steps(frontiers);
        let key = taken
            .as_ref()
            .map(|_| progress.next_key(&index(taken_index)).unwrap());
        for lane in lanes {
            let step = steps[&lane.index];
            match (&taken, key) {
                (Some(taken), Some(key)) if lane.index.as_ref() == taken_index => {
                    progress.published_build(lane, key, taken.oldest, p(at), taken.token, step);
                }
                _ => progress.skipped_step(lane, p(at), step),
            }
        }
        if let Some(taken) = &taken {
            progress.release(&index(taken_index), taken.token);
        }
        key
    }

    /// A take that leaves documents waiting commits only through the change
    /// before the oldest one left: what it took doesn't cover the reads of
    /// what it left (6b's capped takes and backlog; until 6b-1c the one
    /// builder takes whole, so this is `Progress`'s alone).
    #[test]
    fn a_partial_take_counts_committed_only_through_its_oldest_document_left() {
        let users = lane("users", "a");
        let progress = Progress::new([users.clone()]);
        for read in 1..=3 {
            mark(&progress, "users", read as i64, read);
        }
        read_through(&progress, 3);
        let first = grant(&progress, &[&users], "users", 1, 3).unwrap();
        progress.applied(&users, first);
        assert_eq!(progress.committed(&name("a")), 1, "documents 2 and 3 wait");

        let rest = grant(&progress, &[&users], "users", 10, 3).unwrap();
        assert_eq!(
            progress.committed(&name("a")),
            1,
            "their item hasn't landed"
        );
        progress.applied(&users, rest);
        assert_eq!(progress.committed(&name("a")), 3);
    }

    /// An index a grant took nothing of is registered with its own
    /// frontier, not the read count: a document of it waiting from a later
    /// read keeps its lanes short of that read.
    #[test]
    fn a_skipped_index_counts_through_its_frontier_not_the_read_count() {
        let (users, orders) = (lane("users", "a"), lane("orders", "a"));
        let progress = Progress::new([users.clone(), orders.clone()]);
        mark(&progress, "users", 1, 1);
        mark(&progress, "orders", 2, 2);
        read_through(&progress, 2);
        let key = grant(&progress, &[&users, &orders], "users", 10, 1).unwrap();
        progress.applied(&users, key);
        assert_eq!(
            progress.committed(&name("a")),
            1,
            "orders' document of read 2 still waits"
        );
    }

    /// A build taken and not yet released holds its index's lanes: nothing
    /// waits, the lane has nothing unlanded, and still the catch-up must
    /// not credit the build's reads.
    #[test]
    fn a_build_taken_and_not_released_holds_its_lanes_count() {
        let users = lane("users", "a");
        let progress = Progress::new([users.clone()]);
        mark(&progress, "users", 1, 1);
        read_through(&progress, 1);
        let taken = progress.take(&index("users"), 10, &nothing_held).unwrap();
        assert_eq!(
            progress.committed(&name("a")),
            0,
            "the build of read 1 runs"
        );
        let steps = progress.close_steps([(index("users"), taken.frontier)]);
        let key = progress.next_key(&index("users")).unwrap();
        progress.published_build(
            &users,
            key,
            taken.oldest,
            p(1),
            taken.token,
            steps[&users.index],
        );
        progress.release(&index("users"), taken.token);
        progress.applied(&users, key);
        assert_eq!(progress.committed(&name("a")), 1);
    }

    /// A lane caught up to the read count while idle keeps that count when
    /// its index marks a new document: an older entry answering again
    /// never lowers it.
    #[test]
    fn a_lanes_count_never_goes_down_when_its_index_marks_again() {
        let users = lane("users", "a");
        let progress = Progress::new([users.clone()]);
        mark(&progress, "users", 1, 1);
        read_through(&progress, 1);
        let key = grant(&progress, &[&users], "users", 10, 1).unwrap();
        progress.applied(&users, key);
        read_through(&progress, 3);
        assert_eq!(
            progress.committed(&name("a")),
            3,
            "reads 2 and 3 built nothing"
        );

        mark(&progress, "users", 2, 4);
        read_through(&progress, 4);
        assert_eq!(progress.committed(&name("a")), 3, "never down");
    }

    /// A change that builds nothing counts as soon as it is read with
    /// everything before it landed, before any build: the catch-up.
    #[test]
    fn a_change_that_builds_nothing_counts_once_read() {
        let (users, orders) = (lane("users", "a"), lane("orders", "a"));
        let progress = Progress::new([users.clone(), orders.clone()]);
        read_through(&progress, 2);
        assert_eq!(progress.committed(&name("a")), 2);
    }

    /// A sink with no lane has nothing to land: it has every read.
    #[test]
    fn a_sink_with_no_lane_has_every_read() {
        let progress = Progress::new([lane("users", "a")]);
        read_through(&progress, 4);
        assert_eq!(progress.committed(&name("elsewhere")), 4);
    }
}

/// D1: what the reader routed moves a lane with nothing outstanding.
mod routed {
    use super::*;

    fn lane(index: &str) -> LaneKey {
        LaneKey::new(
            kernel::IndexName::try_new(index).unwrap(),
            kernel::SinkName::try_new("a").unwrap(),
        )
    }

    /// A quiet index's lane answers through what the reader routed, not
    /// through the last grant that registered it.
    #[test]
    fn a_quiet_index_answers_through_what_the_reader_routed() {
        let (users, orders) = (lane("users"), lane("orders"));
        let progress = Progress::new([users.clone(), orders.clone()]);
        progress.mark([(id("users", 1), p(1))]);
        progress.change_read();
        progress.routed(p(1));
        assert_eq!(
            progress.lane(&orders, Level::Applied),
            Some(Landed::Through(p(1)))
        );
        assert_eq!(
            progress.lane(&users, Level::Applied),
            Some(Landed::Before(p(1)))
        );
    }

    /// An index that never had a live item pins no confirmation once the
    /// other index landed what it was routed.
    #[test]
    fn a_quiet_index_pins_no_confirmation_while_another_index_builds() {
        let (users, orders) = (lane("users"), lane("orders"));
        let progress = Progress::new([users.clone(), orders.clone()]);
        progress.mark([(id("users", 1), p(1))]);
        progress.change_read();
        progress.routed(p(1));
        let taken = progress.take(&users.index, 10, &nothing_held).unwrap();
        let steps = progress.close_steps([(users.index.clone(), taken.frontier)]);
        let key = progress.next_key(&users.index).unwrap();
        progress.published_build(
            &users,
            key,
            taken.oldest,
            p(1),
            taken.token,
            steps[&users.index],
        );
        progress.release(&users.index, taken.token);
        assert_eq!(progress.landed(), Landed::Before(p(1)));
        progress.applied(&users, key);
        assert_eq!(progress.landed(), Landed::Through(p(1)));
    }

    /// What an ended run routed and never built is forgotten with it: no
    /// lane answers through it after `forget_all`.
    #[test]
    fn a_forgotten_run_answers_through_nothing_it_routed() {
        let users = lane("users");
        let progress = Progress::new([users.clone()]);
        progress.mark([(id("users", 1), p(5))]);
        progress.change_read();
        progress.routed(p(5));
        progress.forget_all();
        assert_eq!(progress.lane(&users, Level::Applied), Some(Landed::Idle));
    }

    /// A routed position moves the answer only forward.
    #[test]
    fn routed_never_moves_back() {
        let users = lane("users");
        let progress = Progress::new([users.clone()]);
        progress.routed(p(4));
        progress.routed(p(2));
        assert_eq!(
            progress.lane(&users, Level::Applied),
            Some(Landed::Through(p(4)))
        );
    }
}

/// D2: an ended run's reads, across `forget_all`.
mod restart {
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

    fn mark(progress: &Progress, index: &str, id_: i64, read: u64) {
        progress.mark_reads([(id(index, id_), p(read), read)]);
    }

    fn read_through(progress: &Progress, total: u64) {
        while progress.changes_read() < total {
            progress.change_read();
        }
    }

    /// Take every waiting document of `index` and publish it on `lanes`
    /// (the other lanes skipped), all at their frontiers; the key.
    fn grant(progress: &Progress, lanes: &[&LaneKey], index: &IndexName, at: u64) -> ItemKey {
        let taken = progress.take(index, usize::MAX, &nothing_held).unwrap();
        let mut frontiers: Vec<(IndexName, u64)> = progress
            .indexes()
            .filter(|other| *other != index)
            .map(|other| (other.clone(), progress.frontier(other)))
            .collect();
        frontiers.push((index.clone(), taken.frontier));
        let steps = progress.close_steps(frontiers);
        let key = progress.next_key(index).unwrap();
        for lane in lanes {
            if lane.index == *index {
                progress.published_build(
                    lane,
                    key,
                    taken.oldest,
                    p(at),
                    taken.token,
                    steps[&lane.index],
                );
            } else {
                progress.skipped_step(lane, p(at), steps[&lane.index]);
            }
        }
        progress.release(index, taken.token);
        key
    }

    /// The first grant after a restart lifts the cap: a change read after it
    /// that builds nothing counts at once, as it did before the restart.
    #[test]
    fn after_the_first_grant_a_change_that_builds_nothing_counts_once_read() {
        let users = lane("users", "a");
        let progress = Progress::new([users.clone()]);
        mark(&progress, "users", 1, 1);
        read_through(&progress, 1);
        progress.forget_all();
        mark(&progress, "users", 1, 2);
        read_through(&progress, 2);
        let replay = grant(&progress, &[&users], &users.index, 2);
        progress.applied(&users, replay);
        assert_eq!(progress.committed(&name("a")), 2);

        read_through(&progress, 3);
        assert_eq!(
            progress.committed(&name("a")),
            3,
            "read 3 touched nothing and everything before it landed"
        );
    }

    /// The intermediate count: an ended run's unbuilt read counts no sooner
    /// than the new run's first live grant, through the replay's marks.
    #[test]
    fn an_ended_runs_reads_count_only_with_the_first_grant_after_the_restart() {
        let users = lane("users", "a");
        let progress = Progress::new([users.clone()]);
        mark(&progress, "users", 1, 1);
        read_through(&progress, 1);
        let key = grant(&progress, &[&users], &users.index, 1);
        progress.applied(&users, key);
        mark(&progress, "users", 2, 2);
        read_through(&progress, 2);
        assert_eq!(progress.committed(&name("a")), 1);

        progress.forget_all();
        assert_eq!(progress.committed(&name("a")), 1, "read 2 was never built");
        mark(&progress, "users", 2, 3);
        read_through(&progress, 3);
        assert_eq!(
            progress.committed(&name("a")),
            1,
            "its replay is read, not built"
        );
        let replay = grant(&progress, &[&users], &users.index, 3);
        assert_eq!(
            progress.committed(&name("a")),
            1,
            "the replay hasn't landed"
        );
        progress.applied(&users, replay);
        assert_eq!(progress.committed(&name("a")), 3);
    }

    /// Two sinks: one had committed more than the other when the run ended;
    /// neither count goes down across the restart and the replay.
    #[test]
    fn a_sinks_count_never_goes_down_across_a_restart() {
        let (on_a, on_b) = (lane("users", "a"), lane("users", "b"));
        let progress = Progress::new([on_a.clone(), on_b.clone()]);
        let last = std::cell::Cell::new((0, 0));
        let check = |progress: &Progress| {
            let now = (
                progress.committed(&name("a")),
                progress.committed(&name("b")),
            );
            let before = last.get();
            assert!(
                now.0 >= before.0 && now.1 >= before.1,
                "{before:?} then {now:?}"
            );
            last.set(now);
        };
        for read in 1..=2 {
            mark(&progress, "users", read as i64, read);
            read_through(&progress, read);
            let key = grant(&progress, &[&on_a, &on_b], &on_a.index, read);
            progress.applied(&on_a, key);
            if read == 1 {
                progress.applied(&on_b, key);
            }
        }
        mark(&progress, "users", 3, 3);
        read_through(&progress, 3);
        check(&progress);
        assert_eq!(last.get(), (2, 1));

        progress.forget_all();
        check(&progress);
        mark(&progress, "users", 3, 4);
        read_through(&progress, 4);
        check(&progress);
        let key = grant(&progress, &[&on_a, &on_b], &on_a.index, 4);
        check(&progress);
        progress.applied(&on_a, key);
        check(&progress);
        progress.applied(&on_b, key);
        check(&progress);
        assert_eq!(last.get(), (4, 4));
    }

    /// The source resumes before what was committed (Postgres's confirmed
    /// position trails what landed): the replay's reads re-mark documents
    /// already built, and nothing past what was committed counts before the
    /// first grant after the restart.
    #[test]
    fn a_replay_from_before_the_committed_point_credits_nothing_before_the_first_grant() {
        let users = lane("users", "a");
        let progress = Progress::new([users.clone()]);
        for read in 1..=3 {
            mark(&progress, "users", read as i64, read);
            read_through(&progress, read);
            let key = grant(&progress, &[&users], &users.index, read);
            progress.applied(&users, key);
        }
        mark(&progress, "users", 4, 4);
        read_through(&progress, 4);
        assert_eq!(progress.committed(&name("a")), 3);

        progress.forget_all();
        assert_eq!(progress.committed(&name("a")), 3, "read 4 was never built");
        for (document, read) in [(2, 5), (3, 6), (4, 7)] {
            mark(&progress, "users", document, read);
            read_through(&progress, read);
            assert_eq!(
                progress.committed(&name("a")),
                3,
                "nothing before the first grant"
            );
        }
        let key = grant(&progress, &[&users], &users.index, 7);
        assert_eq!(progress.committed(&name("a")), 3);
        progress.applied(&users, key);
        assert_eq!(progress.committed(&name("a")), 7);
    }

    /// A replay shorter than the ended run's reads, and an index that gets
    /// no item after the restart: once what was read has landed, the count
    /// reaches every read.
    #[test]
    fn a_short_replay_and_a_quiet_index_count_everything_once_landed() {
        let (users, orders) = (lane("users", "a"), lane("orders", "a"));
        let progress = Progress::new([users.clone(), orders.clone()]);
        mark(&progress, "users", 1, 1);
        mark(&progress, "users", 2, 2);
        read_through(&progress, 3);
        progress.forget_all();
        mark(&progress, "users", 2, 4);
        read_through(&progress, 4);
        let key = grant(&progress, &[&users, &orders], &users.index, 4);
        progress.applied(&users, key);
        assert_eq!(progress.committed(&name("a")), 4);
    }

    /// Reads that built nothing are confirmed once routed and never
    /// replayed: after a restart with no replay they still count.
    #[test]
    fn reads_that_built_nothing_count_across_a_restart_with_no_replay() {
        let users = lane("users", "a");
        let progress = Progress::new([users.clone()]);
        read_through(&progress, 2);
        progress.routed(p(2));
        progress.forget_all();
        assert_eq!(progress.committed(&name("a")), 2);
    }

    /// A build taken and never released when the run ended leaves its reads
    /// unbuilt: they count no sooner than the first grant after the restart.
    #[test]
    fn a_build_in_flight_at_the_restart_caps_the_count() {
        let users = lane("users", "a");
        let progress = Progress::new([users.clone()]);
        mark(&progress, "users", 1, 1);
        read_through(&progress, 2);
        let _in_flight = progress.take(&users.index, 10, &nothing_held).unwrap();
        progress.forget_all();
        assert_eq!(progress.committed(&name("a")), 0);
    }

    /// Reads counted without their marks (a run cut before them, R9's
    /// guard) count no sooner than the first grant after the restart.
    #[test]
    fn reads_cut_before_their_marks_count_no_sooner_than_the_first_grant() {
        let users = lane("users", "a");
        let progress = Progress::new([users.clone()]);
        read_through(&progress, 1);
        progress.read_unmarked(2);
        progress.forget_all();
        assert_eq!(progress.committed(&name("a")), 1);
        mark(&progress, "users", 1, 4);
        read_through(&progress, 4);
        let key = grant(&progress, &[&users], &users.index, 4);
        progress.applied(&users, key);
        assert_eq!(progress.committed(&name("a")), 4);
    }
}

/// The frontier against a model kept apart from the code: seeded reads
/// marking documents of three indexes, grants of the one builder (each
/// index taken whole or capped, every lane registered with an item or a
/// skip), landings in lane order, and ended runs (with reads cut before
/// their marks), on two sinks. The model knows, per read and index, whether
/// its documents wait, which step took them, or that its run ended; a read
/// is done for a lane once each of its takes landed there, and an ended
/// run's read once the lane landed an entry of a step closed after the
/// restart (slice 3's moment). At every step a sink's count never passes
/// the model's and never goes down; once everything landed they are equal.
mod model {
    use std::collections::{BTreeMap, HashMap};

    use super::*;

    const INDEXES: [&str; 3] = ["users", "orders", "items"];
    const SINKS: [&str; 2] = ["a", "b"];

    struct Rng(u64);

    impl Rng {
        fn below(&mut self, n: u64) -> u64 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            self.0 % n
        }
    }

    #[derive(Clone, Copy, PartialEq, Eq)]
    enum Need {
        /// The document `doc` waits.
        Waiting(i64),
        /// The step numbered so took it.
        Taken(u64),
        /// Its run ended; done once an entry of a step from this one landed.
        Ended(u64),
    }

    #[derive(Default)]
    struct Model {
        /// Per read (from 1): what each index needs of it.
        reads: Vec<Vec<(usize, Need)>>,
        /// Per lane: its entries in order, each a step and its item key.
        entries: BTreeMap<LaneKey, Vec<(u64, Option<ItemKey>)>>,
        /// Per lane: how many of its items landed.
        landed: BTreeMap<LaneKey, usize>,
        steps: u64,
    }

    impl Model {
        fn entry_landed(&self, lane: &LaneKey, wanted: impl Fn(u64) -> bool) -> bool {
            let mut items = 0;
            let landed = self.landed.get(lane).copied().unwrap_or(0);
            for (step, key) in self.entries.get(lane).into_iter().flatten() {
                if key.is_some() {
                    items += 1;
                }
                if items > landed {
                    return false;
                }
                if wanted(*step) {
                    return true;
                }
            }
            false
        }

        fn done(&self, read: usize, lane: &LaneKey, index: usize) -> bool {
            self.reads[read]
                .iter()
                .filter(|(of, _)| *of == index)
                .all(|(_, need)| match need {
                    Need::Waiting(_) => false,
                    Need::Taken(step) => self.entry_landed(lane, |s| s == *step),
                    Need::Ended(from) => self.entry_landed(lane, |s| s >= *from),
                })
        }

        fn committed(&self, lanes: &[(LaneKey, usize)], sink: &str) -> u64 {
            let mine: Vec<_> = lanes
                .iter()
                .filter(|(lane, _)| lane.sink.as_ref() == sink)
                .collect();
            let mut done = 0;
            for read in 0..self.reads.len() {
                if mine
                    .iter()
                    .all(|(lane, index)| self.done(read, lane, *index))
                {
                    done = read as u64 + 1;
                } else {
                    break;
                }
            }
            done
        }
    }

    fn run(seed: u64) {
        let lanes: Vec<(LaneKey, usize)> = INDEXES
            .iter()
            .enumerate()
            .flat_map(|(at, index)| {
                SINKS.iter().map(move |sink| {
                    (
                        LaneKey::new(
                            IndexName::try_new(*index).unwrap(),
                            SinkName::try_new(*sink).unwrap(),
                        ),
                        at,
                    )
                })
            })
            .collect();
        let progress = Progress::new(lanes.iter().map(|(lane, _)| lane.clone()));
        let mut model = Model::default();
        let mut rng = Rng(seed | 1);
        let mut seq = 0;
        let mut last: HashMap<&str, u64> = HashMap::new();
        let check = |progress: &Progress, model: &Model, last: &mut HashMap<&str, u64>| {
            for sink in SINKS {
                let ours = progress.committed(&SinkName::try_new(sink).unwrap());
                let theirs = model.committed(&lanes, sink);
                assert!(
                    ours <= theirs,
                    "seed {seed}: {sink} at {ours}, the model at {theirs}"
                );
                let before = last.insert(sink, ours).unwrap_or(0);
                assert!(
                    ours >= before,
                    "seed {seed}: {sink} went from {before} to {ours}"
                );
            }
        };
        let grant = |progress: &Progress, model: &mut Model, rng: &mut Rng, seq: u64| {
            let mut taken: BTreeMap<IndexName, Taken> = BTreeMap::new();
            let mut frontiers = Vec::new();
            for index in INDEXES {
                let index = IndexName::try_new(index).unwrap();
                let max = if rng.below(3) == 0 { 1 } else { usize::MAX };
                match progress.take(&index, max, &nothing_held) {
                    Some(take) => {
                        frontiers.push((index.clone(), take.frontier));
                        taken.insert(index, take);
                    }
                    None => frontiers.push((index.clone(), progress.frontier(&index))),
                }
            }
            let steps = progress.close_steps(frontiers);
            let step = model.steps;
            model.steps += 1;
            for (at, index) in INDEXES.iter().enumerate() {
                let index = IndexName::try_new(*index).unwrap();
                let Some(take) = taken.get(&index) else {
                    continue;
                };
                let docs: Vec<i64> = take
                    .ids
                    .iter()
                    .map(|id| match id.key.0.first() {
                        Some((_, kernel::GenericValue::BigInt(n))) => *n,
                        _ => unreachable!(),
                    })
                    .collect();
                for read in &mut model.reads {
                    for (of, need) in read.iter_mut() {
                        if *of == at && matches!(need, Need::Waiting(doc) if docs.contains(doc)) {
                            *need = Need::Taken(step);
                        }
                    }
                }
            }
            let mut keys: BTreeMap<IndexName, ItemKey> = BTreeMap::new();
            for (lane, _) in &lanes {
                let entry = match taken.get(&lane.index) {
                    Some(take) => {
                        let key = *keys
                            .entry(lane.index.clone())
                            .or_insert_with(|| progress.next_key(&lane.index).unwrap());
                        progress.published_build(
                            lane,
                            key,
                            take.oldest,
                            p(seq),
                            take.token,
                            steps[&lane.index],
                        );
                        Some(key)
                    }
                    None => {
                        progress.skipped_step(lane, p(seq), steps[&lane.index]);
                        None
                    }
                };
                model
                    .entries
                    .entry(lane.clone())
                    .or_default()
                    .push((step, entry));
            }
            for (index, take) in &taken {
                progress.release(index, take.token);
            }
        };
        for _ in 0..400 {
            match rng.below(10) {
                0..=3 => {
                    seq += 1;
                    let read = progress.changes_read() + 1;
                    let mut needs = Vec::new();
                    let mut marks = Vec::new();
                    for _ in 0..rng.below(3) {
                        let at = rng.below(3) as usize;
                        let doc = rng.below(6) as i64;
                        marks.push((id(INDEXES[at], doc), p(seq), read));
                        needs.push((at, Need::Waiting(doc)));
                    }
                    progress.mark_reads(marks);
                    progress.change_read();
                    progress.routed(p(seq));
                    model.reads.push(needs);
                }
                4..=5 => grant(&progress, &mut model, &mut rng, seq),
                6..=8 => {
                    let (lane, _) = &lanes[rng.below(lanes.len() as u64) as usize];
                    let registered = model.entries.get(lane).map_or(0, |entries| {
                        entries.iter().filter(|(_, key)| key.is_some()).count()
                    });
                    let landed = model.landed.entry(lane.clone()).or_insert(0);
                    if *landed < registered {
                        let key = model.entries[lane]
                            .iter()
                            .filter_map(|(_, key)| *key)
                            .nth(*landed)
                            .unwrap();
                        progress.applied(lane, key);
                        *landed += 1;
                    }
                }
                _ => {
                    if rng.below(2) == 0 {
                        let cut = rng.below(3);
                        progress.read_unmarked(cut);
                        for _ in 0..cut {
                            model.reads.push(
                                (0..INDEXES.len())
                                    .map(|at| (at, Need::Waiting(-1)))
                                    .collect(),
                            );
                        }
                    }
                    progress.forget_all();
                    let from = model.steps;
                    for read in &mut model.reads {
                        for (_, need) in read.iter_mut() {
                            if matches!(need, Need::Waiting(_)) {
                                *need = Need::Ended(from);
                            }
                        }
                    }
                }
            }
            check(&progress, &model, &mut last);
        }
        // Build what waits, land everything: every read counts.
        grant(&progress, &mut model, &mut rng, seq);
        while progress.waiting() > 0 {
            grant(&progress, &mut model, &mut rng, seq);
        }
        for (lane, _) in &lanes {
            let keys: Vec<ItemKey> = model.entries[lane]
                .iter()
                .filter_map(|(_, key)| *key)
                .collect();
            if let Some(key) = keys.last() {
                progress.applied(lane, *key);
            }
            model.landed.insert(lane.clone(), keys.len());
        }
        check(&progress, &model, &mut last);
        for sink in SINKS {
            let name = SinkName::try_new(sink).unwrap();
            assert_eq!(
                progress.committed(&name),
                model.committed(&lanes, sink),
                "seed {seed}"
            );
            assert_eq!(
                progress.committed(&name),
                progress.changes_read(),
                "seed {seed}"
            );
        }
    }

    #[test]
    fn the_frontier_never_passes_the_model_and_meets_it_once_everything_landed() {
        for seed in 1..=200 {
            run(seed);
        }
    }
}
