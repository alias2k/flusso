use flusso_query::{FlussoRoot, FlussoValue};

// `tier` declares [free, pro, enterprise]; the marked enum covers two of them.
// Unmarked that is a legal partial projection — `exhaustive` turns it into an
// error naming the declared set.
#[derive(serde::Serialize, serde::Deserialize, FlussoValue)]
#[serde(rename_all = "camelCase")]
#[flusso(keyword, exhaustive)]
enum Tier {
    Free,
    Pro,
}

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "users")]
struct User {
    tier: Tier,
}

fn main() {}
