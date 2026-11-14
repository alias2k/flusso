// A `FlussoMap` type reads one JSON object, so a shape that reads anything
// else is refused where it is declared (issue #289).
use std::collections::HashMap;

use flusso_query::{FlussoFragment, FlussoMap, FlussoValue};

#[derive(serde::Deserialize, FlussoMap)]
#[flusso(keyword)]
struct OverAVec(Vec<String>);

#[derive(serde::Deserialize, FlussoMap)]
#[flusso(keyword)]
struct OverAnOption(Option<HashMap<String, String>>);

#[derive(serde::Deserialize, FlussoMap)]
#[flusso(keyword)]
struct OverAString(String);

#[derive(serde::Deserialize, FlussoMap)]
#[flusso(keyword)]
#[serde(transparent)]
struct NamedOverMaps {
    maps: Vec<HashMap<String, String>>,
}

#[derive(serde::Deserialize, FlussoMap)]
#[flusso(keyword)]
struct Pair(HashMap<String, String>, HashMap<String, String>);

#[derive(serde::Deserialize, FlussoMap)]
#[flusso(keyword)]
struct Unit;

#[derive(serde::Deserialize, FlussoValue)]
struct Codes(Vec<String>);

#[derive(serde::Deserialize, FlussoMap)]
#[flusso(keyword)]
struct OverAnArrayNewtype(Codes);

#[derive(serde::Serialize, serde::Deserialize, FlussoValue)]
#[flusso(keyword)]
struct Code(String);

#[derive(serde::Deserialize, FlussoMap)]
#[flusso(keyword)]
struct OverAValue(Code);

#[derive(serde::Deserialize, FlussoFragment)]
struct Contact {
    email: String,
}

#[derive(serde::Deserialize, FlussoMap)]
#[flusso(keyword)]
struct OverASubDocument(Contact);

/// `from`/`try_from` makes serde read another type, which no check sees.
#[derive(serde::Deserialize, FlussoMap)]
#[flusso(keyword)]
#[serde(from = "Vec<String>")]
struct Converted(HashMap<String, String>);

impl From<Vec<String>> for Converted {
    fn from(_: Vec<String>) -> Self {
        Self(HashMap::new())
    }
}

#[derive(serde::Deserialize, FlussoFragment)]
#[serde(try_from = "String")]
struct Address {
    city: String,
}

impl TryFrom<String> for Address {
    type Error = String;
    fn try_from(city: String) -> Result<Self, String> {
        Ok(Self { city })
    }
}

fn main() {}
