use std::collections::HashMap;

use flusso_query::FlussoMap;

// A map wrapper must declare the value kind it stands in for — there is no
// default, so an untagged derive is a compile error naming the fix. (This is
// the trap that used to silently default to `keyword` and then fail the
// embed check against the `text` map the type was written for.)
#[derive(serde::Deserialize, FlussoMap)]
struct Translation {
    fallback: Option<String>,
    #[serde(flatten)]
    langs: HashMap<String, String>,
}

fn main() {}
