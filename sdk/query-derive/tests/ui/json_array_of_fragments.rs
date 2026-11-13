use flusso_query::{FlussoFragment, FlussoRoot};

// A `json` array declares no fields, so a fragment's fields exist nowhere in
// it: refused on a root and in a fragment alike.

#[derive(serde::Deserialize, FlussoFragment)]
struct Note {
    body: String,
}

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "lists")]
struct OnARoot {
    id: String,
    notes: Option<Vec<Note>>,
}

#[derive(serde::Deserialize, FlussoFragment)]
struct Flat {
    notes: Option<Vec<Note>>,
}

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "lists")]
struct InAFragment {
    id: String,
    #[serde(flatten)]
    flat: Flat,
}

fn main() {}
