//! A lane's health: open, parked, or staged again (ADR 0012 stage 9,
//! `docs/adr/0012-stage-notes/stage-9.md`).
//!
//! A failure about one index stops that index's lane, not the sink:
//!
//! - **Parked**: the lane's front item was refused under `on_error = "stop"`,
//!   or the destination refuses writes to the index for now (a write block,
//!   a role without write permission). The lane is no longer granted; its
//!   items stay delivered and unacknowledged, so its marks, its fences and the
//!   slot stay where they are. When its backoff is due the front item is
//!   written alone (a **probe**): landed, the lane opens; refused again, the
//!   backoff doubles (1 s up to the engine's maximum). Another lane landing
//!   never resets it.
//! - **Restaging**: the index's target is gone. Once nothing of the lane is
//!   in flight its index is staged again ([`Sink::ensure_index`]) and a
//!   backfill requested, then the lane opens and writes its items again.
//!
//! The state outlives a run of the sink engine: a run that fails for another
//! reason leaves a parked lane parked, with its schedule.
//!
//! ```text
//! open ──refused / blocked──▶ parked ──probe lands──▶ open
//!   │                           │  ▲
//!   │                           └──┘ probe refused: backoff × 2
//!   └──missing──▶ restaging ──staged──▶ open
//! ```
//!
//! [`Sink::ensure_index`]: sink::Sink::ensure_index
//!
//! ```
//! use engine::{LaneHealth, ParkReason};
//!
//! let open = LaneHealth::Open;
//! assert_eq!(open.kind(), "open");
//! let reason = ParkReason::Blocked { reason: "index [orders] blocked".to_owned() };
//! assert_eq!(reason.kind(), "blocked");
//! ```

use std::time::Duration;

use tokio::time::Instant;

/// The first backoff of a parked lane: its first probe comes this long after
/// it parked.
pub(crate) const FIRST_BACKOFF: Duration = Duration::from_secs(1);

/// The longest a parked lane waits between probes, unless the engine is told
/// otherwise ([`SinkEngine::with_max_probe_backoff`](crate::SinkEngine::with_max_probe_backoff)).
pub(crate) const MAX_BACKOFF: Duration = Duration::from_secs(60);

/// Where one `(index, sink)` lane is.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum LaneHealth {
    /// Written as usual.
    Open,
    /// Not granted: its front item is written again alone at `next_probe`.
    Parked {
        /// Why it parked (the last probe's answer, once one returned).
        reason: ParkReason,
        /// When it parked.
        since: Instant,
        /// When its front item is written again.
        next_probe: Instant,
    },
    /// Its target was gone: its index is being staged again.
    Restaging {
        /// When the target was found missing.
        since: Instant,
        /// What the destination said.
        reason: String,
    },
}

impl LaneHealth {
    /// The health's name in `/status` and the logs: `open`, `parked` (a
    /// refusal), `blocked` or `restaging`.
    pub fn kind(&self) -> &'static str {
        match self {
            LaneHealth::Open => "open",
            LaneHealth::Parked { reason, .. } => reason.kind(),
            LaneHealth::Restaging { .. } => "restaging",
        }
    }

    /// Whether the lane is written as usual.
    pub fn is_open(&self) -> bool {
        matches!(self, LaneHealth::Open)
    }
}

/// Why a lane parked.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ParkReason {
    /// Documents of its front item were refused, and the index's `on_error`
    /// is `stop`.
    Refused {
        /// The refused documents' ids (at most a few, for the log and
        /// `/status`).
        ids: Vec<String>,
        /// The first refusal's reason.
        reason: String,
    },
    /// The destination refuses every write to the index for now.
    Blocked {
        /// What the destination said.
        reason: String,
    },
}

impl ParkReason {
    /// `parked` for a refusal, `blocked` for a block.
    pub fn kind(&self) -> &'static str {
        match self {
            ParkReason::Refused { .. } => "parked",
            ParkReason::Blocked { .. } => "blocked",
        }
    }

    /// The `why` label of `flusso.lane.parked`: `refused` or `blocked`.
    pub fn label(&self) -> &'static str {
        match self {
            ParkReason::Refused { .. } => "refused",
            ParkReason::Blocked { .. } => "blocked",
        }
    }
}

impl std::fmt::Display for ParkReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ParkReason::Refused { ids, reason } => {
                write!(f, "refused: id {}", ids.join(", "))?;
                if !reason.is_empty() {
                    write!(f, " ({reason})")?;
                }
                Ok(())
            }
            ParkReason::Blocked { reason } => write!(f, "blocked: {reason}"),
        }
    }
}

