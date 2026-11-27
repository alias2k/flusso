//! What this process knows of each lane's detach records, and whether a read
//! of them shows that another process changed them (ADR 0012 stage 10, the
//! eighth re-check: the cut the owner decided on 2026-11-27).
//!
//! flusso doesn't reconcile with another process's record changes: a read
//! that finds one ([`Change::Foreign`]) stops the deployment, and the next
//! start decides from the records with empty memory. So the one question here
//! is "did anyone but this process change these records since it last read or
//! wrote them?", answered conservatively: what this process can't prove is
//! its own, or unchanged, is foreign.
//!
//! - **Known** is the last read of a lane's records, kept exactly, or, after
//!   one of this process's own writes, what that write leaves: exact where
//!   the write decides it, [`Expect::Any`] where the sink may leave either.
//! - **A read under the same opening, the slot held throughout**, is adopted
//!   whatever it finds: no other process can write while this one holds the
//!   slot (the narrowed O29 rule, `slot`). This is also how an own write
//!   whose outcome is unknown is learnt, when a read follows it in time.
//! - **An own detach whose outcome is unknown** (the call failed after its
//!   record may have landed) is recognised by its content: a `Detached`
//!   record with this process's `through`.
//! - **Anything else** that differs is foreign.
//!
//! ```text
//! known (last read, or own write)   found (a read)           →
//! none (never read here)            anything                  Same (adopted)
//! any                               read under the same       Same (adopted)
//!                                   held opening
//! exact fields                      equal on each exact field Same
//!                                   (version too, when both
//!                                   known)
//! own detach, outcome unknown       Detached, this through    Same
//! anything else                                               Foreign
//! ```

use kernel::IndexName;
use sink::{DetachRecord, Detachment, Gap, RecordVersion};

/// A field of [`Known`]: exactly this value, or whatever the sink holds.
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

/// What this process knows of one lane's detach records.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Known {
    record: Option<DetachRecord>,
    attempts: Expect<Option<u64>>,
    through: Expect<Option<String>>,
    gaps: Expect<Vec<Gap>>,
    /// The record's version as read; `None` after an own write.
    version: Option<RecordVersion>,
    /// The opening ([`Rereads::read_begins`](crate::Rereads::read_begins))
    /// this was learnt under.
    at: u64,
    /// An own detach whose outcome is unknown: the record it writes, by its
    /// `live_only` and `through`.
    unsure_detach: Option<(bool, Option<String>)>,
}

/// One of this process's own record writes, as it leaves the records when
/// it returned `Ok`.
#[derive(Debug, Clone)]
pub(crate) enum Wrote {
    /// A detach: the record says detached from `through`.
    Detach {
        live_only: bool,
        through: Option<String>,
    },
    /// A rebuild began: the record says rebuilding, under `attempts`.
    Rebuild { attempts: u64 },
    /// A demotion: the record says detached (live only when asked).
    Demote { live_only: bool },
    /// The record deleted.
    Forget,
    /// A seed: the marker written whole, the record and the gaps gone.
    Seed,
    /// The gaps kept with the marker, as the sink answered.
    Gaps(Vec<Gap>),
    /// An index created: its marker written whole, with no gap.
    Created,
}

/// What a read of a lane's records says about who changed them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Change {
    /// Nothing but this process changed them: go on.
    Same,
    /// Another process changed them since this one last knew them: what.
    Foreign(String),
}

impl Known {
    /// Exactly what a read found, learnt under opening `at`.
    pub(crate) fn read(found: &Detachment, at: u64) -> Self {
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
            at,
            unsure_detach: None,
        }
    }

    /// What `wrote`, returned `Ok` under opening `at`, leaves.
    pub(crate) fn wrote(&self, wrote: &Wrote, at: u64) -> Self {
        let mut next = Self {
            version: None,
            at,
            unsure_detach: None,
            ..self.clone()
        };
        match wrote {
            Wrote::Detach { live_only, through } => {
                next.record = Some(DetachRecord::Detached {
                    live_only: *live_only,
                });
                next.through = Expect::Exactly(through.clone());
                next.attempts = Expect::Any;
                if !live_only {
                    // The marker moves first, written whole.
                    next.gaps = Expect::Any;
                }
            }
            Wrote::Rebuild { attempts } => {
                let live_only = matches!(
                    self.record,
                    Some(DetachRecord::Detached { live_only: true })
                        | Some(DetachRecord::Rebuilding { live_only: true })
                );
                next.record = Some(DetachRecord::Rebuilding { live_only });
                next.attempts = Expect::Exactly(Some(*attempts));
                next.through = Expect::Any;
                next.gaps = Expect::Any;
            }
            Wrote::Demote { live_only } => {
                let was = matches!(
                    self.record,
                    Some(DetachRecord::Detached { live_only: true })
                        | Some(DetachRecord::Rebuilding { live_only: true })
                );
                next.record = Some(DetachRecord::Detached {
                    live_only: *live_only || was,
                });
                next.attempts = Expect::Any;
                next.through = Expect::Any;
                next.gaps = Expect::Any;
            }
            Wrote::Forget => {
                next.record = None;
                next.attempts = Expect::Exactly(None);
                next.through = Expect::Exactly(None);
            }
            Wrote::Seed => {
                next.record = None;
                next.attempts = Expect::Exactly(None);
                next.through = Expect::Exactly(None);
                next.gaps = Expect::Exactly(Vec::new());
            }
            Wrote::Gaps(gaps) => next.gaps = Expect::Exactly(gaps.clone()),
            Wrote::Created => next.gaps = Expect::Exactly(Vec::new()),
        }
        next
    }

    /// A detach that failed after its record may have landed: the record
    /// before it, or the one it writes, recognised by its `through`.
    pub(crate) fn unsure(&self, live_only: bool, through: Option<String>) -> Self {
        Self {
            unsure_detach: Some((live_only, through)),
            ..self.clone()
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
    let Some(differs) = known.differs(found) else {
        return Change::Same;
    };
    if let Some((live_only, through)) = &known.unsure_detach
        && found.record
            == Some(DetachRecord::Detached {
                live_only: *live_only,
            })
        && &found.through == through
    {
        return Change::Same;
    }
    Change::Foreign(format!("{index}: {differs}"))
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
    /// (`None`: it moves no record, the opening known stays). Nothing known
    /// yet stays so: the next read is adopted.
    pub(crate) fn wrote(&self, index: &IndexName, wrote: &Wrote, at: Option<u64>) {
        let mut known = self.lock();
        if let Some(before) = known.get(index) {
            let at = at.unwrap_or(before.at);
            let after = before.wrote(wrote, at);
            known.insert(index.clone(), after);
        }
    }

    /// Whether `index`'s records may keep a gap: one is known, or an own
    /// write may have changed them. Nothing known yet: none known.
    pub(crate) fn keeps_gaps(&self, index: &IndexName) -> bool {
        self.lock()
            .get(index)
            .is_some_and(|known| match &known.gaps {
                Expect::Exactly(gaps) => !gaps.is_empty(),
                Expect::Any => true,
            })
    }

    /// An own detach of `index` failed after its record may have landed.
    pub(crate) fn unsure_detach(
        &self,
        index: &IndexName,
        live_only: bool,
        through: Option<String>,
    ) {
        let mut known = self.lock();
        if let Some(before) = known.get(index) {
            let after = before.unsure(live_only, through);
            known.insert(index.clone(), after);
        }
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
        let rebuild = Known::read(&found, 2).wrote(&Wrote::Rebuild { attempts: 3 }, 2);
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
        let unsure = attached.unsure(false, Some("4".to_owned()));
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
}
