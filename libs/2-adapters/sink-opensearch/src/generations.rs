//! Alias-over-generations addressing: the convenience/hash aliases, the
//! per-index meta doc (active generation + seeded-state), the generation
//! discovery helpers, and the pure naming/alias-planning functions they rely on.
//!
//! The addressable name `{logical}_{hash}` is an **alias**; the data lives in a
//! concrete generation `{logical}_{hash}_{gen}` behind it. These methods read
//! and move those aliases and track which generation is live; the [`Sink`](crate)
//! impl drives them from `ensure_index`/`mark_seeded`/`reindex`.

use serde_json::{Value, json};
use sink::{Result, SinkError};
use tracing::debug;

use crate::OpensearchSink;
use crate::transport::request_error;

impl OpensearchSink {
    /// Point the convenience alias `alias` (the logical index name) at
    /// `target` (the current physical index), removing it from any stale
    /// physical indexes in the same atomic `_aliases` call. Best-effort: a
    /// failure is logged and swallowed, because nothing in flusso reads or
    /// writes through the alias (see the module docs).
    pub(crate) async fn ensure_alias(&self, alias: &str, target: &str) {
        if let Err(e) = self.try_ensure_alias(alias, target).await {
            tracing::warn!(
                alias,
                index = target,
                error = %e,
                "could not point the convenience alias at the index; writes are unaffected",
            );
        }
    }

    /// The fallible body of [`ensure_alias`](Self::ensure_alias).
    pub(crate) async fn try_ensure_alias(&self, alias: &str, target: &str) -> Result<()> {
        let holders = self.alias_holders(alias).await?;
        let Some(actions) = plan_alias_actions(alias, target, &holders) else {
            return Ok(());
        };

        let url = format!("{}/_aliases", self.base_url);
        let req = self
            .client
            .post(&url)
            .header("Content-Type", "application/json")
            .json(&actions);
        self.send_ok(req, "alias update failed").await?;
        debug!(alias, index = target, "pointed alias at the current index");
        Ok(())
    }

    /// The indexes currently holding `alias`. An alias that exists nowhere is
    /// an empty list (404 from the lookup), not an error.
    pub(crate) async fn alias_holders(&self, alias: &str) -> Result<Vec<String>> {
        let url = format!("{}/_alias/{alias}", self.base_url);
        let resp = self
            .maybe_auth(self.client.get(&url))
            .send()
            .await
            .map_err(|e| request_error("alias lookup failed", e))?;

        if resp.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(Vec::new());
        }
        if !resp.status().is_success() {
            return Err(Self::status_error(resp, "alias lookup failed").await);
        }

