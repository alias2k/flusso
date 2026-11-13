use flusso_query::{FlussoFragment, FlussoRoot, FlussoValue};

// An array of values goes at an array leaf or a `json` array, never at a
// `nested` field; an array of sub-documents only at a `nested` field.

#[derive(serde::Serialize, serde::Deserialize, FlussoValue)]
struct Codes(Vec<String>);

#[derive(serde::Deserialize, FlussoFragment)]
struct Line {
    sku: String,
    codes: Vec<String>,
}

#[derive(serde::Deserialize, FlussoFragment)]
#[serde(transparent)]
struct Lines(Vec<Line>);

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "lists")]
struct ValuesAtNested {
    id: String,
    lines: Codes,
}

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "lists")]
struct SubDocumentsAtALeaf {
    id: String,
    labels: Lines,
}

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "lists")]
struct SubDocumentsAtJson {
    id: String,
    notes: Option<Lines>,
}

#[derive(serde::Deserialize, FlussoFragment)]
struct FlatValuesAtNested {
    lines: Codes,
}

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "lists")]
struct FlattenedValuesAtNested {
    id: String,
    #[serde(flatten)]
    flat: FlatValuesAtNested,
}

#[derive(serde::Deserialize, FlussoFragment)]
struct FlatSubDocumentsAtALeaf {
    labels: Lines,
}

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "lists")]
struct FlattenedSubDocumentsAtALeaf {
    id: String,
    #[serde(flatten)]
    flat: FlatSubDocumentsAtALeaf,
}

#[derive(serde::Deserialize, FlussoFragment)]
struct FlatSubDocumentsAtJson {
    notes: Option<Lines>,
}

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "lists")]
struct FlattenedSubDocumentsAtJson {
    id: String,
    #[serde(flatten)]
    flat: FlatSubDocumentsAtJson,
}

fn main() {}
