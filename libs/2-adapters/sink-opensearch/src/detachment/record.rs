//! Writing a detach (ADR 0012 stage 10, R4 and R11): the seed marker moved
//! first, then the detach record, then the unserved generations deleted.
//!
//! - **d2b.** The marker moves to a generation past every existing one,
//!   unseeded, with no `walked` and no `uuid`, keeping the marker as it was in
//!   `detaching_from`, checkpoint included. A marker that already carries `detaching_from` is left
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
//! ```text
//! _mget marker, record ─▶ PUT marker ?if_seq_no ─▶ PUT detach:… (_create | ?if_seq_no) ─▶ DELETE stale generations
//! ```

use kernel::IndexMapping;
use serde_json::{Value, json};
use sink::{Detach, RecordVersion, Recorded, Result, SinkError};

use super::{Found, Kept};
use crate::OpensearchSink;
use crate::generations::{next_generation, parse_generation};
use crate::transport::request_error;

/// How many times a detach reads again after a conflict before it gives up.
const CONFLICT_ROUNDS: usize = 5;

/// What a conditional write found.
enum Written {
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
                    },
                });
                if let Written::Conflict = self.put_if(&hash_alias, &moved, Some(marker)).await? {
                    continue;
                }
            }
            let body = record_body(detach, serving, record.as_ref())?;
            if let Written::Conflict = self.put_if(&record_id, &body, record.as_ref()).await? {
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

    /// Delete `mapping`'s detach record if it is still at `version`.
    pub(crate) async fn delete_detach(
        &self,
        mapping: &IndexMapping,
        version: &RecordVersion,
    ) -> Result<()> {
        let hash_alias = self.hash_alias(mapping.index.as_ref(), &mapping.hash.to_string());
        let Some((seq_no, primary_term)) = parse_version(version) else {
            return Err(SinkError::Write(format!(
                "the detach record's version `{}` is not one this sink wrote",
                version.as_str()
            )));
        };
        let url = format!(
            "{}/{}/_doc/detach:{hash_alias}?if_seq_no={seq_no}&if_primary_term={primary_term}",
            self.base_url,
            self.meta_index()
        );
        let context = "deleting the detach record failed";
        let response = self
            .maybe_auth(self.client.delete(&url))
            .send()
            .await
            .map_err(|e| request_error(context, e))?;
        let status = response.status();
        if status.is_success() || status == reqwest::StatusCode::NOT_FOUND {
            return Ok(());
        }
        let text = response.text().await.unwrap_or_default();
        Err(SinkError::Write(format!(
            "{context}: HTTP {status}: {text}"
        )))
    }

    /// Write `doc` as `id`, conditional on `read`: replaced on its version,
    /// or created when nothing was read. `Conflict` when another write came
    /// first.
    async fn put_if(&self, id: &str, doc: &Value, read: Option<&Found>) -> Result<Written> {
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
/// keeping its attempts, and its `since` when it was already detached.
fn record_body(detach: &Detach, serving: Option<u64>, read: Option<&Found>) -> Result<Value> {
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
    /// the record `rebuilding` with one more attempt. The new attempt.
    pub(crate) async fn write_rebuild(&self, mapping: &IndexMapping) -> Result<u64> {
        let hash_alias = self.hash_alias(mapping.index.as_ref(), &mapping.hash.to_string());
        let record_id = format!("detach:{hash_alias}");
        for _ in 0..CONFLICT_ROUNDS {
            let Kept { marker, record, .. } = self.read_kept(&hash_alias).await?;
            let Some(record) = record else {
                return Err(SinkError::Write(format!(
                    "{hash_alias} has no detach record to rebuild from"
                )));
            };
            let serving = record.source.get("serving").and_then(Value::as_u64);
            self.delete_all_but(&hash_alias, serving).await?;
            if let Written::Conflict = self.move_marker_past(&hash_alias, marker.as_ref()).await? {
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
            if let Written::Conflict = self.put_if(&record_id, &body, Some(&record)).await? {
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
    pub(crate) async fn write_demotion(
        &self,
        mapping: &IndexMapping,
        live_only: bool,
    ) -> Result<()> {
        let hash_alias = self.hash_alias(mapping.index.as_ref(), &mapping.hash.to_string());
        let record_id = format!("detach:{hash_alias}");
        for _ in 0..CONFLICT_ROUNDS {
            let Kept { marker, record, .. } = self.read_kept(&hash_alias).await?;
            let Some(record) = record else {
                return Ok(());
            };
            let serving = record.source.get("serving").and_then(Value::as_u64);
            if serving.is_some() {
                self.delete_all_but(&hash_alias, serving).await?;
            }
            let moved = if live_only {
                self.restore_marker(&hash_alias, marker.as_ref(), serving)
                    .await?
            } else {
                self.move_marker_past(&hash_alias, marker.as_ref()).await?
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
            if let Written::Conflict = self.put_if(&record_id, &body, Some(&record)).await? {
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

    /// The record of a rebuild the seed just finished, deleted on the version
    /// read; none is fine.
    pub(crate) async fn forget_finished_rebuild(&self, hash_alias: &str) -> Result<()> {
        let Kept { record, .. } = self.read_kept(hash_alias).await?;
        let Some((seq_no, primary_term)) = record.as_ref().and_then(Found::version) else {
            return Ok(());
        };
        let url = format!(
            "{}/{}/_doc/detach:{hash_alias}?if_seq_no={seq_no}&if_primary_term={primary_term}",
            self.base_url,
            self.meta_index()
        );
        let response = self
            .maybe_auth(self.client.delete(&url))
            .send()
            .await
            .map_err(|e| request_error("deleting the finished rebuild's record failed", e))?;
        let status = response.status();
        if status.is_success() || status == reqwest::StatusCode::NOT_FOUND {
            return Ok(());
        }
        let text = response.text().await.unwrap_or_default();
        Err(SinkError::Write(format!(
            "deleting the finished rebuild's record failed: HTTP {status}: {text}"
        )))
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
    async fn move_marker_past(&self, hash_alias: &str, marker: Option<&Found>) -> Result<Written> {
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
        self.put_if(hash_alias, &moved, marker).await
    }

    /// The marker back on `serving` as `detaching_from` says it was (R14).
    async fn restore_marker(
        &self,
        hash_alias: &str,
        marker: Option<&Found>,
        serving: Option<u64>,
    ) -> Result<Written> {
        let from = marker.and_then(|found| found.source.get("detaching_from"));
        let Some(generation) = serving else {
            return Ok(Written::Done);
        };
        let mut restored = json!({
            "active_generation": generation,
            "seeded": from.and_then(|from| from.get("seeded")).and_then(Value::as_bool).unwrap_or(true),
        });
        if let (Some(uuid), Some(fields)) = (
            from.and_then(|from| from.get("uuid"))
                .and_then(Value::as_str),
            restored.as_object_mut(),
        ) {
            fields.insert("uuid".to_owned(), Value::from(uuid));
        }
        self.put_if(hash_alias, &restored, marker).await
    }
}