        let body: Value = resp
            .json()
            .await
            .map_err(|e| request_error("failed to parse alias response", e))?;
        Ok(body
            .as_object()
            .map(|indexes| indexes.keys().cloned().collect())
            .unwrap_or_default())
    }

    /// The holders of each of `aliases`, in one lookup. An alias held by no
    /// index maps to an empty list.
    pub(crate) async fn alias_map(&self, aliases: &[&str]) -> Result<Holders> {
        let url = format!("{}/_alias/{}", self.base_url, aliases.join(","));
        let resp = self
            .maybe_auth(self.client.get(&url))
            .send()
            .await
            .map_err(|e| request_error("alias lookup failed", e))?;
        // A 404 still lists the aliases that exist, beside an `error` naming
        // the missing ones.
        let status = resp.status();
        if !status.is_success() && status != reqwest::StatusCode::NOT_FOUND {
            return Err(Self::status_error(resp, "alias lookup failed").await);
        }
        let body: Value = resp
            .json()
            .await
            .map_err(|e| request_error("failed to parse alias response", e))?;
        Ok(holders_of(aliases, &body))
    }

    /// Apply a seed's `_aliases` body, as [`update_aliases`](Self::update_aliases).
    /// A refusal because one of `old` (the generations the seed retires) is
    /// blocked is the typed [`SinkError::RetireBlocked`] naming it and the
    /// block, whatever status carried it (`403` for `read_only`, `429` for
    /// `read_only_allow_delete`): never "busy", never retried.
    pub(crate) async fn update_seed_aliases(
        &self,
        actions: &Value,
        new: &str,
        old: &[&String],
    ) -> Result<()> {
        let url = format!("{}/_aliases", self.base_url);
        let context = "alias update failed";
        let response = self
            .maybe_auth(
                self.client
                    .post(&url)
                    .header("Content-Type", "application/json")
                    .json(actions),
            )
            .send()
            .await
            .map_err(|e| request_error(context, e))?;
        let status = response.status();
        if status.is_success() {
            return Ok(());
        }
        let text = response.text().await.unwrap_or_default();
        Err(retire_blocked(&text, new, old)
            .unwrap_or_else(|| SinkError::Write(format!("{context}: HTTP {status}: {text}"))))
    }

    /// Apply an `_aliases` body: every action in one cluster-state update.
    pub(crate) async fn update_aliases(&self, actions: &Value) -> Result<()> {
        let url = format!("{}/_aliases", self.base_url);
        let req = self
            .client
            .post(&url)
            .header("Content-Type", "application/json")
            .json(actions);
        self.send_ok(req, "alias update failed").await?;
        Ok(())
    }

    /// Write `doc` whole as `id`, stamped `stamp` (`null` for a caller that
    /// keeps no records): the one unconditional whole write of a marker.
    async fn put_meta(&self, id: &str, mut doc: Value, stamp: Option<&sink::Stamp>) -> Result<()> {
        if let Some(fields) = doc.as_object_mut() {
            fields.insert("stamp".to_owned(), crate::detachment::stamp_value(stamp));
        }
        let url = format!("{}/{}/_doc/{id}", self.base_url, self.meta_index());
        let req = self
            .client
            .put(&url)
            .header("Content-Type", "application/json")
            .json(&doc);

        self.send_ok(req, "meta put failed").await?;
        Ok(())
    }

    /// Fetch a document from the meta index by id. Returns `None` on 404.
    async fn get_meta(&self, id: &str) -> Result<Option<Value>> {
        let url = format!("{}/{}/_doc/{id}", self.base_url, self.meta_index());
        let resp = self
            .maybe_auth(self.client.get(&url))
            .send()
            .await
            .map_err(|e| request_error("meta get failed", e))?;

        if resp.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }

        if resp.status().is_success() {
            let body: Value = resp
                .json()
                .await
                .map_err(|e| request_error("failed to parse meta response", e))?;
            Ok(Some(body))
        } else {
            Err(Self::status_error(resp, "meta get failed").await)
        }
    }

    /// Typed read of an index's meta doc (keyed by its hash alias): the active
    /// generation number and whether it has been seeded. `None` if absent (or a
    /// legacy doc with no generation).
    pub(crate) async fn read_index_meta(&self, hash_alias: &str) -> Result<Option<(u64, bool)>> {
        Ok(self
            .read_meta(hash_alias)
            .await?
            .map(|meta| (meta.generation, meta.seeded)))
    }

    /// The whole meta doc of an index, `walked` included. `None` if absent (or
    /// a legacy doc with no generation).
    pub(crate) async fn read_meta(&self, hash_alias: &str) -> Result<Option<Meta>> {
        Ok(self
            .get_meta(hash_alias)
            .await?
            .and_then(|doc| Meta::read(doc.get("_source")?)))
    }

    /// Store how far the active generation's backfill got: a partial update
    /// that touches nothing else, and creates no document (a missing one is an
    /// error the caller logs).
    pub(crate) async fn update_walked(&self, hash_alias: &str, walked: Value) -> Result<()> {
        let url = format!(
            "{}/{}/_update/{hash_alias}",
            self.base_url,
            self.meta_index()
        );
        let req = self
            .client
            .post(&url)
            .header("Content-Type", "application/json")
            .json(&json!({ "doc": { "walked": walked } }));
        self.send_ok(req, "meta update failed").await?;
        Ok(())
    }

    /// Write an index's meta doc whole: its active generation, seeded-state,
    /// the `index.uuid` the generation was created with, once it exists, and
    /// who wrote it (`stamp`, `null` for a caller that keeps no records). No
    /// gap and no `detaching_from`: the seed's write, the one write that may
    /// drop them (every other goes through
    /// [`rewrite_marker`](Self::rewrite_marker)).
    pub(crate) async fn write_index_meta(
        &self,
        hash_alias: &str,
        generation: u64,
        seeded: bool,
        uuid: Option<&str>,
        stamp: Option<&sink::Stamp>,
    ) -> Result<()> {
        let doc = crate::meta::whole_marker(generation, seeded, uuid);
        self.put_meta(hash_alias, doc, stamp).await
    }

    /// Record the `index.uuid` of the active generation in its meta doc,
    /// touching nothing else: for a doc an older flusso wrote without one.
    pub(crate) async fn record_uuid(&self, hash_alias: &str, uuid: &str) -> Result<()> {
        let url = format!(
            "{}/{}/_update/{hash_alias}",
            self.base_url,
            self.meta_index()
        );
        let req = self
            .client
            .post(&url)
            .header("Content-Type", "application/json")
            .json(&json!({ "doc": { "uuid": uuid } }));
        self.send_ok(req, "meta update failed").await?;
        Ok(())
    }

    /// The existing generation indexes of a hash alias (`{hash_alias}_{n}`).
    pub(crate) async fn list_generations(&self, hash_alias: &str) -> Result<Vec<String>> {
        self.list_indices(&format!("{hash_alias}_*")).await
    }

    /// Whether a *concrete index* named exactly `name` exists (as opposed to an
    /// alias of that name) — detects a legacy `{logical}_{hash}` index that must
    /// be migrated before the name can become an alias.
    pub(crate) async fn concrete_index_exists(&self, name: &str) -> Result<bool> {
        Ok(self
            .list_indices(name)
            .await?
            .iter()
            .any(|found| found == name))
    }
}

