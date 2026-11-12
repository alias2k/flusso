// `skip` already turns off every check, so `opaque` beside it does nothing.
use flusso_query::FlussoRoot;

#[derive(serde::Deserialize)]
struct Plain {}

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "users")]
struct User {
    id: i32,
    #[flusso(skip, opaque)]
    #[serde(skip)]
    extra: Option<Plain>,
}

fn main() {}
