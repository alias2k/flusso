//! The [`Sink`] trait implementation: how the sink engine's `ensure_index` /
//! `write` / `is_seeded` / `mark_seeded` / `reindex` calls drive the
//! alias-over-generations scheme and the bulk writes. The
//! lower-level pieces live in [`transport`](crate), [`generations`](crate),
//! [`mapping`](crate), and [`bulk`](crate).

use std::collections::{BTreeMap, BTreeSet, HashMap};

use async_trait::async_trait;
use kernel::{Envelope, IndexMapping, IndexName, Op};
use serde_json::{Value, json};
use sink::{
    CaughtUp, Ensured, RejectedDocument, Result, Seeding, Sink, SinkError, TargetId, Visibility,
    WriteReport,
};
use tracing::{debug, info, warn};

use crate::bulk::{BulkAction, bulk_action_fragment, plan_chunks, refused_id, render_document};
use crate::generations::{
    self, generation_name, next_generation, parse_generation, plan_seed_actions,
};
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
    /// - **unseeded, but the generation is not the one flusso created** — its
    ///   `index.uuid` differs from the one the meta doc recorded at creation.
    ///   An unseeded generation is written by its concrete name, so after it
    ///   is deleted, a write under OpenSearch's default
    ///   `action.auto_create_index` recreates it, dynamically mapped and
    ///   holding only what came after. It is deleted and created again,
    ///   strict and empty, with no checkpoint. (A seeded generation is
    ///   written only through its alias with `require_alias`, so nothing
    ///   auto-creates it; another uuid there is a restore, adopted.)
    ///
    /// The answer names the generation by its `index.uuid`, and says it was
    /// created when it did not exist (or was replaced) on this call.
    ///
    /// The outcome also picks where [`write`](Self::write) writes: a seeded
    /// index through its hash alias, an unseeded one through its generation.
    ///
    /// Safe to run for several indexes at once: each has its own meta
    /// document (keyed by its hash alias), generations and aliases, and the
    /// shared meta index is created by its first write, which OpenSearch's
    /// auto-create takes from several writers at once.
    #[tracing::instrument(
        name = "os.ensure_index",
        skip_all,
        fields(index = mapping.index.as_ref()),
        err,
    )]
    async fn ensure_index(&self, mapping: &IndexMapping) -> Result<Ensured> {
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

        let meta = self.read_meta(&hash_alias).await?;
        let (generation, mut seeded, recorded) = match &meta {
            Some(meta) => (meta.generation, meta.seeded, meta.uuid.clone()),
            None => {
                // No meta doc, but a generation of ours already holds the hash
                // alias: a start that died between creating it (with its
                // aliases) and writing the doc. Reuse it, unseeded: a second
                // generation beside it would leave readers on the empty first
                // until the seed. Any other generation is swept by the seed.
                let holding = self
                    .alias_holders(&hash_alias)
                    .await?
                    .iter()
                    .find_map(|holder| parse_generation(&hash_alias, holder));
                let generation = match holding {
                    Some(generation) => generation,
                    None => {
                        next_generation(&self.list_generations(&hash_alias).await?, &hash_alias)
                    }
                };
                (generation, false, None)
            }
        };
        let index = generation_name(&hash_alias, generation);
        let convenience = self.convenience_alias(logical);
        let mut live = self.index_uuid(&index).await?;
        if let (Some(found), Some(made)) = (&live, &recorded)
            && found != made
        {
            if seeded {
                info!(
                    index,
                    "the seeded generation has another index.uuid than when it was created \
                     (restored from a snapshot, or recreated by hand?); keeping it"
                );
            } else {
                warn!(
                    index,
                    "this generation is not the one flusso created (deleted, then recreated by a \
                     write under action.auto_create_index); deleting it and creating it again"
                );
                self.delete_index(&index).await?;
                live = None;
            }
        }
        let exists = live.is_some();
        if exists {
            debug!(index, "generation exists; leaving its mapping untouched");
            // What an earlier run wrote without a refresh is remembered
            // nowhere. One refresh per start makes it searchable, so a run can
            // begin from "everything written so far is searchable". A closed
            // generation is skipped, not a failure: staging it must not stop
            // the sink, and its lane parks as blocked at its first write
            // (stage 9 slice 3, C2).
            if seeded {
                self.refresh_index_if_open(&index).await?;
            }
        } else if seeded {
            warn!(
                index,
                "the seed marker names a generation that no longer exists; \
                 recreating it empty and reseeding",
            );
            seeded = false;
        }

        let mut holders = self
            .alias_map(&[hash_alias.as_str(), convenience.as_str()])
            .await?;
        let held = |holders: &generations::Holders, alias: &str| {
            holders.get(alias).cloned().unwrap_or_default()
        };
        // Point the aliases at this generation when it's the live seeded one,
        // or when nothing serves yet (the first generation). A rebuild of an
        // already-served index leaves them on the old generation until
        // mark_seeded swaps them, so reads never hit a half-built index.
        let first = held(&holders, &hash_alias).is_empty();

        if !exists {
            // Before the create: from here on the doc says unseeded and names
            // no uuid, so a failure after the create can't leave a "seeded"
            // or another generation's uuid beside the new, empty one. The
            // next staging adopts what it finds, unseeded.
            self.write_index_meta(&hash_alias, generation, false, None)
                .await?;
            // The aliases this generation takes anyway, attached at creation:
            // one cluster-state update instead of one per alias. Never the
            // convenience alias while another index holds it: an alias added
            // at creation removes it from nothing.
            let mut attached: Vec<&str> = Vec::new();
            if first {
                attached.push(&hash_alias);
                if held(&holders, &convenience).is_empty() {
                    attached.push(&convenience);
                }
            }
            let created = match self.create_index(&index, mapping, &attached).await {
                // An index (not an alias) already has the convenience alias's
                // name: OpenSearch refuses the whole create. The convenience
                // alias is best effort; create with the hash alias alone and
                // let the alias step below warn, as before #403.
                Err(SinkError::Write(refused))
                    if attached.contains(&convenience.as_str())
                        && refused.contains("invalid_alias_name_exception") =>
                {
                    warn!(
                        index,
                        alias = %convenience,
                        "an index holds the convenience alias's name; creating the generation \
                         with the hash alias alone"
                    );
                    attached.retain(|alias| *alias != convenience.as_str());
                    self.create_index(&index, mapping, &attached).await?
                }
                other => other?,
            };
            if created {
                for alias in attached {
                    holders.insert(alias.to_owned(), vec![index.clone()]);
                }
            } else {
                holders = self
                    .alias_map(&[hash_alias.as_str(), convenience.as_str()])
                    .await?;
            }
            live = self.index_uuid(&index).await?;
            let Some(uuid) = &live else {
                return Err(SinkError::Write(format!(
                    "{index} was created but is gone again"
                )));
            };
            self.record_uuid(&hash_alias, uuid).await?;
        }

        // Persist the active generation and its uuid, so a resumed run
        // targets the same one and can tell it from one recreated under its
        // name. A generation that did not exist (created here, or by a
        // concurrent creator since the check) holds nothing: its doc was
        // rewritten above without a checkpoint. One that already existed
        // keeps its doc, checkpoint (`walked`) included; a doc with no uuid
        // (an older flusso's, or a staging that failed after its create)
        // gains it.
        if exists {
            if meta.is_none() {
                self.write_index_meta(&hash_alias, generation, false, live.as_deref())
                    .await?;
            } else if recorded.is_none()
                && let Some(uuid) = &live
            {
                self.record_uuid(&hash_alias, uuid).await?;
            }
        }

        // A caught-up write may skip refreshing an index only when its own
        // `?refresh=true` bulk refreshed all of it: one primary shard. Unknown
        // counts as several.
        let single_shard = match self.primary_shards(&index).await {
            Ok(shards) => shards == Some(1),
            Err(error) => {
                debug!(%error, index = %index, "could not read the shard count; a caught-up write will refresh it separately");
                false
            }
        };

        // The hash alias is load-bearing (the query client reads through it),
        // so its failure propagates; the convenience alias stays best-effort.
        if seeded || first {
            let serving = held(&holders, &hash_alias);
            let named = held(&holders, &convenience);
            let hash = (hash_alias.as_str(), serving.as_slice());
            let both = [hash, (convenience.as_str(), named.as_slice())];
            let none = BTreeSet::new();
            if let Some(actions) = plan_seed_actions(&index, &both, &none)
                && self.update_aliases(&actions).await.is_err()
            {
                if let Some(hash_only) =
                    plan_seed_actions(&index, std::slice::from_ref(&hash), &none)
                {
                    self.update_aliases(&hash_only).await?;
                }
                self.ensure_alias(&convenience, &index).await;
            }
        }
        let target = live.clone().map(TargetId::new);
        self.set_target(
            logical,
            IndexTarget {
                generation: index,
                hash_alias,
                seeded,
                single_shard,
                uuid: live,
            },
        );
        Ok(if exists {
            Ensured::existing(target)
        } else {
            Ensured::created(target)
        })
    }

    /// Write `envelopes` as one or more bulk requests.
    ///
    /// Each envelope becomes one bulk action, addressed by its index's write
    /// target (see `IndexTarget::write_target`). Nothing is kept between
    /// calls, so concurrent writes share only the targets and the set of
    /// indexes left unrefreshed, each behind its own lock. Caught-up writes
    /// refresh one at a time: a later one waits for an earlier refresh. A write that fails
    /// before sending (an upsert carrying no document) sends nothing.
    ///
    /// The actions are split into bulk requests bounded by **both** caps: at
    /// most `batch_size` documents *and* at most `max_bytes` serialized bytes
    /// per request, so a few large documents can't push a request past
    /// OpenSearch's `http.max_content_length`. A single document larger than
    /// `max_bytes` is sent on its own (it can't be split) with a warning.
    ///
    /// **Refresh is forced only when `caught_up`**: the requests carry
    /// `?refresh=true`, and every index an earlier write left unrefreshed is
    /// refreshed after them, so everything written by the writes that returned
    /// before this one is searchable when it returns (`Immediate`). A write
    /// that isn't caught up forces nothing, keeping bulk indexing fast while a
    /// backlog drains; it reports `Within(refresh_interval)`, or `Deferred`
    /// with automatic refresh off (`refresh_interval = "-1"`), and its indexes
    /// are remembered for the next caught-up write.
    ///
    /// A missing target, or one refusing writes for now, is reported for its
    /// index as a whole, and the rest of the write still lands (see the `bulk`
    /// module docs). An id OpenSearch would refuse before running the
    /// request (empty, or an upsert's past 512 bytes) is one rejected
    /// document: it is never sent, so the rest of the write still lands.
    async fn write(&self, envelopes: &[Envelope], caught_up: bool) -> Result<WriteReport> {
        let caught_up = if caught_up {
            CaughtUp::All
        } else {
            CaughtUp::No
        };
        self.write_parts(&[envelopes], caught_up).await
    }

    /// Several lane items as one write: their documents go out in the same
    /// `_bulk` requests, chunked as one write's are (`batch_size`,
    /// `max_bytes`), so a merged write of up to `batch_size` documents is one
    /// request. Everything [`write`](Self::write) says holds for the whole
    /// call.
    #[tracing::instrument(
        name = "os.write",
        skip_all,
        fields(documents = parts.iter().map(|part| part.len()).sum::<usize>(), parts = parts.len(), caught_up = caught_up.is_caught_up()),
        err
    )]
    async fn write_parts(
        &self,
        parts: &[&[Envelope]],
        caught_up: CaughtUp<'_>,
    ) -> Result<WriteReport> {
        let sent_at = self.lock_unrefreshed().point();
        let mut actions = Vec::with_capacity(parts.iter().map(|part| part.len()).sum());
        let mut logical: HashMap<String, IndexName> = HashMap::new();
        for envelope in parts.iter().flat_map(|part| part.iter()) {
            let action = self.bulk_action(envelope)?;
            if !logical.contains_key(action.index()) {
                logical.insert(action.index().to_owned(), envelope.index.clone());
            }
            actions.push(action);
        }

        let mut rejected: Vec<RejectedDocument> = Vec::new();
        actions.retain(|action| match refused_id(action) {
            None => true,
            // No document can have the id, so the delete has nothing to remove.
            Some(_) if matches!(action, BulkAction::Delete { .. }) => false,
            Some(reason) => {
                rejected.push(RejectedDocument {
                    index: action.index().to_owned(),
                    id: action.id().to_owned(),
                    reason,
                });
                false
            }
        });

        if actions.is_empty() && rejected.is_empty() {
            let (visibility, refreshes) =
                self.settle_visibility(caught_up, &actions, sent_at).await;
            return Ok(WriteReport::clean()
                .visible(visibility)
                .sent(0)
                .refreshed(refreshes));
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
            "writing",
        );

        let mut blocked: BTreeMap<IndexName, String> = BTreeMap::new();
        let mut missing: BTreeMap<IndexName, String> = BTreeMap::new();
        let mut start = 0usize;
        for &count in &plan {
            let end = start + count;
            let chunk_fragments = fragments.get(start..end).unwrap_or_default();
            let chunk_actions = actions.get(start..end).unwrap_or_default();
            let mut body = String::with_capacity(chunk_fragments.iter().map(String::len).sum());
            for fragment in chunk_fragments {
                body.push_str(fragment);
            }
            let chunk = self
                .send_bulk_chunk(&body, chunk_actions, caught_up.is_caught_up())
                .await?;
            rejected.extend(chunk.rejected);
            for (refused, into) in [(chunk.blocked, &mut blocked), (chunk.missing, &mut missing)] {
                for (physical, reason) in refused {
                    let Some(index) = logical.get(&physical) else {
                        return Err(SinkError::Write(format!(
                            "OpenSearch refused writes to {physical}, which no document of the \
                             write was bound for; failing the write so it is redelivered: {reason}"
                        )));
                    };
                    into.entry(index.clone()).or_insert(reason);
                }
            }
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

        let (visibility, refreshes) = self.settle_visibility(caught_up, &actions, sent_at).await;
        let refreshing_bulks = if caught_up.is_caught_up() {
            plan.len() as u64
        } else {
            0
        };
        let mut report = WriteReport::clean().rejecting(rejected);
        for (index, reason) in blocked {
            warn!(%index, %reason, "OpenSearch refuses writes to this index for now");
            report = report.blocking(index, reason);
        }
        for (index, reason) in missing {
            warn!(%index, %reason, "this index's target is gone");
            report = report.missing_target(index, reason);
        }
        Ok(report
            .visible(visibility)
            .sent(total_bytes as u64)
            .refreshed(refreshing_bulks + refreshes))
    }

    /// `batch_size`: the engine merges ready items up to it, so a merged
    /// write is not then split into a full request and a small one.
    fn write_limit(&self) -> Option<usize> {
        Some(self.batch_size)
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

    /// The hash alias serves a generation other than the one written: the
    /// rebuild's generation is being filled while readers stay on the old.
    async fn is_rebuilding(&self, index: &IndexName) -> Result<bool> {
        let Some(target) = self.target(index.as_ref()) else {
            return Ok(false);
        };
        let serving = self.alias_holders(&target.hash_alias).await?;
        Ok(!serving.is_empty() && !serving.contains(&target.generation))
    }

    async fn detachment(&self, mapping: &IndexMapping) -> Result<sink::Detachment> {
        self.read_detachment(mapping).await
    }

    async fn detach(
        &self,
        mapping: &IndexMapping,
        detach: &sink::Detach,
    ) -> Result<sink::Recorded> {
        self.write_detach(mapping, detach).await
    }

    async fn forget_detachment(
        &self,
        mapping: &IndexMapping,
        version: &sink::RecordVersion,
    ) -> Result<()> {
        self.delete_detach(mapping, version).await
    }

    async fn record_gaps(&self, mapping: &IndexMapping, gaps: &[sink::Gap]) -> Result<()> {
        self.write_gaps(mapping, gaps).await
    }

    async fn begin_rebuild(&self, mapping: &IndexMapping) -> Result<u64> {
        self.write_rebuild(mapping).await
    }

    async fn demote(&self, mapping: &IndexMapping, live_only: bool) -> Result<()> {
        self.write_demotion(mapping, live_only).await
    }

    async fn seeding(&self, index: &IndexName) -> Result<Seeding> {
        let Some(target) = self.target(index.as_ref()) else {
            return Ok(Seeding::Unseeded { walked: None });
        };
        Ok(match self.read_meta(&target.hash_alias).await? {
            Some(meta) if meta.seeded => Seeding::Seeded,
            Some(meta) => Seeding::Unseeded {
                walked: meta.walked,
            },
            None => Seeding::Unseeded { walked: None },
        })
    }

    /// A partial update of the index's meta doc, naming the active
    /// generation, so a later reindex (a new generation) or seed (whose
    /// read ignores it) leaves it unread.
    async fn record_walked(&self, index: &IndexName, mark: &str) -> Result<()> {
        let logical = index.as_ref();
        let Some(target) = self.target(logical) else {
            return Err(SinkError::Write(format!(
                "cannot record how far {logical}'s backfill got: it has no active generation"
            )));
        };
        let generation = parse_generation(&target.hash_alias, &target.generation).unwrap_or(1);
        self.update_walked(
            &target.hash_alias,
            json!({ "mark": mark, "generation": generation }),
        )
        .await
    }

    async fn forget_walked(&self, index: &IndexName) -> Result<()> {
        let Some(target) = self.target(index.as_ref()) else {
            return Ok(());
        };
        if self.read_meta(&target.hash_alias).await?.is_none() {
            return Ok(());
        }
        self.update_walked(&target.hash_alias, Value::Null).await
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
            uuid,
            ..
        } = &target;
        let active = parse_generation(hash_alias, generation).unwrap_or(1);

        // A generation deleted since staging, or recreated under its name by
        // a write under auto-create, holds only part of the rows: never seed
        // it. The engine stages the index again, which replaces it.
        let live = self.index_uuid(generation).await?;
        if live.is_none() || (uuid.is_some() && live != *uuid) {
            return Err(SinkError::TargetChanged(format!(
                "{generation} is not the generation staged for {logical} (deleted, or recreated \
                 by a write under action.auto_create_index)"
            )));
        }

        // The refresh, the steady-state refresh interval and the lookups are
        // independent: run at once, the step costs the slowest of them. The
        // aliases move only after the refresh, so they never point readers at
        // an unrefreshed generation.
        let mut steps = Steps::start();
        let convenience = self.convenience_alias(logical);
        let ((), (), (serving, named, stale)) = tokio::try_join!(
            self.refresh_index(generation),
            self.restore_auto_refresh(generation),
            self.seed_lookup(hash_alias, &convenience, generation),
        )?;
        steps.mark("refresh+settings+lookup");

        // One atomic call moves both aliases and drops the old generations:
        // readers see the old generation or the new one. Refused (the
        // convenience alias's name taken by an index, or a stale generation
        // gone meanwhile), the holders are read again and the hash alias
        // moves alone; the convenience alias stays best-effort.
        let hash = (hash_alias.as_str(), serving.as_slice());
        let both = [hash, (convenience.as_str(), named.as_slice())];
        let retired: Vec<&String> = serving
            .iter()
            .chain(&stale)
            .filter(|name| *name != generation)
            .collect();
        let first = match plan_seed_actions(generation, &both, &stale) {
            Some(actions) => {
                self.update_seed_aliases(&actions, generation, &retired)
                    .await
            }
            None => Ok(()),
        };
        if let Err(error) = first {
            // The old generation is blocked (#479): no other request form
            // would pass; the engine parks the seed and retries it.
            if matches!(error, SinkError::RetireBlocked { .. }) {
                return Err(error);
            }
            warn!(
                index = logical,
                %error,
                "could not swap the aliases and drop the old generations in one call; \
                 reading them again and moving the hash alias alone",
            );
            let (serving, _, stale) = self
                .seed_lookup(hash_alias, &convenience, generation)
                .await?;
            let hash = (hash_alias.as_str(), serving.as_slice());
            if let Some(hash_only) =
                plan_seed_actions(generation, std::slice::from_ref(&hash), &stale)
            {
                let retired: Vec<&String> = serving
                    .iter()
                    .chain(&stale)
                    .filter(|name| *name != generation)
                    .collect();
                self.update_seed_aliases(&hash_only, generation, &retired)
                    .await?;
            }
            self.ensure_alias(&convenience, generation).await;
        }
        steps.mark("aliases");

        self.write_index_meta(hash_alias, active, true, live.as_deref())
            .await?;
        steps.mark("marker");
        // A rebuild after a detach is finished with the seed (r4): its record
        // goes, after the marker. One left by a failure here is found
        // finished by the next staging and deleted then.
        if let Err(error) = self.forget_finished_rebuild(hash_alias).await {
            warn!(index = logical, %error, "could not delete the finished rebuild's detach record; the next staging does");
        }
        debug!(index = logical, %generation, steps = %steps, "marked seeded");
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
        self.write_index_meta(&hash_alias, next, false, None)
            .await?;
        debug!(
            logical,
            hash_alias, next, "staged reindex into a new generation"
        );
        Ok(())
    }
}

