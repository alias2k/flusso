#![allow(clippy::unwrap_used, clippy::indexing_slicing)]

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use config::yaml::SchemaYaml;
use kernel::{IndexSchema, ParseFrom};

use super::*;

fn spec(fields: &str) -> SourceSpec {
    let yaml = format!("version: 1\ntable: products\nfields:\n{fields}");
    let schema = IndexSchema::try_from(SchemaYaml::try_parse(&yaml).unwrap()).unwrap();
    SourceSpec::new(BTreeMap::from([(products(), schema)]))
}

fn products() -> IndexName {
    IndexName::try_new("products").unwrap()
}

fn doc(pairs: &[(&str, GenericValue)]) -> GenericValue {
    GenericValue::Map(
        pairs
            .iter()
            .map(|(k, v)| ((*k).to_owned(), v.clone()))
            .collect(),
    )
}

fn text(s: &str) -> GenericValue {
    GenericValue::String(s.to_owned())
}

fn get<'a>(value: &'a GenericValue, key: &str) -> Option<&'a GenericValue> {
    match value {
        GenericValue::Map(map) => map.get(key),
        _ => None,
    }
}

const NAME_AND_LABEL: &str = "  \
  - keyword: first\n    required: true\n  \
  - keyword: last\n    required: true\n    emit: false\n  \
  - keyword: label\n    required: true\n    script: '`${this.first} ${this.last}`'\n";

#[test]
fn a_script_computes_its_field_and_input_only_fields_are_stripped() {
    let scripts = Scripts::compile(&spec(NAME_AND_LABEL)).unwrap();
    let mut body = doc(&[
        ("first", text("Ada")),
        ("last", text("Lovelace")),
        ("label", GenericValue::Null),
    ]);
    scripts.apply(&products(), &mut body).unwrap();
    assert_eq!(get(&body, "label"), Some(&text("Ada Lovelace")));
    assert_eq!(get(&body, "last"), None, "input-only field is stripped");
    assert_eq!(get(&body, "first"), Some(&text("Ada")));
}

#[test]
fn an_index_without_scripts_is_not_planned() {
    let scripts = Scripts::compile(&spec("  - keyword: first\n    required: true\n")).unwrap();
    assert!(scripts.is_empty());
}

#[test]
fn a_script_inside_a_has_many_runs_per_element_with_root_in_reach() {
    let scripts = Scripts::compile(&spec(
        "  \
  - keyword: currency\n    required: true\n  \
  - has_many: variants\n    table: variants\n    primary_key: id\n    foreign_key: product_id\n    fields:\n      \
    - double: price\n        required: true\n      \
    - keyword: shown\n        required: true\n        script: '`${this.price} ${root.currency}`'\n",
    ))
    .unwrap();
    let variant = |price: f64| {
        doc(&[
            ("price", GenericValue::Double(price)),
            ("shown", GenericValue::Null),
        ])
    };
    let mut body = doc(&[
        ("currency", text("EUR")),
        (
            "variants",
            GenericValue::Array(vec![variant(1.5), variant(2.0)]),
        ),
    ]);
    scripts.apply(&products(), &mut body).unwrap();
    let Some(GenericValue::Array(items)) = get(&body, "variants") else {
        panic!("variants should stay an array");
    };
    assert_eq!(get(&items[0], "shown"), Some(&text("1.5 EUR")));
    assert_eq!(get(&items[1], "shown"), Some(&text("2.0 EUR")));
}

#[test]
fn a_script_sees_no_other_scripts_output() {
    let scripts = Scripts::compile(&spec(
        "  \
  - keyword: a\n    required: true\n    script: '\"x\"'\n  \
  - keyword: b\n    required: false\n    script: this.a\n",
    ))
    .unwrap();
    let mut body = doc(&[("a", GenericValue::Null), ("b", GenericValue::Null)]);
    scripts.apply(&products(), &mut body).unwrap();
    assert_eq!(get(&body, "a"), Some(&text("x")));
    assert_eq!(
        get(&body, "b"),
        Some(&GenericValue::Null),
        "b read a's placeholder"
    );
}

#[test]
fn a_script_cannot_mutate_what_another_reads() {
    let scripts = Scripts::compile(&spec(
        "  \
  - keyword: name\n    required: true\n  \
  - keyword: a\n    required: true\n    script: 'this.name = \"changed\"; \"a\"'\n  \
  - keyword: b\n    required: true\n    script: this.name\n",
    ))
    .unwrap();
    let mut body = doc(&[
        ("name", text("kept")),
        ("a", GenericValue::Null),
        ("b", GenericValue::Null),
    ]);
    scripts.apply(&products(), &mut body).unwrap();
    assert_eq!(get(&body, "b"), Some(&text("kept")));
    assert_eq!(get(&body, "name"), Some(&text("kept")));
}

