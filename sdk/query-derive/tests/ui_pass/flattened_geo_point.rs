// flusso's own `GeoPoint`, flattened, is the built-in leaf type: it carries no
// fragment check, so it is not checked, on a root or in a fragment.
use flusso_query::{FlussoFragment, FlussoRoot, GeoPoint};

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "users")]
struct User {
    id: i32,
    #[serde(flatten)]
    point: GeoPoint,
}

#[derive(serde::Deserialize, FlussoFragment)]
struct Place {
    #[serde(flatten)]
    point: flusso_query::GeoPoint,
}

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "users")]
struct UserPlace {
    id: i32,
    #[serde(flatten)]
    place: Place,
}

fn main() {}
