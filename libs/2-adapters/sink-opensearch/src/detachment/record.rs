//! Writing a detach (ADR 0012 stage 10, R4 and R11): the seed marker moved
//! first, then the detach record, then the unserved generations deleted.
//!
//! - **d2b.** The marker moves to a generation past every existing one,
//!   unseeded, with no `walked` and no `uuid`, keeping the marker as it was in
//!   `detaching_from`, checkpoint and gaps included: a put-back or a live-only
//!   demotion brings them back with the marker onto the serving generation,
//!   which still lacks the gaps' changes (the review of round 11, S-1). Only a
//!   seed that completed drops them (the review of the closing round, A): a
//!   rebuild, a demotion, a generation created and a reindex all carry them.
//!   A marker that already carries `detaching_from` is left
//!   exactly as it is, so a detach run again never moves it again. Skipped
//!   for a `live_only` detach: its marker stays on the serving generation.
//! - **d2a.** The record, `detach:{hash_alias}`: created when none was read,
//!   else replaced on the version read. `serving` is the generation the hash
//!   alias holds, what reads see; `refused_keys` is stored as JSON text, since
//!   the meta index maps its fields dynamically and keys of other types
//!   would refuse each other.
//! - **d2c.** Every generation but `serving` is deleted, best effort, and
//!   only while some generation serves.
//!
//! Every write is conditional on the `_mget` read before it. A conflict
//! (another writer, or a late write of a dead process) reads again and decides
//! again, so no write lands on a state it didn't read.
//!
//! Every write of the marker or the record carries the caller's
//! [`Stamp`]. A record is never deleted: forgetting it writes a
//! tombstone (`state: "forgotten"` and the stamp) on the version read, so who
//! forgot it stays readable. A tombstone counts as no record for every write
//! here, which replaces it on its version.
//!
//! ```text
//! _mget marker, record ─▶ PUT marker ?if_seq_no ─▶ PUT detach:… (_create | ?if_seq_no) ─▶ DELETE stale generations
//! ```

use kernel::IndexMapping;
use serde_json::{Value, json};
use sink::{Detach, RecordVersion, Recorded, Result, SinkError, Stamp};

use super::plan::{self, Plan, Step};
use super::{Found, Kept, stamp_value};
use crate::OpensearchSink;
use crate::generations::{generation_name, parse_generation};
use crate::transport::request_error;

/// How many times a detach reads again after a conflict before it gives up.
const CONFLICT_ROUNDS: usize = 5;

/// How long a marker rewrite waits for the meta index's primary shard when a
/// read finds it not ready yet.
const META_PRIMARY_WAIT: std::time::Duration = std::time::Duration::from_secs(30);

/// Whether `refused` says the meta index's shard is not ready yet: it is
/// still being created or recovering.
fn shard_not_ready(refused: &SinkError) -> bool {
    let text = refused.to_string();
    text.contains("no_shard_available_action_exception")
        || text.contains("illegal_index_shard_state_exception")
}

/// Whether a plan needs the generations listed, or deletes from the list
/// best effort only.
#[derive(Debug, Clone, Copy)]
enum Listing {
    Needed,
    BestEffort,
}

/// What a conditional write found.
pub(super) enum Written {
    Done,
    Conflict,
}

/// What a plan stands on: the documents read with their versions, and the
/// generations behind the hash alias.
struct Read {
    marker: Option<Found>,
    record: Option<Found>,
    serving: Option<u64>,
    generations: Vec<u64>,
}

impl Read {
    fn kept<'a>(&'a self, now: &'a str) -> plan::Kept<'a> {
        plan::Kept {
            marker: self.marker.as_ref().map(|found| &found.source),
            record: self.record.as_ref().map(|found| &found.source),
            serving: self.serving,
            generations: &self.generations,
            now,
        }
    }
}

impl OpensearchSink {
    /// The documents and generations a plan for `hash_alias` reads.
    async fn read_for_plan(&self, hash_alias: &str) -> Result<Read> {
        self.read_for_plan_listing(hash_alias, Listing::Needed)
            .await
    }

