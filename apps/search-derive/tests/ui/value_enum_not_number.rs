use flusso_search::FlussoValue;

// Only the `keyword` kind accepts enums (a unit variant serializes to a string).
// A `number` field needs a newtype wrapper, so this is rejected.
#[derive(serde::Deserialize, FlussoValue)]
#[flusso(number)]
enum Tier {
    Free,
    Pro,
}

fn main() {}
