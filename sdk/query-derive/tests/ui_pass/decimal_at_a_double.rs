//! `Decimal` at a `double` field (and at an `avg`, a `double` too) compiles on
//! a root as it does in a fragment, and is a query value there (issue #306),
//! and reads the doubles flusso writes inside its range.
use flusso_query::{Decimal, FlussoFragment, FlussoRoot};

#[derive(Debug, serde::Deserialize, FlussoRoot)]
#[flusso(index = "boundary")]
struct Root {
    db: Decimal,
    av: Option<Decimal>,
}

#[derive(Debug, serde::Deserialize, FlussoFragment)]
struct Fragment {
    db: Decimal,
    av: Option<Decimal>,
}

#[derive(Debug, serde::Deserialize, FlussoRoot)]
#[flusso(index = "boundary")]
struct Embedding {
    #[serde(flatten)]
    f: Fragment,
}

fn main() {
    let doc = r#"{"db":1.5,"av":null}"#;
    let root: Root = serde_json::from_str(doc).unwrap();
    assert_eq!(root.db, Decimal::new(15, 1));
    assert!(root.av.is_none());
    let _ = Root::db().gte(Decimal::new(15, 1));
    let embedding: Embedding = serde_json::from_str(doc).unwrap();
    assert_eq!(embedding.f.db, Decimal::new(15, 1));
}
