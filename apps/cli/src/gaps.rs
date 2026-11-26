//! Gaps in capture that `flusso run` won't close (issue #485).
//!
//! An index works only when every change to a table it reads reaches it, and
//! a deleted or re-parented child row names its old parent. Two reports say
//! whether that holds: the publication's coverage and the replica
//! identities. A gap `run` closes itself (the role may, and management is on)
//! is no problem, nor is a table that only loses the update skip of #169 or
//! one flusso keeps on purpose. Every other gap leaves an index that does not
//! hold what its schema says, so it is an error: `check` fails on it, and
//! `run --strict` refuses to start.
//!
//! ```text
//! [error] public.orders  publication_gap: `public.orders` is not in the publication, …
//! ```

use std::collections::BTreeMap;

use kernel::IndexName;
use source::{CoverageReport, PreImageGap, PreImageReport, QualifiedTable, SourceSpec};

/// What a gap is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum GapKind {
    /// A table the indexes read that the publication doesn't stream under
    /// its own name.
    Publication,
    /// A table whose replica identity misses its primary key or a parent link.
    ReplicaIdentity,
}

impl GapKind {
    /// The kind as a stable `snake_case` code.
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Publication => "publication_gap",
            Self::ReplicaIdentity => "replica_identity_gap",
        }
    }
}

/// One gap `flusso run` won't close.
#[derive(Debug, Clone)]
pub(crate) struct Gap {
    pub(crate) kind: GapKind,
    pub(crate) table: QualifiedTable,
    /// The indexes that read the table.
    pub(crate) indexes: Vec<IndexName>,
    pub(crate) message: String,
}

/// The gaps the reports show that `flusso run` won't close, under the
/// management settings it runs with.
pub(crate) fn of(
    spec: &SourceSpec,
    coverage: &CoverageReport,
    manage_publication: bool,
    pre_image: &PreImageReport,
    manage_replica_identity: bool,
) -> Vec<Gap> {
    let mut gaps = Vec::new();
    let covered = coverage.satisfied || (coverage.manageable && manage_publication);
    if !covered {
        let why = if coverage.manageable {
            "manage_publication = false".to_owned()
        } else {
            coverage.blockers.join("; ")
        };
        for table in &coverage.missing {
            gaps.push((
                GapKind::Publication,
                table,
                format!(
                    "`{table}` is not in the publication, and `flusso run` won't add it ({why}): \
                     its changes never reach the indexes that read it"
                ),
            ));
        }
        for table in &coverage.misrouted {
            gaps.push((
                GapKind::Publication,
                table,
                format!(
                    "`{table}` is partitioned and streams under its partitions' names, which no \
                     index reads, and `flusso run` won't set `publish_via_partition_root` \
                     ({why}): its changes never reach the indexes that read it"
                ),
            ));
        }
    }
    for gap in pre_image
        .gaps
        .iter()
        .filter(|gap| gap.kept.is_none() && !gap.will_manage(manage_replica_identity))
    {
        gaps.push((
            GapKind::ReplicaIdentity,
            &gap.table,
            replica_identity_message(gap),
        ));
    }
    let readers = readers(spec);
    gaps.into_iter()
        .map(|(kind, table, message)| Gap {
            kind,
            indexes: readers.get(table).cloned().unwrap_or_default(),
            table: table.clone(),
            message,
        })
        .collect()
}

/// What `gaps` tell the capture the start reported, so its `prepare` doesn't
/// warn about them again.
pub(crate) fn reported(gaps: &[Gap]) -> source_postgres::ReportedGaps {
    source_postgres::ReportedGaps {
        publication: gaps
            .iter()
            .filter(|gap| gap.kind == GapKind::Publication)
            .map(|gap| gap.table.clone())
            .collect(),
        replica_identity: gaps
            .iter()
            .filter(|gap| gap.kind == GapKind::ReplicaIdentity)
            .map(|gap| gap.table.clone())
            .collect(),
    }
}

fn replica_identity_message(gap: &PreImageGap) -> String {
    let names = |columns: &[config::ColumnName]| {
        columns
            .iter()
            .map(|c| format!("`{c}`"))
            .collect::<Vec<_>>()
            .join(", ")
    };
    let mut missing = Vec::new();
    let mut harm = Vec::new();
    if !gap.missing_key.is_empty() {
        missing.push(format!("its primary key ({})", names(&gap.missing_key)));
        harm.push(
            "changes there are keyed by other columns or by none, so a write can land under \
             another id and a delete can leave its document",
        );
    }
    if !gap.missing.is_empty() {
        missing.push(format!("its parent link ({})", names(&gap.missing)));
        harm.push("deleting or re-parenting a row leaves the old parent's document stale");
    }
    let why = if gap.manageable {
        "manage_replica_identity = false".to_owned()
    } else {
        gap.blockers.join("; ")
    };
    format!(
        "`{}` doesn't log {} on delete or update, and `flusso run` won't set REPLICA IDENTITY \
         FULL ({why}): {}",
        gap.table,
        missing.join(" or "),
        harm.join("; ")
    )
}

