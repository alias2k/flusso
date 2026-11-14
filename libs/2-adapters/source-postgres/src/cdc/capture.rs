use std::collections::BTreeSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use async_trait::async_trait;
use futures::stream::BoxStream;
use kernel::Position;
use pgwire_replication::{Lsn, ReplicationClient, ReplicationConfig};
use source::cdc::{
    AdvanceListener, ChangeCapture, ChangeEvent, Continuity, LiveChange, SourcePosition,
};
use source::{
    CaptureProvisioning, CoverageReport, PreImageColumns, PreImageReport, QualifiedTable, Result,
    SnapshotTable, SourceError,
};
use sqlx::{PgPool, Row};
use tokio::sync::OnceCell;

use super::ack::{Listeners, Positions};
use super::{backfill, publication, replica_identity, stream};

/// How often the replication client reports the confirmed position to the
/// server, and how long an idle connection waits before reporting anyway.
/// Together with `confirm` handing the position over at once, the slot
/// releases WAL within about this long after the last sink flush.
const STATUS_INTERVAL: Duration = Duration::from_secs(1);

/// The replication connection's `application_name`, shown in
/// `pg_stat_replication` and `pg_stat_activity`, unless the caller set one.
const APPLICATION_NAME: &str = "flusso";

/// Postgres change capture over logical replication (pgoutput).
///
/// Exposes the three [`ChangeCapture`] capabilities the engine orchestrates:
///
/// - [`continuity`](ChangeCapture::continuity) reports, read-only, whether the
///   replication slot exists. The slot *is* the resume point: a missing slot will
///   be created at the current WAL position and knows nothing of earlier
///   changes, so this reports [`Continuity::Fresh`] and the engine rebuilds
///   every index the sink still calls seeded. [`prepare`](ChangeCapture::prepare)
///   then creates the slot — after that reconciliation and before any snapshot,
///   so a backfill can't miss a write that lands between snapshot and stream.
/// - [`live`](ChangeCapture::live) connects to the replication slot and streams
///   committed row changes as thin, positioned [`ChangeEvent`]s. Resume is the
///   slot's: its `confirmed_flush_lsn` is the durable cursor on the server,
///   advanced as far as [`confirm`](ChangeCapture::confirm) has been called.
/// - [`snapshot`](ChangeCapture::snapshot) reads current rows over an ordinary
///   SQL connection for an initial backfill (see the crate-private `backfill`). The engine calls
///   it only for tables backing an index the sink reports as unseeded.
///
/// # Prerequisites
///
/// The server must have `wal_level = logical`. The replication **slot** is
/// created automatically by [`prepare`](ChangeCapture::prepare) (before the first
/// backfill) if it does not exist yet, and the
/// **publication** is created/extended (also by `prepare`, before the slot) to
/// cover every table any index reads —
/// see [`CaptureProvisioning`] — when the role is privileged enough and
/// management is not opted out; otherwise flusso warns with the SQL to run.
#[derive(Debug, Clone)]
pub struct WalChangeCapture {
    config: ReplicationConfig,
    /// Ordinary SQL connection URL, used by [`snapshot`](Self::snapshot) and
    /// for the automatic slot creation check.
    connection_url: String,
    /// A small, lazily-opened SQL pool shared by the slot check and the
    /// out-of-band [`lag`](Self::lag) polling, so periodic status probes reuse
    /// connections instead of opening and tearing one down each time. Shared
    /// across clones (an `Arc`), opened on first use. The bulk snapshot read
    /// stays on its own connection (see [`snapshot`](Self::snapshot)).
    admin_pool: Arc<OnceCell<PgPool>>,
    /// Every table the enabled indexes read — the set the publication must
    /// cover. Empty unless set via [`with_publication_management`](Self::with_publication_management).
    required_tables: BTreeSet<QualifiedTable>,
    /// Whether to auto-create/extend the publication in [`prepare`](ChangeCapture::prepare), before the slot.
    /// When false, a coverage gap is only reported, never provisioned.
    manage_publication: bool,
    /// Every table whose whole old row the pre-image should carry, with the
    /// link columns resolution can't do without. Ensured by
    /// [`prepare`](ChangeCapture::prepare); set via
    /// [`with_pre_image_management`](Self::with_pre_image_management).
    pre_image_columns: PreImageColumns,
    /// Whether [`prepare`](ChangeCapture::prepare) sets `REPLICA IDENTITY FULL`
    /// on the tables that aren't. When false, a gap is only reported.
    manage_replica_identity: bool,
    /// The position bookkeeping of the open live stream, so `confirm` can reach
    /// it. Replaced each time `live` opens a stream; `None` before the first.
    positions: Arc<Mutex<Option<Arc<Positions>>>>,
    /// Who to wake when a stream's horizon moves; handed to every stream
    /// `live` opens.
    listeners: Arc<Listeners>,
    /// Set by [`prepare`](ChangeCapture::prepare) once it has ensured coverage,
    /// taken by the next [`live`](ChangeCapture::live) so it doesn't repeat it.
    coverage_ensured: Arc<AtomicBool>,
}

