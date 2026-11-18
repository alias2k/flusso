use std::sync::atomic::AtomicBool;
use std::thread;

use super::*;

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
        progress.published(lane, p(oldest), p(last));
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
    progress.published(&a, p(1), p(3));
    assert_eq!(progress.landed(), Landed::Before(p(1)));

    progress.applied(&a, p(3));
    assert_eq!(progress.landed(), Landed::Through(p(3)));
}

#[test]
fn a_mark_covers_every_item_through_it_and_the_oldest_uncovered_decides() {
    let a = sink("a");
    let progress = Progress::new([a.clone()]);
    publish(&progress, &[&a], 1, 2);
    publish(&progress, &[&a], 3, 4);
    publish(&progress, &[&a], 5, 6);

    progress.applied(&a, p(2));
    assert_eq!(progress.landed(), Landed::Before(p(3)));
    progress.applied(&a, p(6));
    assert_eq!(progress.landed(), Landed::Through(p(6)));
}

#[test]
fn a_stalled_lane_pins_confirmation_and_its_own_fence_only() {
    let (fast, stalled) = (sink("fast"), sink("stalled"));
    let progress = Progress::new([fast.clone(), stalled.clone()]);
    publish(&progress, &[&fast, &stalled], 1, 2);
    publish(&progress, &[&fast, &stalled], 3, 4);
    progress.applied(&fast, p(4));
    progress.searchable(&fast, p(4));

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
    progress.applied(&healthy, p(1));
    progress.applied(&full, p(1));
    publish(&progress, &[&healthy], 2, 2);
    progress.applied(&healthy, p(2));

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
    progress.published(&busy, p(1), p(2));
    progress.skipped(&idle, p(2));
    assert_eq!(progress.landed(), Landed::Before(p(1)));
    assert_eq!(
        progress.lane(&idle, Level::Applied),
        Some(Landed::Through(p(2))),
        "it needs nothing of the batch"
    );
    progress.applied(&busy, p(2));
    assert_eq!(progress.landed(), Landed::Through(p(2)));
}

#[test]
fn a_landing_reported_before_its_registration_still_counts() {
    let a = sink("a");
    let progress = Progress::new([a.clone()]);
    progress.applied(&a, p(2));
    progress.searchable(&a, p(2));
    assert_eq!(
        progress.landed(),
        Landed::Idle,
        "a mark alone claims nothing"
    );
    progress.published(&a, p(1), p(2));
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
    progress.applied(&a, p(4));
    assert_eq!(
        progress.lane(&a, Level::Applied),
        Some(Landed::Through(p(4)))
    );
    assert_eq!(
        progress.lane(&a, Level::Searchable),
        Some(Landed::Before(p(1)))
    );
    progress.searchable(&a, p(2));
    assert_eq!(
        progress.lane(&a, Level::Searchable),
        Some(Landed::Before(p(3)))
    );
    progress.searchable(&a, p(4));
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
    progress.applied(&a, p(4));
    progress.applied(&a, p(2));
    assert_eq!(progress.landed(), Landed::Through(p(4)));
    publish(&progress, &[&a], 5, 6);
    assert_eq!(progress.landed(), Landed::Before(p(5)));
}

