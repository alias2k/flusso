use flusso_query::{FlussoDocument, SortBuilder, SortOrder};

#[derive(serde::Deserialize, FlussoDocument)]
#[flusso(index = "users")]
struct User {
    id: i32,
}

fn main() {
    // `location` is a `geo` field; `Geo` is not `Sortable`, so it can't be a
    // `by` key — geo sorts go through `SortBuilder::near` / `raw`.
    let _ = SortBuilder::new()
        .by(User::location(), SortOrder::Asc)
        .build();
}
