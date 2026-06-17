//! The [`Sink`] trait implementation: how the engine's `ensure_index` /
//! `upsert` / `delete` / `flush` / `is_seeded` / `mark_seeded` / `reindex`
//! calls drive the alias-over-generations scheme and the bulk buffer. The
//! lower-level pieces live in [`transport`](crate), [`generations`](crate),
//! [`mapping`](crate), and [`bulk`](crate).

use std::sync::PoisonError;

use async_trait::async_trait;
use schema_core::{GenericValue, IndexMapping, IndexName};
use sinks_core::{FlushReport, RejectedDocument, Result, Sink, SinkError, to_json};
use tracing::{debug, warn};

use crate::OpensearchSink;
use crate::bulk::{BulkAction, bulk_action_fragment, plan_chunks};
use crate::generations::{generation_name, hash_alias_of, next_generation, parse_generation};

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
    #[tracing::instrument(
        name = "os.ensure_index",
        skip_all,
        fields(index = mapping.index.as_ref()),
        err,
    )]
    async fn ensure_index(&self, mapping: &IndexMapping) -> Result<()> {
        let logical = mapping.index.as_ref();
        let hash_alias = format!("{logical}_{}", mapping.hash);

        // The previous scheme created `{logical}_{hash}` as a *concrete* index;
        // the new scheme needs that name free for an alias. Refuse rather than
        // clobber, so an operator migrates deliberately.
        if self.concrete_index_exists(&hash_alias).await? {
            return Err(SinkError::Write(format!(
                "{hash_alias} exists as a concrete index from an older flusso version; \
                 reindex it into {hash_alias}_1 and delete {hash_alias} so the name can become an alias"
            )));
        }

        // Resolve the active generation + seeded-state from meta, or start fresh
        // at the next free generation number.
        let (generation, seeded) = match self.read_index_meta(&hash_alias).await? {
            Some(state) => state,
            None => {
                let generation =
                    next_generation(&self.list_generations(&hash_alias).await?, &hash_alias);
                (generation, false)
            }
        };
        let index = generation_name(&hash_alias, generation);

        self.index_names
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(logical.to_owned(), index.clone());

        if self.index_exists(&index).await? {
            debug!(index, "generation exists; leaving its mapping untouched");
        } else {
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
            self.ensure_alias(logical, &index).await;
        }
        Ok(())
    }

    async fn upsert(&self, index: &IndexName, id: &str, document: &GenericValue) -> Result<()> {
        let action = BulkAction::Index {
            index: self.physical(index.as_ref()),
            id: id.to_owned(),
            doc: to_json(document),
        };
        self.buffer.lock().await.push(action);
        Ok(())
    }

    async fn delete(&self, index: &IndexName, id: &str) -> Result<()> {
        let action = BulkAction::Delete {
            index: self.physical(index.as_ref()),
            id: id.to_owned(),
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
    #[tracing::instrument(name = "os.flush", skip_all, fields(caught_up), err)]
    async fn flush(&self, caught_up: bool) -> Result<FlushReport> {
        let actions = {
            let mut buf = self.buffer.lock().await;
            std::mem::take(&mut *buf)
        };

        if actions.is_empty() {
            return Ok(FlushReport::clean());
        }

        // Serialize each action's NDJSON fragment once, then group fragments
        // into requests honoring the count and byte caps (see `plan_chunks`).
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
            // A caught-up flush is small (it drained the queue), so forcing the
            // refresh on each of its chunks — rather than only the last — keeps
            // every touched index searchable with negligible extra cost.
            rejected.extend(
                self.send_bulk_chunk(&body, chunk_actions, caught_up)
                    .await?,
            );
            start = end;
        }

        // Rejections carry the *physical* index (what the bulk request used);
        // map each back to its *logical* name so the engine can resolve a
        // per-index failure policy. Reverse the logical→physical table learned
        // at `ensure_index`; fall back to the physical name if it's unknown.
        if !rejected.is_empty() {
            let names = self
                .index_names
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            let to_logical: std::collections::HashMap<&str, &str> = names
                .iter()
                .map(|(l, p)| (p.as_str(), l.as_str()))
                .collect();
            for doc in &mut rejected {
                if let Some(&logical) = to_logical.get(doc.index.as_str()) {
                    doc.index = logical.to_owned();
                }
            }
        }

        Ok(FlushReport { rejected })
    }

    async fn is_seeded(&self, index: &IndexName) -> Result<bool> {
        // The active generation was learned at `ensure_index`; its hash alias is
        // that name minus the generation suffix.
        let Some(hash_alias) = hash_alias_of(&self.physical(index.as_ref())) else {
            return Ok(false);
        };
        Ok(self
            .read_index_meta(&hash_alias)
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
    async fn mark_seeded(&self, index: &IndexName) -> Result<()> {
        let logical = index.as_ref();
        let generation = self.physical(logical);
        let Some(hash_alias) = hash_alias_of(&generation) else {
            return Err(SinkError::Write(format!(
                "cannot mark {logical} seeded: it has no active generation (ensure_index not run?)"
            )));
        };
        let active = parse_generation(&hash_alias, &generation).unwrap_or(1);

        self.refresh_index(&generation).await?;
        self.restore_auto_refresh(&generation).await?;

        // The generations the hash alias is about to move off — drop them once
        // both aliases have swapped. Computed before the swap.
        let superseded: Vec<String> = self
            .alias_holders(&hash_alias)
            .await?
            .into_iter()
            .filter(|holder| {
                holder != &generation && parse_generation(&hash_alias, holder).is_some()
            })
            .collect();

        self.try_ensure_alias(&hash_alias, &generation).await?;
        self.ensure_alias(logical, &generation).await;

        for stale in &superseded {
            self.delete_index(stale).await?;
        }

        self.write_index_meta(&hash_alias, active, true).await
    }

    /// Stage a from-scratch rebuild of `index` into a fresh generation, leaving
    /// the current one serving reads. Only flips meta to the next (unseeded)
    /// generation; the next run's [`ensure_index`](Self::ensure_index) builds it,
    /// the backfill seeds it, and [`mark_seeded`](Self::mark_seeded) swaps the
    /// alias and drops the old generation. Creating the index is deferred to
    /// `ensure_index` because that's where the mapping is available.
    async fn reindex(&self, mapping: &IndexMapping) -> Result<()> {
        let logical = mapping.index.as_ref();
        let hash_alias = format!("{logical}_{}", mapping.hash);
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
