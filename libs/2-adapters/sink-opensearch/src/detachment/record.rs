//! Writing a detach (ADR 0012 stage 10, R4 and R11): the seed marker moved
//! first, then the detach record, then the unserved generations deleted.
//!
//! - **d2b.** The marker moves to a generation past every existing one,
//!   unseeded, with no `walked` and no `uuid`, keeping the marker as it was in
//!   `detaching_from`, checkpoint and gaps included: a put-back or a live-only
//!   demotion brings them back with the marker onto the serving generation,
//!   which still lacks the gaps' changes (the review of round 11, S-1). Only a
//!   write that rebuilds the index (a seed, a reindex) drops them. A marker that already carries `detaching_from` is left
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

use super::{FORGOTTEN, Found, Kept, carried_gaps, gaps_value, read_gaps, stamp_value};
use crate::OpensearchSink;
use crate::generations::{next_generation, parse_generation};
use crate::transport::request_error;

/// How many times a detach reads again after a conflict before it gives up.
const CONFLICT_ROUNDS: usize = 5;

/// What a conditional write found.
pub(super) enum Written {
    Done,
    Conflict,
}

impl OpensearchSink {
    /// Record a detach of `mapping`'s index (see the module doc).
    pub(crate) async fn write_detach(
        &self,
        mapping: &IndexMapping,
        detach: &Detach,
    ) -> Result<Recorded> {
        let hash_alias = self.hash_alias(mapping.index.as_ref(), &mapping.hash.to_string());
        let record_id = format!("detach:{hash_alias}");
        for _ in 0..CONFLICT_ROUNDS {
            let Kept { marker, record, .. } = self.read_kept(&hash_alias).await?;
            let serving = self
                .alias_holders(&hash_alias)
                .await?
                .iter()
                .filter_map(|holder| parse_generation(&hash_alias, holder))
                .max();
            if !detach.live_only
                && let Some(marker) = &marker
                && marker
                    .source
                    .get("detaching_from")
                    .is_none_or(Value::is_null)
            {
                let next = next_generation(&self.list_generations(&hash_alias).await?, &hash_alias);
                let moved = json!({
                    "active_generation": next,
                    "seeded": false,
                    "walked": null,
                    "uuid": null,
                    "detaching_from": {
                        "generation": marker.source.get("active_generation"),
                        "seeded": marker.source.get("seeded").and_then(Value::as_bool).unwrap_or(false),
                        "uuid": marker.source.get("uuid"),
                        "walked": marker.source.get("walked"),
                        "gaps": gaps_value(&read_gaps(&marker.source)),
                    },
                });
                if let Written::Conflict = self
                    .put_if(&hash_alias, &moved, Some(marker), detach.stamp.as_ref())
                    .await?
                {
                    continue;
                }
            }
            let body = record_body(detach, serving, record.as_ref())?;
            if let Written::Conflict = self
                .put_if(&record_id, &body, record.as_ref(), detach.stamp.as_ref())
                .await?
            {
                continue;
            }
            if let Some(serving) = serving {
                self.delete_unserved(&hash_alias, serving).await;
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
            &json!({ "state": FORGOTTEN }),
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

    /// d2c: delete every generation of `hash_alias` but `serving`, best
    /// effort: one left behind is deleted by the rebuild (R9).
    async fn delete_unserved(&self, hash_alias: &str, serving: u64) {
        let generations = match self.list_generations(hash_alias).await {
            Ok(generations) => generations,
            Err(error) => {
                tracing::warn!(hash_alias, %error, "could not list the generations to delete after a detach; the rebuild deletes them");
                return;
            }
        };
        for generation in generations {
            if parse_generation(hash_alias, &generation).is_none_or(|n| n == serving) {
                continue;
            }
            if let Err(error) = self.delete_index(&generation).await {
                tracing::warn!(generation, %error, "could not delete an unserved generation after a detach; the rebuild deletes it");
            }
        }
    }
}

/// The detach record to write: a fresh one, or `read` brought to `detached`,
/// keeping its attempts, and its `since` when it was already detached. A
/// tombstone read keeps nothing.
pub(crate) fn record_body(
    detach: &Detach,
    serving: Option<u64>,
    read: Option<&Found>,
) -> Result<Value> {
    let read = read.filter(|found| !found.is_tombstone());
    let kept = |key: &str| {
        read.and_then(|found| found.source.get(key))
            .cloned()
            .unwrap_or(Value::Null)
    };
    let was_detached = read
        .and_then(|found| found.source.get("state"))
        .and_then(Value::as_str)
        == Some("detached");
    let since = if was_detached && !kept("since").is_null() {
        kept("since")
    } else {
        Value::from(now())
    };
    let refused_keys = serde_json::to_string(&detach.refused_keys)
        .map_err(|e| SinkError::Write(format!("the refused keys can't be written: {e}")))?;
    Ok(json!({
        "state": "detached",
        "live_only": detach.live_only,
        "serving": serving,
        "why": detach.why,
        "refused_keys": refused_keys,
        "attempts": kept("attempts").as_u64().unwrap_or(0),
        "last_attempt": kept("last_attempt"),
        "since": since,
        "through": match &detach.through {
            Some(through) => Value::from(through.as_str()),
            None => kept("through"),
        },
    }))
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
    /// r1 of a rebuild (see [`Sink::begin_rebuild`](sink::Sink::begin_rebuild)):
    /// delete every generation but the record's `serving`, move the marker
    /// past every existing generation (keeping `detaching_from`), and write
    /// the record `rebuilding` with one more attempt, both stamped `stamp`.
    /// The new attempt.
    pub(crate) async fn write_rebuild(&self, mapping: &IndexMapping, stamp: &Stamp) -> Result<u64> {
        let hash_alias = self.hash_alias(mapping.index.as_ref(), &mapping.hash.to_string());
        let record_id = format!("detach:{hash_alias}");
        for _ in 0..CONFLICT_ROUNDS {
            let Kept { marker, record, .. } = self.read_kept(&hash_alias).await?;
            let Some(record) = record.filter(|found| !found.is_tombstone()) else {
                return Err(SinkError::Write(format!(
                    "{hash_alias} has no detach record to rebuild from"
                )));
            };
            let serving = record.source.get("serving").and_then(Value::as_u64);
            self.delete_all_but(&hash_alias, serving).await?;
            if let Written::Conflict = self
                .move_marker_past(&hash_alias, marker.as_ref(), stamp)
                .await?
            {
                continue;
            }
            let attempts = record
                .source
                .get("attempts")
                .and_then(Value::as_u64)
                .unwrap_or(0)
                + 1;
            let mut body = record.source.clone();
            if let Some(fields) = body.as_object_mut() {
                fields.insert("state".to_owned(), Value::from("rebuilding"));
                fields.insert("attempts".to_owned(), Value::from(attempts));
                fields.insert("last_attempt".to_owned(), Value::from(now()));
            }
            if let Written::Conflict = self
                .put_if(&record_id, &body, Some(&record), Some(stamp))
                .await?
            {
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

    /// A rebuild put back to detached (O26, R14): the partial generation and
    /// its checkpoint go, the record says `detached` again; the marker moves
    /// past every generation, or, `live_only`, back on `serving` as it was.
    /// The marker and the record are stamped `stamp`.
    pub(crate) async fn write_demotion(
        &self,
        mapping: &IndexMapping,
        live_only: bool,
        stamp: &Stamp,
    ) -> Result<()> {
        let hash_alias = self.hash_alias(mapping.index.as_ref(), &mapping.hash.to_string());
        let record_id = format!("detach:{hash_alias}");
        for _ in 0..CONFLICT_ROUNDS {
            let Kept { marker, record, .. } = self.read_kept(&hash_alias).await?;
            let Some(record) = record.filter(|found| !found.is_tombstone()) else {
                return Ok(());
            };
            let serving = record.source.get("serving").and_then(Value::as_u64);
            if serving.is_some() {
                self.delete_all_but(&hash_alias, serving).await?;
            }
            let moved = if live_only {
                self.restore_marker(&hash_alias, marker.as_ref(), serving, stamp)
                    .await?
            } else {
                self.move_marker_past(&hash_alias, marker.as_ref(), stamp)
                    .await?
            };
            if let Written::Conflict = moved {
                continue;
            }
            let mut body = record.source.clone();
            if let Some(fields) = body.as_object_mut() {
                fields.insert("state".to_owned(), Value::from("detached"));
                if live_only {
                    fields.insert("live_only".to_owned(), Value::from(true));
                }
            }
            if let Written::Conflict = self
                .put_if(&record_id, &body, Some(&record), Some(stamp))
                .await?
            {
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
    pub(crate) async fn forget_finished_rebuild(
        &self,
        hash_alias: &str,
        stamp: Option<&Stamp>,
    ) -> Result<()> {
        let Kept { record, .. } = self.read_kept(hash_alias).await?;
        let Some(read) = record
            .as_ref()
            .filter(|found| !found.is_tombstone())
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

    /// Delete every generation of `hash_alias` but `keep`; a refused delete is
    /// an error (r1a: nothing is read while a partial stays).
    async fn delete_all_but(&self, hash_alias: &str, keep: Option<u64>) -> Result<()> {
        for generation in self.list_generations(hash_alias).await? {
            let Some(number) = parse_generation(hash_alias, &generation) else {
                continue;
            };
            if Some(number) != keep {
                self.delete_index(&generation).await?;
            }
        }
        Ok(())
    }

    /// The marker moved past every existing generation, unseeded, with no
    /// checkpoint and no uuid, keeping its `detaching_from`: the explicit move
    /// of a rebuild or a demotion (never R4's d2b, which leaves a moved marker
    /// as it is).
    async fn move_marker_past(
        &self,
        hash_alias: &str,
        marker: Option<&Found>,
        stamp: &Stamp,
    ) -> Result<Written> {
        let next = next_generation(&self.list_generations(hash_alias).await?, hash_alias);
        let detaching_from = marker
            .and_then(|found| found.source.get("detaching_from"))
            .cloned()
            .unwrap_or(Value::Null);
        let moved = json!({
            "active_generation": next,
            "seeded": false,
            "walked": null,
            "uuid": null,
            "detaching_from": detaching_from,
        });
        self.put_if(hash_alias, &moved, marker, Some(stamp)).await
    }

    /// The marker back on `serving` as `detaching_from` says it was (R14).
    async fn restore_marker(
        &self,
        hash_alias: &str,
        marker: Option<&Found>,
        serving: Option<u64>,
        stamp: &Stamp,
    ) -> Result<Written> {
        let from = marker.and_then(|found| found.source.get("detaching_from"));
        let Some(generation) = serving else {
            return Ok(Written::Done);
        };
        let mut restored = json!({
            "active_generation": generation,
            "seeded": from.and_then(|from| from.get("seeded")).and_then(Value::as_bool).unwrap_or(true),
        });
        if let Some(fields) = restored.as_object_mut() {
            if let Some(uuid) = from
                .and_then(|from| from.get("uuid"))
                .and_then(Value::as_str)
            {
                fields.insert("uuid".to_owned(), Value::from(uuid));
            }
            if let Some(gaps) = carried_gaps(from) {
                fields.insert("gaps".to_owned(), gaps);
            }
        }
        self.put_if(hash_alias, &restored, marker, Some(stamp))
            .await
    }
}

impl OpensearchSink {
    /// Change the gaps kept with the seed marker as `decide` says: a partial
    /// update touching nothing else but the stamp (an earlier single `gap`
    /// nulled), conditional on the marker read, read again and decided again
    /// on a conflict. A whole-document write of the marker (a reindex, a
    /// seed) drops them: the index is rebuilt then, and nothing is missing.
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
            let body = json!({ "doc": {
                "gaps": gaps_value(&gaps),
                "gap": null,
                "stamp": stamp_value(Some(stamp)),
            } });
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
