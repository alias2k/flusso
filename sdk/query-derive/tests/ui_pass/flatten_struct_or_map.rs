// What serde can flatten, a struct or a map, stays accepted on a root and in a
// fragment, and reads (issue #289).
use std::collections::HashMap;

use flusso_query::{FlussoFragment, FlussoMap, FlussoRoot};

#[derive(serde::Deserialize, FlussoFragment)]
struct Contact {
    email: String,
}

#[derive(serde::Deserialize, FlussoFragment)]
#[serde(transparent)]
struct Wrapped(Contact);

#[derive(serde::Deserialize, FlussoMap)]
#[flusso(keyword)]
struct Rest(HashMap<String, serde_json::Value>);

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "users")]
struct User {
    id: i32,
    #[serde(flatten)]
    contact: Contact,
}

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "users")]
struct UserOptional {
    id: i32,
    #[serde(flatten)]
    contact: Option<Contact>,
}

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "users")]
struct UserWrapped {
    id: i32,
    #[serde(flatten)]
    contact: Wrapped,
}

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "users")]
struct UserRest {
    id: i32,
    #[serde(flatten)]
    rest: Rest,
}

#[derive(serde::Deserialize, FlussoFragment)]
struct Flat {
    #[serde(flatten)]
    contact: Contact,
}

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "users")]
struct UserFlat {
    id: i32,
    #[serde(flatten)]
    flat: Flat,
}

fn main() {
    let doc = r#"{"id": 1, "email": "a@b"}"#;
    assert_eq!(serde_json::from_str::<User>(doc).unwrap().contact.email, "a@b");
    assert!(serde_json::from_str::<UserOptional>(doc).unwrap().contact.is_some());
    assert_eq!(serde_json::from_str::<UserWrapped>(doc).unwrap().contact.0.email, "a@b");
    assert!(serde_json::from_str::<UserRest>(doc).unwrap().rest.0.contains_key("email"));
    assert_eq!(serde_json::from_str::<UserFlat>(doc).unwrap().flat.contact.email, "a@b");
}
