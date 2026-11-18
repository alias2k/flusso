use super::*;

fn sink(name: &str) -> SinkName {
    SinkName::try_new(name).unwrap()
}

fn p(seq: u64) -> Position {
    Position::new(seq)
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
    assert_eq!(progress.routed(), None);
}

#[test]
fn a_routed_item_holds_everything_from_its_oldest_change() {
    let progress = Progress::new([sink("a")]);
    progress.published([&sink("a")], p(1), p(3));
    assert_eq!(progress.routed(), Some(p(3)));
    assert_eq!(progress.landed(), Landed::Before(p(1)));

    progress.applied(&sink("a"), p(3));
    assert_eq!(progress.landed(), Landed::Through(p(3)));
}

#[test]
fn a_mark_covers_every_item_through_it_and_the_oldest_uncovered_decides() {
    let a = sink("a");
    let progress = Progress::new([a.clone()]);
    progress.published([&a], p(1), p(2));
    progress.published([&a], p(3), p(4));
    progress.published([&a], p(5), p(6));

    progress.applied(&a, p(2));
    assert_eq!(progress.landed(), Landed::Before(p(3)));
    progress.applied(&a, p(6));
    assert_eq!(progress.landed(), Landed::Through(p(6)));
}

#[test]
fn a_stalled_lane_pins_confirmation_and_its_own_fence_only() {
    let (fast, stalled) = (sink("fast"), sink("stalled"));
    let progress = Progress::new([fast.clone(), stalled.clone()]);
    progress.published([&fast, &stalled], p(1), p(2));
    progress.published([&fast, &stalled], p(3), p(4));
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

#[test]
fn a_lane_with_nothing_routed_to_it_pins_nothing() {
    let (busy, idle) = (sink("busy"), sink("idle"));
    let progress = Progress::new([busy.clone(), idle.clone()]);
    progress.published([&busy], p(1), p(2));
    assert_eq!(progress.landed(), Landed::Before(p(1)));
    assert_eq!(
        progress.lane(&idle, Level::Applied),
        Some(Landed::Through(p(2))),
        "nothing outstanding on it: it has every routed change it needs"
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
    progress.published([&a], p(1), p(2));
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
    progress.published([&a], p(1), p(2));
    progress.published([&a], p(3), p(4));
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
    progress.published([&a], p(1), p(2));
    progress.published([&a], p(3), p(4));
    progress.applied(&a, p(4));
    progress.applied(&a, p(2));
    assert_eq!(progress.landed(), Landed::Through(p(4)));
    progress.published([&a], p(5), p(6));
    assert_eq!(progress.landed(), Landed::Before(p(5)));
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
    lanes: Vec<usize>,
    registered: bool,
}

/// Random interleavings of publishing, registering, and marks moving on each
/// lane (also past items not registered yet), against a model of what each
/// lane has: every answer must equal the model's.
#[test]
fn every_interleaving_answers_as_the_model_does() {
    let names: Vec<SinkName> = ["a", "b", "c"].iter().map(|n| sink(n)).collect();
    for seed in 1..=300u64 {
        let mut rng = Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1);
        let lanes = 1 + rng.below(3);
        let progress = Progress::new(names.iter().take(lanes).cloned());
        let mut batches: Vec<Batch> = Vec::new();
        // Per lane, how many of its batches (in lane order) are written, and
        // searchable.
        let mut applied = vec![0usize; lanes];
        let mut searchable = vec![0usize; lanes];
        let mut next = 1u64;
        for step in 0..200 {
            let on_lane = |batches: &[Batch], l: usize| -> Vec<u64> {
                batches
                    .iter()
                    .filter(|b| b.lanes.contains(&l))
                    .map(|b| b.last)
                    .collect()
            };
            match rng.below(5) {
                0 => {
                    let size = 1 + rng.below(3) as u64;
                    let mut on: Vec<usize> = (0..lanes).filter(|_| rng.below(4) != 0).collect();
                    if on.is_empty() {
                        on.push(rng.below(lanes));
                    }
                    batches.push(Batch {
                        oldest: next,
                        last: next + size - 1,
                        lanes: on,
                        registered: false,
                    });
                    next += size;
                }
                1 => {
                    if let Some(batch) = batches.iter_mut().find(|b| !b.registered) {
                        let on: Vec<&SinkName> = batch.lanes.iter().map(|l| &names[*l]).collect();
                        progress.published(on, p(batch.oldest), p(batch.last));
                        batch.registered = true;
                    }
                }
                2 | 3 => {
                    let l = rng.below(lanes);
                    let keys = on_lane(&batches, l);
                    if applied[l] < keys.len() {
                        applied[l] += 1 + rng.below(keys.len() - applied[l]);
                        progress.applied(&names[l], p(keys[applied[l] - 1]));
                    }
                }
                _ => {
                    let l = rng.below(lanes);
                    let keys = on_lane(&batches, l);
                    if searchable[l] < applied[l] {
                        searchable[l] += 1 + rng.below(applied[l] - searchable[l]);
                        progress.searchable(&names[l], p(keys[searchable[l] - 1]));
                    }
                }
            }

            let routed = batches
                .iter()
                .filter(|b| b.registered)
                .map(|b| b.last)
                .max();
            let outstanding = |lane: Option<usize>, level: Level| {
                (0..lanes)
                    .filter(|l| lane.is_none_or(|lane| lane == *l))
                    .flat_map(|l| {
                        let done = match level {
                            Level::Applied => applied[l],
                            Level::Searchable => searchable[l].min(applied[l]),
                        };
                        batches
                            .iter()
                            .filter(move |b| b.lanes.contains(&l))
                            .skip(done)
                            .filter(|b| b.registered)
                            .map(|b| b.oldest)
                    })
                    .min()
            };
            let expect = |oldest: Option<u64>| match (oldest, routed) {
                (Some(oldest), _) => Landed::Before(p(oldest)),
                (None, Some(routed)) => Landed::Through(p(routed)),
                (None, None) => Landed::Idle,
            };
            assert_eq!(
                progress.landed(),
                expect(outstanding(None, Level::Applied)),
                "seed {seed}, step {step}: {batches:?}, applied {applied:?}"
            );
            for (l, name) in names.iter().enumerate().take(lanes) {
                for level in [Level::Applied, Level::Searchable] {
                    assert_eq!(
                        progress.lane(name, level),
                        Some(expect(outstanding(Some(l), level))),
                        "seed {seed}, step {step}, lane {name}, {level:?}"
                    );
                }
            }
        }
    }
}
