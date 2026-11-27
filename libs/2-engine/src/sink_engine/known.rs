//! What this process knows of each lane's detach records, and whether a read
//! of them shows that another process changed them (ADR 0012 stage 10, the
//! cut the owner decided on 2026-11-27, and the review of the cut).
//!
//! flusso doesn't reconcile with another process's record changes: a read
//! that finds one ([`Change::Foreign`]) stops the deployment, and the next
//! start decides from the records with empty memory. So the one question here
//! is "did anyone but this process change these records since it last read or
//! wrote them?", answered conservatively: what this process can't show to be
//! its own, or unchanged, is foreign.
//!
//! - **Known** is a set of states the records may be in, each kept exactly
//!   or, where the sink decides a field, [`Expect::Any`]. A read is the same
//!   when one of them admits it, and is then what this process knows.
//! - **A read** leaves one state: exactly what it found.
//! - **An own write that returned `Ok`** leaves what the write leaves, from
//!   each state ([`Wrote`]). A write of the record (a detach, a rebuild, a
//!   demotion, a seed) is read back at once while the slot is still held
//!   (`slot`'s `read_back`), which settles it and learns the record's
//!   version; a write of the marker alone (a created generation, a reindex,
//!   a gap) keeps the record's version.
//! - **An own write whose outcome is unknown** (its call failed: it may have
//!   landed, in part or whole, or not at all) keeps every state it may have
//!   left: the one before, the one it leaves, and the one where only its
//!   marker was written. A read back while the slot is held settles which.
//!   Never "anything": only what this process's own write may have left.
//! - **A read under the same opening, the slot held throughout**, is adopted
//!   whatever it finds: no other process can write while this one holds the
//!   slot (the narrowed O29 rule, `slot`).
//! - With nothing known yet, a write that decides the record (a detach, a
//!   delete, a seed) is what this process knows.
//! - **Anything else** that differs is foreign.
//!
//! ```text
//! known                                found (a read)               →
//! nothing (never read here)            anything                      Same
//! anything                             read under the same held      Same
//!                                      opening
//! states (a read, or own writes)       one state admits it           Same
//!   an own write, Ok                     what it leaves
//!   an own write, outcome unknown        before, after, or its
//!                                        marker alone written
//! anything else                                                      Foreign
//! ```

use kernel::IndexName;
use sink::{DetachRecord, Detachment, Gap, RecordVersion};

/// The most states kept for one index: own writes of unknown outcome, one
/// after another with no read between them, each add a few.
const MOST_STATES: usize = 64;

/// A field of a known state: exactly this value, or whatever the sink holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Expect<T> {
    /// What a read found, or an own write decided.
    Exactly(T),
    /// An own write may have left any value (the sink decides it).
    Any,
}

impl<T: PartialEq> Expect<T> {
    fn admits(&self, found: &T) -> bool {
        match self {
            Expect::Exactly(known) => known == found,
            Expect::Any => true,
        }
    }
}

/// One state a lane's records may be in.
#[derive(Debug, Clone, PartialEq, Eq)]
struct State {
    record: Option<DetachRecord>,
    attempts: Expect<Option<u64>>,
    through: Expect<Option<String>>,
    gaps: Expect<Vec<Gap>>,
    /// The record's version, when known: as read, or kept by a write of the
    /// marker alone. `None` after an own write of the record.
    version: Option<RecordVersion>,
}

/// What this process knows of one lane's detach records.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Known {
    /// Every state the records may be in; never empty.
    states: Vec<State>,
    /// The opening ([`Rereads::read_begins`](crate::Rereads::read_begins))
    /// this was learnt under.
    at: u64,
}