impl WalChangeCapture {
    /// Create a capture from a `pgwire-replication` configuration and the
    /// ordinary SQL connection URL the snapshot reads through (the same URL the
    /// document builder connects with).
    ///
    /// Leave `config.start_lsn` at [`Lsn::ZERO`](pgwire_replication::Lsn::ZERO)
    /// to resume from the slot's `confirmed_flush_lsn` — the usual choice.
    /// A non-zero `start_lsn` is taken as already confirmed by every sink.
    /// The client reports its position to the server every second, idle or not.
    ///
    /// `config.publication` names one publication: the one
    /// [`CaptureProvisioning`] inspects and provisions. Without an
    /// `application_name`, the connection reports itself as `flusso`. The
    /// session renders values with flusso's own `DateStyle`, `bytea_output`
    /// and `extra_float_digits`, whatever the database or role sets.
    pub fn new(config: ReplicationConfig, connection_url: impl Into<String>) -> Self {
        let named = config.application_name.is_some();
        // Ours go last, so they win over a caller's own `-c` for the same name:
        // the decoder parses the text these settings fix.
        let options = match config.options.as_deref() {
            Some(theirs) => format!("{theirs} {}", crate::text_value::startup_options()),
            None => crate::text_value::startup_options(),
        };
        let config = config
            .with_status_interval(STATUS_INTERVAL)
            .with_wakeup_interval(STATUS_INTERVAL)
            .with_options(options);
        Self {
            config: if named {
                config
            } else {
                config.with_application_name(APPLICATION_NAME)
            },
            connection_url: connection_url.into(),
            admin_pool: Arc::new(OnceCell::new()),
            required_tables: BTreeSet::new(),
            manage_publication: false,
            pre_image_columns: PreImageColumns::new(),
            manage_replica_identity: false,
            positions: Arc::new(Mutex::new(None)),
            listeners: Arc::new(Listeners::default()),
            coverage_ensured: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Declare the tables the publication must cover and whether to provision
    /// the gap automatically on [`prepare`](ChangeCapture::prepare) (or
    /// [`live`](ChangeCapture::live), for a caller that skips it). `required` is typically
    /// [`SourceSpec::all_tables`](source::SourceSpec::all_tables); the
    /// composition root supplies it along with the `manage` opt-out.
    pub fn with_publication_management(
        mut self,
        required: BTreeSet<QualifiedTable>,
        manage: bool,
    ) -> Self {
        self.required_tables = required;
        self.manage_publication = manage;
        self
    }

    /// Declare the tables whose whole old row the pre-image should carry, with
    /// the link columns resolution needs from each — typically
    /// [`SourceSpec::pre_images`](source::SourceSpec::pre_images) — and whether
    /// to set `REPLICA IDENTITY FULL` on the ones that aren't.
    /// [`prepare`](ChangeCapture::prepare) sets it before the slot where the
    /// role may (warning where a link is still missing, one info line for the
    /// rest), and the stream warns once per table when a change arrives
    /// without its link.
    pub fn with_pre_image_management(mut self, columns: PreImageColumns, manage: bool) -> Self {
        self.pre_image_columns = columns;
        self.manage_replica_identity = manage;
        self
    }

    /// Ensure the declared pre-image columns. Advisory: a failure degrades
    /// reverse resolution, it doesn't stop capture, so it is logged.
    async fn ensure_pre_images(&self) {
        if self.pre_image_columns.is_empty() {
            return;
        }
        if let Err(error) = self
            .ensure_pre_image(&self.pre_image_columns, self.manage_replica_identity)
            .await
        {
            tracing::warn!(%error, "could not ensure replica identities");
        }
    }

    /// The shared admin pool, opened on first call and reused thereafter. Kept
    /// deliberately small — it serves only the slot check, lag probes, and
    /// catalog introspection, not the change or snapshot paths.
    pub(super) async fn admin_pool(&self) -> Result<&PgPool> {
        self.admin_pool
            .get_or_try_init(|| async {
                sqlx::postgres::PgPoolOptions::new()
                    .max_connections(2)
                    .connect(&self.connection_url)
                    .await
                    .map_err(|e| SourceError::Connection(e.to_string()))
            })
            .await
    }

    /// Whether the replication slot exists — read-only. If it does, its plugin
    /// is validated: a slot with the wrong plugin was created for a different
    /// consumer and must not be clobbered, so that is an error.
    async fn slot_continuity(&self) -> Result<Continuity> {
        let pool = self.admin_pool().await?;
        let row = sqlx::query("SELECT plugin FROM pg_replication_slots WHERE slot_name = $1")
            .bind(&self.config.slot)
            .fetch_optional(pool)
            .await
            .map_err(|e| SourceError::Query(e.to_string()))?;
        let Some(row) = row else {
            return Ok(Continuity::Fresh);
        };
        let plugin: String = row
            .try_get("plugin")
            .map_err(|e| SourceError::Query(e.to_string()))?;
        if plugin != "pgoutput" {
            return Err(SourceError::Connection(format!(
                "replication slot '{}' exists but uses plugin '{}', expected 'pgoutput'",
                self.config.slot, plugin,
            )));
        }
        Ok(Continuity::Resumed)
    }

    /// Where the stream about to open starts: the configured start LSN, or the
    /// slot's `confirmed_flush_lsn` when resuming from the slot. Everything at
    /// or before it was confirmed by every sink, which is what lets a fence on
    /// an old position pass before the first keepalive arrives. Advisory: a
    /// failed lookup starts the bookkeeping at zero and the first event the
    /// stream reads corrects it.
    async fn stream_start_lsn(&self) -> u64 {
        if !self.config.start_lsn.is_zero() {
            return self.config.start_lsn.as_u64();
        }
        let confirmed = async {
            let pool = self.admin_pool().await?;
            let row = sqlx::query(
                "SELECT confirmed_flush_lsn::text AS lsn FROM pg_replication_slots \
                 WHERE slot_name = $1",
            )
            .bind(&self.config.slot)
            .fetch_optional(pool)
            .await
            .map_err(|e| SourceError::Query(e.to_string()))?;
            let text: Option<String> = match row {
                Some(row) => row
                    .try_get("lsn")
                    .map_err(|e| SourceError::Query(e.to_string()))?,
                None => None,
            };
            Ok::<_, SourceError>(text.and_then(|text| Lsn::parse(&text).ok()))
        };
        match confirmed.await {
            Ok(Some(lsn)) => lsn.as_u64(),
            Ok(None) => 0,
            Err(error) => {
                tracing::warn!(%error, "could not read the slot's confirmed position");
                0
            }
        }
    }

    fn open_positions(&self) -> Option<Arc<Positions>> {
        self.positions
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// Ensure the replication slot exists, creating it if it does not. Runs
    /// over the shared admin pool so it can run before the replication
    /// connection is opened.
    async fn ensure_slot(&self) -> Result<()> {
        if self.slot_continuity().await? == Continuity::Resumed {
            tracing::debug!(slot = %self.config.slot, "replication slot already exists");
            return Ok(());
        }
        let pool = self.admin_pool().await?;
        sqlx::query("SELECT pg_create_logical_replication_slot($1, 'pgoutput')")
            .bind(&self.config.slot)
            .execute(pool)
            .await
            .map_err(|e| {
                SourceError::Connection(format!(
                    "failed to create replication slot '{}': {e}",
                    self.config.slot,
                ))
            })?;
        tracing::info!(slot = %self.config.slot, "created replication slot");
        Ok(())
    }
}

#[async_trait]
impl ChangeCapture for WalChangeCapture {
    #[tracing::instrument(name = "wal.continuity", skip_all, err)]
    async fn continuity(&self) -> Result<Continuity> {
        self.slot_continuity().await
    }

    #[tracing::instrument(name = "wal.prepare", skip_all, err)]
    async fn prepare(&self) -> Result<()> {
        // Before the slot: a change logged after the slot exists but before the
        // ALTERs would decode with the old identity, or against the old
        // publication (a missing table, a partition's name), and be lost.
        self.ensure_pre_images().await;
        self.ensure_coverage(&self.required_tables, self.manage_publication)
            .await?;
        self.coverage_ensured.store(true, Ordering::SeqCst);
        self.ensure_slot().await
    }

    /// Opens the replication stream. The slot check and, unless `prepare` just
    /// did it, the coverage check are repeated here (both idempotent) so `live`
    /// stays correct for a caller that never ran
    /// [`prepare`](ChangeCapture::prepare).
    #[tracing::instrument(name = "wal.live", skip_all, err)]
    async fn live(&self) -> Result<BoxStream<'static, Result<LiveChange>>> {
        if !self.coverage_ensured.swap(false, Ordering::SeqCst) {
            self.ensure_coverage(&self.required_tables, self.manage_publication)
                .await?;
        }
        self.ensure_slot().await?;

        let catalog = self.admin_pool().await?.clone();
        let client = ReplicationClient::connect(self.config.clone())
            .await
            .map_err(|e| SourceError::Connection(e.to_string()))?;

        let start_lsn = self.stream_start_lsn().await;
        let mut slot = self
            .positions
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let first_seq = slot.as_ref().map_or(0, |previous| previous.next_seq());
        let positions = Arc::new(Positions::new(
            start_lsn,
            first_seq,
            Arc::clone(&self.listeners),
        ));
        *slot = Some(Arc::clone(&positions));
        drop(slot);
        tracing::info!(start_lsn, "opened replication stream");
        Ok(stream::build(
            client,
            catalog,
            self.pre_image_columns.clone(),
            positions,
        ))
    }

    fn confirm(&self, position: Position) {
        if let Some(positions) = self.open_positions() {
            positions.confirm(position.0);
        }
    }

    /// A position is a WAL LSN in Postgres's own text form (`0/16B3748`), what
    /// `SELECT pg_current_wal_lsn()` returns.
    fn parse_position(&self, text: &str) -> Result<SourcePosition> {
        Lsn::parse(text.trim())
            .map(|lsn| SourcePosition(lsn.as_u64()))
            .map_err(|error| {
                SourceError::Decode(format!("`{text}` is not a WAL position (an LSN): {error}"))
            })
    }

    fn format_position(&self, position: SourcePosition) -> String {
        Lsn::from_u64(position.0).to_pg_string()
    }

    fn reached(&self, position: Option<Position>) -> Option<SourcePosition> {
        let positions = self.open_positions()?;
        Some(SourcePosition(
            positions.reached(position.map(|position| position.0)),
        ))
    }

    fn on_advance(&self, listener: AdvanceListener) {
        self.listeners.add(listener);
    }

    #[tracing::instrument(name = "wal.snapshot", skip_all, fields(tables = tables.len()), err)]
    async fn snapshot(
        &self,
        tables: &[SnapshotTable],
    ) -> Result<BoxStream<'static, Result<ChangeEvent>>> {
        tracing::info!(tables = tables.len(), "starting snapshot");
        backfill::snapshot(&self.connection_url, tables).await
    }

    /// Bytes between the slot's `confirmed_flush_lsn` and the server's current
    /// WAL LSN — how far behind the destination is. Returns `None` until the
    /// slot exists (it is created on the first [`live`](Self::live) connect).
    #[tracing::instrument(name = "wal.lag", skip_all, err)]
    async fn lag(&self) -> Result<Option<u64>> {
        let pool = self.admin_pool().await?;

        // `pg_wal_lsn_diff` yields a numeric byte distance; cast to bigint so it
        // decodes as an integer. A slot whose consumer is fully caught up reads
        // zero; a never-connected slot has no row, hence `Option`.
        let row = sqlx::query(
            "SELECT pg_wal_lsn_diff(pg_current_wal_lsn(), confirmed_flush_lsn)::bigint AS lag \
             FROM pg_replication_slots WHERE slot_name = $1",
        )
        .bind(&self.config.slot)
        .fetch_optional(pool)
        .await
        .map_err(|e| SourceError::Query(e.to_string()))?;

        let lag = match row {
            Some(row) => {
                let bytes: i64 = row
                    .try_get("lag")
                    .map_err(|e| SourceError::Query(e.to_string()))?;
                // A negative diff (slot momentarily ahead of the read LSN) clamps
                // to zero — there is no meaningful "negative lag".
                Some(bytes.max(0) as u64)
            }
            None => None,
        };
        Ok(lag)
    }
}

#[async_trait]
impl CaptureProvisioning for WalChangeCapture {
    async fn inspect_coverage(
        &self,
        required: &BTreeSet<QualifiedTable>,
    ) -> Result<CoverageReport> {
        let pool = self.admin_pool().await?;
        publication::inspect_publication(pool, &self.config.publication.to_string(), required).await
    }

