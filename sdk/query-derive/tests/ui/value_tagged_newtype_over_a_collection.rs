// A tagged `FlussoValue` newtype stands for one value of its tag, so it cannot
// wrap a map, a collection or a `Box` of one: the first document fails with
// `expected a map` (issue #294).
use std::collections::{HashMap, HashSet, VecDeque};

use flusso_query::FlussoValue;

#[derive(serde::Serialize, serde::Deserialize, FlussoValue)]
#[flusso(keyword)]
struct Codes(HashMap<String, String>);

#[derive(serde::Serialize, serde::Deserialize, FlussoValue)]
#[flusso(keyword)]
struct Boxed(Box<HashMap<String, String>>);

#[derive(serde::Serialize, serde::Deserialize, FlussoValue)]
#[flusso(keyword)]
struct Set(HashSet<String>);

#[derive(serde::Serialize, serde::Deserialize, FlussoValue)]
#[flusso(keyword)]
struct Queue(VecDeque<String>);

#[derive(serde::Serialize, serde::Deserialize, FlussoValue)]
#[flusso(keyword)]
struct Pair((String, String));

fn main() {}