    /// [`read_for_plan`](Self::read_for_plan), where a failed listing of the
    /// generations reads as none when `listing` says the plan only deletes
    /// from it best effort.
    async fn read_for_plan_listing(&self, hash_alias: &str, listing: Listing) -> Result<Read> {
        let Kept { marker, record, .. } = self.read_kept(hash_alias).await?;
        let serving = self
            .alias_holders(hash_alias)
            .await?
            .iter()
            .filter_map(|holder| parse_generation(hash_alias, holder))
            .max();
        let generations = match (self.list_generations(hash_alias).await, listing) {
            (Ok(names), _) => names
                .iter()
                .filter_map(|name| parse_generation(hash_alias, name))
                .collect(),
            (Err(error), Listing::BestEffort) => {
                tracing::warn!(hash_alias, %error, "could not list the generations; a live-only detach deletes none of them, the next rebuild does");
                Vec::new()
            }
            (Err(error), Listing::Needed) => return Err(error),
        };
        Ok(Read {
            marker,
            record,
            serving,
            generations,
        })
    }

    /// Send `plan` for `hash_alias`, in order, each write conditional on
    /// `read` and stamped `stamp`; then its best-effort deletes. `Conflict`
    /// when a write met another first: the caller reads again.
    async fn apply(
        &self,
        hash_alias: &str,
        plan: &Plan,
        read: &Read,
        stamp: Option<&Stamp>,
    ) -> Result<Written> {
        let record_id = format!("detach:{hash_alias}");
        for step in &plan.steps {
            let written = match step {
                Step::Delete(generation) => {
                    self.delete_index(&generation_name(hash_alias, *generation))
                        .await?;
                    Written::Done
                }
                Step::Marker(doc) => {
                    self.put_if(hash_alias, doc, read.marker.as_ref(), stamp)
                        .await?
                }
                Step::Record(doc) => {
                    self.put_if(&record_id, doc, read.record.as_ref(), stamp)
                        .await?
                }
            };
            if let Written::Conflict = written {
                return Ok(Written::Conflict);
            }
        }
        for generation in &plan.best_effort {
            let name = generation_name(hash_alias, *generation);
            if let Err(error) = self.delete_index(&name).await {
                tracing::warn!(generation = name, %error, "could not delete an unserved generation after a detach; the rebuild deletes it");
            }
        }
        Ok(Written::Done)
    }

    /// Record a detach of `mapping`'s index (see the module doc).
    pub(crate) async fn write_detach(
        &self,
        mapping: &IndexMapping,
        detach: &Detach,
    ) -> Result<Recorded> {
        let hash_alias = self.hash_alias(mapping.index.as_ref(), &mapping.hash.to_string());
        // A live-only detach moves no marker: the generations are only its
        // best-effort deletes, so a failed listing doesn't fail it (m-4). A
        // rebuildable one moves the marker past every generation and needs
        // them all.
        let listing = if detach.live_only {
            Listing::BestEffort
        } else {
            Listing::Needed
        };
        for _ in 0..CONFLICT_ROUNDS {
            let read = self.read_for_plan_listing(&hash_alias, listing).await?;
            let now = now();
            let plan = plan::detach(&read.kept(&now), detach).map_err(SinkError::Write)?;
            if let Written::Conflict = self
                .apply(&hash_alias, &plan, &read, detach.stamp.as_ref())
                .await?
            {
                continue;
            }
            tracing::warn!(
                index = mapping.index.as_ref(),
                hash_alias,
                why = detach.why,
                live_only = detach.live_only,
                "the index is detached: reads are frozen at the detach, and changes since are not applied"
            );
            return Ok(Recorded::Kept);
        }
        Err(SinkError::Write(format!(
            "the detach records of {hash_alias} kept changing under the detach; it is tried again"
        )))
    }

    /// Forget `mapping`'s detach record if it is still at `version`: a
    /// tombstone stamped `stamp` takes its place. A record changed or gone
    /// since is an error, so the caller reads again.
    pub(crate) async fn forget_detach(
        &self,
        mapping: &IndexMapping,
        version: &RecordVersion,
        stamp: &Stamp,
    ) -> Result<()> {
        let hash_alias = self.hash_alias(mapping.index.as_ref(), &mapping.hash.to_string());
        let Some(read) = parse_version(version) else {
            return Err(SinkError::Write(format!(
                "the detach record's version `{}` is not one this sink wrote",
                version.as_str()
            )));
        };
        match self.put_tombstone(&hash_alias, read, Some(stamp)).await? {
            Written::Done => Ok(()),
            Written::Conflict => Err(SinkError::Write(format!(
                "the detach record of {hash_alias} changed since it was read; it is read again"
            ))),
        }
    }

    /// A tombstone stamped `stamp` over `detach:{hash_alias}`, conditional on
    /// `(seq_no, primary_term)`.
    async fn put_tombstone(
        &self,
        hash_alias: &str,
        (seq_no, primary_term): (u64, u64),
        stamp: Option<&Stamp>,
    ) -> Result<Written> {
        let read = Found {
            source: Value::Null,
            seq_no: Some(seq_no),
            primary_term: Some(primary_term),
        };
        self.put_if(
            &format!("detach:{hash_alias}"),
            &plan::tombstone(),
            Some(&read),
            stamp,
        )
        .await
    }

