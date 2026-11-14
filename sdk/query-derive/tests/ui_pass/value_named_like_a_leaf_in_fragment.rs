//! A `FlussoValue` type of your own named like a built-in leaf goes where its
//! own kinds fit, in a fragment as on a root (issue #306): a fragment reads the
//! type's metadata as well as its name. Bare, in a `Vec` and as map values.
use std::collections::HashMap;

use flusso_query::{FlussoFragment, FlussoRoot, FlussoValue};

#[derive(Debug, serde::Serialize, serde::Deserialize, FlussoValue)]
struct Date(String);

#[derive(Debug, serde::Serialize, serde::Deserialize, FlussoValue)]
struct Uuid(String);

#[derive(Debug, serde::Serialize, serde::Deserialize, FlussoValue)]
struct Decimal(String);

#[derive(Debug, serde::Serialize, serde::Deserialize, FlussoValue)]
struct GeoPoint(String);

#[derive(Debug, serde::Deserialize, FlussoFragment)]
struct Fragment {
    kw: Date,
    kwo: Option<Decimal>,
    kns: GeoPoint,
    ev: Uuid,
    dt: Uuid,
    ts: GeoPoint,
    kwa: Vec<Date>,
    mp: HashMap<String, Option<Date>>,
}

#[derive(Debug, serde::Deserialize, FlussoRoot)]
#[flusso(index = "boundary")]
struct Root {
    #[serde(flatten)]
    f: Fragment,
}

fn main() {
    let doc = r#"{"kw":"a","kwo":null,"kns":"b","ev":"a","dt":"2024-01-02","ts":"03:04:05","kwa":["a"],"mp":{"x":"a"}}"#;
    let root: Root = serde_json::from_str(doc).unwrap();
    assert_eq!(root.f.kw.0, "a");
    assert_eq!(root.f.dt.0, "2024-01-02");
    assert_eq!(root.f.kwa.len(), 1);
}