/// How a parked lane's probe returned.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ProbeOutcome {
    /// It landed: the lane is open.
    Landed,
    /// Refused again under `stop`.
    Refused,
    /// The index is still blocked.
    Blocked,
    /// The index's target is gone: the lane is staged again.
    Missing,
    /// The write failed as a whole (the sink's failure, not the lane's).
    Failed,
}

impl ProbeOutcome {
    /// The `outcome` label of `flusso.lane.probes`.
    pub fn label(self) -> &'static str {
        match self {
            ProbeOutcome::Landed => "landed",
            ProbeOutcome::Refused => "refused",
            ProbeOutcome::Blocked => "blocked",
            ProbeOutcome::Missing => "missing",
            ProbeOutcome::Failed => "failed",
        }
    }
}

/// A lane's health with what the engine needs to schedule it.
#[derive(Debug, Clone)]
pub(crate) struct LaneState {
    pub(crate) health: LaneHealth,
    /// The wait before the next probe (parked) or staging attempt
    /// (restaging), once the current one fails.
    pub(crate) backoff: Duration,
    /// When the lane is next probed or staged.
    pub(crate) due: Instant,
    /// Probes made since it parked.
    pub(crate) probes: u32,
    /// Opened by a staging, and no write of it has landed since: a target
    /// missing again is staged after the backoff, not at once, so a target
    /// staging can't cure is never a loop of writes and stagings.
    pub(crate) unproven: bool,
}

impl Default for LaneState {
    fn default() -> Self {
        Self {
            health: LaneHealth::Open,
            backoff: FIRST_BACKOFF,
            due: Instant::now(),
            probes: 0,
            unproven: false,
        }
    }
}

impl LaneState {
    /// Whether the lane is written as usual.
    pub(crate) fn is_open(&self) -> bool {
        self.health.is_open()
    }

    /// Park on `reason`: an open lane starts its backoff at one second; a
    /// parked lane whose probe returned `reason` doubles it, up to `max`; a
    /// lane being staged again stays so (its staging comes first). Whether the
    /// health changed.
    pub(crate) fn park(
        &mut self,
        reason: ParkReason,
        probe: bool,
        now: Instant,
        max: Duration,
    ) -> bool {
        match &self.health {
            LaneHealth::Open => {
                self.backoff = FIRST_BACKOFF;
                self.probes = 0;
                self.due = now + self.backoff;
                self.health = LaneHealth::Parked {
                    reason,
                    since: now,
                    next_probe: self.due,
                };
                true
            }
            LaneHealth::Parked { since, .. } if probe => {
                self.backoff = (self.backoff * 2).min(max);
                self.due = now + self.backoff;
                self.health = LaneHealth::Parked {
                    reason,
                    since: *since,
                    next_probe: self.due,
                };
                true
            }
            LaneHealth::Parked { .. } | LaneHealth::Restaging { .. } => false,
        }
    }

    /// The index's target is gone: stage it again, at once the first time;
    /// after the backoff, doubled up to `max`, when no write of the lane
    /// landed since its last staging.
    pub(crate) fn restage(&mut self, reason: String, now: Instant, max: Duration) -> bool {
        if matches!(self.health, LaneHealth::Restaging { .. }) {
            return false;
        }
        if self.unproven {
            self.due = now + self.backoff;
            self.backoff = (self.backoff * 2).min(max);
        } else {
            self.backoff = FIRST_BACKOFF;
            self.due = now;
        }
        self.health = LaneHealth::Restaging { since: now, reason };
        true
    }

    /// The index is staged again: the lane opens, keeping its backoff until
    /// one of its writes lands. How long it was not open.
    pub(crate) fn reopen(&mut self, now: Instant) -> Option<Duration> {
        let since = match &self.health {
            LaneHealth::Open => return None,
            LaneHealth::Parked { since, .. } | LaneHealth::Restaging { since, .. } => *since,
        };
        self.health = LaneHealth::Open;
        self.probes = 0;
        self.unproven = true;
        Some(now.saturating_duration_since(since))
    }

    /// A write of the lane landed: whatever backoff it kept is forgotten.
    /// Whether there was one.
    pub(crate) fn landed(&mut self) -> bool {
        if !self.unproven || !self.is_open() {
            return false;
        }
        *self = Self::default();
        true
    }

    /// Whether the engine keeps this state: a lane not open, or one keeping
    /// its backoff.
    pub(crate) fn is_kept(&self) -> bool {
        !self.is_open() || self.unproven
    }

    /// Staging failed: try again after the backoff, doubled each time up to
    /// `max`.
    pub(crate) fn restage_failed(&mut self, now: Instant, max: Duration) {
        self.due = now + self.backoff;
        self.backoff = (self.backoff * 2).min(max);
    }

