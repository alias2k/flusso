// The derive recognizes built-in leaf types by name, so a user struct named
// `Date` is treated as the date leaf, and `opaque` cannot opt it out.
use flusso_query::FlussoFragment;

#[derive(serde::Deserialize)]
struct Date {}

#[derive(serde::Deserialize, FlussoFragment)]
struct Order {
    #[flusso(opaque)]
    shipping: Date,
}

fn main() {}
