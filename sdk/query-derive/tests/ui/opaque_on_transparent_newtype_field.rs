// The field of a `#[serde(transparent)]` fragment is checked as the newtype
// itself: `opaque` belongs on the field that embeds the newtype.
use flusso_query::FlussoFragment;

#[derive(serde::Deserialize)]
struct Address {
    city: String,
}

#[derive(serde::Deserialize, FlussoFragment)]
#[serde(transparent)]
struct Billing(#[flusso(opaque)] Address);

fn main() {}