    /// Write `doc` as `id`, stamped `stamp` (`null` for a caller that keeps
    /// no records), conditional on `read`: replaced on its version, or
    /// created when nothing was read. `Conflict` when another write came
    /// first. The one way a record or marker is written whole here, so no
    /// whole write goes out without its writer's stamp.
    pub(super) async fn put_if(
        &self,
        id: &str,
        doc: &Value,
        read: Option<&Found>,
        stamp: Option<&Stamp>,
    ) -> Result<Written> {
        let mut doc = doc.clone();
        if let Some(fields) = doc.as_object_mut() {
            fields.insert("stamp".to_owned(), stamp_value(stamp));
        }
        let doc = &doc;
        let meta = self.meta_index();
        let url = match read {
            None => format!("{}/{meta}/_create/{id}", self.base_url),
            Some(found) => {
                let Some((seq_no, primary_term)) = found.version() else {
                    return Err(SinkError::Write(format!(
                        "{id} was read without its sequence number"
                    )));
                };
                format!(
                    "{}/{meta}/_doc/{id}?if_seq_no={seq_no}&if_primary_term={primary_term}",
                    self.base_url
                )
            }
        };
        let context = "writing a detach record failed";
        let response = self
            .maybe_auth(self.client.put(&url).json(doc))
            .send()
            .await
            .map_err(|e| request_error(context, e))?;
        let status = response.status();
        if status.is_success() {
            return Ok(Written::Done);
        }
        if status == reqwest::StatusCode::CONFLICT {
            return Ok(Written::Conflict);
        }
        let text = response.text().await.unwrap_or_default();
        Err(SinkError::Write(format!(
            "{context}: HTTP {status}: {text}"
        )))
    }
}

/// `seq_no:primary_term`, as [`read_detachment`](OpensearchSink::read_detachment) writes it.
fn parse_version(version: &RecordVersion) -> Option<(u64, u64)> {
    let (seq_no, primary_term) = version.as_str().split_once(':')?;
    Some((seq_no.parse().ok()?, primary_term.parse().ok()?))
}

/// The wall clock now, as RFC 3339 in UTC.
fn now() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

impl OpensearchSink {
    /// r1 of a rebuild (see [`Records::begin_rebuild`](sink::Records::begin_rebuild)),
    /// as [`plan::begin_rebuild`] decides it, stamped `stamp`. The new
    /// attempt.
    pub(crate) async fn write_rebuild(&self, mapping: &IndexMapping, stamp: &Stamp) -> Result<u64> {
        let hash_alias = self.hash_alias(mapping.index.as_ref(), &mapping.hash.to_string());
        for _ in 0..CONFLICT_ROUNDS {
            let read = self.read_for_plan(&hash_alias).await?;
            let now = now();
            let (plan, attempts) = plan::begin_rebuild(&read.kept(&now))
                .map_err(|why| SinkError::Write(format!("{hash_alias}: {why}")))?;
            if let Written::Conflict = self.apply(&hash_alias, &plan, &read, Some(stamp)).await? {
                continue;
            }
            tracing::info!(
                index = mapping.index.as_ref(),
                hash_alias,
                attempts,
                "rebuild started: the record says rebuilding"
            );
            return Ok(attempts);
        }
        Err(SinkError::Write(format!(
            "the detach records of {hash_alias} kept changing under the rebuild; it is tried again"
        )))
    }

    /// A rebuild put back to detached (O26, R14), as [`plan::demote`]
    /// decides it, stamped `stamp`.
    pub(crate) async fn write_demotion(
        &self,
        mapping: &IndexMapping,
        live_only: bool,
        stamp: &Stamp,
    ) -> Result<()> {
        let hash_alias = self.hash_alias(mapping.index.as_ref(), &mapping.hash.to_string());
        for _ in 0..CONFLICT_ROUNDS {
            let read = self.read_for_plan(&hash_alias).await?;
            let now = now();
            let Some(plan) = plan::demote(&read.kept(&now), live_only) else {
                return Ok(());
            };
            if let Written::Conflict = self.apply(&hash_alias, &plan, &read, Some(stamp)).await? {
                continue;
            }
            tracing::warn!(
                index = mapping.index.as_ref(),
                hash_alias,
                live_only,
                "the rebuild is put back to detached"
            );
            return Ok(());
        }
        Err(SinkError::Write(format!(
            "the detach records of {hash_alias} kept changing under the demotion"
        )))
    }