impl OpensearchSink {
    /// What a seed of `generation` finds: the hash alias's holders, the
    /// convenience alias's, and every other generation of the hash alias
    /// (alias holders and orphans alike, #121), to be dropped.
    async fn seed_lookup(
        &self,
        hash_alias: &str,
        convenience: &str,
        generation: &str,
    ) -> Result<(Vec<String>, Vec<String>, BTreeSet<String>)> {
        let aliases = [hash_alias, convenience];
        let (mut holders, generations) =
            tokio::try_join!(self.alias_map(&aliases), self.list_generations(hash_alias),)?;
        let serving = holders.remove(hash_alias).unwrap_or_default();
        let named = holders.remove(convenience).unwrap_or_default();
        let stale = serving
            .iter()
            .cloned()
            .chain(generations)
            .filter(|name| name != generation && parse_generation(hash_alias, name).is_some())
            .collect();
        Ok((serving, named, stale))
    }

    /// When the write that just sent `written` becomes searchable.
    ///
    /// A write that isn't caught up leaves its indexes to the next caught-up
    /// write, and to the periodic refresh while one is set. A caught-up write
    /// forced a refresh of what it sent; it also refreshes every index earlier
    /// writes left, so `Immediate` covers every write that returned before it,
    /// as the port requires. If that refresh fails the documents are still
    /// durable: the indexes stay remembered for the next caught-up write, and
    /// the write reports what the periodic refresh promises.
    async fn settle_visibility(
        &self,
        caught_up: CaughtUp<'_>,
        written: &[BulkAction],
        sent_at: u64,
    ) -> (Visibility, u64) {
        let unforced = match self.refresh_period() {
            Some(period) => Visibility::Within(period),
            None => Visibility::Deferred,
        };
        if !caught_up.is_caught_up() {
            if written.is_empty() {
                return (Visibility::Immediate, 0);
            }
            self.lock_unrefreshed()
                .mark(written.iter().map(|action| action.index().to_owned()));
            return (unforced, 0);
        }
        // One caught-up refresh at a time: a second one waits here, then sees
        // what this one put back if it failed, so neither answers `Immediate`
        // before a refresh covering the earlier writes succeeded.
        let _refreshing = self.refreshing.lock().await;
        // Only the marks of the indexes the write is caught up for: a trickle
        // on one index doesn't refresh another index's backlog. A mark whose
        // index the sink can't name is refreshed (more than asked, never less).
        let logical = self.logical_names();
        let in_scope = |index: &str| logical.get(index).is_none_or(|name| caught_up.covers(name));
        let pending = self
            .lock_unrefreshed()
            .take_uncovered(sent_at, in_scope, |index| {
                written.iter().any(|action| action.index() == index) && self.single_shard(index)
            });
        if pending.is_empty() {
            return (Visibility::Immediate, 0);
        }
        let names: BTreeSet<String> = pending.keys().cloned().collect();
        match self.refresh_indexes(&names).await {
            Ok(()) => (Visibility::Immediate, 1),
            Err(error) => {
                warn!(%error, "could not refresh indexes earlier writes left unrefreshed; retrying on the next caught-up write");
                self.lock_unrefreshed().restore(pending);
                (unforced, 1)
            }
        }
    }

    fn lock_unrefreshed(&self) -> std::sync::MutexGuard<'_, crate::refresh::Unrefreshed> {
        self.unrefreshed
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// The bulk action that writes `envelope`, addressed by its index's write
    /// target (see `IndexTarget::write_target`).
    fn bulk_action(&self, envelope: &Envelope) -> Result<BulkAction> {
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
                doc: render_document(document)?,
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
        Ok(action)
    }
}

/// How long each step of a `mark_seeded` took, for its debug line.
struct Steps {
    last: std::time::Instant,
    done: Vec<(&'static str, std::time::Duration)>,
}

impl Steps {
    fn start() -> Self {
        Self {
            last: std::time::Instant::now(),
            done: Vec::new(),
        }
    }

    fn mark(&mut self, step: &'static str) {
        let now = std::time::Instant::now();
        self.done.push((step, now - self.last));
        self.last = now;
    }
}

impl std::fmt::Display for Steps {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for (n, (step, took)) in self.done.iter().enumerate() {
            let sep = if n == 0 { "" } else { " " };
            write!(f, "{sep}{step}={:.1}ms", took.as_secs_f64() * 1000.0)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
