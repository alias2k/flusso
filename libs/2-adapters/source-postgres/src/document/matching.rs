//! Pairing the rows a batched document query returned with the ids it was
//! asked for.
//!
//! The query reports, beside each row, the position of the requested key it
//! matched (`query::documents_query`): Postgres compares each key with the
//! root key exactly as the single-document query does, so the batched build
//! serves exactly the ids a single build serves, whatever the key's kind or
//! spelling. Pairing is by that position alone. A position no row answers is
//! a tombstone; a row is built under the id its position carried.
//!
//! Where the key's `=` is identity and every requested key is of the kind the
//! column reads back as ([`pairs_by_key`]), the query selects each row's key
//! instead and the rows pair by that value ([`match_by_key`]): both sides go
//! through the one key parser, and equal values are the same key.

use std::collections::HashMap;

use kernel::GenericValue;
use source::document::{BuildFailure, Document, DocumentId};

/// A row's assembled body, or why it could not be finished.
pub(super) type Outcome = Result<GenericValue, BuildFailure>;

/// Push one document per requested id onto `out`, in `requested`'s order:
/// the row reported at its 1-based position, or a tombstone when none was.
pub(super) fn match_rows(
    rows: Vec<(i64, Outcome)>,
    requested: &[(&GenericValue, &DocumentId)],
    out: &mut Vec<Document>,
) {
    let mut answered: Vec<Option<Outcome>> = std::iter::repeat_with(|| None)
        .take(requested.len())
        .collect();
    for (position, outcome) in rows {
        let slot = usize::try_from(position)
            .ok()
            .and_then(|position| position.checked_sub(1))
            .and_then(|at| answered.get_mut(at));
        if let Some(slot @ None) = slot {
            *slot = Some(outcome);
        }
    }
    for ((_, id), outcome) in requested.iter().zip(answered) {
        let id = (*id).clone();
        out.push(match outcome {
            Some(Ok(body)) => Document::Upsert { id, body },
            Some(Err(failure)) => Document::Failed { id, failure },
            None => Document::Delete { id },
        });
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests;

/// Whether the rows of a batch over a key column of base type `base_name`
/// pair with `keys` by value. True only where Postgres's `=` on the column is
/// identity on the values flusso reads back (`deterministic` collation; no
/// `numeric`, float, `citext` or `char(n)`, whose equal values can differ)
/// and every key is already of the kind the column reads back as: a key read
/// before a type change, or under another kind, pairs by position, where
/// Postgres compares it.
pub(super) fn pairs_by_key(base_name: &str, deterministic: bool, keys: &[GenericValue]) -> bool {
    let reads_back_as: fn(&GenericValue) -> bool = match base_name {
        "bool" => |key| matches!(key, GenericValue::Bool(_)),
        "int2" => |key| matches!(key, GenericValue::SmallInt(_)),
        "int4" => |key| matches!(key, GenericValue::Int(_)),
        "int8" => |key| matches!(key, GenericValue::BigInt(_)),
        "uuid" => |key| matches!(key, GenericValue::Uuid(_)),
        "bytea" => |key| matches!(key, GenericValue::Bytes(_)),
        "date" => |key| matches!(key, GenericValue::Date(_)),
        "time" => |key| matches!(key, GenericValue::Time(_)),
        "timestamp" => |key| matches!(key, GenericValue::Timestamp(_)),
        "timestamptz" => |key| matches!(key, GenericValue::TimestampTz(_)),
        "text" | "varchar" | "name" => |key| matches!(key, GenericValue::String(_)),
        _ => return false,
    };
    deterministic && keys.iter().all(reads_back_as)
}

/// A batch's rows by key, as [`match_by_key`] takes them: each row's outcome
/// until an id takes it, then where in `out` its document went.
pub(super) type ByKey = HashMap<GenericValue, std::result::Result<Outcome, usize>>;

/// Push one document per requested id onto `out`, in `requested`'s order:
/// the row whose key equals the id's key, or a tombstone when none does. A
/// key requested twice gets the same document twice.
pub(super) fn match_by_key(
    mut rows: ByKey,
    requested: &[(&GenericValue, &DocumentId)],
    out: &mut Vec<Document>,
) {
    for (key, id) in requested {
        let id = (*id).clone();
        let document = match rows.get_mut(*key) {
            None => Document::Delete { id },
            Some(answer) => match std::mem::replace(answer, Err(out.len())) {
                Ok(Ok(body)) => Document::Upsert { id, body },
                Ok(Err(failure)) => Document::Failed { id, failure },
                Err(earlier) => {
                    *answer = Err(earlier);
                    match out.get(earlier) {
                        Some(Document::Upsert { body, .. }) => Document::Upsert {
                            id,
                            body: body.clone(),
                        },
                        Some(Document::Failed { failure, .. }) => Document::Failed {
                            id,
                            failure: failure.clone(),
                        },
                        Some(Document::Delete { .. }) | None => Document::Delete { id },
                    }
                }
            },
        };
        out.push(document);
    }
}
