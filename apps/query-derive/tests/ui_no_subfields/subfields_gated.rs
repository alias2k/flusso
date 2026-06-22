use flusso_query::FlussoDocument;

// Resolved against `no_subfields.toml` (FLUSSO_CONFIG, set by the harness),
// whose OpenSearch sink has `auto_subfields = false`. So the derive stamps
// `text`/`keyword` handles `NoSubfields` and the auto-subfield accessors don't
// exist — a would-be runtime 400 becomes a compile error.
#[derive(serde::Deserialize, FlussoDocument)]
#[flusso(index = "users")]
struct User {
    email: String,
    #[flusso(rename = "fullName")]
    full_name: Option<String>,
}

fn main() {
    // `fullName` is a `text` field; with subfields off there is no `.keyword()`.
    let _ = User::full_name().keyword();
}
