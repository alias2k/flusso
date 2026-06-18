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
//! The contract is deliberately mechanism-neutral — the trait and the
//! [`CoverageReport`] never name "publication". A backend that *can* provision
//! the gap describes how to in [`CoverageReport::remediation`] (Postgres puts the
//! `CREATE`/`ALTER PUBLICATION` SQL there as opaque strings); a caller prints
//! those steps but never interprets them. This keeps the daemon, the CLI, and the
//! shared printer free of any Postgres specifics.

use std::collections::BTreeSet;

use async_trait::async_trait;

use crate::{QualifiedTable, Result};

/// What a source found when asked whether it can stream a set of tables.
///
/// `present` + `missing` partition the requested set. When `missing` is
/// non-empty the source is not yet streaming every table an index reads (live
/// changes to a `missing` table would be silently dropped); `manageable` says
/// whether *this* source — with its current credentials — can close the gap
/// itself, and `remediation` carries the steps to do so (the operator can run
/// them by hand regardless of `manageable`). `blockers` explains a
/// `manageable == false` verdict in human terms.
#[derive(Debug, Clone, Default)]
pub struct CoverageReport {
    /// Every requested table is already streamable — nothing to do.
    pub satisfied: bool,
    /// Requested tables the source already streams.
    pub present: Vec<QualifiedTable>,
    /// Requested tables not yet streamed.
    pub missing: Vec<QualifiedTable>,
    /// Whether the source can provision the `missing` tables with its current
    /// privileges. Meaningless when `missing` is empty.
    pub manageable: bool,
    /// Why `manageable` is false (e.g. "role does not own table public.orders").
    pub blockers: Vec<String>,
    /// Backend-specific steps that would close the gap — opaque to callers,
    /// meant to be shown verbatim (for Postgres, the `CREATE`/`ALTER PUBLICATION`
    /// statements). Empty when `satisfied`.
    pub remediation: Vec<String>,
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

    /// Provision any missing tables when `manage` is set and the gap is
    /// manageable; otherwise a no-op. Returns the report as observed *before*
    /// acting (so a caller can log what was — or would have been — done). A
    /// no-op for an already-satisfied set.
    async fn ensure_coverage(
        &self,
        required: &BTreeSet<QualifiedTable>,
        manage: bool,
    ) -> Result<CoverageReport>;
}
