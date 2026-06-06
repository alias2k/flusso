use flusso_search::FlussoDocument;

// No `#[flusso(index = "…")]` — the derive can't know which index to bind to.
#[derive(serde::Deserialize, FlussoDocument)]
struct User {
    id: i32,
}

fn main() {}