#[test]
fn every_compile_error_is_reported_with_its_position() {
    let err = Scripts::compile(&spec(
        "  \
  - keyword: a\n    required: true\n    script: 'this.x +'\n  \
  - keyword: b\n    required: true\n    script: |\n      let y = 1;\n      undefined_name\n",
    ))
    .unwrap_err();
    assert_eq!(err.0.len(), 2, "{err}");
    let a = &err.0[0].failure;
    assert_eq!(a.field, "a");
    let b = &err.0[1].failure;
    assert_eq!(b.field, "b");
    let at = b.position.as_ref().unwrap();
    assert_eq!((at.line, at.excerpt.as_str()), (2, "undefined_name"));
    assert_eq!(at.column, 1);
    assert!(
        err.to_string().contains("2 scripts failed to compile"),
        "{err}"
    );
}

#[test]
fn a_column_on_the_first_line_maps_back_past_the_wrapper() {
    let err = Scripts::compile(&spec(
        "  - keyword: a\n    required: true\n    script: 'let q = nope;'\n",
    ))
    .unwrap_err();
    let at = err.0[0].failure.position.clone().unwrap();
    assert_eq!((at.line, at.column), (1, 9));
}

#[test]
fn a_misspelled_property_fails_loudly_with_the_line() {
    let scripts = Scripts::compile(&spec(
        "  \
  - keyword: first\n    required: true\n  \
  - keyword: label\n    required: true\n    script: |\n      let a = 1;\n      this.frist\n",
    ))
    .unwrap();
    let mut body = doc(&[("first", text("Ada")), ("label", GenericValue::Null)]);
    let failure = scripts.apply(&products(), &mut body).unwrap_err();
    assert_eq!(failure.field, "label");
    assert!(failure.message.contains("frist"), "{}", failure.message);
    let at = failure.position.unwrap();
    assert_eq!((at.line, at.excerpt.as_str()), (2, "this.frist"));
}

#[test]
fn a_wrong_type_names_the_element_path() {
    let scripts = Scripts::compile(&spec(
        "  \
  - has_many: variants\n    table: variants\n    primary_key: id\n    foreign_key: product_id\n    fields:\n      \
    - long: n\n        required: true\n        script: '\"nope\"'\n",
    ))
    .unwrap();
    let mut body = doc(&[(
        "variants",
        GenericValue::Array(vec![doc(&[("n", GenericValue::Null)])]),
    )]);
    let failure = scripts.apply(&products(), &mut body).unwrap_err();
    assert_eq!(failure.field, "variants[0].n");
    assert_eq!(failure.message, "returned a string, expected an integer");
}

#[test]
fn a_runaway_script_is_stopped() {
    let scripts = Scripts::compile(&spec(
        "  - long: n\n    required: true\n    script: 'loop {}'\n",
    ))
    .unwrap();
    let mut body = doc(&[("n", GenericValue::Null)]);
    let failure = scripts.apply(&products(), &mut body).unwrap_err();
    assert!(
        failure.message.to_lowercase().contains("operations"),
        "{}",
        failure.message
    );
}

#[test]
fn eval_and_the_clock_are_not_available() {
    for source in ["eval(\"1\")", "timestamp()"] {
        let result = Scripts::compile(&spec(&format!(
            "  - long: n\n    required: true\n    script: '{source}'\n"
        )));
        let failed_to_compile = result.is_err();
        let failed_to_run = result.is_ok_and(|scripts| {
            scripts
                .apply(&products(), &mut doc(&[("n", GenericValue::Null)]))
                .is_err()
        });
        assert!(
            failed_to_compile || failed_to_run,
            "`{source}` should not work"
        );
    }
}

#[test]
fn prints_reach_the_printer_with_the_field() {
    let seen: Arc<Mutex<Vec<String>>> = Arc::default();
    let sink = Arc::clone(&seen);
    let printer: Printer = Arc::new(move |context: &ScriptContext<'_>, text: &str| {
        sink.lock()
            .unwrap()
            .push(format!("{}/{}: {text}", context.index, context.field));
    });
    let scripts = Scripts::compile_with_printer(
        &spec("  - long: n\n    required: true\n    script: 'print(\"hi\"); 1'\n"),
        Some(printer),
    )
    .unwrap();
    scripts
        .apply(&products(), &mut doc(&[("n", GenericValue::Null)]))
        .unwrap();
    assert_eq!(seen.lock().unwrap().as_slice(), ["products/n: hi"]);
}

