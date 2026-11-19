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

#[cfg(unix)]
#[test]
fn hangup_is_left_alone_when_the_process_started_with_it_ignored() {
    let status = |mask: &str| {
        format!("Name:\tflusso\nSigBlk:\t0000000000000000\nSigIgn:\t{mask}\nSigCgt:\t0\n")
    };

    assert!(
        super::hangup_ignored(&status("0000000000001001")),
        "nohup: SIGHUP and SIGPIPE ignored"
    );
    assert!(
        !super::hangup_ignored(&status("0000000000001000")),
        "only SIGPIPE ignored"
    );
    assert!(
        !super::hangup_ignored("Name:\tflusso\n"),
        "no mask: not ignored"
    );
}

#[tokio::test(start_paused = true)]
async fn a_drain_that_is_already_done_wins_over_a_zero_deadline() {
    for _ in 0..100 {
        let drained = within(
            Duration::ZERO,
            &Progress::default(),
            std::future::pending(),
            async {},
        )
        .await;
        assert_eq!(drained, Drained::Done(()));
    }
}

mod repeats {
    use std::time::Duration;

    use tokio::time::Instant;

    use super::super::{Repeats, Throttle, Verdict};

    /// Feed `signals` (offset from the first, sender) after a first signal
    /// at 0 from `first`; the verdict of each.
    fn verdicts(first: Option<i64>, signals: &[(u64, Option<i64>)]) -> Vec<Verdict> {
        let start = Instant::now();
        let mut repeats = Repeats::new(start, first);
        signals
            .iter()
            .map(|(ms, sender)| repeats.record(start + Duration::from_millis(*ms), *sender))
            .collect()
    }

    const KERNEL: Option<i64> = Some(-1);
    const RUNNER: Option<i64> = Some(4242);
    const UNKNOWN: Option<i64> = None;
    use Verdict::{Forced, Repeat};

    #[test]
    fn one_press_under_a_forwarding_runner_is_one_request_however_late_the_copy() {
        for ms in [0, 1, 20, 300, 900] {
            assert_eq!(
                verdicts(KERNEL, &[(ms, RUNNER)]),
                vec![Repeat],
                "copy {ms} ms late"
            );
            assert_eq!(verdicts(RUNNER, &[(ms, KERNEL)]), vec![Repeat]);
        }
    }

    #[test]
    fn the_same_sender_twice_quickly_forces() {
        assert_eq!(
            verdicts(KERNEL, &[(150, KERNEL)]),
            vec![Forced],
            "two presses, run directly"
        );
        assert_eq!(
            verdicts(KERNEL, &[(5, RUNNER), (150, KERNEL)]),
            vec![Repeat, Forced],
            "two presses under a runner"
        );
        assert_eq!(
            verdicts(KERNEL, &[(150, RUNNER), (155, RUNNER)]),
            vec![Repeat, Forced],
            "the terminal's second signal coalesced away: the runner's copies repeat"
        );
    }

    #[test]
    fn the_same_sender_again_after_the_window_never_forces() {
        let loop_every_second: Vec<_> = (1..=10).map(|n| (n * 1_000, RUNNER)).collect();
        assert!(
            verdicts(RUNNER, &loop_every_second)
                .iter()
                .all(|v| *v == Repeat)
        );
        assert_eq!(verdicts(KERNEL, &[(600, KERNEL)]), vec![Repeat]);
    }

    #[test]
    fn two_senders_are_one_request() {
        assert_eq!(
            verdicts(Some(0), &[(10, KERNEL)]),
            vec![Repeat],
            "a SIGTERM from docker stop, then Ctrl-C"
        );
        assert_eq!(
            verdicts(Some(9001), &[(0, RUNNER)]),
            vec![Repeat],
            "a user's kill on the process group, and the runner's forwarded copy"
        );
    }

    #[test]
    fn three_from_three_senders_inside_a_second_force() {
        assert_eq!(
            verdicts(Some(11), &[(100, Some(12)), (200, Some(13))]),
            vec![Repeat, Forced],
            "/bin/kill three times"
        );
        assert_eq!(
            verdicts(Some(11), &[(600, Some(12)), (1_200, Some(13))]),
            vec![Repeat, Repeat],
        );
    }

    #[test]
    fn without_senders_three_inside_a_second_force_and_two_never_do() {
        for ms in [0, 1, 20, 500, 999] {
            assert_eq!(
                verdicts(UNKNOWN, &[(ms, UNKNOWN)]),
                vec![Repeat],
                "two, {ms} ms apart"
            );
        }
        assert_eq!(
            verdicts(UNKNOWN, &[(300, UNKNOWN), (600, UNKNOWN)]),
            vec![Repeat, Forced]
        );
        assert_eq!(
            verdicts(
                UNKNOWN,
                &[(600, UNKNOWN), (1_200, UNKNOWN), (1_800, UNKNOWN)]
            ),
            vec![Repeat, Repeat, Repeat],
            "spread wider than the window"
        );
        let slow: Vec<_> = (1..=5).map(|n| (n * 2_000, UNKNOWN)).collect();
        assert!(verdicts(UNKNOWN, &slow).iter().all(|v| *v == Repeat));
    }

    #[test]
    fn a_burst_late_in_a_long_drain_still_forces() {
        assert_eq!(
            verdicts(
                UNKNOWN,
                &[(20_000, UNKNOWN), (20_100, UNKNOWN), (20_200, UNKNOWN)]
            ),
            vec![Repeat, Repeat, Forced]
        );
        assert_eq!(
            verdicts(KERNEL, &[(20_000, KERNEL), (20_150, KERNEL)]),
            vec![Repeat, Forced]
        );
    }

    #[test]
    fn the_repeat_message_is_rate_limited_to_one_a_second() {
        let start = Instant::now();
        let mut throttle = Throttle::default();
        let shown: Vec<bool> = [0, 100, 900, 1_000, 1_100, 2_500]
            .iter()
            .map(|ms| throttle.allow(start + Duration::from_millis(*ms)))
            .collect();
        assert_eq!(shown, vec![true, false, false, true, false, true]);
    }
}

/// A lane the drain is stuck on is named, with since when and why.
#[test]
fn a_parked_lane_is_named_in_the_deadline_message() {
    let lane = |health| daemon::LaneSnapshot {
        applied_position: None,
        searchable_position: None,
        outstanding: 1,
        stall_seconds: None,
        wal_behind_bytes: None,
        backfill_checkpoints: 0,
        health,
        parked_since: Some("2026-11-19T12:03:11Z".to_owned()),
        next_probe: Some("2026-11-19T12:04:11Z".to_owned()),
        reason: Some("refused: id 42 (mapper_parsing_exception)".to_owned()),
    };
    assert_eq!(
        super::lane_note("users", &lane(daemon::LaneHealthState::Parked)).as_deref(),
        Some(
            "lane users parked since 2026-11-19T12:03:11Z (refused: id 42 (mapper_parsing_exception))"
        )
    );
    let open = daemon::LaneSnapshot {
        parked_since: None,
        next_probe: None,
        reason: None,
        ..lane(daemon::LaneHealthState::Open)
    };
    assert_eq!(super::lane_note("users", &open), None);
}
