//! `Box<str>`, `Cow<str>`, `Arc<str>` and `Rc<str>` go wherever `String` goes
//! as a value (issue #306): a `keyword`, `text`-like, `uuid`, `date` or `enum`
//! field without variants, bare, in `Option`, in a `Vec`, as map values, on a
//! root and in a fragment, and as a query value.
use std::borrow::Cow;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

use flusso_query::{FlussoFragment, FlussoRoot};

#[derive(Debug, serde::Deserialize, FlussoRoot)]
#[flusso(index = "boundary")]
struct Root {
    kw: Box<str>,
    kwo: Option<Cow<'static, str>>,
    kwa: Vec<Arc<str>>,
    kwao: Option<Vec<Rc<str>>>,
    uu: Box<str>,
    dt: Cow<'static, str>,
    ev: Arc<str>,
    mp: HashMap<String, Option<Box<str>>>,
    #[serde(flatten)]
    f: Fragment,
}

#[derive(Debug, serde::Deserialize, FlussoFragment)]
struct Fragment {
    kns: Box<str>,
    ts: Rc<str>,
    mpo: Option<HashMap<String, Option<Cow<'static, str>>>>,
    ob: Ob,
}

#[derive(Debug, serde::Deserialize, FlussoFragment)]
struct Ob {
    k: Arc<str>,
}

fn main() {
    let doc = r#"{"kw":"a","kwo":null,"kwa":["a","b"],"kwao":["c"],"uu":"6f1c3b2a-9d4e-4c1a-8b7f-2e5d6c7a8b90","dt":"2024-01-02","ev":"z","mp":{"x":"a","y":null},"kns":"b","ts":"03:04:05","mpo":{"x":"a"},"ob":{"k":"a","n":1}}"#;
    let root: Root = serde_json::from_str(doc).unwrap();
    assert_eq!(&*root.kw, "a");
    assert_eq!(root.kwa.len(), 2);
    assert_eq!(&*root.f.ob.k, "a");
    assert_eq!(root.mp.get("y"), Some(&None));

    let _ = Root::kw().eq(Box::<str>::from("x"));
    let _ = Root::kw().any_of([Arc::<str>::from("x"), Arc::<str>::from("y")]);
    let _ = Root::uu().eq(Cow::<str>::Borrowed("x"));
    let _ = Root::dt().gte(Rc::<str>::from("2024-01-01"));
}