/// An index's meta doc as read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Meta {
    pub(crate) generation: u64,
    pub(crate) seeded: bool,
    /// Where the active generation's backfill durably got: only from an
    /// unseeded doc whose `walked` names the active generation, so a stray
    /// record (on a seeded doc, or of a generation since replaced) reads as
    /// none.
    pub(crate) walked: Option<String>,
    /// The `index.uuid` the active generation was created with: a live index
    /// under its name with another uuid is not the one flusso made. `None`
    /// before the generation exists, or in a doc an older flusso wrote.
    pub(crate) uuid: Option<String>,
}

impl Meta {
    pub(crate) fn read(source: &Value) -> Option<Self> {
        let generation = source.get("active_generation").and_then(Value::as_u64)?;
        let seeded = source
            .get("seeded")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let walked = source.get("walked").filter(|_| !seeded).and_then(|walked| {
            (walked.get("generation").and_then(Value::as_u64) == Some(generation))
                .then(|| {
                    walked
                        .get("mark")
                        .and_then(Value::as_str)
                        .map(str::to_owned)
                })
                .flatten()
        });
        let uuid = source
            .get("uuid")
            .and_then(Value::as_str)
            .map(str::to_owned);
        Some(Self {
            generation,
            seeded,
            walked,
            uuid,
        })
    }
}

/// Build the `POST /_aliases` body that moves `alias` to point at exactly
/// `target`: one `remove` per stale holder plus an `add` for the target, all
/// in a single atomic call (no window where the alias dangles). Returns `None`
/// when the alias already points at exactly the target, so the caller can skip
/// the request entirely.
fn plan_alias_actions(alias: &str, target: &str, holders: &[String]) -> Option<Value> {
    if holders.len() == 1 && holders.iter().all(|h| h == target) {
        return None;
    }

    let mut actions: Vec<Value> = holders
        .iter()
        .filter(|holder| holder.as_str() != target)
        .map(|holder| json!({ "remove": { "index": holder, "alias": alias } }))
        .collect();
    actions.push(json!({ "add": { "index": target, "alias": alias } }));
    Some(json!({ "actions": actions }))
}

/// Which indexes hold each alias: alias name → its holders, sorted.
pub(crate) type Holders = std::collections::BTreeMap<String, Vec<String>>;

/// Read an `_alias/{a},{b}` answer into [`Holders`], every asked alias
/// present, an absent one with no holders. The answer is keyed by index;
/// `error` and `status` sit beside them when an alias is missing.
fn holders_of(aliases: &[&str], body: &Value) -> Holders {
    let mut holders: Holders = aliases
        .iter()
        .map(|alias| ((*alias).to_owned(), Vec::new()))
        .collect();
    if let Some(indexes) = body.as_object() {
        for (index, entry) in indexes {
            let Some(names) = entry.get("aliases").and_then(Value::as_object) else {
                continue;
            };
            for alias in names.keys() {
                if let Some(list) = holders.get_mut(alias) {
                    list.push(index.clone());
                }
            }
        }
    }
    for list in holders.values_mut() {
        list.sort();
    }
    holders
}