#[test]
fn root_cannot_be_changed_for_the_next_script() {
    let scripts = Scripts::compile(&spec(
        "  \
  - keyword: name\n    required: true\n  \
  - keyword: a\n    required: false\n    script: 'root.name = \"changed\"; ()'\n  \
  - keyword: b\n    required: true\n    script: root.name\n",
    ))
    .unwrap();
    let mut body = doc(&[
        ("name", text("kept")),
        ("a", GenericValue::Null),
        ("b", GenericValue::Null),
    ]);
    match scripts.apply(&products(), &mut body) {
        Ok(()) => assert_eq!(get(&body, "b"), Some(&text("kept"))),
        Err(failure) => assert_eq!(failure.field, "a", "only the writing script fails"),
    }
}

fn big(text: &str) -> GenericValue {
    GenericValue::BigDecimal(kernel::BigDecimal::parse(text).unwrap())
}

const DECIMAL_FROM_SCRIPTS: &str = "  \
  - decimal: x\n    required: false\n  \
  - decimal: same\n    required: false\n    script: 'this.x'\n  \
  - decimal: twice\n    required: false\n    script: 'this.x * 2'\n  \
  - double: float\n    required: false\n    script: 'this.x.to_float() + 1.0'\n  \
  - keyword: kind\n    required: false\n    script: 'type_of(this.x)'\n";

/// A `decimal` script field is a declared decimal: its result is written with
/// every digit `Decimal` holds, never through `f64` (issue #327's review).
#[test]
fn a_decimal_script_field_keeps_every_digit_of_its_result() {
    let scripts = Scripts::compile(&spec(DECIMAL_FROM_SCRIPTS)).unwrap();
    for (x, same, twice) in [
        (
            "1234567890.123456789",
            "1234567890.123456789",
            "2469135780.246913578",
        ),
        ("12.50", "12.50", "25.00"),
        ("7", "7", "14"),
    ] {
        let mut body = doc(&[("x", big(x))]);
        scripts.apply(&products(), &mut body).unwrap();
        assert_eq!(get(&body, "same"), Some(&big(same)), "{x}");
        assert_eq!(get(&body, "twice"), Some(&big(twice)), "{x}");
        assert_eq!(get(&body, "kind"), Some(&text("decimal")), "{x}");
    }
}

/// A declared decimal inside `Decimal`'s magnitude reaches a script as a
/// decimal however small, rounded to what `Decimal` holds, as before issue
/// #327: `1.5e-30` (a `float8` read under `decimal`) is a decimal 0 and its
/// arithmetic works.
#[test]
fn a_tiny_decimal_written_with_an_exponent_reaches_a_script_as_a_decimal() {
    let scripts = Scripts::compile(&spec(DECIMAL_FROM_SCRIPTS)).unwrap();
    for x in ["1.5e-30", "5e-324", "-2.5E-29"] {
        let mut body = doc(&[("x", big(x))]);
        scripts.apply(&products(), &mut body).unwrap();
        assert_eq!(get(&body, "kind"), Some(&text("decimal")), "{x}");
        assert_eq!(get(&body, "float"), Some(&GenericValue::Double(1.0)), "{x}");
    }
}

/// A string returned to a `decimal` script field fails the document with
/// `main`'s message, whatever it holds (issue #327's third review): a value
/// past `Decimal`'s range reaches a script as a string, so `+` concatenates
/// it into a number ten times too large.
#[test]
fn a_string_returned_to_a_decimal_field_fails_the_document() {
    for source in [
        r#"this.x + parse_decimal("0.10")"#,
        "this.v.to_string()",
        r#""12.5""#,
        r#""abc""#,
    ] {
        let fields = format!(
            "  - decimal: x\n    required: false\n  \
               - decimal: v\n    required: false\n  \
               - decimal: out\n    required: false\n    script: '{source}'\n"
        );
        let scripts = Scripts::compile(&spec(&fields)).unwrap();
        let mut body = doc(&[
            ("x", big("79228162514264337593543950336")),
            ("v", big("12.5")),
        ]);
        let failure = scripts.apply(&products(), &mut body).unwrap_err();
        assert_eq!(failure.field, "out", "{source}");
        assert_eq!(
            failure.message, "returned a string, expected a decimal (or an integer)",
            "{source}"
        );
    }
}