/// Several lanes take the least answer (what the stream's watermark, the
/// minimum over lanes, used to be).
#[test]
fn several_lanes_answer_their_least_and_an_unknown_lane_none() {
    let (a, b) = (sink("a"), sink("b"));
    let progress = Progress::new([a.clone(), b.clone()]);
    publish(&progress, &[&a, &b], 1, 1);
    publish(&progress, &[&a, &b], 2, 3);
    progress.applied(&a, p(3));
    progress.applied(&b, p(1));
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
    progress.published(&a, p(1), p(1));
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
                            progress.published(&names[l], p(batch.oldest), p(batch.last));
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
                        progress.applied(&names[l], p(keys[applied[l] - 1]));
                    }
                }
                _ => {
                    let l = rng.below(lanes);
                    let keys = items(&batches, l);
                    if searchable[l] < applied[l] {
                        searchable[l] += 1 + rng.below(applied[l] - searchable[l]);
                        progress.searchable(&names[l], p(keys[searchable[l] - 1]));
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
                        progress.published(&names[l], p(seq), p(seq));
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
                            progress.applied(&name, p(seq));
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

mod pending {
    use super::*;
    use source::RowKey;

    fn users() -> IndexName {
        IndexName::try_new("users").unwrap()
    }

    fn orders() -> IndexName {
        IndexName::try_new("orders").unwrap()
    }

    fn lane(index: IndexName) -> LaneKey {
        LaneKey::new(index, kernel::SinkName::try_new("primary").unwrap())
    }

    fn id(index: IndexName, key: i64) -> DocumentId {
        DocumentId {
            index,
            key: RowKey(vec![(
                kernel::ColumnName::try_new("id").unwrap(),
                kernel::GenericValue::BigInt(key),
            )]),
        }
    }

    fn two() -> Progress {
        Progress::new([lane(orders()), lane(users())])
    }

    /// A marked id holds its index's lanes before its change from the mark
    /// until its build lands; another index's lane has every routed change.
    #[test]
    fn a_marked_id_holds_its_index_until_its_build_lands() {
        let progress = two();
        progress.mark(&users(), [id(users(), 1)], p(5));
        progress.route(p(5));
        assert_eq!(
            progress.lane(&lane(users()), Level::Applied),
            Some(Landed::Before(p(5)))
        );
        assert_eq!(
            progress.lane(&lane(orders()), Level::Applied),
            Some(Landed::Through(p(5))),
            "a quiet index pins nothing"
        );
        let taken = progress.take(&users(), 10).unwrap();
        assert_eq!(taken.ids, [id(users(), 1)]);
        assert_eq!(taken.oldest, p(5));
        assert_eq!(progress.landed(), Landed::Before(p(5)), "building");
        let key = progress.publish_taken(&users(), taken.token, &[lane(users())]);
        assert_eq!(progress.landed(), Landed::Before(p(5)), "on the lane");
        progress.applied(&lane(users()), key);
        assert_eq!(progress.landed(), Landed::Through(p(5)));
    }

    /// Two marks of one waiting id: one build, at the older position.
    #[test]
    fn a_second_mark_of_a_waiting_id_keeps_the_older_position() {
        let progress = two();
        progress.mark(&users(), [id(users(), 1)], p(3));
        progress.mark(&users(), [id(users(), 1), id(users(), 2)], p(4));
        progress.route(p(4));
        assert_eq!(progress.waiting(), 2);
        let taken = progress.take(&users(), 10).unwrap();
        assert_eq!(taken.ids, [id(users(), 1), id(users(), 2)]);
        assert_eq!(taken.oldest, p(3));
    }

    /// A change to a document while its build runs is neither lost nor
    /// built beside it: the id waits again, and isn't taken until the build
    /// is published.
    #[test]
    fn an_id_marked_while_it_builds_is_built_again_after() {
        let progress = two();
        progress.mark(&users(), [id(users(), 1)], p(3));
        let first = progress.take(&users(), 10).unwrap();
        progress.mark(&users(), [id(users(), 1)], p(7));
        progress.route(p(7));
        assert_eq!(progress.take(&users(), 10), None, "in flight");
        progress.publish_taken(&users(), first.token, &[lane(users())]);
        let second = progress.take(&users(), 10).unwrap();
        assert_eq!(second.ids, [id(users(), 1)]);
        assert_eq!(second.oldest, p(7));
    }

    /// A build given back returns its ids at their older position.
    #[test]
    fn a_build_given_back_waits_again_at_its_oldest() {
        let progress = two();
        progress.mark(&users(), [id(users(), 1)], p(3));
        let taken = progress.take(&users(), 10).unwrap();
        progress.mark(&users(), [id(users(), 1)], p(9));
        progress.give_back(&users(), taken.token);
        let again = progress.take(&users(), 10).unwrap();
        assert_eq!(again.oldest, p(3));
        progress.mark(&users(), [id(users(), 2)], p(10));
        progress.give_back_all();
        assert_eq!(progress.waiting(), 2);
    }

    /// Takes are oldest first and capped; publish sequences grow.
    #[test]
    fn takes_are_oldest_first_and_publish_sequences_grow() {
        let progress = two();
        for key in 0..5 {
            progress.mark(&users(), [id(users(), key)], p(10 - key as u64));
        }
        let first = progress.take(&users(), 2).unwrap();
        assert_eq!(first.ids, [id(users(), 4), id(users(), 3)]);
        let second = progress.take(&users(), 2).unwrap();
        let a = progress.publish_taken(&users(), first.token, &[lane(users())]);
        let b = progress.publish_taken(&users(), second.token, &[lane(users())]);
        assert!(a < b);
    }
}
