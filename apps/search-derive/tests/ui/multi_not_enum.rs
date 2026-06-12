use flusso_search::FlussoMultiDocument;

// The union is an enum by construction — one variant per document type.
#[derive(FlussoMultiDocument)]
struct SearchItem {
    name: String,
}

fn main() {}
