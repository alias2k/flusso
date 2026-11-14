// A `Decimal` of your own reads a string, not a number: at a `double` it is
// judged by its own kinds, on a root and in a fragment (issue #306).
use flusso_query::{FlussoFragment, FlussoRoot, FlussoValue};

#[derive(Debug, serde::Serialize, serde::Deserialize, FlussoValue)]
struct Decimal(String);

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "boundary")]
struct Root {
    db: Decimal,
}

#[derive(serde::Deserialize, FlussoFragment)]
struct Fragment {
    db: Decimal,
}

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "boundary")]
struct Embedding {
    #[serde(flatten)]
    f: Fragment,
}

fn main() {}
