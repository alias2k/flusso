//! Matching the rows a batched document query returned to the ids it was
//! asked for.
//!
//! A row comes back keyed by the root key as the database reads it now; an
//! id carries the key as its change was decoded, perhaps before the column
//! changed type. A row serves every requested id that names the same
//! document; an id no row serves is a tombstone.

use std::collections::HashMap;

use kernel::GenericValue;
use source::document::{BuildFailure, Document, DocumentId, key_part};

/// A row's assembled body, or why it could not be finished.
pub(super) type Outcome = Result<GenericValue, BuildFailure>;

/// Push one document per requested id onto `out`, in `requested`'s order.
///
/// A row matches an id whose key renders the same `_id`, so an `int4` and
/// an `int8` key of one row are one document. The first id a row serves
/// takes its body; a later one gets a copy.
pub(super) fn match_rows(
    rows: Vec<(GenericValue, Outcome)>,
    requested: &[(&GenericValue, &DocumentId)],
    out: &mut Vec<Document>,
) {
    let mut bodies: HashMap<String, Outcome> = rows
        .into_iter()
        .map(|(key, outcome)| (key_part(&key), outcome))
        .collect();
    let mut built: HashMap<String, usize> = HashMap::with_capacity(requested.len());
    for (value, id) in requested {
        let id = (*id).clone();
        let key = key_part(value);
        let document = match bodies.remove_entry(&key) {
            Some((key, outcome)) => {
                built.insert(key, out.len());
                match outcome {
                    Ok(body) => Document::Upsert { id, body },
                    Err(failure) => Document::Failed { id, failure },
                }
            }
            None => match built.get(&key).and_then(|&at| out.get(at)) {
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
        };
        out.push(document);
    }
}
