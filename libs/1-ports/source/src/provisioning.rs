//! Provisioning the prerequisites a source needs to stream a set of tables.
//!
//! A change-capture mechanism only sees a row change if the source is configured
//! to stream that row's table. For Postgres logical replication that means a
//! *publication* covering the table; another mechanism would have its own
//! prerequisite. This module abstracts that uniformly: given the set of tables an
//! index reads (see [`SourceSpec::all_tables`](crate::SourceSpec::all_tables)), a
//! source can **inspect** whether they're covered and, when it has the privilege,
//! **ensure** they are.
//!
//! The same surface covers a second prerequisite: whether the change feed
//! carries each table's whole **pre-image** (old row). Reverse resolution needs
//! its link columns to trace a deleted or re-parented row to its old parent,
//! and the rest lets an update that changes no column a document reads rebuild
//! nothing ([`SourceSpec::pre_images`](crate::SourceSpec::pre_images)).
//! It follows the same inspect / ensure split, per table:
//! [`inspect_pre_image`](CaptureProvisioning::inspect_pre_image) reports,
//! [`ensure_pre_image`](CaptureProvisioning::ensure_pre_image) closes each gap
//! the source is allowed to.
//!
//! The contract is deliberately mechanism-neutral — the trait and the
//! [`CoverageReport`] never name "publication". A backend that *can* provision
//! the gap describes how to in [`CoverageReport::remediation`] (Postgres puts the
//! `CREATE`/`ALTER PUBLICATION` SQL there as opaque strings); a caller prints
//! those steps but never interprets them. This keeps the daemon, the CLI, and the
//! shared printer free of any Postgres specifics.

use std::collections::BTreeSet;

use async_trait::async_trait;
use kernel::ColumnName;

use crate::{PreImageColumns, QualifiedTable, Result};

/// What a source found when asked whether it can stream a set of tables.
///
/// `present` + `missing` partition the requested set. When `missing` is
/// non-empty the source is not yet streaming every table an index reads (live
/// changes to a `missing` table would be silently dropped). `misrouted` cuts
/// across both: tables whose changes arrive (or would, once added) under a
/// name other than their own, so they match no index either. `manageable` says
/// whether *this* source — with its current credentials — can close the gap
/// itself, and `remediation` carries the steps to do so (the operator can run
/// them by hand regardless of `manageable`). `blockers` explains a
/// `manageable == false` verdict in human terms.
#[derive(Debug, Clone, Default)]
pub struct CoverageReport {
    /// Every requested table is streamed under its own name — nothing to do.
    pub satisfied: bool,
    /// Requested tables the source already streams.
    pub present: Vec<QualifiedTable>,
    /// Requested tables not yet streamed.
    pub missing: Vec<QualifiedTable>,
    /// Requested tables streamed, or to be streamed, under another name than
    /// their own (for Postgres, a partitioned table whose changes arrive under
    /// each partition's name), so their changes match no index.
    pub misrouted: Vec<QualifiedTable>,
    /// Whether the source can provision the `missing` tables and reroute the
    /// `misrouted` ones with its current privileges. Meaningless when
    /// `satisfied`.
    pub manageable: bool,
    /// Why `manageable` is false (e.g. "role does not own table public.orders").
    pub blockers: Vec<String>,
    /// Backend-specific steps that would close the gap — opaque to callers,
    /// meant to be shown verbatim (for Postgres, the `CREATE`/`ALTER PUBLICATION`
    /// statements). Empty when `satisfied`.
    pub remediation: Vec<String>,
}

