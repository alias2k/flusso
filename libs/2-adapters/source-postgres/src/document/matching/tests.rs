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
fn outcomes(rows: Vec<(i64, Outcome)>, n: i32) -> Vec<String> {
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

/// Pair `rows` (key, body) with requested `keys` by value and say, per id,
/// what it got.
fn by_key(rows: Vec<(GenericValue, Outcome)>, keys: &[i32]) -> Vec<String> {
    let values: Vec<GenericValue> = keys.iter().copied().map(GenericValue::Int).collect();
    let ids: Vec<DocumentId> = values.iter().cloned().map(id).collect();
    let keyed: Vec<(&GenericValue, &DocumentId)> = values.iter().zip(&ids).collect();
    let mut out = Vec::new();
    let rows: ByKey = rows
        .into_iter()
        .map(|(key, outcome)| (key, Ok(outcome)))
        .collect();
    assert!(match_by_key(rows, &keyed, &mut out), "every row claimed");
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

/// A row answers every requested id with its key, a key requested twice
/// gets its document twice, and a key no row has is a tombstone.
#[test]
fn a_row_answers_each_id_its_key_names() {
    let failure = BuildFailure {
        field: "tags".to_owned(),
        message: "an object in an array column".to_owned(),
        position: None,
    };
    assert_eq!(
        by_key(
            vec![
                (GenericValue::Int(3), body("c")),
                (GenericValue::Int(1), Err(failure))
            ],
            &[1, 2, 3, 3, 1]
        ),
        [
            "failed",
            "delete",
            r#"upsert String("c")"#,
            r#"upsert String("c")"#,
            "failed"
        ]
    );
}

/// Only keys of the kind their column reads back as, whose cast keeps them
/// as they are, pair by value (the caller has checked the column's `=` is
/// identity).
#[test]
fn only_keys_of_the_columns_own_kind_kept_by_its_cast_pair_by_value() {
    let ints = [GenericValue::Int(1), GenericValue::Int(2)];
    assert!(pairs_by_key("int4", ints.iter()));
    assert!(
        !pairs_by_key("int8", ints.iter()),
        "read before a type change"
    );
    let mixed = [GenericValue::Int(1), GenericValue::BigInt(2)];
    assert!(!pairs_by_key("int4", mixed.iter()));
    let text = [GenericValue::String("ab".to_owned())];
    assert!(pairs_by_key("text", text.iter()));
    assert!(pairs_by_key("varchar", text.iter()));
    assert!(
        !pairs_by_key("bpchar", text.iter()),
        "a cast pads a requested text"
    );
    assert!(
        !pairs_by_key("name", text.iter()),
        "a cast cuts a requested text"
    );
    assert!(!pairs_by_key("numeric", text.iter()));
    assert!(!pairs_by_key("citext", text.iter()));
}

/// A row whose key equals no requested key is reported, so the caller can
/// rerun the chunk by position.
#[test]
fn a_row_no_requested_key_equals_is_reported() {
    let values = [GenericValue::Int(2)];
    let ids: Vec<DocumentId> = values.iter().cloned().map(id).collect();
    let keyed: Vec<(&GenericValue, &DocumentId)> = values.iter().zip(&ids).collect();
    let rows: ByKey = [(GenericValue::String("2.00".to_owned()), body("b"))]
        .into_iter()
        .map(|(key, outcome)| (key, Ok(outcome)))
        .collect();
    let mut out = Vec::new();
    assert!(!match_by_key(rows, &keyed, &mut out));
}
