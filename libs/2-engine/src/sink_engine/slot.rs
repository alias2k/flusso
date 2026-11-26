//! The one way the sink engine makes a request that may erase, or loosen, a
//! record another process may rely on (ADR 0012 stage 10, O29 as the owner
//! narrowed it on 2026-11-26): this process destroys or rewrites no such
//! record unless it holds the slot.
//!
//! Each gated method checks [`Rereads::may_erase_records`] immediately before
//! the request is sent, after every await that precedes it in its caller, so
//! a stream that ended while the caller read, ensured or waited is seen
//! (the sixth re-check, B-1). What stays open is a request already sent when
//! the stream ends: an owner lease closes that (#490).
//!
//! The sink's erasing methods are called nowhere else in the engine: a test
//! here fails if one is (`erasing_requests_go_only_through_the_slot`).
//!
//! | Request | Here |
//! | --- | --- |
//! | `mark_seeded` | gated: its marker is written whole, erasing any record |
//! | `forget_detachment` | gated (the failed-detach clear, a reattach); ungated only for a finished rebuild's record (`forget_finished`) |
//! | `reindex` | gated; ungated only for a `Fresh` source's staging (`reindex_at_a_fresh_start`) |
//! | `begin_rebuild` | gated |
//! | `demote`, `detach` | gated: both delete every generation but the serving one, which may be another process's rebuild |
//!
//! ```text
//! caller: read records, ensure, write a gap …   (awaits; the stream may end)
//! Erasing::forget_detachment(..)                 may_erase_records()? then send
//!   held     -> the request is sent
//!   not held -> NotHeld: the caller leaves the record and retries later
//! ```

use kernel::{IndexMapping, IndexName};
use sink::{Detach, RecordVersion, Recorded, Sink};

use crate::Rereads;

/// This process may not erase or loosen a record now: it holds no slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct NotHeld;

impl std::fmt::Display for NotHeld {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(
            "this process doesn't hold the slot (its stream ended or its opening failed), so the \
             record may be another process's",
        )
    }
}

/// The sink's requests that may erase or loosen a record, each checked
/// against the slot immediately before it is sent.
#[derive(Clone, Copy)]
pub(super) struct Erasing<'a> {
    sink: &'a dyn Sink,
    rereads: &'a Rereads,
}

impl std::fmt::Debug for Erasing<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Erasing").finish_non_exhaustive()
    }
}

impl<'a> Erasing<'a> {
    pub(super) fn new(sink: &'a dyn Sink, rereads: &'a Rereads) -> Self {
        Self { sink, rereads }
    }

    fn held(&self) -> Result<(), NotHeld> {
        if self.rereads.may_erase_records() {
            Ok(())
        } else {
            Err(NotHeld)
        }
    }

    /// Seed `index`: its marker is written whole, erasing any record.
    pub(super) async fn mark_seeded(&self, index: &IndexName) -> Result<sink::Result<()>, NotHeld> {
        self.held()?;
        Ok(self.sink.mark_seeded(index).await)
    }

    /// Delete the detach record read at `version`.
    pub(super) async fn forget_detachment(
        &self,
        mapping: &IndexMapping,
        version: &RecordVersion,
    ) -> Result<sink::Result<()>, NotHeld> {
        self.held()?;
        Ok(self.sink.forget_detachment(mapping, version).await)
    }

    /// Delete the record of a rebuild a newer seeded generation finished.
    /// Ungated: the record is obsolete, and the delete is conditional on the
    /// version read, so any write since fails it.
    pub(super) async fn forget_finished(
        &self,
        mapping: &IndexMapping,
        version: &RecordVersion,
    ) -> sink::Result<()> {
        self.sink.forget_detachment(mapping, version).await
    }

    /// Move `mapping`'s index to a new generation; its marker is written
    /// whole.
    pub(super) async fn reindex(
        &self,
        mapping: &IndexMapping,
    ) -> Result<sink::Result<()>, NotHeld> {
        self.held()?;
        Ok(self.sink.reindex(mapping).await)
    }

    /// [`reindex`](Self::reindex) at a `Fresh` source's staging, ungated by
    /// necessity: it runs before `prepare` creates the slot (#120), so no
    /// slot exists for anyone to hold.
    pub(super) async fn reindex_at_a_fresh_start(
        &self,
        mapping: &IndexMapping,
    ) -> sink::Result<()> {
        self.sink.reindex(mapping).await
    }

    /// Begin a rebuild: other generations deleted, the marker moved, the
    /// record rewritten into `rebuilding`.
    pub(super) async fn begin_rebuild(
        &self,
        mapping: &IndexMapping,
    ) -> Result<sink::Result<u64>, NotHeld> {
        self.held()?;
        Ok(self.sink.begin_rebuild(mapping).await)
    }

    /// Put a rebuild back to detached: every generation but the serving one
    /// deleted, the walk checkpoint with it, the marker and the record
    /// rewritten. Gated: what it deletes may be the rebuild another process
    /// holding the slot is walking (the sixth re-check, S-2).
    pub(super) async fn demote(
        &self,
        mapping: &IndexMapping,
        live_only: bool,
    ) -> Result<sink::Result<()>, NotHeld> {
        self.held()?;
        Ok(self.sink.demote(mapping, live_only).await)
    }

    /// Detach `mapping`'s index: the marker moved, the record written, every
    /// generation but the serving one deleted. Gated as
    /// [`demote`](Self::demote) is.
    pub(super) async fn detach(
        &self,
        mapping: &IndexMapping,
        detach: &Detach,
    ) -> Result<sink::Result<Recorded>, NotHeld> {
        self.held()?;
        Ok(self.sink.detach(mapping, detach).await)
    }
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    fn sources(dir: &Path, found: &mut Vec<PathBuf>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if path.is_dir() {
                if name != "tests" && name != "benches" {
                    sources(&path, found);
                }
            } else if name.ends_with(".rs") && name != "tests.rs" {
                found.push(path);
            }
        }
    }

    /// The sixth re-check, B-1: every request that may erase or loosen a
    /// record goes through `Erasing`, which checks the slot immediately
    /// before it; a call of one of the sink's erasing methods anywhere else
    /// in the engine fails here. Whitespace is dropped before the search, so
    /// a call split over lines is found too.
    #[test]
    fn erasing_requests_go_only_through_the_slot() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut files = Vec::new();
        sources(&root, &mut files);
        // Spelled in parts so this test doesn't find itself.
        let calls: Vec<String> = [
            "mark_seeded",
            "forget_detachment",
            "reindex",
            "begin_rebuild",
            "demote",
            "detach",
        ]
        .iter()
        .map(|method| ["sink.", method, "("].concat())
        .collect();
        let mut callers = Vec::new();
        for file in &files {
            let text: String = std::fs::read_to_string(file)
                .unwrap()
                .lines()
                .filter(|line| !line.trim_start().starts_with("//"))
                .collect::<String>()
                .chars()
                .filter(|c| !c.is_whitespace())
                .collect();
            for call in &calls {
                if text.contains(call.as_str()) {
                    callers.push((file.clone(), call.clone()));
                }
            }
        }
        let outside: Vec<_> = callers
            .iter()
            .filter(|(file, _)| !file.ends_with("sink_engine/slot.rs"))
            .collect();
        assert!(
            callers
                .iter()
                .any(|(file, _)| file.ends_with("sink_engine/slot.rs")),
            "the slot makes the requests"
        );
        assert!(
            outside.is_empty(),
            "erasing requests outside the slot: {outside:?}"
        );
    }
}
