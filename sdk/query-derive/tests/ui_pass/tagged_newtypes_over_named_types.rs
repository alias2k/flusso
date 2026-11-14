//! A tagged `FlussoValue` newtype over a type of your own that reads a string
//! compiles and reads, whatever the type is named: the derive refuses by name
//! only what can't be a string type of your own (issue #294). A `char` reads
//! a one-character string.
use flusso_query::{FlussoRoot, FlussoValue};

type Date = String;

mod my {
    #[derive(Debug, serde::Serialize, serde::Deserialize)]
    pub struct Date(pub String);
    #[derive(Debug, serde::Serialize, serde::Deserialize)]
    pub struct DateTime(pub String);
    #[derive(Debug, serde::Serialize, serde::Deserialize)]
    pub struct Decimal(pub String);
    #[derive(Debug, serde::Serialize, serde::Deserialize)]
    pub struct Duration(pub String);
}

#[derive(Debug, serde::Serialize, serde::Deserialize, FlussoValue)]
#[flusso(keyword)]
struct Day(my::Date);

#[derive(Debug, serde::Serialize, serde::Deserialize, FlussoValue)]
#[flusso(keyword)]
struct Stamp(Option<my::DateTime>);

#[derive(Debug, serde::Serialize, serde::Deserialize, FlussoValue)]
#[flusso(keyword)]
struct Amount(my::Decimal);

#[derive(Debug, serde::Serialize, serde::Deserialize, FlussoValue)]
#[flusso(keyword)]
struct Span(my::Duration);

#[derive(Debug, serde::Serialize, serde::Deserialize, FlussoValue)]
#[flusso(keyword)]
struct Aliased(Date);

#[derive(Debug, serde::Serialize, serde::Deserialize, FlussoValue)]
#[flusso(keyword)]
struct Letter(char);

#[derive(Debug, serde::Deserialize, FlussoRoot)]
#[flusso(index = "users")]
struct A {
    email: Day,
}
#[derive(Debug, serde::Deserialize, FlussoRoot)]
#[flusso(index = "users")]
struct B {
    email: Stamp,
}
#[derive(Debug, serde::Deserialize, FlussoRoot)]
#[flusso(index = "users")]
struct C {
    email: Amount,
}
#[derive(Debug, serde::Deserialize, FlussoRoot)]
#[flusso(index = "users")]
struct D {
    email: Span,
}
#[derive(Debug, serde::Deserialize, FlussoRoot)]
#[flusso(index = "users")]
struct E {
    email: Aliased,
}
#[derive(Debug, serde::Deserialize, FlussoRoot)]
#[flusso(index = "users")]
struct F {
    email: Letter,
}

fn main() {
    let doc = r#"{"email":"2024-01-02"}"#;
    assert_eq!(serde_json::from_str::<A>(doc).unwrap().email.0.0, "2024-01-02");
    assert!(serde_json::from_str::<B>(doc).unwrap().email.0.is_some());
    serde_json::from_str::<C>(doc).unwrap();
    serde_json::from_str::<D>(doc).unwrap();
    assert_eq!(serde_json::from_str::<E>(doc).unwrap().email.0, "2024-01-02");
    assert_eq!(serde_json::from_str::<F>(r#"{"email":"a"}"#).unwrap().email.0, 'a');
}
