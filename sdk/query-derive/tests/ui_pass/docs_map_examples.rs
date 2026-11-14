//! The map examples of the query manual (`docs/src/query/maps.md`,
//! "Document types", included from here by anchor) and the plugin skill
//! (`plugin/skills/flusso-query/maps.md`, checked equal to the `plugin`
//! anchor) compile and read a `null` map value (issue #294).
use std::collections::{BTreeMap, HashMap};

use flusso_query::{FieldSpec, FlussoFragment, FlussoMap, FlussoRoot, KindTag};

// docs/src/query/maps.md includes this by anchor.
// ANCHOR: translation
#[derive(serde::Deserialize, FlussoMap)]
#[flusso(text)]
struct Translation {
    fallback: Option<String>,
    #[serde(flatten)]
    langs: BTreeMap<String, Option<String>>,   // a jsonb column may hold {"it": null}
}

#[derive(serde::Deserialize, FlussoFragment)]
struct Localized { name: Translation }
// ANCHOR_END: translation

// The manual's `name` is a `map` of `text` values.
const LEVEL: &[FieldSpec] = &[FieldSpec {
    name: "name",
    kind: KindTag::Object,
    nullable: false,
    array: false,
    variants: &[],
    map_values: Some(KindTag::Text),
    children: &[],
}];

const _: () = Localized::__flusso_check(LEVEL);

// plugin/skills/flusso-query/maps.md holds this text; `boundary.rs`'s
// `the_docs_map_examples_are_the_tested_ones` keeps them equal.
// ANCHOR: plugin
#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "products")]
struct Product {
    sku: String,
    title: HashMap<String, Option<String>>,          // map<text>    → TextMap
    codes: Option<HashMap<String, Option<String>>>,  // map<keyword> → KeywordMap (nullable)
    prices: Option<HashMap<String, Option<f64>>>,    // map<double>  → NumberMap
}
// ANCHOR_END: plugin

fn main() {
    let localized: Localized =
        serde_json::from_str(r#"{"name":{"fallback":null,"it":null,"en":"Shoes"}}"#).unwrap();
    assert_eq!(localized.name.fallback, None);
    assert_eq!(localized.name.langs.get("it"), Some(&None));

    let product: Product = serde_json::from_str(
        r#"{"sku":"a","title":{"it":null},"codes":{"x":null},"prices":{"eur":null}}"#,
    )
    .unwrap();
    assert_eq!(product.title.get("it"), Some(&None));
    assert_eq!(product.prices.unwrap().get("eur"), Some(&None));
}
