//! Matching the rows a batched document query returned to the ids it was
//! asked for.
//!
//! A row comes back keyed by the root key as the database reads it now; an
//! id carries the key as its change was decoded, perhaps before the column
//! changed type. A row serves every requested id that names the same row:
//!
//! - first one whose key renders the same `_id` (an `int4` and an `int8`
//!   key of one row, `Int(1)` and `BigInt(1)`);
//! - else one whose key equals it as a value (a `numeric` key read as `1.5`
//!   before the column took a scale, where the row now reads `1.50`).
//!
//! The document is built under the requested id, as a single-document
//! `build` of that id is, since the database finds the row by `=` in both.
//! Only an id no row serves is a tombstone.

use std::collections::HashMap;

use kernel::GenericValue;
use source::document::{BuildFailure, Document, DocumentId, key_part};

/// A row's assembled body, or why it could not be finished.
pub(super) type Outcome = Result<GenericValue, BuildFailure>;

/// Push one document per requested id onto `out`, in `requested`'s order,
/// each under its requested id. The first id a row serves takes its body; a
/// later one gets a copy.
pub(super) fn match_rows(
    rows: Vec<(GenericValue, Outcome)>,
    requested: &[(&GenericValue, &DocumentId)],
    out: &mut Vec<Document>,
) {
    let (keys, outcomes): (Vec<GenericValue>, Vec<Outcome>) = rows.into_iter().unzip();
    let mut outcomes: Vec<Option<Outcome>> = outcomes.into_iter().map(Some).collect();
    let mut served_at: Vec<Option<usize>> = vec![None; keys.len()];
    let by_text: HashMap<String, usize> = keys
        .iter()
        .enumerate()
        .map(|(row, key)| (key_part(key), row))
        .collect();
    let mut by_value: Option<HashMap<&GenericValue, usize>> = None;
    for (value, id) in requested {
        let id = (*id).clone();
        let row = by_text.get(&key_part(value)).copied().or_else(|| {
            by_value
                .get_or_insert_with(|| {
                    keys.iter()
                        .enumerate()
                        .map(|(row, key)| (key, row))
                        .collect()
                })
                .get(value)
                .copied()
        });
        let document = match row {
            None => Document::Delete { id },
            Some(row) => match outcomes.get_mut(row).and_then(Option::take) {
                Some(outcome) => {
                    if let Some(at) = served_at.get_mut(row) {
                        *at = Some(out.len());
                    }
                    match outcome {
                        Ok(body) => Document::Upsert { id, body },
                        Err(failure) => Document::Failed { id, failure },
                    }
                }
                None => match served_at
                    .get(row)
                    .copied()
                    .flatten()
                    .and_then(|at| out.get(at))
                {
                    Some(Document::Upsert { body, .. }) => Document::Upsert {
                        id,
                        body: body.clone(),
                    },
                    Some(Document::Failed { failure, .. }) => Document::Failed {
                        id,
                        failure: failure.clone(),
                    },
                    Some(Document::Delete { .. }) | None => Document::Delete { id },
                },
            },
        };
        out.push(document);
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests;