/// Every table any index reads, with the indexes that read it.
fn readers(spec: &SourceSpec) -> BTreeMap<QualifiedTable, Vec<IndexName>> {
    let mut readers: BTreeMap<QualifiedTable, Vec<IndexName>> = BTreeMap::new();
    for (name, schema) in spec.indexes() {
        let one = SourceSpec::new(BTreeMap::from([(name.clone(), schema.clone())]));
        for table in one.all_tables() {
            readers.entry(table).or_default().push(name.clone());
        }
    }
    readers
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    fn table(name: &str) -> QualifiedTable {
        QualifiedTable::new(
            config::DatabaseSchema::try_new("public").unwrap(),
            config::TableName::try_new(name).unwrap(),
        )
    }

    fn spec() -> SourceSpec {
        let schema = |table: &str| {
            serde_json::from_value(serde_json::json!({
                "version": 1, "table": table, "db_schema": "public", "primary_key": "id",
                "fields": [],
            }))
            .unwrap()
        };
        SourceSpec::new(BTreeMap::from([
            (IndexName::try_new("users").unwrap(), schema("users")),
            (IndexName::try_new("orders").unwrap(), schema("orders")),
        ]))
    }

    /// What the start reported, so `prepare` doesn't warn about it again:
    /// the publication when any publication gap was, and each table with a
    /// replica identity gap.
    #[test]
    fn the_reported_gaps_name_the_publication_and_each_replica_identity_table() {
        let gap = |kind, name| Gap {
            kind,
            table: table(name),
            indexes: Vec::new(),
            message: String::new(),
        };
        let both = reported(&[
            gap(GapKind::Publication, "orders"),
            gap(GapKind::ReplicaIdentity, "items"),
        ]);
        assert_eq!(
            both.publication,
            std::collections::BTreeSet::from([table("orders")])
        );
        assert_eq!(
            both.replica_identity,
            std::collections::BTreeSet::from([table("items")])
        );
        let none = reported(&[gap(GapKind::ReplicaIdentity, "items")]);
        assert!(none.publication.is_empty());
    }

    fn coverage(manageable: bool) -> CoverageReport {
        CoverageReport {
            satisfied: false,
            present: vec![table("users")],
            missing: vec![table("orders")],
            misrouted: Vec::new(),
            manageable,
            blockers: vec!["role does not own table public.orders".to_owned()],
            remediation: vec!["ALTER PUBLICATION flusso ADD TABLE public.orders;".to_owned()],
        }
    }

    fn gap(manageable: bool, kept: Option<&str>) -> PreImageGap {
        PreImageGap {
            table: table("users"),
            missing: vec![config::ColumnName::try_new("team_id").unwrap()],
            missing_key: Vec::new(),
            manageable,
            blockers: vec!["role does not own table public.users".to_owned()],
            remediation: "ALTER TABLE public.users REPLICA IDENTITY FULL;".to_owned(),
            kept: kept.map(str::to_owned),
        }
    }

    fn kinds(gaps: &[Gap]) -> Vec<(GapKind, String, Vec<String>)> {
        gaps.iter()
            .map(|g| {
                (
                    g.kind,
                    g.table.to_string(),
                    g.indexes.iter().map(ToString::to_string).collect(),
                )
            })
            .collect()
    }

    /// A gap `run` closes itself is none; one it won't is, named with the
    /// indexes that read its table.
    #[test]
    fn a_publication_gap_is_one_only_when_run_wont_close_it() {
        let complete = PreImageReport::default();
        assert!(of(&spec(), &coverage(true), true, &complete, true).is_empty());
        for (manageable, manage) in [(true, false), (false, true), (false, false)] {
            let gaps = of(&spec(), &coverage(manageable), manage, &complete, true);
            assert_eq!(
                kinds(&gaps),
                vec![(
                    GapKind::Publication,
                    "public.orders".to_owned(),
                    vec!["orders".to_owned()]
                )],
                "{manageable} {manage}"
            );
            let why = if manageable {
                "manage_publication = false"
            } else {
                "role does not own table public.orders"
            };
            assert!(gaps[0].message.contains(why), "{}", gaps[0].message);
        }
    }

    /// A missing link `run` won't set is a gap; one it sets, a partial entry
    /// (only the update skip is lost) and a kept one are not.
    #[test]
    fn a_replica_identity_gap_is_one_only_when_run_wont_set_it() {
        let covered = CoverageReport {
            satisfied: true,
            missing: Vec::new(),
            ..coverage(true)
        };
        let report =
            |gaps: Vec<PreImageGap>, partial: Vec<PreImageGap>| PreImageReport { gaps, partial };
        assert!(
            of(
                &spec(),
                &covered,
                true,
                &report(vec![gap(true, None)], vec![]),
                true
            )
            .is_empty()
        );
        assert!(
            of(
                &spec(),
                &covered,
                true,
                &report(vec![], vec![gap(false, None)]),
                false
            )
            .is_empty()
        );
        assert!(
            of(
                &spec(),
                &covered,
                true,
                &report(vec![], vec![gap(true, Some("on an index"))]),
                false
            )
            .is_empty()
        );
        let gaps = of(
            &spec(),
            &covered,
            true,
            &report(vec![gap(false, None)], vec![]),
            true,
        );
        assert_eq!(
            kinds(&gaps),
            vec![(
                GapKind::ReplicaIdentity,
                "public.users".to_owned(),
                vec!["users".to_owned()]
            )]
        );
        assert!(
            gaps[0].message.contains("parent link (`team_id`)")
                && gaps[0].message.contains("old parent's document stale"),
            "{}",
            gaps[0].message
        );
        let off = of(
            &spec(),
            &covered,
            true,
            &report(vec![gap(true, None)], vec![]),
            false,
        );
        assert!(off[0].message.contains("manage_replica_identity = false"));
    }
}
