//! Alias-over-generations addressing: the convenience/hash aliases, the
//! per-index meta doc (active generation + seeded-state), the generation
//! discovery helpers, and the pure naming/alias-planning functions they rely on.
//!
//! The addressable name `{logical}_{hash}` is an **alias**; the data lives in a
//! concrete generation `{logical}_{hash}_{gen}` behind it. These methods read
//! and move those aliases and track which generation is live; the [`Sink`](crate)
//! impl drives them from `ensure_index`/`mark_seeded`/`reindex`.

use serde_json::{Value, json};
use sinks_core::{Result, SinkError};
use tracing::debug;

use crate::OpensearchSink;

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
            .map_err(|e| SinkError::Write(format!("alias lookup failed: {e}")))?;

        if resp.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(Vec::new());
        }
        if !resp.status().is_success() {
            return Err(Self::status_error(resp, "alias lookup failed").await);
        }

        let body: Value = resp
            .json()
            .await
            .map_err(|e| SinkError::Write(format!("failed to parse alias response: {e}")))?;
        Ok(body
            .as_object()
            .map(|indexes| indexes.keys().cloned().collect())
            .unwrap_or_default())
    }

    async fn put_meta(&self, id: &str, doc: Value) -> Result<()> {
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
            .map_err(|e| SinkError::Write(format!("meta get failed: {e}")))?;

        if resp.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }

        if resp.status().is_success() {
            let body: Value = resp
                .json()
                .await
                .map_err(|e| SinkError::Write(format!("failed to parse meta response: {e}")))?;
            Ok(Some(body))
        } else {
            Err(Self::status_error(resp, "meta get failed").await)
        }
    }

    /// Typed read of an index's meta doc (keyed by its hash alias): the active
    /// generation number and whether it has been seeded. `None` if absent (or a
    /// legacy doc with no generation).
    pub(crate) async fn read_index_meta(&self, hash_alias: &str) -> Result<Option<(u64, bool)>> {
        let Some(doc) = self.get_meta(hash_alias).await? else {
            return Ok(None);
        };
        let source = doc.get("_source");
        let generation = source
            .and_then(|s| s.get("active_generation"))
            .and_then(Value::as_u64);
        let seeded = source
            .and_then(|s| s.get("seeded"))
            .and_then(Value::as_bool)
            .unwrap_or(false);
        Ok(generation.map(|g| (g, seeded)))
    }

    /// Write an index's meta doc: its active generation and seeded-state.
    pub(crate) async fn write_index_meta(
        &self,
        hash_alias: &str,
        generation: u64,
        seeded: bool,
    ) -> Result<()> {
        self.put_meta(
            hash_alias,
            json!({ "active_generation": generation, "seeded": seeded }),
        )
        .await
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

/// The hash alias an active generation belongs to — the name minus its
/// `_{n}` suffix (the inverse of [`generation_name`]). `None` if the name has no
/// numeric generation suffix.
pub(crate) fn hash_alias_of(generation: &str) -> Option<String> {
    let (prefix, suffix) = generation.rsplit_once('_')?;
    suffix.parse::<u64>().ok()?;
    Some(prefix.to_owned())
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
