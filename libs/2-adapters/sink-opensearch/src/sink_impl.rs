//! The [`Sink`] trait implementation: how the sink engine's `ensure_index` /
//! `apply` / `flush` / `is_seeded` / `mark_seeded` / `reindex` calls drive the
//! alias-over-generations scheme and the bulk buffer. The
//! lower-level pieces live in [`transport`](crate), [`generations`](crate),
//! [`mapping`](crate), and [`bulk`](crate).

use std::collections::BTreeSet;

use async_trait::async_trait;
use kernel::{Envelope, IndexMapping, IndexName, Op};
use sink::{FlushReport, RejectedDocument, Result, Sink, SinkError, to_json};
use tracing::{debug, info, warn};

use crate::bulk::{BulkAction, bulk_action_fragment, plan_chunks};
use crate::generations::{generation_name, next_generation, parse_generation};
use crate::{IndexTarget, OpensearchSink};

#[async_trait]
impl Sink for OpensearchSink {
    /// Ensure a concrete generation index exists for `mapping`, reachable through
    /// the stable hash alias `{logical}_{hash}`.
    ///
    /// The addressable name `{logical}_{hash}` is an **alias**; the data lives in
    /// a concrete generation `{logical}_{hash}_{gen}` behind it (created
    /// `dynamic: strict`, `refresh_interval: -1` for fast bulk seeding). The
    /// per-index meta doc records the active generation and whether it's seeded:
    ///
    /// - **seeded** — reuse that generation; (re)assert the aliases onto it.
    /// - **unseeded** (fresh, or a [`reindex`](Self::reindex) staged a new
    ///   generation) — make sure the generation index exists, but point the alias
    ///   at it *only* when nothing else is serving yet. A reindex leaves the alias
    ///   on the old generation until [`mark_seeded`](Self::mark_seeded) swaps it,
    ///   so reads never see a half-built index.
    /// - **seeded, but the generation is gone** — the marker is retracted
    ///   (rewritten unseeded, with a warning) and the generation recreated
    ///   empty, so the engine's [`is_seeded`](Self::is_seeded) reports `false`
    ///   and the backfill refills it. Deleting the generation index is therefore
    ///   a "rebuild this index on the next start" signal, never a way to end up
    ///   serving an empty index that claims to be seeded.
    ///
    /// The outcome also picks where [`apply`](Self::apply) writes: a seeded
    /// index through its hash alias, an unseeded one through its generation.
    #[tracing::instrument(
        name = "os.ensure_index",
        skip_all,
        fields(index = mapping.index.as_ref()),
        err,
    )]
    async fn ensure_index(&self, mapping: &IndexMapping) -> Result<()> {
        let logical = mapping.index.as_ref();
        let hash_alias = self.hash_alias(logical, &mapping.hash.to_string());

        // The previous scheme created `{logical}_{hash}` as a *concrete* index;
        // the new scheme needs that name free for an alias. Refuse rather than
        // clobber, so an operator migrates deliberately.
        if self.concrete_index_exists(&hash_alias).await? {
            return Err(SinkError::Write(format!(
                "{hash_alias} exists as a concrete index from an older flusso version; \
                 reindex it into {hash_alias}_1 and delete {hash_alias} so the name can become an alias"
            )));
        }

        let (generation, mut seeded) = match self.read_index_meta(&hash_alias).await? {
            Some(state) => state,
            None => {
                let generation =
                    next_generation(&self.list_generations(&hash_alias).await?, &hash_alias);
                (generation, false)
            }
        };
        let index = generation_name(&hash_alias, generation);

        if self.index_exists(&index).await? {
            debug!(index, "generation exists; leaving its mapping untouched");
        } else {
            if seeded {
                warn!(
                    index,
                    "the seed marker names a generation that no longer exists; \
                     recreating it empty and reseeding",
                );
                seeded = false;
            }
            self.create_index(&index, mapping).await?;
        }

        // Persist the active generation for an unseeded index, so a resumed run
        // targets the same one. A seeded index already has correct meta.
        if !seeded {
            self.write_index_meta(&hash_alias, generation, false)
                .await?;
        }

        // Point the aliases at this generation when it's the live seeded one, or
        // the first generation (nothing else serving yet). A reindex of an
        // already-served index leaves the aliases on the old generation until
        // mark_seeded swaps them — so reads never hit a half-built index. The
        // hash alias is load-bearing (the query client reads through it), so its
        // failure propagates; the `{logical}` convenience alias stays best-effort.
        let serving = self.alias_holders(&hash_alias).await?;
        if seeded || serving.is_empty() {
            self.try_ensure_alias(&hash_alias, &index).await?;
            self.ensure_alias(&self.convenience_alias(logical), &index)
                .await;
        }
        self.set_target(
            logical,
            IndexTarget {
                generation: index,
                hash_alias,
                seeded,
            },
        );
        Ok(())
    }

    /// Buffer one envelope's bulk action, addressed by its index's write
    /// target (see `IndexTarget::write_target`).
    async fn apply(&self, envelope: &Envelope) -> Result<()> {
        let logical = envelope.index.as_ref();
        let (index, require_alias) = match self.target(logical) {
            Some(target) => {
                let (index, require_alias) = target.write_target();
                (index.to_owned(), require_alias)
            }
            None => (logical.to_owned(), false),
        };
        let id = envelope.id.clone();
        let action = match (envelope.op, &envelope.document) {
            (Op::Upsert, Some(document)) => BulkAction::Index {
                index,
                id,
                doc: to_json(document),
                require_alias,
            },
            (Op::Upsert, None) => {
                return Err(SinkError::Write(format!(
                    "upsert of {}/{id} carries no document",
                    envelope.index
                )));
            }
            (Op::Delete, _) => BulkAction::Delete { index, id },
        };
        self.buffer.lock().await.push(action);
        Ok(())
    }

    /// Drain the buffer and send all buffered operations to OpenSearch.
    ///
    /// The drained operations are split into bulk requests bounded by **both**
    /// caps: at most `batch_size` documents *and* at most `max_bytes` serialized
    /// bytes per request, so a few large documents can't push a request past
    /// OpenSearch's `http.max_content_length`. A single document larger than
    /// `max_bytes` is sent on its own (it can't be split) with a warning.
    ///
    /// Refresh is forced **only when `caught_up`**: if this flush drained the
    /// queue (no backlog behind it), the bulk requests carry `?refresh=true` so
    /// the just-written documents are searchable immediately — cheap precisely
    /// because the pipeline is idle. While a backlog is draining (`!caught_up`)
    /// no refresh is forced; visibility is left to the index's configured
    /// `refresh_interval`, keeping bulk indexing fast so the backlog clears (see
    /// the module docs).
    ///
    /// A missing target fails the whole flush, not one document (see the
    /// `bulk` module docs).
    #[tracing::instrument(name = "os.flush", skip_all, fields(caught_up), err)]
    async fn flush(&self, caught_up: bool) -> Result<FlushReport> {
        let actions = {
            let mut buf = self.buffer.lock().await;
            std::mem::take(&mut *buf)
        };

        if actions.is_empty() {
            return Ok(FlushReport::clean());
        }

        let mut fragments = Vec::with_capacity(actions.len());
        for action in &actions {
            let fragment = bulk_action_fragment(action)?;
            if fragment.len() > self.max_bytes {
                warn!(
                    bytes = fragment.len(),
                    max_bytes = self.max_bytes,
                    "a single document exceeds the bulk byte cap; sending it in its own request",
                );
            }
            fragments.push(fragment);
        }

        let sizes: Vec<usize> = fragments.iter().map(String::len).collect();
        let total_bytes: usize = sizes.iter().sum();
        let plan = plan_chunks(&sizes, self.batch_size, self.max_bytes);
        debug!(
            documents = actions.len(),
            requests = plan.len(),
            bytes = total_bytes,
            "flushing buffered operations",
        );

        let mut rejected: Vec<RejectedDocument> = Vec::new();
        let mut start = 0usize;
        for &count in &plan {
            let end = start + count;
            let chunk_fragments = fragments.get(start..end).unwrap_or_default();
            let chunk_actions = actions.get(start..end).unwrap_or_default();
            let mut body = String::with_capacity(chunk_fragments.iter().map(String::len).sum());
            for fragment in chunk_fragments {
                body.push_str(fragment);
            }
            rejected.extend(
                self.send_bulk_chunk(&body, chunk_actions, caught_up)
                    .await?,
            );
            start = end;
        }

        // The engine resolves failure policies by logical name.
        if !rejected.is_empty() {
            let to_logical = self.logical_names();
            for doc in &mut rejected {
                if let Some(logical) = to_logical.get(&doc.index) {
                    doc.index.clone_from(logical);
                }
            }
        }

        Ok(FlushReport { rejected })
    }

    async fn is_seeded(&self, index: &IndexName) -> Result<bool> {
        let Some(target) = self.target(index.as_ref()) else {
            return Ok(false);
        };
        Ok(self
            .read_index_meta(&target.hash_alias)
            .await?
            .is_some_and(|(_, seeded)| seeded))
    }

    /// Record that `index`'s active generation has been seeded, and make it the
    /// one the aliases serve.
    ///
    /// First makes the freshly-seeded generation searchable (one refresh) and
    /// hands it back to automatic refresh, *then* atomically repoints the hash
    /// alias (and the `{logical}` convenience alias) onto it and drops the
    /// superseded generation(s), *then* writes the seed marker. The ordering
    /// matters: on a fresh seed the alias is already on this generation (a no-op
    /// swap); on a reindex the swap is the moment the rebuild becomes visible —
    /// until here reads stayed on the old generation. If any step fails the
    /// marker isn't written, so the next run re-runs the (idempotent) backfill.
    ///
    /// Every other `{hash_alias}_{n}` is dropped too, not only the ones the
    /// alias moved off: a generation nothing serves and nothing builds (an
    /// orphan from a crash, or from an older flusso staging a second reindex
    /// over an in-flight one) would otherwise hold a full copy forever.
    async fn mark_seeded(&self, index: &IndexName) -> Result<()> {
        let logical = index.as_ref();
        let Some(target) = self.target(logical) else {
            return Err(SinkError::Write(format!(
                "cannot mark {logical} seeded: it has no active generation (ensure_index not run?)"
            )));
        };
        let IndexTarget {
            generation,
            hash_alias,
            ..
        } = &target;
        let active = parse_generation(hash_alias, generation).unwrap_or(1);

        self.refresh_index(generation).await?;
        self.restore_auto_refresh(generation).await?;

        // Computed before the swap: afterwards the alias holders are just the
        // active generation.
        let stale: BTreeSet<String> = self
            .alias_holders(hash_alias)
            .await?
            .into_iter()
            .chain(self.list_generations(hash_alias).await?)
            .filter(|name| name != generation && parse_generation(hash_alias, name).is_some())
            .collect();

        self.try_ensure_alias(hash_alias, generation).await?;
        self.ensure_alias(&self.convenience_alias(logical), generation)
            .await;

        for name in &stale {
            self.delete_index(name).await?;
        }

        self.write_index_meta(hash_alias, active, true).await?;
        self.set_target(
            logical,
            IndexTarget {
                seeded: true,
                ..target
            },
        );
        Ok(())
    }

    /// Stage a from-scratch rebuild of `index` into a fresh generation, leaving
    /// the current one serving reads. Only flips meta to the next (unseeded)
    /// generation; the following [`ensure_index`](Self::ensure_index) builds
    /// it, the backfill seeds it, and [`mark_seeded`](Self::mark_seeded) swaps
    /// the alias and drops the old generation. Creating the index is deferred
    /// to `ensure_index` because that's where the mapping is available.
    ///
    /// A no-op when the index isn't seeded (no meta doc, or `seeded: false`):
    /// a from-scratch rebuild is already pending, and staging yet another
    /// generation would orphan the one being built.
    async fn reindex(&self, mapping: &IndexMapping) -> Result<()> {
        let logical = mapping.index.as_ref();
        let hash_alias = self.hash_alias(logical, &mapping.hash.to_string());
        if !matches!(self.read_index_meta(&hash_alias).await?, Some((_, true))) {
            info!(
                logical,
                hash_alias,
                "reindex requested while a rebuild is already pending; nothing to stage"
            );
            return Ok(());
        }
        // Past everything that currently exists, so a crashed earlier reindex
        // can't make us reuse a live generation's name.
        let next = next_generation(&self.list_generations(&hash_alias).await?, &hash_alias);
        self.write_index_meta(&hash_alias, next, false).await?;
        debug!(
            logical,
            hash_alias, next, "staged reindex into a new generation"
        );
        Ok(())
    }
}
