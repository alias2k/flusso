use flusso_query::FlussoMultiDocument;

// Every variant must be a single-field tuple variant carrying the document
// type it decodes into; all offenders are reported at once.
#[derive(FlussoMultiDocument)]
enum SearchItem {
    Unit,
    Named { name: String },
    Wide(String, String),
}

fn main() {}