/// One of this process's own record or marker writes.
#[derive(Debug, Clone)]
pub(crate) enum Wrote {
    /// A detach: the record says detached from `through`; one that can
    /// rebuild moves the marker first, written whole, unless a detach moved
    /// it already.
    Detach {
        live_only: bool,
        through: Option<String>,
    },
    /// A rebuild began: the record says rebuilding, under `attempts`
    /// (`None`: unknown, one more than before); the marker moves, written
    /// whole.
    Rebuild { attempts: Option<u64> },
    /// A demotion: the record says detached (live only when asked); the
    /// marker moves, or is put back, written whole.
    Demote { live_only: bool },
    /// The record deleted.
    Forget,
    /// A seed: the marker written whole; the record's delete is best effort
    /// (OpenSearch only warns when it fails), so the record may stay.
    Seed,
    /// The gaps kept with the marker: as the sink answered, or, when the
    /// call failed, as it was deciding to write them (`None`: never
    /// decided).
    Gaps(Option<Vec<Gap>>),
    /// The marker written whole, alone: a created generation, a reindex.
    Marker,
}

/// What a read of a lane's records says about who changed them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Change {
    /// Nothing but this process changed them: go on.
    Same,
    /// Another process changed them since this one last knew them: what.
    Foreign(String),
}

impl State {
    fn read(found: &Detachment) -> Self {
        Self {
            record: found.record.clone(),
            attempts: Expect::Exactly(found.attempts),
            through: Expect::Exactly(found.through.clone()),
            // A put-back writes the marker whole: the gaps read before it
            // are gone (the read found a half-moved marker, §5).
            gaps: Expect::Exactly(if found.restored.is_some() {
                Vec::new()
            } else {
                found.gaps.clone()
            }),
            version: found.version.clone(),
        }
    }

    /// Nothing read yet: every field but the record may hold anything. Only
    /// a write that decides the record starts from it.
    fn unknown() -> Self {
        Self {
            record: None,
            attempts: Expect::Any,
            through: Expect::Any,
            gaps: Expect::Any,
            version: None,
        }
    }

    fn live_only(&self) -> bool {
        matches!(
            self.record,
            Some(DetachRecord::Detached { live_only: true })
                | Some(DetachRecord::Rebuilding { live_only: true })
        )
    }

    /// This state with only the marker written whole: no gap, the record
    /// and its version untouched.
    fn marker_written(&self) -> Self {
        Self {
            gaps: Expect::Exactly(Vec::new()),
            ..self.clone()
        }
    }

    /// Every state `wrote`, returned `Ok`, leaves from this one.
    fn after(&self, wrote: &Wrote) -> Vec<Self> {
        let rewritten = Self {
            version: None,
            ..self.clone()
        };
        match wrote {
            Wrote::Detach { live_only, through } => {
                let detached = Self {
                    record: Some(DetachRecord::Detached {
                        live_only: *live_only,
                    }),
                    through: Expect::Exactly(through.clone()),
                    attempts: Expect::Any,
                    ..rewritten
                };
                if *live_only {
                    vec![detached]
                } else {
                    vec![detached.marker_written(), detached]
                }
            }
            Wrote::Rebuild { attempts } => {
                let attempts = match (attempts, &self.attempts) {
                    (Some(attempts), _) => Expect::Exactly(Some(*attempts)),
                    (None, Expect::Exactly(before)) => {
                        Expect::Exactly(Some(before.unwrap_or(0) + 1))
                    }
                    (None, Expect::Any) => Expect::Any,
                };
                vec![Self {
                    record: Some(DetachRecord::Rebuilding {
                        live_only: self.live_only(),
                    }),
                    attempts,
                    gaps: Expect::Exactly(Vec::new()),
                    ..rewritten
                }]
            }
            Wrote::Demote { live_only } => {
                let demoted = Self {
                    record: Some(DetachRecord::Detached {
                        live_only: *live_only || self.live_only(),
                    }),
                    ..rewritten
                };
                vec![demoted.marker_written(), demoted]
            }
            Wrote::Forget => vec![Self {
                record: None,
                attempts: Expect::Exactly(None),
                through: Expect::Exactly(None),
                ..rewritten
            }],
            Wrote::Seed => {
                let cleared = Self {
                    record: None,
                    attempts: Expect::Exactly(None),
                    through: Expect::Exactly(None),
                    gaps: Expect::Exactly(Vec::new()),
                    version: None,
                };
                if self.record.is_some() {
                    vec![cleared, self.marker_written()]
                } else {
                    vec![cleared]
                }
            }
            Wrote::Gaps(Some(gaps)) => vec![Self {
                gaps: Expect::Exactly(gaps.clone()),
                ..self.clone()
            }],
            Wrote::Gaps(None) => vec![self.clone()],
            Wrote::Marker => vec![self.marker_written()],
        }
    }

