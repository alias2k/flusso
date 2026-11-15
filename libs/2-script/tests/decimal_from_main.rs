//! What a script sees of a declared `decimal`, compared with `main` (issue
//! #327's third review).
//!
//! `fixtures/decimal_from_main.tsv` was recorded by running this file in a
//! worktree at `main` (987ce77e), with `declared` swapped for `main`'s reader
//! of a `decimal` field (a `serde_json::Value`, `Decimal::from_str` of the
//! number's text, else through `f64`), under `FLUSSO_BLESS=1`. Each line is the
//! input's label and JSON text, then `type_of(x)`, `${x}` and `x.to_float()`.
//!
//! What must hold, by label:
//! - every decimal: `to_float()` is its `${x}` parsed as `f64` (`0.0` for a
//!   zero). That reference is the algorithm under test, so it proves the
//!   sandbox's own `to_float` is the one called; `the_nearest_double_is_known`
//!   checks the rounding itself against doubles known independently. `main`'s
//!   was `Decimal::to_f64` of a value serde_json's default float parser read,
//!   so a long value could land one unit in the last place off; the test
//!   counts those rows.
//! - `zero`, `exponent`: the kind and `${x}` as on `main`. A value that rounds
//!   to zero, or is written with an exponent, reads as `main` read it.
//! - `long`, `exact`: the kind as on `main`, and `${x}` is the nearest
//!   `Decimal` to the input: about 28 significant digits, at most 28 after
//!   the point, where `main` kept about 17.
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
    let mut main_off = 0;
    for (main, branch) in FIXTURE.lines().zip(actual.lines()) {
        let main: Vec<&str> = main.split('\t').collect();
        let branch: Vec<&str> = branch.split('\t').collect();
        assert_eq!(main[..2], branch[..2], "the inputs differ");
        let (label, input) = (main[0], main[1]);
        let mut problems = Vec::new();
        if label != "max" && main[2] != branch[2] {
            problems.push("kind");
        }
        if matches!(label, "zero" | "exponent") && main[3] != branch[3] {
            problems.push("text");
        }
        if matches!(label, "long" | "exact") {
            let nearest = <rust_decimal::Decimal as std::str::FromStr>::from_str(input)
                .unwrap()
                .to_string();
            if branch[3] != nearest {
                problems.push("nearest decimal");
            }
        }
        if branch[2] == "decimal" {
            let held: f64 = branch[3].parse().unwrap();
            let nearest = if held == 0.0 { 0.0 } else { held };
            if branch[4] != serde_json::to_string(&nearest).unwrap() {
                problems.push("correctly rounded");
            }
            if main[2] == "decimal" && main[4] != branch[4] {
                main_off += 1;
            }
        }
        if !problems.is_empty() {
            wrong.push(format!(
                "{label} {input} ({})\n  main:   {}\n  branch: {}",
                problems.join(", "),
                main[2..].join(" | "),
                branch[2..].join(" | ")
            ));
        }
    }
    println!("rows where main's to_float() was not the nearest f64: {main_off}");
    assert_eq!(FIXTURE.lines().count(), actual.lines().count());
    assert!(
        wrong.is_empty(),
        "{} inputs reach a script differently from main:\n{}",
        wrong.len(),
        wrong.join("\n")
    );
}

/// `to_float()` of a declared decimal is the nearest double, against values
/// whose nearest double is known independently of Rust's parser.
#[test]
fn the_nearest_double_is_known() {
    let schema = IndexSchema::try_from(SchemaYaml::try_parse(SCHEMA).unwrap()).unwrap();
    let index = IndexName::try_new("products").unwrap();
    let scripts =
        Scripts::compile(&SourceSpec::new(BTreeMap::from([(index.clone(), schema)]))).unwrap();
    for (text, nearest) in [
        ("3.14159265358979323846", "3.141592653589793"),
        ("1689181831.774158792227902", "1689181831.7741587"),
        ("0.1", "0.1"),
        ("9007199254740993", "9007199254740992.0"),
    ] {
        let seen = run(&scripts, &index, declared(text));
        assert_eq!(seen.split('\t').nth(2), Some(nearest), "{text}: {seen}");
    }
}