    #[tracing::instrument(name = "wal.ensure_coverage", skip_all, err)]
    async fn ensure_coverage(
        &self,
        required: &BTreeSet<QualifiedTable>,
        manage: bool,
    ) -> Result<CoverageReport> {
        let pool = self.admin_pool().await?;
        let report =
            publication::inspect_publication(pool, &self.config.publication.to_string(), required)
                .await?;

        if report.satisfied {
            tracing::debug!(
                publication = %self.config.publication,
                "publication covers every required table",
            );
            return Ok(report);
        }

        let join = |tables: &[QualifiedTable]| {
            tables
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(", ")
        };
        let missing = join(&report.missing);
        let misrouted = join(&report.misrouted);

        if manage && report.manageable {
            publication::apply_publication(
                pool,
                &self.config.publication.to_string(),
                &report.missing,
                !report.misrouted.is_empty(),
            )
            .await?;
            tracing::info!(
                publication = %self.config.publication,
                tables = %missing,
                partitioned = %misrouted,
                "provisioned publication for missing tables and partitioned tables",
            );
        } else {
            let reason = if !manage {
                "automatic publication management is disabled".to_owned()
            } else {
                report.blockers.join("; ")
            };
            tracing::warn!(
                publication = %self.config.publication,
                missing = %missing,
                partitioned = %misrouted,
                reason = %reason,
                remediation = %report.remediation.join(" "),
                "publication is missing tables, or streams partitioned tables under their \
                 partitions' names, and flusso will not fix it automatically; run the printed \
                 SQL (until then, changes to those tables are dropped)",
            );
        }

        Ok(report)
    }

