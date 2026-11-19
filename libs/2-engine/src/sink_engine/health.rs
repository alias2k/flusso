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
}
