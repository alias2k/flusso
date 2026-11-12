// A skipped field the schema doesn't have names no level, so `scope` on it
// would rename nothing. (A skipped field whose key *is* a nested or object
// level still renames it.)
use flusso_query::FlussoRoot;

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "users")]
struct User {
    id: i32,
    #[serde(skip)]
    #[flusso(skip, scope = "Cache")]
    cache: Vec<String>,
}

fn main() {}