    async fn inspect_pre_image(&self, required: &PreImageColumns) -> Result<PreImageReport> {
        replica_identity::inspect(self.admin_pool().await?, required).await
    }

    #[tracing::instrument(name = "wal.ensure_pre_image", skip_all, err)]
    async fn ensure_pre_image(
        &self,
        required: &PreImageColumns,
        manage: bool,
    ) -> Result<PreImageReport> {
        let pool = self.admin_pool().await?;
        let report = replica_identity::inspect(pool, required).await?;
        let mut unset = Vec::new();
        let mut kept = Vec::new();
        for gap in report.gaps.iter().chain(&report.partial) {
            if let Some(reason) = &gap.kept {
                kept.push(format!("{} ({reason})", gap.table));
                continue;
            }
            let reason = if manage {
                let mut reasons = gap.blockers.clone();
                let links = required
                    .get(&gap.table)
                    .map(Vec::as_slice)
                    .unwrap_or_default();
                if let Err(error) = replica_identity::apply(pool, gap, links).await {
                    reasons.push(error.to_string());
                }
                if reasons.is_empty() {
                    tracing::info!(
                        table = %gap.table,
                        missing = %replica_identity::join_columns(&gap.missing),
                        missing_key = %replica_identity::join_columns(&gap.missing_key),
                        "set REPLICA IDENTITY FULL: deleted and re-parented rows reach their old \
                         parent's document, and an update that changes no column a document reads \
                         rebuilds nothing",
                    );
                    continue;
                }
                reasons.join("; ")
            } else {
                "automatic replica identity management is disabled".to_owned()
            };
            if gap.missing.is_empty() && gap.missing_key.is_empty() {
                unset.push((gap, reason));
                continue;
            }
            if !gap.missing_key.is_empty() {
                replica_identity::warn_unkeyed(
                    &gap.table,
                    &gap.missing_key,
                    &reason,
                    &gap.remediation,
                );
            }
            if !gap.missing.is_empty() {
                replica_identity::warn_untraceable(
                    &gap.table,
                    &gap.missing,
                    &reason,
                    &gap.remediation,
                );
            }
        }
        if !kept.is_empty() {
            tracing::info!(
                tables = %kept.join("; "),
                "replica identity kept as is on these tables: its index already carries the \
                 primary key and every parent link, so flusso does not set it to FULL; an update there always rebuilds every \
                 document that reads the table",
            );
        }
        if !unset.is_empty() {
            let tables = unset
                .iter()
                .map(|(gap, reason)| format!("{} ({reason}): {}", gap.table, gap.remediation))
                .collect::<Vec<_>>()
                .join("; ");
            tracing::info!(
                %tables,
                "replica identity is not FULL on these tables, so an update there rebuilds every \
                 document that reads the table even when it changes no column a document reads; \
                 run the printed SQL to skip those",
            );
        }
        Ok(report)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> ReplicationConfig {
        ReplicationConfig::new("localhost", "app", "pw", "db", "slot", "pub")
    }

    #[test]
    fn the_replication_connection_is_named_flusso_unless_the_caller_named_it() {
        let capture = WalChangeCapture::new(config(), "postgres://app@localhost/db");
        assert_eq!(capture.config.application_name.as_deref(), Some("flusso"));

        let named = config().with_application_name("search-sync");
        let capture = WalChangeCapture::new(named, "postgres://app@localhost/db");
        assert_eq!(
            capture.config.application_name.as_deref(),
            Some("search-sync")
        );
    }
}
