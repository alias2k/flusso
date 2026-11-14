//! A `custom` field whose OpenSearch type the derive does not model (`ip`
//! here) takes any type on a root, so it does in a fragment too (issue #306):
//! a sub-document, a type without flusso metadata, an `exhaustive` enum.
use std::borrow::Cow;

use flusso_query::{FlussoFragment, FlussoRoot, FlussoValue};

#[derive(Debug, serde::Deserialize, FlussoFragment)]
struct Sub {
    k: String,
}

#[derive(Debug, serde::Deserialize, FlussoFragment)]
#[serde(deny_unknown_fields)]
struct Strict {
    k: String,
}

#[derive(Debug, serde::Serialize, serde::Deserialize, FlussoValue)]
#[flusso(keyword, exhaustive)]
enum Ex {
    A,
}

#[derive(Debug, serde::Deserialize, FlussoFragment)]
struct BoxF {
    cu: Box<str>,
}

#[derive(Debug, serde::Deserialize, FlussoRoot)]
#[flusso(index = "boundary")]
struct BoxR {
    #[serde(flatten)]
    f: BoxF,
}

#[derive(Debug, serde::Deserialize, FlussoFragment)]
struct CowF {
    cu: Cow<'static, str>,
}

#[derive(Debug, serde::Deserialize, FlussoRoot)]
#[flusso(index = "boundary")]
struct CowR {
    #[serde(flatten)]
    f: CowF,
}

#[derive(Debug, serde::Deserialize, FlussoFragment)]
struct BoxStringF {
    cu: Box<String>,
}

#[derive(Debug, serde::Deserialize, FlussoRoot)]
#[flusso(index = "boundary")]
struct BoxStringR {
    #[serde(flatten)]
    f: BoxStringF,
}

#[derive(Debug, serde::Deserialize, FlussoFragment)]
struct CharF {
    cu: char,
}

#[derive(Debug, serde::Deserialize, FlussoRoot)]
#[flusso(index = "boundary")]
struct CharR {
    #[serde(flatten)]
    f: CharF,
}

#[derive(Debug, serde::Deserialize, FlussoFragment)]
struct WideF {
    cu: u128,
}

#[derive(Debug, serde::Deserialize, FlussoRoot)]
#[flusso(index = "boundary")]
struct WideR {
    #[serde(flatten)]
    f: WideF,
}

#[derive(Debug, serde::Deserialize, FlussoFragment)]
struct SubF {
    cu: Sub,
}

#[derive(Debug, serde::Deserialize, FlussoRoot)]
#[flusso(index = "boundary")]
struct SubR {
    #[serde(flatten)]
    f: SubF,
}

#[derive(Debug, serde::Deserialize, FlussoFragment)]
struct SubsF {
    cu: Vec<Sub>,
}

#[derive(Debug, serde::Deserialize, FlussoRoot)]
#[flusso(index = "boundary")]
struct SubsR {
    #[serde(flatten)]
    f: SubsF,
}

#[derive(Debug, serde::Deserialize, FlussoFragment)]
struct StrictF {
    cu: Strict,
}

#[derive(Debug, serde::Deserialize, FlussoRoot)]
#[flusso(index = "boundary")]
struct StrictR {
    #[serde(flatten)]
    f: StrictF,
}

#[derive(Debug, serde::Deserialize, FlussoFragment)]
struct ExF {
    cu: Ex,
}

#[derive(Debug, serde::Deserialize, FlussoRoot)]
#[flusso(index = "boundary")]
struct ExR {
    #[serde(flatten)]
    f: ExF,
}

#[derive(Debug, serde::Deserialize, FlussoFragment)]
struct OptionsF {
    cu: Vec<Option<String>>,
}

#[derive(Debug, serde::Deserialize, FlussoRoot)]
#[flusso(index = "boundary")]
struct OptionsR {
    #[serde(flatten)]
    f: OptionsF,
}

fn main() {
    let doc = r#"{"cu":"192.168.0.1"}"#;
    let boxed: BoxR = serde_json::from_str(doc).unwrap();
    assert_eq!(&*boxed.f.cu, "192.168.0.1");
    let cow: CowR = serde_json::from_str(doc).unwrap();
    assert_eq!(cow.f.cu, "192.168.0.1");
    let string: BoxStringR = serde_json::from_str(doc).unwrap();
    assert_eq!(*string.f.cu, "192.168.0.1");
}
