// `flags` has no `nested` or `object` level, so the root generates no module
// of query scopes for `scope_mod` to rename.
use flusso_query::FlussoRoot;

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "flags", scope_mod = "flag_queries")]
struct Flags {
    id: i32,
}

fn main() {}
