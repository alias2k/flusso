use std::time::Duration;

use tokio::time::Instant;

use super::{Drained, Progress, StopSignal, within};

const DEADLINE: Duration = Duration::from_secs(5);

#[tokio::test(start_paused = true)]
async fn a_drain_that_never_ends_is_given_up_at_the_deadline_naming_its_step() {
    let progress = Progress::default();
    let started = Instant::now();

    let drained = within(DEADLINE, &progress, std::future::pending(), async {
        progress.set("closing the HTTP surfaces");
        std::future::pending::<()>().await;
    })
    .await;

    assert_eq!(
        drained,
        Drained::TimedOut {
            step: "closing the HTTP surfaces"
        }
    );
    assert_eq!(started.elapsed(), DEADLINE);
}

#[tokio::test(start_paused = true)]
async fn a_second_signal_ends_the_drain_at_once() {
    let progress = Progress::default();
    let started = Instant::now();

    let drained = within(
        DEADLINE,
        &progress,
        async {
            tokio::time::sleep(Duration::from_secs(1)).await;
            StopSignal::Interrupt
        },
        std::future::pending::<()>(),
    )
    .await;

    assert_eq!(drained, Drained::Forced(StopSignal::Interrupt));
    assert_eq!(started.elapsed(), Duration::from_secs(1));
}

#[tokio::test(start_paused = true)]
async fn a_drain_that_finishes_in_time_returns_its_value() {
    let progress = Progress::default();

    let drained = within(DEADLINE, &progress, std::future::pending(), async {
        tokio::time::sleep(Duration::from_secs(2)).await;
        7
    })
    .await;

    assert_eq!(drained, Drained::Done(7));
}

#[test]
fn a_signal_exits_with_the_shells_code_for_it() {
    assert_eq!(StopSignal::Hangup.exit_code(), 129);
    assert_eq!(StopSignal::Interrupt.exit_code(), 130);
    assert_eq!(StopSignal::Terminate.exit_code(), 143);
}

#[cfg(unix)]
#[tokio::test]
async fn a_parent_that_is_still_there_is_not_reported_gone() {
    let parent = super::parent().unwrap();

    let watched = tokio::time::timeout(
        Duration::from_millis(300),
        super::parent_exited(parent, Duration::from_millis(10)),
    )
    .await;

    assert!(watched.is_err(), "the test runner is still flusso's parent");
}

#[cfg(unix)]
#[tokio::test]
async fn a_parent_that_is_no_longer_ours_is_reported_gone() {
    let gone = u32::MAX;

    let watched = tokio::time::timeout(
        Duration::from_secs(1),
        super::parent_exited(gone, Duration::from_millis(10)),
    )
    .await;

    assert_eq!(watched, Ok(gone));
}

#[tokio::test(start_paused = true)]
async fn a_side_step_ends_before_the_deadline_so_a_finished_drain_still_exits_cleanly() {
    let progress = Progress::default();
    let deadline = Instant::now() + Duration::from_secs(2);

    let drained = within(
        Duration::from_secs(2),
        &progress,
        std::future::pending(),
        async { super::side_step(deadline, std::future::pending()).await },
    )
    .await;

    assert_eq!(drained, Drained::Done(false), "given up on, but in time");
}

#[tokio::test(start_paused = true)]
async fn a_side_step_takes_at_most_its_own_bound() {
    let started = Instant::now();

    let done = super::side_step(started + Duration::from_secs(60), std::future::pending()).await;

    assert!(!done);
    assert_eq!(started.elapsed(), super::SIDE_STEP);
}
