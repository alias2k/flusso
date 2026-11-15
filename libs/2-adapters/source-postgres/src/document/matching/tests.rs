use kernel::{ColumnName, IndexName};
use source::RowKey;

use super::*;

fn id(value: GenericValue) -> DocumentId {
    DocumentId {
        index: IndexName::try_new("users").unwrap(),
        key: RowKey(vec![(ColumnName::try_new("id").unwrap(), value)]),
    }
}

fn body(name: &str) -> Outcome {
    Ok(GenericValue::String(name.to_owned()))
}

/// Pair `rows` (position, body) with `n` requested ids and say, per id, what
/// it got, checking each document carries its own requested id.
fn outcomes(rows: Vec<(i32, Outcome)>, n: i32) -> Vec<String> {
    let values: Vec<GenericValue> = (0..n).map(GenericValue::Int).collect();
    let ids: Vec<DocumentId> = values.iter().cloned().map(id).collect();
    let keyed: Vec<(&GenericValue, &DocumentId)> = values.iter().zip(&ids).collect();
    let mut out = Vec::new();
    match_rows(rows, &keyed, &mut out);
    assert_eq!(out.len(), ids.len());
    out.iter()
        .zip(&ids)
        .map(|(document, id)| match document {
            Document::Upsert { id: built, body } => {
                assert_eq!(built, id);
                format!("upsert {body:?}")
            }
            Document::Failed { id: built, .. } => {
                assert_eq!(built, id);
                "failed".to_owned()
            }
            Document::Delete { id: built } => {
                assert_eq!(built, id);
                "delete".to_owned()
            }
        })
        .collect()
}

/// Each row answers the id at its reported position, in any order; an id no
/// row answers is a tombstone.
#[test]
fn a_row_answers_the_id_at_its_position_and_the_rest_are_tombstones() {
    assert_eq!(
        outcomes(vec![(3, body("c")), (1, body("a"))], 4),
        [
            r#"upsert String("a")"#,
            "delete",
            r#"upsert String("c")"#,
            "delete"
        ]
    );
}

/// One row matched by two positions (the same key read as `int4` and as
/// `int8`) comes back once per position, and each id is built.
#[test]
fn a_row_matched_by_several_positions_answers_each() {
    assert_eq!(
        outcomes(vec![(1, body("a")), (2, body("a"))], 2),
        [r#"upsert String("a")"#, r#"upsert String("a")"#]
    );
}

#[test]
fn a_failed_row_fails_the_id_at_its_position() {
    let failure = BuildFailure {
        field: "tags".to_owned(),
        message: "an object in an array column".to_owned(),
        position: None,
    };
    assert_eq!(
        outcomes(vec![(2, Err(failure)), (1, body("a"))], 2),
        [r#"upsert String("a")"#, "failed"]
    );
}

/// A position outside the request, or a second row at one position, is
/// ignored: the first row reported for a position answers it.
#[test]
fn a_position_outside_the_request_or_answered_twice_changes_nothing() {
    assert_eq!(
        outcomes(
            vec![
                (0, body("x")),
                (3, body("y")),
                (1, body("a")),
                (1, body("b"))
            ],
            2
        ),
        [r#"upsert String("a")"#, "delete"]
    );
}
