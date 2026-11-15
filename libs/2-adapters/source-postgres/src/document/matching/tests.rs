use kernel::{ColumnName, IndexName};
use rust_decimal::Decimal;
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

fn decimal(text: &str) -> GenericValue {
    GenericValue::Decimal(text.parse::<Decimal>().unwrap())
}

/// Match `requested` against `rows` and say, per id, whether it got a body.
fn outcomes(rows: Vec<(GenericValue, Outcome)>, requested: &[GenericValue]) -> Vec<&'static str> {
    let ids: Vec<DocumentId> = requested.iter().cloned().map(id).collect();
    let keyed: Vec<(&GenericValue, &DocumentId)> = requested.iter().zip(&ids).collect();
    let mut out = Vec::new();
    match_rows(rows, &keyed, &mut out);
    out.iter()
        .zip(&ids)
        .map(|(document, id)| match document {
            Document::Upsert { id: built, .. } => {
                assert_eq!(built, id);
                "upsert"
            }
            Document::Failed { .. } => "failed",
            Document::Delete { .. } => "delete",
        })
        .collect()
}

/// A `numeric` key equal to the row's but spelled with another scale is the
/// same row: a change read before `numeric` became `numeric(10,2)` carries
/// `1.5` where the row now reads `1.50`. It is built, never a tombstone.
#[test]
fn a_numeric_key_of_another_scale_is_the_row_it_equals() {
    let requested = [decimal("1.50"), decimal("1.5"), decimal("1.500")];
    assert_eq!(
        outcomes(vec![(decimal("1.5"), body("a"))], &requested),
        ["upsert", "upsert", "upsert"]
    );
    assert_eq!(
        outcomes(vec![(decimal("1.50"), body("a"))], &requested),
        ["upsert", "upsert", "upsert"]
    );
}

#[test]
fn integer_keys_of_every_width_are_one_row() {
    let requested = [
        GenericValue::Int(1),
        GenericValue::BigInt(1),
        GenericValue::SmallInt(1),
    ];
    assert_eq!(
        outcomes(vec![(GenericValue::BigInt(1), body("a"))], &requested),
        ["upsert", "upsert", "upsert"]
    );
}

#[test]
fn a_repeated_id_is_built_each_time() {
    let requested = [GenericValue::Int(1), GenericValue::Int(1)];
    assert_eq!(
        outcomes(vec![(GenericValue::Int(1), body("a"))], &requested),
        ["upsert", "upsert"]
    );
}

#[test]
fn an_id_no_row_equals_is_a_tombstone() {
    let requested = [GenericValue::Int(1), decimal("2.5"), decimal("1.5")];
    assert_eq!(
        outcomes(vec![(decimal("1.50"), body("a"))], &requested),
        ["delete", "delete", "upsert"]
    );
}

/// A `BigDecimal` (a `decimal` past `Decimal`'s range) renders as its exact
/// text, so the row holding that text serves the id and another number's
/// does not.
#[test]
fn a_big_decimal_key_matches_the_row_holding_its_text() {
    let big = |text: &str| GenericValue::BigDecimal(kernel::BigDecimal::parse(text).unwrap());
    let requested = [big("1e30"), big("2e30")];
    assert_eq!(
        outcomes(vec![(big("1e30"), body("a"))], &requested),
        ["upsert", "delete"]
    );
}