    /// The record of a rebuild the seed just finished, forgotten on the
    /// version read by a tombstone stamped `stamp`; none (or a tombstone) is
    /// fine.
    /// Write the marker whole on `generation`, unseeded, for a write that is
    /// not a seed (a generation created, a reindex): it keeps the gaps and
    /// the `detaching_from` of the marker read, conditional on it, read
    /// again on a conflict (the review of the closing round, A). Only the
    /// seed's write ([`write_index_meta`](Self::write_index_meta)) drops them.
    pub(crate) async fn rewrite_marker(
        &self,
        hash_alias: &str,
        generation: u64,
        uuid: Option<&str>,
        stamp: Option<&Stamp>,
    ) -> Result<()> {
        for _ in 0..CONFLICT_ROUNDS {
            let read = match self.read_kept(hash_alias).await {
                // The meta index was just created by a write elsewhere (a
                // write waits for its primary, a read is refused at once):
                // wait for its primary, then read again (the review of
                // round 13, H).
                Err(refused) if shard_not_ready(&refused) => {
                    self.wait_for_meta_primary().await?;
                    self.read_kept(hash_alias).await
                }
                read => read,
            };
            let Kept { marker, .. } = read?;
            let doc = plan::rewritten(marker.as_ref().map(|found| &found.source), generation, uuid);
            if let Written::Done = self
                .put_if(hash_alias, &doc, marker.as_ref(), stamp)
                .await?
            {
                return Ok(());
            }
        }
        Err(SinkError::Write(format!(
            "the seed marker of {hash_alias} kept changing while it was rewritten"
        )))
    }

    /// Record `uuid`, the `index.uuid` of `generation`, on the marker, only
    /// while the marker still names `generation` and records no uuid,
    /// conditional on the marker read and decided again on a conflict (the
    /// review of round 14, m-B: another process's demotion landing in
    /// between would otherwise get another generation's uuid, and the next
    /// `ensure_index` would delete the generation it names). A marker that
    /// moved meanwhile is left as it is: the next ensure records the uuid of
    /// what it names.
    pub(crate) async fn record_uuid(
        &self,
        hash_alias: &str,
        generation: u64,
        uuid: &str,
        stamp: Option<&Stamp>,
    ) -> Result<()> {
        for _ in 0..CONFLICT_ROUNDS {
            let Kept { marker, .. } = self.read_kept(hash_alias).await?;
            let Some(found) = marker.as_ref() else {
                return Ok(());
            };
            let names = found
                .source
                .get("active_generation")
                .and_then(Value::as_u64)
                == Some(generation);
            let recorded = found
                .source
                .get("uuid")
                .is_some_and(|recorded| !recorded.is_null());
            if !names || recorded {
                return Ok(());
            }
            let mut doc = found.source.clone();
            if let Some(fields) = doc.as_object_mut() {
                fields.remove("stamp");
                fields.insert("uuid".to_owned(), Value::from(uuid));
            }
            if let Written::Done = self
                .put_if(hash_alias, &doc, marker.as_ref(), stamp)
                .await?
            {
                return Ok(());
            }
        }
        Err(SinkError::Write(format!(
            "the seed marker of {hash_alias} kept changing while its uuid was recorded"
        )))
    }

    /// The seed's marker write (see [`plan::seeded`]): whole and seeded;
    /// `in_place`, keeping the gaps of the marker read, conditional on it.
    pub(crate) async fn write_seeded_marker(
        &self,
        hash_alias: &str,
        generation: u64,
        uuid: Option<&str>,
        in_place: bool,
        stamp: Option<&Stamp>,
    ) -> Result<()> {
        if !in_place {
            return self
                .write_index_meta(hash_alias, generation, true, uuid, stamp)
                .await;
        }
        for _ in 0..CONFLICT_ROUNDS {
            let Kept { marker, .. } = self.read_kept(hash_alias).await?;
            let doc = plan::seeded(
                marker.as_ref().map(|found| &found.source),
                generation,
                uuid,
                true,
            );
            if let Written::Done = self
                .put_if(hash_alias, &doc, marker.as_ref(), stamp)
                .await?
            {
                return Ok(());
            }
        }
        Err(SinkError::Write(format!(
            "the seed marker of {hash_alias} kept changing while it was seeded"
        )))
    }