    /// Whether `wrote` writes the marker whole.
    fn writes_the_marker(wrote: &Wrote) -> bool {
        match wrote {
            Wrote::Detach { live_only, .. } => !live_only,
            Wrote::Rebuild { .. } | Wrote::Demote { .. } | Wrote::Seed | Wrote::Marker => true,
            Wrote::Forget | Wrote::Gaps(_) => false,
        }
    }

    /// The first field `found` differs on, if any.
    fn differs(&self, found: &Detachment) -> Option<String> {
        if self.record != found.record {
            return Some(format!(
                "the record was {:?}, now {:?}",
                self.record, found.record
            ));
        }
        if !self.attempts.admits(&found.attempts) {
            return Some(format!(
                "the rebuild attempts were {:?}, now {:?}",
                self.attempts, found.attempts
            ));
        }
        if !self.through.admits(&found.through) {
            return Some(format!(
                "the detach was from {:?}, now from {:?}",
                self.through, found.through
            ));
        }
        if !self.gaps.admits(&found.gaps) {
            return Some(format!(
                "the gaps were {:?}, now {:?}",
                self.gaps, found.gaps
            ));
        }
        if let (Some(known), Some(now)) = (&self.version, &found.version)
            && known != now
        {
            return Some(format!(
                "the record was rewritten (version {} then {})",
                known.as_str(),
                now.as_str()
            ));
        }
        None
    }
}

impl Known {
    /// Exactly what a read found, learnt under opening `at`.
    pub(crate) fn read(found: &Detachment, at: u64) -> Self {
        Self {
            states: vec![State::read(found)],
            at,
        }
    }

    fn of(states: Vec<State>, at: u64) -> Self {
        let mut kept: Vec<State> = Vec::with_capacity(states.len());
        for state in states {
            if !kept.contains(&state) {
                kept.push(state);
            }
        }
        // Past the bound, the oldest go: a state dropped can only make a
        // later read of this process's own write foreign (a stop), never
        // admit another process's.
        let excess = kept.len().saturating_sub(MOST_STATES);
        kept.drain(..excess);
        Self { states: kept, at }
    }

    /// What `wrote`, returned `Ok` under opening `at`, leaves.
    pub(crate) fn wrote(&self, wrote: &Wrote, at: u64) -> Self {
        Self::of(
            self.states
                .iter()
                .flat_map(|state| state.after(wrote))
                .collect(),
            at,
        )
    }

    /// `wrote` failed under opening `at`: it may have landed in whole, in
    /// part (its marker alone), or not at all.
    pub(crate) fn unsure(&self, wrote: &Wrote, at: u64) -> Self {
        let mut states = self.states.clone();
        for state in &self.states {
            states.extend(state.after(wrote));
            if State::writes_the_marker(wrote) {
                states.push(state.marker_written());
            }
        }
        Self::of(states, at)
    }

    /// Why `found` is none of the states known, if it isn't: told against
    /// a state holding the record found, when one does.
    fn differs(&self, found: &Detachment) -> Option<String> {
        let mut why = None;
        for state in &self.states {
            match state.differs(found) {
                None => return None,
                Some(differs) if state.record == found.record => why = Some(differs),
                Some(differs) => {
                    why.get_or_insert(differs);
                }
            }
        }
        why
    }

    /// Whether the records may keep a gap.
    fn keeps_gaps(&self) -> bool {
        self.states.iter().any(|state| match &state.gaps {
            Expect::Exactly(gaps) => !gaps.is_empty(),
            Expect::Any => true,
        })
    }
}

/// Whether `found`, a read of `index`'s records, shows a change by another
/// process since `known`. `same_opening`: the read began under the opening
/// `known` was learnt under, and that opening's stream still held the slot
/// once the read returned, so no other process could have written.
pub(crate) fn change(
    index: &IndexName,
    known: Option<&Known>,
    found: &Detachment,
    read_at: u64,
    same_opening: bool,
) -> Change {
    let Some(known) = known else {
        return Change::Same;
    };
    if same_opening && known.at == read_at {
        return Change::Same;
    }
    match known.differs(found) {
        None => Change::Same,
        Some(differs) => Change::Foreign(format!("{index}: {differs}")),
    }
}

