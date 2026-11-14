//! A map key type that reads any string compiles and reads, whatever its
//! name: a `type` alias of `String`, `Box<str>`, a `String` newtype of your
//! own (issue #294: keys are refused only when the derive knows by name they
//! read no arbitrary string).
use std::collections::{BTreeMap, HashMap};

use flusso_query::{FieldSpec, FlussoFragment, FlussoMap, FlussoRoot, KindTag};

type Key = String;

mod my {
    #[derive(Debug, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Deserialize)]
    pub struct Key(pub String);
}

#[derive(Debug, serde::Deserialize, FlussoMap)]
#[flusso(text)]
struct Title(HashMap<Box<str>, String>);

#[derive(Debug, serde::Deserialize, FlussoRoot)]
#[flusso(index = "products")]
struct Product {
    sku: String,
    title: Title,
    codes: Option<HashMap<Key, String>>,
}

#[derive(Debug, serde::Deserialize, FlussoFragment)]
struct Localized {
    title: BTreeMap<my::Key, String>,
}

const LEVEL: &[FieldSpec] = &[FieldSpec {
    name: "title",
    kind: KindTag::Object,
    nullable: false,
    array: false,
    variants: &[],
    map_values: Some(KindTag::Text),
    children: &[],
}];

const _: () = Localized::__flusso_check(LEVEL);

fn main() {
    let product: Product =
        serde_json::from_str(r#"{"sku":"a","title":{"it":"Scarpe"},"codes":{"x":"a"}}"#).unwrap();
    assert_eq!(product.title.0.get("it").map(String::as_str), Some("Scarpe"));
    assert_eq!(product.codes.unwrap().get("x").map(String::as_str), Some("a"));
    let localized: Localized = serde_json::from_str(r#"{"title":{"it":"Scarpe"}}"#).unwrap();
    assert_eq!(localized.title.get(&my::Key("it".into())).map(String::as_str), Some("Scarpe"));
}
