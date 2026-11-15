//! Pairing the rows a batched document query returned with the ids it was
//! asked for.
//!
//! The query reports, beside each row, the position of the requested key it
//! matched (`query::documents_query`): Postgres compares each key with the
//! root key exactly as the single-document query does, so the batched build
//! serves exactly the ids a single build serves, whatever the key's kind or
//! spelling. Pairing is by that position alone. A position no row answers is
//! a tombstone; a row is built under the id its position carried.

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