/// [`Known`] for each of a sink engine's indexes: what every read judges
/// against, and what every own write updates (`slot` and `Plain`).
#[derive(Debug, Default)]
pub(crate) struct KnownRecords(std::sync::Mutex<std::collections::BTreeMap<IndexName, Known>>);

impl KnownRecords {
    fn lock(&self) -> std::sync::MutexGuard<'_, std::collections::BTreeMap<IndexName, Known>> {
        self.0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Judge a read of `index`'s records ([`change`]); one that is the same
    /// is what this process knows from now on.
    pub(crate) fn judge(
        &self,
        index: &IndexName,
        found: &Detachment,
        read_at: u64,
        same_opening: bool,
    ) -> Change {
        let mut known = self.lock();
        let judged = change(index, known.get(index), found, read_at, same_opening);
        if judged == Change::Same {
            known.insert(index.clone(), Known::read(found, read_at));
        }
        judged
    }

    /// An own write of `index`'s records returned `Ok`, under opening `at`
    /// (`None`: it moves no record, the opening known stays). With nothing
    /// known yet, a write that decides the record (a detach, a delete) is
    /// what this process knows; any other leaves nothing known, and the
    /// next read is adopted.
    pub(crate) fn wrote(&self, index: &IndexName, wrote: &Wrote, at: Option<u64>) {
        let mut known = self.lock();
        let after = match known.get(index) {
            Some(before) => before.wrote(wrote, at.unwrap_or(before.at)),
            None if matches!(wrote, Wrote::Detach { .. } | Wrote::Forget) => {
                let at = at.unwrap_or(0);
                Known {
                    states: vec![State::unknown()],
                    at,
                }
                .wrote(wrote, at)
            }
            None => return,
        };
        known.insert(index.clone(), after);
    }

    /// An own write of `index`'s records failed under opening `at`: whether
    /// it landed is unknown. With nothing known yet, nothing is: the state
    /// before it can't be named, and the next read is adopted.
    pub(crate) fn unsure(&self, index: &IndexName, wrote: &Wrote, at: Option<u64>) {
        let mut known = self.lock();
        if let Some(before) = known.get(index) {
            let after = before.unsure(wrote, at.unwrap_or(before.at));
            known.insert(index.clone(), after);
        }
    }

    /// The opening `index`'s records were last read or written under, if
    /// any.
    pub(crate) fn learnt_at(&self, index: &IndexName) -> Option<u64> {
        self.lock().get(index).map(|known| known.at)
    }