/// The one `_aliases` body that finishes a seed: `aliases` all pointing at
/// exactly `target`, and every index in `stale` removed, in a single atomic
/// call. Readers see the old generation or the new one, never neither and
/// never both. `None` when there is nothing to do.
///
/// An alias held by an index the call removes needs no `remove` of its own:
/// removing an index removes its aliases.
pub(crate) fn plan_seed_actions(
    target: &str,
    aliases: &[(&str, &[String])],
    stale: &std::collections::BTreeSet<String>,
) -> Option<Value> {
    let mut actions: Vec<Value> = Vec::new();
    for (alias, holders) in aliases {
        if holders.len() == 1 && holders.iter().all(|holder| holder == target) {
            continue;
        }
        actions.extend(
            holders
                .iter()
                .filter(|holder| holder.as_str() != target && !stale.contains(*holder))
                .map(|holder| json!({ "remove": { "index": holder, "alias": alias } })),
        );
        if !holders.iter().any(|holder| holder == target) {
            actions.push(json!({ "add": { "index": target, "alias": alias } }));
        }
    }
    actions.extend(
        stale
            .iter()
            .map(|index| json!({ "remove_index": { "index": index } })),
    );
    (!actions.is_empty()).then(|| json!({ "actions": actions }))
}

/// `text`, an `_aliases` refusal, as a seed that couldn't retire `old`: a
/// `cluster_block_exception` whose reason names, one `index [x] blocked by:
/// [...];` segment each, only generations the seed retires (never `new`).
/// The old generations named are listed; a delete is offered only when every
/// named block allows it.
pub(crate) fn retire_blocked(text: &str, new: &str, old: &[&String]) -> Option<SinkError> {
    let body: Value = serde_json::from_str(text).ok()?;
    let error = body.get("error")?;
    if error.get("type").and_then(Value::as_str) != Some("cluster_block_exception") {
        return None;
    }
    let reason = error.get("reason").and_then(Value::as_str)?;
    let segments = blocked_indexes(reason);
    let retired = |name: &str| name != new && old.iter().any(|old| old.as_str() == name);
    if segments.is_empty() || !segments.iter().all(|(name, _)| retired(name)) {
        return None;
    }
    let deletable = segments.iter().all(|(_, blocks)| {
        let blocks = blocks.to_lowercase();
        blocks.contains("allow delete") || blocks.contains("allow-delete")
    });
    let names: Vec<&str> = segments.iter().map(|(name, _)| *name).collect();
    Some(SinkError::RetireBlocked {
        new: new.to_owned(),
        old: names.join(", "),
        block: reason.to_owned(),
        deletable,
    })
}

/// Each `index [x] blocked by: [...]` segment of a block refusal's reason:
/// the index and what blocks it.
fn blocked_indexes(reason: &str) -> Vec<(&str, &str)> {
    reason
        .split("index [")
        .skip(1)
        .filter_map(|segment| {
            let (name, rest) = segment.split_once(']')?;
            let blocks = rest.trim_start().strip_prefix("blocked by:")?;
            Some((name, blocks.split(';').next().unwrap_or(blocks)))
        })
        .collect()
}

/// The concrete index name for generation `gen` behind a hash alias — what the
/// alias `{hash_alias}` points at. A reindex builds the *next* generation
/// alongside the current one, then atomically repoints the alias.
pub(crate) fn generation_name(hash_alias: &str, generation: u64) -> String {
    format!("{hash_alias}_{generation}")
}

/// Parse the generation number out of a concrete index name, given its hash
/// alias — the inverse of [`generation_name`]. `None` for anything that isn't
/// `{hash_alias}_{n}` with a numeric suffix, so a legacy concrete index named
/// exactly `{hash_alias}`, an unrelated index, or a prefix-collision is ignored.
pub(crate) fn parse_generation(hash_alias: &str, index: &str) -> Option<u64> {
    index
        .strip_prefix(hash_alias)?
        .strip_prefix('_')?
        .parse::<u64>()
        .ok()
}

/// The generation to build next, given the existing generation indexes of a hash
/// alias: one past the highest existing generation, or `1` when none exist — so a
/// generation number is never reused, even if a crashed reindex left an orphan.
pub(crate) fn next_generation(existing: &[String], hash_alias: &str) -> u64 {
    existing
        .iter()
        .filter_map(|name| parse_generation(hash_alias, name))
        .max()
        .map_or(1, |highest| highest + 1)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests;