    /// Probe at once: a stop doesn't wait out the backoff.
    pub(crate) fn probe_now(&mut self, now: Instant) -> bool {
        if let LaneHealth::Parked { next_probe, .. } = &mut self.health
            && *next_probe > now
        {
            *next_probe = now;
            self.due = now;
            return true;
        }
        false
    }

    /// Open: a probe landed or the index is staged. How long it was not.
    pub(crate) fn open(&mut self, now: Instant) -> Option<Duration> {
        let since = match &self.health {
            LaneHealth::Open => return None,
            LaneHealth::Parked { since, .. } | LaneHealth::Restaging { since, .. } => *since,
        };
        *self = Self::default();
        Some(now.saturating_duration_since(since))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn blocked() -> ParkReason {
        ParkReason::Blocked {
            reason: "index write (api)".to_owned(),
        }
    }

    #[tokio::test(start_paused = true)]
    async fn the_backoff_doubles_on_each_failed_probe_up_to_the_maximum_and_resets_on_open() {
        let max = Duration::from_secs(4);
        let mut state = LaneState::default();
        let start = Instant::now();
        assert!(state.park(blocked(), false, start, max));
        assert_eq!(state.due, start + Duration::from_secs(1));
        assert!(
            !state.park(blocked(), false, start, max),
            "a write of the lane that was in flight when it parked moves nothing"
        );
        let mut waits = Vec::new();
        for _ in 0..4 {
            let now = state.due;
            state.park(blocked(), true, now, max);
            waits.push((state.due - now).as_secs());
        }
        assert_eq!(waits, [2, 4, 4, 4]);
        assert!(matches!(state.health, LaneHealth::Parked { since, .. } if since == start));
        assert_eq!(
            state.open(start + Duration::from_secs(20)),
            Some(Duration::from_secs(20))
        );
        assert!(state.is_open());
        assert_eq!(state.backoff, FIRST_BACKOFF);
    }

    #[tokio::test(start_paused = true)]
    async fn a_missing_target_wins_over_a_park() {
        let now = Instant::now();
        let mut state = LaneState::default();
        assert!(state.restage("gone".to_owned(), now, MAX_BACKOFF));
        assert!(!state.park(blocked(), true, now, MAX_BACKOFF));
        assert_eq!(state.health.kind(), "restaging");
        assert!(!state.restage("gone".to_owned(), now, MAX_BACKOFF));
    }

    /// A park in between keeps the staging's own backoff: a lane that never
    /// landed a write since its staging waits the staging backoff (not the
    /// park's, doubled by its probes); one whose write landed is staged again
    /// at once.
    #[tokio::test(start_paused = true)]
    async fn a_park_between_two_stagings_keeps_the_stagings_own_backoff() {
        let max = MAX_BACKOFF;
        let start = Instant::now();
        let mut unproven = LaneState::default();
        unproven.restage("gone".to_owned(), start, max);
        unproven.reopen(start);
        unproven.park(blocked(), false, start, max);
        for _ in 0..3 {
            let now = unproven.due;
            unproven.park(blocked(), true, now, max);
        }
        let now = unproven.due;
        assert!(unproven.restage("gone".to_owned(), now, max));
        assert_eq!(
            unproven.due - now,
            FIRST_BACKOFF,
            "the staging's backoff, not the park's 8 s"
        );

        let mut cured = LaneState::default();
        cured.restage("gone".to_owned(), start, max);
        cured.reopen(start);
        assert!(cured.landed());
        cured.park(blocked(), false, start, max);
        let now = cured.due;
        cured.park(blocked(), true, now, max);
        let now = cured.due;
        cured.restage("gone".to_owned(), now, max);
        assert_eq!(cured.due, now, "a write landed since: at once");
    }

    #[tokio::test(start_paused = true)]
    async fn a_reopened_lane_missing_again_waits_its_backoff_until_a_write_lands() {
        let max = Duration::from_secs(4);
        let start = Instant::now();
        let mut state = LaneState::default();
        state.restage("gone".to_owned(), start, max);
        assert_eq!(state.due, start, "the first staging goes at once");
        let mut waits = Vec::new();
        for _ in 0..4 {
            let now = state.due;
            state.reopen(now);
            assert!(state.is_open() && state.is_kept());
            state.restage("gone".to_owned(), now, max);
            waits.push((state.due - now).as_secs());
        }
        assert_eq!(waits, [1, 2, 4, 4]);
        state.reopen(state.due);
        assert!(state.landed(), "a landing write forgets the backoff");
        assert!(!state.is_kept());
        state.restage("gone".to_owned(), start, max);
        assert_eq!(state.due, start, "at once again");
    }
}
