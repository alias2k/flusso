use flusso_query::FlussoDocument;

// Numeric query values are typed per kind, accepted only when lossless. These
// must be compile errors — the safety guarantee of the per-type `Number<K>`
// split. If a lossy `FlussoValue` impl ever leaks in, this snapshot breaks.
#[derive(serde::Deserialize, FlussoDocument)]
#[flusso(index = "users")]
struct User {
    #[flusso(rename = "orderCount")]
    order_count: i64,
}

fn main() {
    // `orderCount` is a `long` — a float is lossy, rejected.
    let _ = User::order_count().eq(1.5);
    // a string isn't a number value at all.
    let _ = User::order_count().eq("nope");
}
