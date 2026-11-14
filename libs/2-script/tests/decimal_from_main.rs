//! What a script sees of a declared `decimal`, compared with `main` (issue
//! #327's third review).
//!
//! `fixtures/decimal_from_main.tsv` was recorded by running this file in a
//! worktree at `main` (987ce77e), with `declared` swapped for `main`'s reader
//! of a `decimal` field (a `serde_json::Value`, `Decimal::from_str` of the
//! number's text, else through `f64`), under `FLUSSO_BLESS=1`. Each line is the
//! input's label and JSON text, then `type_of(x)`, `${x}` and `x.to_float()`.
//!
//! What must match `main`, by label:
//! - `zero`, `exponent`: all three. A value that rounds to zero, or is written
//!   with an exponent, reads as `main` read it.
//! - `long`, `exact`: the kind and `to_float()`, the correctly rounded double.
//!   `${x}` keeps every digit the text has, where `main` kept about 17.
//! - `past`: the kind (a string). Its text is the exact digits, where `main`'s
//!   went through `f64`.
//! - `max`: nothing. A value `Decimal` holds exactly but `f64` rounds past its
//!   range was a string on `main` and is a decimal now.

#![allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    unused_crate_dependencies
)]

use std::collections::BTreeMap;
use std::fmt::Write as _;

use config::yaml::SchemaYaml;
use kernel::{GenericValue, IndexName, IndexSchema, ParseFrom};
use script::Scripts;
use source::SourceSpec;

/// The document value a declared `decimal` holding `text` reads as.
fn declared(text: &str) -> GenericValue {
    GenericValue::BigDecimal(kernel::BigDecimal::parse(text).unwrap())
}

const FIXTURE: &str = include_str!("fixtures/decimal_from_main.tsv");

const SCHEMA: &str = "version: 1
table: products
fields:
  - decimal: x
    required: false
  - keyword: kind
    required: false
    script: 'type_of(this.x)'
  - keyword: text
    required: false
    script: '`${this.x}`'
  - double: float
    required: false
    script: 'if type_of(this.x) == \"decimal\" { this.x.to_float() } else { () }'
";

fn inputs() -> Vec<(&'static str, String)> {
    let mut out: Vec<(&'static str, String)> = [
        ("zero", "0"),
        ("zero", "-0"),
        ("zero", "0.0"),
        ("zero", "0e99999999999999999999"),
        ("zero", "1e-99999999999999999999"),
        ("zero", "0.01e-9223372036854775808"),
        ("zero", "-2.5E-29"),
        ("zero", "-1.5e-30"),
        ("zero", "1.5e-30"),
        ("zero", "5e-324"),
        ("zero", "4e-29"),
        ("zero", "-0.000000000000000000000000000001234"),
        ("exponent", "1e2"),
        ("exponent", "-1.5E+3"),
        ("exponent", "1e-07"),
        ("exponent", "9.007199254740992e+15"),
        ("exponent", "6e-29"),
        ("exponent", "1.5e-28"),
        ("exponent", "1e+28"),
        ("exact", "12.50"),
        ("exact", "7"),
        ("exact", "-1234567890.123456789"),
        ("max", "79228162514264337593543950334"),
        ("long", "3.14159265358979323846"),
        ("long", "0.123456789012345678901234567890"),
        ("long", "1.000000000000000000000000000000"),
        ("past", "79228162514264337593543950336"),
        ("past", "1000000000000000000000000000000"),
        ("past", "-1e30"),
        ("past", "1e+308"),
    ]
    .into_iter()
    .map(|(label, text)| (label, text.to_owned()))
    .collect();
    let mut state: u64 = 0x9e37_79b9_7f4a_7c15;
    let mut next = |modulo: u64| {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state % modulo
    };
    for _ in 0..300 {
        let whole_digits = 1 + next(12) as usize;
        let fraction_digits = 1 + next(20) as usize;
        let mut text = String::new();
        if next(4) == 0 {
            text.push('-');
        }
        text.push(char::from(b'1' + next(9) as u8));
        for _ in 1..whole_digits {
            text.push(char::from(b'0' + next(10) as u8));
        }
        text.push('.');
        for _ in 0..fraction_digits {
            text.push(char::from(b'0' + next(10) as u8));
        }
        let label = if whole_digits + fraction_digits > 17 {
            "long"
        } else {
            "exact"
        };
        out.push((label, text));
    }
    out
}

fn run(scripts: &Scripts, index: &IndexName, x: GenericValue) -> String {
    let mut doc = GenericValue::Map(BTreeMap::from([("x".to_owned(), x)]));
    scripts.apply(index, &mut doc).unwrap();
    let GenericValue::Map(map) = doc else {
        unreachable!()
    };
    let text = |key: &str| match map.get(key) {
        Some(GenericValue::String(s)) => s.clone(),
        Some(GenericValue::Double(f)) => serde_json::to_string(f).unwrap(),
        Some(GenericValue::Null) | None => "null".to_owned(),
        Some(other) => format!("{other:?}"),
    };
    format!("{}\t{}\t{}", text("kind"), text("text"), text("float"))
}

#[test]
fn a_declared_decimal_reaches_a_script_as_main_gave_it() {
    let schema = IndexSchema::try_from(SchemaYaml::try_parse(SCHEMA).unwrap()).unwrap();
    let index = IndexName::try_new("products").unwrap();
    let scripts =
        Scripts::compile(&SourceSpec::new(BTreeMap::from([(index.clone(), schema)]))).unwrap();
    let mut actual = String::new();
    for (label, text) in inputs() {
        let seen = run(&scripts, &index, declared(&text));
        writeln!(actual, "{label}\t{text}\t{seen}").unwrap();
    }
    if std::env::var_os("FLUSSO_BLESS").is_some() {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/decimal_from_main.tsv"
        );
        std::fs::write(path, &actual).unwrap();
        return;
    }
    let mut wrong = Vec::new();
    for (main, branch) in FIXTURE.lines().zip(actual.lines()) {
        let main: Vec<&str> = main.split('\t').collect();
        let branch: Vec<&str> = branch.split('\t').collect();
        assert_eq!(main[..2], branch[..2], "the inputs differ");
        let must_match: &[usize] = match main[0] {
            "zero" | "exponent" => &[2, 3, 4],
            "long" | "exact" => &[2, 4],
            "max" => &[],
            _ => &[2],
        };
        if must_match
            .iter()
            .any(|&column| main[column] != branch[column])
        {
            wrong.push(format!(
                "{} {}\n  main:   {}\n  branch: {}",
                main[0],
                main[1],
                main[2..].join(" | "),
                branch[2..].join(" | ")
            ));
        }
    }
    assert_eq!(FIXTURE.lines().count(), actual.lines().count());
    assert!(
        wrong.is_empty(),
        "{} inputs reach a script differently from main:\n{}",
        wrong.len(),
        wrong.join("\n")
    );
}