/// What a source found when asked whether its change feed carries the whole
/// pre-image (old row) of every table an index reads.
///
/// Two severities, kept apart:
///
/// - [`gaps`](Self::gaps) — the pre-image misses a **link** column: a delete
///   or re-parent of that table's rows leaves the old parent's document stale.
///   A correctness gap.
/// - [`partial`](Self::partial) — every link is there, but not the rest of
///   the old row: an update can't be told apart from one that changes a column
///   a document reads, so it always rebuilds. Correct, only slower.
#[derive(Debug, Clone, Default)]
pub struct PreImageReport {
    /// Tables whose pre-image misses a required link column.
    pub gaps: Vec<PreImageGap>,
    /// Tables whose pre-image carries every link column but not the whole
    /// row. Each entry's `missing` is empty.
    pub partial: Vec<PreImageGap>,
}

impl PreImageGap {
    /// Whether a source told to `manage` will close this gap itself.
    pub fn will_manage(&self, manage: bool) -> bool {
        manage && self.manageable && self.kept.is_none()
    }
}

impl PreImageReport {
    /// Every table's pre-image carries what resolution needs (its links).
    pub fn satisfied(&self) -> bool {
        self.gaps.is_empty()
    }

    /// Every table's pre-image is the whole old row: links and the
    /// unread-column skip both work.
    pub fn complete(&self) -> bool {
        self.gaps.is_empty() && self.partial.is_empty()
    }
}

/// One table whose pre-image is not the whole old row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreImageGap {
    /// The table whose pre-image falls short.
    pub table: QualifiedTable,
    /// The required link columns the pre-image does not carry. Empty for a
    /// [`partial`](PreImageReport::partial) entry.
    pub missing: Vec<ColumnName>,
    /// Whether this source, with its current credentials, can close the gap.
    pub manageable: bool,
    /// Why `manageable` is false (e.g. "role does not own table public.orders").
    pub blockers: Vec<String>,
    /// The backend-specific step that makes the pre-image the whole row —
    /// opaque, shown verbatim (for Postgres, `ALTER TABLE … REPLICA IDENTITY FULL`).
    pub remediation: String,
    /// Why the source leaves this table as it is on purpose, even when told to
    /// manage it: the operator chose a setting that already carries the row's key and every link
    /// (for Postgres, a replica identity on an index). Only ever on a
    /// [`partial`](PreImageReport::partial) entry; `remediation` still names
    /// the step for an operator who wants the whole row anyway.
    pub kept: Option<String>,
}

/// A source's ability to report and provision the prerequisites for streaming a
/// set of tables. Implemented per mechanism (Postgres backs it with a
/// publication); consumed by the CLI (`check` reports, `run` ensures) only
/// through this neutral surface.
#[async_trait]
pub trait CaptureProvisioning: Send + Sync {
    /// Read-only: report coverage of `required` against what the source streams,
    /// and whether the gap (if any) is `manageable`. Never mutates anything.
    async fn inspect_coverage(&self, required: &BTreeSet<QualifiedTable>)
    -> Result<CoverageReport>;

    /// Provision any missing tables, and reroute any misrouted ones, when
    /// `manage` is set and the gap is manageable; otherwise a no-op. Returns the report as observed *before*
    /// acting (so a caller can log what was — or would have been — done). A
    /// no-op for an already-satisfied set.
    async fn ensure_coverage(
        &self,
        required: &BTreeSet<QualifiedTable>,
        manage: bool,
    ) -> Result<CoverageReport>;

    /// Read-only: report which `required` tables' pre-images are not the whole
    /// row, and which of those miss a link column. A table the source can't
    /// find is skipped — coverage reports it.
    async fn inspect_pre_image(&self, required: &PreImageColumns) -> Result<PreImageReport>;

    /// When `manage` is set, make the pre-image the whole row on every listed
    /// table the source is allowed to and doesn't [keep](PreImageGap::kept),
    /// and report (log) the rest: a link gap loudly, a partial or kept one
    /// quietly. Returns the report as observed *before*
    /// acting, so a gap it just closed is still listed — re-inspect for the
    /// current state. One table that can't be set doesn't stop the others, nor
    /// capture.
    async fn ensure_pre_image(
        &self,
        required: &PreImageColumns,
        manage: bool,
    ) -> Result<PreImageReport>;
}
