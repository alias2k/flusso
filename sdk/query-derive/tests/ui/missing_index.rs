use flusso_query::FlussoRoot;

// No `#[flusso(index = "…")]` — the derive can't know which index to bind to.
#[derive(serde::Deserialize, FlussoRoot)]
struct User {
    id: i32,
}

fn main() {}
