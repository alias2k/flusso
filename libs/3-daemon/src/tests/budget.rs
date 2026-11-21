//! The memory budget a daemon builds: shared per index only with two or more
//! indexes (ADR 0012 stage 8a slice 4), at the configured share.

use std::sync::Arc;

use super::committed::{backends_over, change, scripted, two_indexes};
use super::{fast_restarts, primary};
use crate::Daemon;

#[tokio::test]
async fn the_budget_is_shared_only_with_two_or_more_indexes() {
    let (source, _end) = scripted(vec![change("users", 1)]);
    let sink: Arc<dyn sink::Sink> = Arc::new(super::RecordingSink::seeded());
    let two = Daemon::new(
        two_indexes(),
        backends_over(Arc::new(source), vec![(primary(), Arc::clone(&sink))]),
    )
    .start()
    .await
    .unwrap();
    let budget = two.budget();
    assert_eq!(budget.index_share(), Some(budget.limit() / 2));

    let mut config = two_indexes();
    config.concurrency.index_budget_share = Some(config::ShareValue {
        share: 0.25,
        origin: config::Origin::Flag,
    });
    let (source, _end) = scripted(vec![change("users", 1)]);
    let quarter = Daemon::new(
        config,
        backends_over(Arc::new(source), vec![(primary(), Arc::clone(&sink))]),
    )
    .start()
    .await
    .unwrap();
    assert_eq!(
        quarter.budget().index_share(),
        Some(quarter.budget().limit() / 4)
    );

    let (source, _end) = scripted(vec![change("users", 1)]);
    let one = Daemon::new(
        fast_restarts(),
        super::backends_for(
            Arc::new(source),
            vec![(primary(), sink, sink::SinkOptions::default())],
        ),
    )
    .start()
    .await
    .unwrap();
    assert_eq!(
        one.budget().index_share(),
        None,
        "one index holds the whole budget"
    );
}

#[tokio::test]
async fn a_share_outside_zero_to_one_stops_the_start() {
    let mut config = two_indexes();
    config.concurrency.index_budget_share = Some(config::ShareValue {
        share: 0.0,
        origin: config::Origin::Env,
    });
    let (source, _end) = scripted(vec![]);
    let sink: Arc<dyn sink::Sink> = Arc::new(super::RecordingSink::seeded());
    let error = Daemon::new(
        config,
        backends_over(Arc::new(source), vec![(primary(), sink)]),
    )
    .start()
    .await
    .unwrap_err()
    .to_string();
    assert!(
        error.contains("index_budget_share = 0 (set by FLUSSO_INDEX_BUDGET_SHARE)"),
        "{error}"
    );
}

/// ADR 0012 stage 8a, slice 6a: a reader's lead `run` would refuse stops
/// `Daemon::start` too, naming where each value came from.
#[tokio::test]
async fn a_waiting_bound_below_one_index_stops_the_start() {
    let mut config = two_indexes();
    config.concurrency.max_waiting_per_index = Some(config::CountValue {
        count: 300,
        origin: config::Origin::Flag,
    });
    config.concurrency.max_waiting = Some(config::CountValue::from_file(200));
    let (source, _end) = scripted(vec![]);
    let sink: Arc<dyn sink::Sink> = Arc::new(super::RecordingSink::seeded());
    let error = Daemon::new(
        config,
        backends_over(Arc::new(source), vec![(primary(), sink)]),
    )
    .start()
    .await
    .unwrap_err()
    .to_string();
    assert!(
        error.contains(
            "max_waiting = 200 (set in flusso.toml's [concurrency] max_waiting) is below \
             max_waiting_per_index = 300 (set by --max-waiting-per-index)"
        ),
        "{error}"
    );
}
