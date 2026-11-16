//! What a script sees of a number with no declared type (issue #366's review).
//!
//! A `json` or `map` number reached a script as a `Decimal` on `main` whatever
//! its magnitude (rounded, or zero when tiny), and as a string past
//! `Decimal`'s range. It still does: one numeric kind, so `+`, `*`, `>`, `==`
//! and `round` against a decimal keep working. Rhai has no mixed
//! decimal/float operators, so an `f64` here would fail the document.
//!
//! The document value is what the Postgres reader builds for an untyped
//! number, `kernel::untyped_number`; `source-postgres`'s value tests pin the
//! reader to it.

#![allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    unused_crate_dependencies
)]

use std::collections::BTreeMap;
use std::str::FromStr;

use config::yaml::SchemaYaml;
use kernel::{GenericValue, IndexName, IndexSchema, ParseFrom};
use rust_decimal::Decimal;
use script::Scripts;
use source::SourceSpec;

const ARITHMETIC: &str = "version: 1
table: things
fields:
  - json: j
    required: false
  - keyword: kind
    required: false
    script: 'type_of(this.j.n)'
  - keyword: text
    required: false
    script: '`${this.j.n}`'
  - decimal: sum
    required: false
    script: 'this.j.n + parse_decimal(\"1\")'
  - decimal: product
    required: false
    script: 'this.j.n * parse_decimal(\"2\")'
  - boolean: positive
    required: false
    script: 'this.j.n > parse_decimal(\"0\")'
  - boolean: same
    required: false
    script: 'this.j.n == this.j.n'
  - decimal: rounded
    required: false
    script: 'this.j.n.round(2)'
";

const KIND: &str = "version: 1
table: things
fields:
  - json: j
    required: false
  - keyword: kind
    required: false
    script: 'type_of(this.j.n)'
";

fn scripts(schema: &str) -> (Scripts, IndexName) {
    let schema = IndexSchema::try_from(SchemaYaml::try_parse(schema).unwrap()).unwrap();
    let index = IndexName::try_new("things").unwrap();
    let spec = SourceSpec::new(BTreeMap::from([(index.clone(), schema)]));
    (Scripts::compile(&spec).unwrap(), index)
}

fn run(
    (scripts, index): &(Scripts, IndexName),
    n: GenericValue,
) -> Result<BTreeMap<String, GenericValue>, String> {
    let inner = GenericValue::Map(BTreeMap::from([("n".to_owned(), n)]));
    let mut doc = GenericValue::Map(BTreeMap::from([("j".to_owned(), inner)]));
    scripts
        .apply(index, &mut doc)
        .map_err(|failure| format!("{failure:?}"))?;
    match doc {
        GenericValue::Map(map) => Ok(map),
        other => panic!("not a map: {other:?}"),
    }
}

fn text(value: Option<&GenericValue>) -> String {
    match value {
        Some(GenericValue::String(s)) => s.clone(),
        Some(GenericValue::BigDecimal(d)) => d.as_str().to_owned(),
        other => format!("{other:?}"),
    }
}

/// Every magnitude a `Decimal`'s range covers reaches a script as a decimal
/// and goes through arithmetic, comparison and `round` against one; `text`
/// is the decimal the script holds: the double's own digits when a `Decimal`
/// holds them, else the nearest `Decimal` (28 fractional places).
#[test]
fn an_untyped_number_is_a_decimal_to_a_script_at_every_magnitude() {
    let scripts = scripts(ARITHMETIC);
    for (value, held) in [
        (0.1, "0.1"),
        (906_404.209_596_897_9, "906404.2095968979"),
        (-476_228.372_971_136_53, "-476228.37297113653"),
        (1e-7, "0.0000001"),
        (1e-28, "0.0000000000000000000000000001"),
        (3.141_592_653_589_793e-14, "0.0000000000000314159265358979"),
        (1.5e-28, "0.0000000000000000000000000002"),
        (6e-29, "0.0000000000000000000000000001"),
        (4e-29, "0"),
        (1e-29, "0"),
        (-2.5e-29, "0"),
        (1e-300, "0"),
        (5e-324, "0"),
        (1e28, "10000000000000000000000000000"),
        (3.9e28, "39000000000000000000000000000"),
    ] {
        let doc = run(&scripts, kernel::untyped_number(value))
            .unwrap_or_else(|failure| panic!("{value:e}: {failure}"));
        assert_eq!(text(doc.get("kind")), "decimal", "{value:e}");
        assert_eq!(text(doc.get("text")), held, "{value:e}");
        let held = Decimal::from_str(held).unwrap();
        let exact = |key: &str| Decimal::from_str(&text(doc.get(key))).unwrap();
        assert_eq!(exact("sum"), held + Decimal::ONE, "{value:e}");
        assert_eq!(exact("product"), held * Decimal::TWO, "{value:e}");
        assert_eq!(exact("rounded"), held.round_dp(2), "{value:e}");
        assert_eq!(
            doc.get("positive"),
            Some(&GenericValue::Bool(held > Decimal::ZERO)),
            "{value:e}"
        );
        assert_eq!(doc.get("same"), Some(&GenericValue::Bool(true)), "{value:e}");
    }
}

/// Past `Decimal`'s range an untyped number is the string of its digits, as
/// on `main`.
#[test]
fn an_untyped_number_past_the_decimal_range_is_a_string_as_on_main() {
    let scripts = scripts(KIND);
    for value in [7.93e28, 1e29, -1e30, 3.402_823_6e38, 1.5e300, f64::MAX] {
        let doc = run(&scripts, kernel::untyped_number(value)).unwrap();
        assert_eq!(text(doc.get("kind")), "string", "{value:e}");
    }
}
