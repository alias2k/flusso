use flusso_query::FlussoDocument;

// `email` is a `keyword` in the schema. A user enum is allowed *only* if it
// implements `FlussoValue<kind::Keyword>` (via `#[derive(FlussoValue)]`). This
// one doesn't, so the deferred bound fails to resolve.
#[derive(serde::Deserialize)]
enum Tier {
    Free,
    Pro,
}

#[derive(serde::Deserialize, FlussoDocument)]
#[flusso(index = "users")]
struct User {
    id: i32,
    email: Tier,
}

fn main() {}
