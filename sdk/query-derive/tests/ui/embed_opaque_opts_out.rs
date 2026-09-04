// `#[flusso(opaque)]` opts a field out of the embed check, but the field still
// has to exist in the schema — opting out of the shape check is not opting out
// of the mapping.
use flusso_query::FlussoRoot;

#[derive(serde::Deserialize)]
struct Plain {
    whatever: String,
}

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "users")]
struct User {
    id: i32,
    #[flusso(opaque)]
    nonesuch: Vec<Plain>,
}

fn main() {}
