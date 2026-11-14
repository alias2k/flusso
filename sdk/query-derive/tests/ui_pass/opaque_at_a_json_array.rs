// `opaque` at a `json` array: a root and a fragment take the same types
// (issue #289), each array newtype at the field it belongs to.
use flusso_query::{FlussoFragment, FlussoRoot, FlussoValue};

#[derive(serde::Serialize, serde::Deserialize, FlussoValue)]
#[flusso(keyword)]
#[serde(rename_all = "snake_case")]
enum Tag {
    Urgent,
}

#[derive(serde::Serialize, serde::Deserialize, FlussoValue)]
struct Tags(Vec<Tag>);

#[derive(serde::Deserialize)]
struct Note {}

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "lists")]
struct OnARoot {
    id: String,
    #[flusso(opaque)]
    notes: Option<Vec<Tag>>,
}

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "lists")]
struct NewtypeOnARoot {
    id: String,
    #[flusso(opaque)]
    notes: Option<Tags>,
}

#[derive(serde::Deserialize, FlussoFragment)]
struct Values {
    #[flusso(opaque)]
    notes: Option<Vec<Tag>>,
}

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "lists")]
struct InAFragment {
    id: String,
    #[serde(flatten)]
    flat: Values,
}

#[derive(serde::Deserialize, FlussoFragment)]
struct Newtype {
    #[flusso(opaque)]
    notes: Option<Tags>,
    tags: Option<Tags>,
}

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "lists")]
struct NewtypeInAFragment {
    id: String,
    #[serde(flatten)]
    flat: Newtype,
}

#[derive(serde::Deserialize, FlussoFragment)]
struct Plain {
    #[flusso(opaque)]
    notes: Option<Vec<Note>>,
}

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "lists")]
struct PlainInAFragment {
    id: String,
    #[serde(flatten)]
    flat: Plain,
}

fn main() {}
