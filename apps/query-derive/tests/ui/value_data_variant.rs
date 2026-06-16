use flusso_query::FlussoValue;

// A keyword `FlussoValue` requires unit variants — a data-carrying variant
// serializes to an object/array under serde, not a keyword string.
#[derive(serde::Deserialize, FlussoValue)]
enum Tier {
    Free,
    Custom(String),
}

fn main() {}