    /// Whether `index`'s records may keep a gap: one is known, or an own
    /// write may have left one. Nothing known yet: none known.
    pub(crate) fn keeps_gaps(&self, index: &IndexName) -> bool {
        self.lock().get(index).is_some_and(Known::keeps_gaps)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use sink::MarkerAt;

    fn index() -> IndexName {
        IndexName::try_new("users").unwrap()
    }

    fn detached(through: &str, version: &str) -> Detachment {
        let mut found = Detachment::recorded(
            DetachRecord::Detached { live_only: false },
            Some(MarkerAt::Newer { seeded: false }),
            Some(RecordVersion::new(version)),
        );
        found.through = Some(through.to_owned());
        found
    }

    fn gap(from: &str) -> Gap {
        Gap {
            from: Some(from.to_owned()),
            to: Some("9".to_owned()),
        }
    }

    /// Found by the world at 50,000 cases: a lane this process detached
    /// before any read of its records succeeded. Its own detach is what it
    /// knows, so another process's reattach is foreign; its own record read
    /// back is the same.
    #[test]
    fn an_own_write_with_nothing_read_before_is_known() {
        let known = KnownRecords::default();
        known.wrote(
            &index(),
            &Wrote::Detach {
                live_only: false,
                through: Some("14".to_owned()),
            },
            Some(1),
        );
        let mut reattached = Detachment::default();
        reattached.gaps = vec![gap("14")];
        assert!(matches!(
            known.judge(&index(), &reattached, 2, false),
            Change::Foreign(_)
        ));
        let ours = detached("14", "3:1");
        assert_eq!(known.judge(&index(), &ours, 2, false), Change::Same);

        let forgotten = KnownRecords::default();
        forgotten.wrote(&index(), &Wrote::Forget, Some(1));
        assert!(matches!(
            forgotten.judge(&index(), &detached("1", "1:1"), 2, false),
            Change::Foreign(_)
        ));
    }

    /// Found by the world at 50,000 cases: creating a generation, or
    /// writing a gap, writes the marker, never the record. The record's
    /// version read before stays known, so another process's identical
    /// record written since is told apart by its version.
    #[test]
    fn a_marker_write_keeps_the_record_version() {
        let mut rebuilding = Detachment::recorded(
            DetachRecord::Rebuilding { live_only: false },
            Some(MarkerAt::Newer { seeded: false }),
            Some(RecordVersion::new("1:1")),
        );
        rebuilding.attempts = Some(1);
        for wrote in [Wrote::Marker, Wrote::Gaps(Some(Vec::new()))] {
            let known = KnownRecords::default();
            assert_eq!(known.judge(&index(), &rebuilding, 1, false), Change::Same);
            known.wrote(&index(), &wrote, None);
            let mut again = rebuilding.clone();
            again.version = Some(RecordVersion::new("3:1"));
            assert!(
                matches!(known.judge(&index(), &again, 2, false), Change::Foreign(_)),
                "after {wrote:?}"
            );
        }
    }

    #[test]
    fn nothing_known_is_adopted() {
        let found = detached("1", "1:1");
        assert_eq!(change(&index(), None, &found, 1, false), Change::Same);
    }

    #[test]
    fn a_read_under_the_same_held_opening_is_adopted() {
        let known = Known::read(&Detachment::default(), 2);
        let found = detached("1", "1:1");
        assert_eq!(
            change(&index(), Some(&known), &found, 2, true),
            Change::Same
        );
        assert!(matches!(
            change(&index(), Some(&known), &found, 2, false),
            Change::Foreign(_)
        ));
        assert!(matches!(
            change(&index(), Some(&known), &found, 3, true),
            Change::Foreign(_)
        ));
    }

    #[test]
    fn the_same_records_read_again_are_the_same() {
        let found = detached("1", "1:1");
        let known = Known::read(&found, 1);
        assert_eq!(
            change(&index(), Some(&known), &found, 2, false),
            Change::Same
        );
    }

    #[test]
    fn every_field_another_process_changes_is_foreign() {
        let before = detached("1", "1:1");
        let known = Known::read(&before, 1);
        let mut gone = Detachment::default();
        gone.gaps = vec![gap("1")];
        let mut rewritten = before.clone();
        rewritten.version = Some(RecordVersion::new("2:1"));
        let mut moved = before.clone();
        moved.through = Some("5".to_owned());
        let mut rebuilding = before.clone();
        rebuilding.record = Some(DetachRecord::Rebuilding { live_only: false });
        rebuilding.attempts = Some(1);
        let mut gapped = before.clone();
        gapped.gaps = vec![gap("1")];
        for found in [gone, rewritten, moved, rebuilding, gapped] {
            assert!(
                matches!(
                    change(&index(), Some(&known), &found, 2, false),
                    Change::Foreign(_)
                ),
                "{found:?}"
            );
        }
        let attached = Known::read(&Detachment::default(), 1);
        assert!(matches!(
            change(&index(), Some(&attached), &before, 2, false),
            Change::Foreign(_)
        ));
        let mut only_a_gap = Detachment::default();
        only_a_gap.gaps = vec![gap("1")];
        assert!(matches!(
            change(&index(), Some(&attached), &only_a_gap, 2, false),
            Change::Foreign(_)
        ));
    }

    #[test]
    fn what_an_own_write_leaves_is_the_same() {
        let attached = Known::read(&Detachment::default(), 1);
        let detach = attached.wrote(
            &Wrote::Detach {
                live_only: false,
                through: Some("4".to_owned()),
            },
            1,
        );
        let mut found = detached("4", "3:1");
        found.attempts = Some(2);
        assert_eq!(
            change(&index(), Some(&detach), &found, 2, false),
            Change::Same
        );
        let rebuild = Known::read(&found, 2).wrote(&Wrote::Rebuild { attempts: Some(3) }, 2);
        let mut rebuilding = found.clone();
        rebuilding.record = Some(DetachRecord::Rebuilding { live_only: false });
        rebuilding.attempts = Some(3);
        assert_eq!(
            change(&index(), Some(&rebuild), &rebuilding, 3, false),
            Change::Same
        );
        rebuilding.attempts = Some(4);
        assert!(matches!(
            change(&index(), Some(&rebuild), &rebuilding, 3, false),
            Change::Foreign(_)
        ));
        let seed = rebuild.wrote(&Wrote::Seed, 3);
        assert_eq!(
            change(&index(), Some(&seed), &Detachment::default(), 4, false),
            Change::Same
        );
    }

    #[test]
    fn an_own_detach_of_unknown_outcome_is_recognised_by_its_through() {
        let attached = Known::read(&Detachment::default(), 1);
        let unsure = attached.unsure(
            &Wrote::Detach {
                live_only: false,
                through: Some("4".to_owned()),
            },
            1,
        );
        assert_eq!(
            change(&index(), Some(&unsure), &Detachment::default(), 2, false),
            Change::Same
        );
        assert_eq!(
            change(&index(), Some(&unsure), &detached("4", "1:1"), 2, false),
            Change::Same
        );
        assert!(matches!(
            change(&index(), Some(&unsure), &detached("7", "1:1"), 2, false),
            Change::Foreign(_)
        ));
    }

    fn with(mut found: Detachment, change: impl FnOnce(&mut Detachment)) -> Detachment {
        change(&mut found);
        found
    }

    fn rebuilding(attempts: u64, version: &str) -> Detachment {
        let mut found = Detachment::recorded(
            DetachRecord::Rebuilding { live_only: false },
            Some(MarkerAt::Newer { seeded: false }),
            Some(RecordVersion::new(version)),
        );
        found.attempts = Some(attempts);
        found.through = Some("1".to_owned());
        found
    }

    /// An own write of unknown outcome from `before`: each of `admitted` is
    /// the same (the state before, every one the write may leave, its
    /// marker alone written), each of `foreign` is another process's.
    fn unknown_outcome(
        before: &Detachment,
        wrote: &Wrote,
        admitted: &[Detachment],
        foreign: &[Detachment],
    ) {
        let known = Known::read(before, 1).unsure(wrote, 1);
        for found in std::iter::once(before).chain(admitted) {
            assert_eq!(
                change(&index(), Some(&known), found, 2, false),
                Change::Same,
                "{wrote:?}: {found:?}"
            );
        }
        for found in foreign {
            assert!(
                matches!(
                    change(&index(), Some(&known), found, 2, false),
                    Change::Foreign(_)
                ),
                "{wrote:?}: {found:?}"
            );
        }
    }

    /// The review of the cut, B-2, one write kind at a time: whatever an own
    /// write whose call failed may have left is the same, and nothing else.
    #[test]
    fn a_detach_of_unknown_outcome_admits_before_and_after_only() {
        let attached = with(Detachment::default(), |found| found.gaps = vec![gap("1")]);
        let detached_here = with(detached("4", "9:1"), |found| found.attempts = Some(2));
        unknown_outcome(
            &attached,
            &Wrote::Detach {
                live_only: false,
                through: Some("4".to_owned()),
            },
            &[
                detached_here.clone(),
                with(detached_here.clone(), |found| found.gaps = vec![gap("1")]),
                Detachment::default(),
            ],
            &[
                detached("7", "9:1"),
                with(Detachment::default(), |found| found.gaps = vec![gap("2")]),
            ],
        );
    }

    #[test]
    fn a_rebuild_of_unknown_outcome_admits_before_and_after_only() {
        let before = with(detached("1", "1:1"), |found| {
            found.attempts = Some(1);
            found.gaps = vec![gap("1")];
        });
        unknown_outcome(
            &before,
            &Wrote::Rebuild { attempts: None },
            &[
                rebuilding(2, "2:1"),
                with(before.clone(), |found| found.gaps = Vec::new()),
            ],
            &[
                rebuilding(3, "2:1"),
                with(rebuilding(2, "2:1"), |found| {
                    found.through = Some("8".to_owned())
                }),
            ],
        );
    }

    #[test]
    fn a_demotion_of_unknown_outcome_admits_before_and_after_only() {
        let before = rebuilding(1, "1:1");
        let demoted = with(before.clone(), |found| {
            found.record = Some(DetachRecord::Detached { live_only: false });
            found.version = Some(RecordVersion::new("2:1"));
        });
        unknown_outcome(
            &before,
            &Wrote::Demote { live_only: false },
            &[demoted.clone()],
            &[
                with(demoted.clone(), |found| {
                    found.through = Some("8".to_owned())
                }),
                with(demoted, |found| {
                    found.record = Some(DetachRecord::Detached { live_only: true });
                }),
            ],
        );
    }

    #[test]
    fn a_delete_of_unknown_outcome_admits_before_and_after_only() {
        let before = detached("1", "1:1");
        unknown_outcome(
            &before,
            &Wrote::Forget,
            &[Detachment::default()],
            &[
                with(before.clone(), |found| {
                    found.version = Some(RecordVersion::new("5:1"))
                }),
                with(Detachment::default(), |found| found.gaps = vec![gap("3")]),
            ],
        );
    }

    #[test]
    fn a_seed_of_unknown_outcome_admits_before_and_after_only() {
        let before = with(rebuilding(1, "1:1"), |found| found.gaps = vec![gap("1")]);
        unknown_outcome(
            &before,
            &Wrote::Seed,
            &[
                Detachment::default(),
                with(before.clone(), |found| found.gaps = Vec::new()),
            ],
            &[
                rebuilding(2, "1:1"),
                with(before.clone(), |found| {
                    found.version = Some(RecordVersion::new("4:1"))
                }),
            ],
        );
    }

    #[test]
    fn a_gap_write_of_unknown_outcome_admits_before_and_after_only() {
        let before = with(Detachment::default(), |found| found.gaps = vec![gap("1")]);
        let decided = vec![gap("1"), gap("5")];
        unknown_outcome(
            &before,
            &Wrote::Gaps(Some(decided.clone())),
            &[with(Detachment::default(), |found| found.gaps = decided)],
            &[
                with(Detachment::default(), |found| found.gaps = vec![gap("6")]),
                Detachment::default(),
            ],
        );
    }

    #[test]
    fn a_marker_write_of_unknown_outcome_admits_before_and_after_only() {
        let before = with(detached("1", "1:1"), |found| found.gaps = vec![gap("1")]);
        unknown_outcome(
            &before,
            &Wrote::Marker,
            &[with(before.clone(), |found| found.gaps = Vec::new())],
            &[
                with(before.clone(), |found| {
                    found.version = Some(RecordVersion::new("2:1"))
                }),
                with(before.clone(), |found| found.gaps = vec![gap("7")]),
            ],
        );
    }

    /// The review of the cut, B-1: a seed that returned `Ok` may have left
    /// the finished rebuild's record (its delete is best effort).
    #[test]
    fn a_seed_may_leave_the_finished_rebuilds_record() {
        let before = rebuilding(1, "1:1");
        let known = Known::read(&before, 1).wrote(&Wrote::Seed, 1);
        let left = with(before.clone(), |found| {
            found.marker = Some(MarkerAt::Newer { seeded: true });
        });
        for found in [Detachment::default(), left] {
            assert_eq!(
                change(&index(), Some(&known), &found, 2, false),
                Change::Same
            );
        }
        assert!(matches!(
            change(&index(), Some(&known), &rebuilding(2, "1:1"), 2, false),
            Change::Foreign(_)
        ));
    }

    /// With nothing known, an own write of unknown outcome leaves nothing
    /// known: the next read is adopted.
    #[test]
    fn an_unknown_outcome_with_nothing_known_adopts_the_next_read() {
        let known = KnownRecords::default();
        known.unsure(&index(), &Wrote::Forget, Some(1));
        assert_eq!(
            known.judge(&index(), &detached("3", "1:1"), 2, false),
            Change::Same
        );
    }
}