    /// Whether a reindex moves `hash_alias`'s unseeded marker to a new
    /// generation: when it keeps a gap on the generation the alias serves
    /// ([`plan::reindex_moves`]).
    pub(crate) async fn reindex_moves(&self, hash_alias: &str) -> Result<bool> {
        let Kept { marker, .. } = self.read_kept(hash_alias).await?;
        let serving = self
            .alias_holders(hash_alias)
            .await?
            .iter()
            .filter_map(|holder| parse_generation(hash_alias, holder))
            .max();
        Ok(plan::reindex_moves(
            marker.as_ref().map(|found| &found.source),
            serving,
        ))
    }

    /// Wait until the meta index's primary shard is active, at most
    /// [`META_PRIMARY_WAIT`]: OpenSearch's cluster health at `yellow` for it.
    async fn wait_for_meta_primary(&self) -> Result<()> {
        let context = "waiting for the meta index's primary shard failed";
        let url = format!(
            "{}/_cluster/health/{}?wait_for_status=yellow&timeout={}s",
            self.base_url,
            self.meta_index(),
            META_PRIMARY_WAIT.as_secs()
        );
        let response = self
            .maybe_auth(self.client.get(&url))
            .send()
            .await
            .map_err(|e| request_error(context, e))?;
        let status = response.status();
        let text = response
            .text()
            .await
            .map_err(|e| request_error(context, e))?;
        let timed_out = serde_json::from_str::<Value>(&text)
            .ok()
            .and_then(|body| body.get("timed_out").and_then(Value::as_bool))
            .unwrap_or(false);
        if !status.is_success() || timed_out {
            return Err(SinkError::Write(format!(
                "{context}: HTTP {status}: {text}"
            )));
        }
        Ok(())
    }

    pub(crate) async fn forget_finished_rebuild(
        &self,
        hash_alias: &str,
        stamp: Option<&Stamp>,
    ) -> Result<()> {
        let Kept { record, .. } = self.read_kept(hash_alias).await?;
        let Some(read) = record
            .as_ref()
            .filter(|found| !plan::is_tombstone(&found.source))
            .and_then(Found::version)
        else {
            return Ok(());
        };
        match self.put_tombstone(hash_alias, read, stamp).await? {
            Written::Done => Ok(()),
            Written::Conflict => Err(SinkError::Write(format!(
                "the finished rebuild's record of {hash_alias} changed since it was read"
            ))),
        }
    }
}

impl OpensearchSink {
    /// Change the gaps kept with the seed marker as `decide` says: a partial
    /// update touching nothing else but the stamp (an earlier single `gap`
    /// nulled), conditional on the marker read, read again and decided again
    /// on a conflict. Only the seed's whole write of the marker drops them:
    /// the index then holds everything (the review of the closing round, A).
    pub(crate) async fn write_gaps(
        &self,
        mapping: &IndexMapping,
        decide: &sink::DecideGaps<'_>,
        stamp: &Stamp,
    ) -> Result<Vec<sink::Gap>> {
        let hash_alias = self.hash_alias(mapping.index.as_ref(), &mapping.hash.to_string());
        let context = "recording the reattach's gaps failed";
        for _ in 0..CONFLICT_ROUNDS {
            let Kept { marker, .. } = self.read_kept(&hash_alias).await?;
            let Some(marker) = marker else {
                return Err(SinkError::Write(format!(
                    "{context}: {hash_alias} has no seed marker"
                )));
            };
            let kept = super::read_gaps(&marker.source);
            let Some(gaps) = decide(&kept) else {
                return Ok(kept);
            };
            let Some((seq_no, primary_term)) = marker.version() else {
                return Err(SinkError::Write(format!(
                    "{context}: the seed marker of {hash_alias} was read without its sequence number"
                )));
            };
            let url = format!(
                "{}/{}/_update/{hash_alias}?if_seq_no={seq_no}&if_primary_term={primary_term}",
                self.base_url,
                self.meta_index()
            );
            let mut doc = plan::gaps_update(&gaps);
            if let Some(fields) = doc.as_object_mut() {
                fields.insert("stamp".to_owned(), stamp_value(Some(stamp)));
            }
            let body = json!({ "doc": doc });
            let response = self
                .maybe_auth(self.client.post(&url).json(&body))
                .send()
                .await
                .map_err(|e| request_error(context, e))?;
            let status = response.status();
            if status.is_success() {
                return Ok(gaps);
            }
            if status == reqwest::StatusCode::CONFLICT {
                continue;
            }
            let text = response.text().await.unwrap_or_default();
            return Err(SinkError::Write(format!(
                "{context}: HTTP {status}: {text}"
            )));
        }
        Err(SinkError::Write(format!(
            "the seed marker of {hash_alias} kept changing under the gaps' write"
        )))
    }
}
