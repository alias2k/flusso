// A fragment field is held to the same rule: `city` is a `String`, so `opaque`
// has no sub-shape to opt out of. `skip` is the attribute that turns its
// checks off.
use flusso_query::FlussoFragment;

#[derive(serde::Deserialize, FlussoFragment)]
struct Address {
    #[flusso(opaque)]
    city: String,
}

fn main() {}
