//! Translating resolved schema fields into an OpenSearch index body: the
//! `dynamic: strict` mapping, the `flusso_*` analysis definitions, and the
//! production-ready subfield enrichment for `text`/`keyword` fields.

use schema_core::{MappingType, ResolvedField, TextAnalysis};
use serde_json::{Map, Value, json};
use sinks_core::to_json;

/// The settings that shape every index this sink creates. Held by the sink and
/// threaded into [`build_index_body`] so the body builder stays a pure function
/// of `(fields, options)` — easy to unit-test without a live sink.
#[derive(Debug, Clone)]
pub(crate) struct IndexOptions {
    pub(crate) number_of_shards: u32,
    pub(crate) number_of_replicas: u32,
    pub(crate) text_analysis: TextAnalysis,
    pub(crate) auto_subfields: bool,
}

/// The subfield key holding the exact, case-sensitive value of a string field —
/// for aggregations, exact-term filters, and exact sort.
const KEYWORD_SUBFIELD: &str = "keyword";
/// The subfield key holding the lowercased, accent-folded value — for
/// case-insensitive sort and exact lookup.
const KEYWORD_LOWERCASE_SUBFIELD: &str = "keyword_lowercase";
/// The subfield key holding the full-text-analyzed value of a `keyword` field,
/// so a `keyword` is still searchable in a search box.
const TEXT_SUBFIELD: &str = "text";
/// The identifier analyzer (`type: identifier` points fields here, as do
/// `keyword` text subfields) — punctuation-splitting, case- and
/// accent-insensitive. Tuned for short identifier-like text (names, codes, SKUs,
/// statuses).
const CODE_ANALYZER: &str = "flusso_code";
/// The natural-language analyzer attached to `text` fields by default. Plain
/// tokenize + fold, no code-splitting.
const TEXT_ANALYZER: &str = "flusso_text";
/// The normalizer attached to lowercase keyword subfields.
const LOWERCASE_NORMALIZER: &str = "flusso_lowercase";
/// Strings longer than this are not indexed in a `keyword` subfield (they are
/// still stored). Matches OpenSearch's own dynamic-mapping default.
const KEYWORD_IGNORE_ABOVE: u32 = 256;

/// Build the `PUT /{index}` request body: a `dynamic: strict` mapping with one
/// typed property per field, the shard counts, `refresh_interval: -1` for bulk
/// seeding, and the `flusso_*` analysis definitions the field shapes reference.
pub(crate) fn build_index_body(fields: &[ResolvedField], options: &IndexOptions) -> Value {
    json!({
        "settings": {
            "index": {
                "refresh_interval": "-1",
                "number_of_shards": options.number_of_shards,
                "number_of_replicas": options.number_of_replicas,
            },
            // Always emitted so an explicit `analyzer: flusso_text` works even
            // when `auto_subfields` is off; an unused analyzer is harmless.
            "analysis": build_analysis(options.text_analysis),
        },
        "mappings": {
            "dynamic": "strict",
            "properties": build_properties(fields, options),
        },
    })
}

/// The `analysis` block defining the `flusso_*` analyzers, the code-splitting
/// token filter, and the lowercase normalizer. The folding components swap
/// between built-in (`asciifolding`) and ICU (`icu_folding`) per `mode`.
fn build_analysis(mode: TextAnalysis) -> Value {
    // `flusso_code`: split on punctuation / case / letter-digit boundaries
    // (so `C-01234` → `c`, `01234`, `c01234`, `c-01234`), then lowercase + fold.
    // `flatten_graph` is required after `word_delimiter_graph` at index time.
    let code_fold = match mode {
        TextAnalysis::Builtin => "asciifolding",
        TextAnalysis::Icu => "icu_folding",
    };
    let code_analyzer = json!({
        "type": "custom",
        "tokenizer": "whitespace",
        "filter": ["flusso_word_delimiter", "flatten_graph", "lowercase", code_fold],
    });

    // `flusso_text`: natural language. Built-in standard tokenizer + fold, or the ICU
    // tokenizer/normalizer/folding which segment CJK/Thai and fold every script.
    let text_analyzer = match mode {
        TextAnalysis::Builtin => json!({
            "type": "custom",
            "tokenizer": "standard",
            "filter": ["lowercase", "asciifolding"],
        }),
        TextAnalysis::Icu => json!({
            "type": "custom",
            "tokenizer": "icu_tokenizer",
            "filter": ["icu_normalizer", "icu_folding"],
        }),
    };

    // Normalizers accept only a restricted filter set; `icu_normalizer` is the
    // ICU member that qualifies (it lowercases and folds), while built-in mode
    // uses `lowercase` + `asciifolding`.
    let normalizer_filters = match mode {
        TextAnalysis::Builtin => json!(["lowercase", "asciifolding"]),
        TextAnalysis::Icu => json!(["icu_normalizer"]),
    };

    let mut analyzers = Map::new();
    analyzers.insert(CODE_ANALYZER.to_owned(), code_analyzer);
    analyzers.insert(TEXT_ANALYZER.to_owned(), text_analyzer);

    let mut normalizers = Map::new();
    normalizers.insert(
        LOWERCASE_NORMALIZER.to_owned(),
        json!({ "type": "custom", "filter": normalizer_filters }),
    );

    json!({
        "filter": {
            "flusso_word_delimiter": {
                "type": "word_delimiter_graph",
                "catenate_all": true,
                "preserve_original": true,
            },
        },
        "analyzer": Value::Object(analyzers),
        "normalizer": Value::Object(normalizers),
    })
}

/// Translate resolved fields into an OpenSearch `properties` object.
fn build_properties(fields: &[ResolvedField], options: &IndexOptions) -> Value {
    let mut props = Map::new();
    for field in fields {
        props.insert(
            field.name.as_ref().to_owned(),
            build_property(field, options),
        );
    }
    Value::Object(props)
}

/// Translate one resolved field into its OpenSearch property.
///
/// For a scalar `text`/`keyword` field (and `auto_subfields` on) this starts
/// from a production-ready default — a good analyzer plus exact / sortable /
/// searchable subfields — then overlays the field's own `extra` on top, so an
/// explicit `analyzer`, `fields`, etc. always wins. `object`/`nested` recurse
/// into their children; other types pass through with just their `extra`.
fn build_property(field: &ResolvedField, options: &IndexOptions) -> Value {
    let mut prop = Map::new();
    prop.insert(
        "type".to_owned(),
        Value::String(opensearch_type(&field.mapping.mapping_type)),
    );

    // Auto-enrichment applies only to scalar string fields; container types
    // (object/nested, which carry children) and numerics are left as-is.
    if options.auto_subfields && field.children.is_empty() {
        match field.mapping.mapping_type {
            MappingType::Text => {
                prop.insert("analyzer".to_owned(), json!(TEXT_ANALYZER));
                prop.insert("fields".to_owned(), text_subfields());
            }
            MappingType::Keyword => {
                prop.insert("fields".to_owned(), keyword_subfields());
            }
            _ => {}
        }
    }

    // The field's explicit mapping wins, key by key — overriding the analyzer,
    // replacing the auto subfields wholesale, etc.
    for (key, value) in &field.mapping.extra {
        prop.insert(key.clone(), to_json(value));
    }

    if !field.children.is_empty() {
        prop.insert(
            "properties".to_owned(),
            build_properties(&field.children, options),
        );
    }
    Value::Object(prop)
}

/// The case/accent-insensitive `keyword_lowercase` subfield, shared by the
/// `text` and `keyword` defaults — for case-insensitive sort and exact lookup.
fn keyword_lowercase_subfield() -> Value {
    json!({
        "type": "keyword",
        "normalizer": LOWERCASE_NORMALIZER,
        "ignore_above": KEYWORD_IGNORE_ABOVE,
    })
}

/// Default subfields for a `text` field: an exact `keyword` and a
/// case/accent-insensitive `keyword_lowercase` (both for filter/sort/agg).
fn text_subfields() -> Value {
    let mut fields = Map::new();
    fields.insert(
        KEYWORD_SUBFIELD.to_owned(),
        json!({ "type": "keyword", "ignore_above": KEYWORD_IGNORE_ABOVE }),
    );
    fields.insert(
        KEYWORD_LOWERCASE_SUBFIELD.to_owned(),
        keyword_lowercase_subfield(),
    );
    Value::Object(fields)
}

/// Default subfields for a `keyword` field: a full-text `text` (so it is still
/// searchable) and a case/accent-insensitive `keyword_lowercase` for sort.
fn keyword_subfields() -> Value {
    let mut fields = Map::new();
    fields.insert(
        TEXT_SUBFIELD.to_owned(),
        json!({ "type": "text", "analyzer": CODE_ANALYZER }),
    );
    fields.insert(
        KEYWORD_LOWERCASE_SUBFIELD.to_owned(),
        keyword_lowercase_subfield(),
    );
    Value::Object(fields)
}

/// The OpenSearch type string for a [`MappingType`] — the canonical name from
/// [`MappingType::name`], which is also what the type serializes as.
fn opensearch_type(mapping_type: &MappingType) -> String {
    mapping_type.name().to_owned()
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use std::collections::BTreeMap;

    use schema_core::{FieldName, GenericValue, Mapping};

    use super::*;

    fn field(name: &str, mapping_type: MappingType, children: Vec<ResolvedField>) -> ResolvedField {
        ResolvedField {
            name: FieldName::try_new(name).unwrap(),
            mapping: Mapping {
                mapping_type,
                extra: BTreeMap::new(),
            },
            nullable: true,
            children,
        }
    }

    /// Default options: auto-subfields on, built-in analysis, 1 shard / 1 replica.
    fn opts() -> IndexOptions {
        IndexOptions {
            number_of_shards: 1,
            number_of_replicas: 1,
            text_analysis: TextAnalysis::Builtin,
            auto_subfields: true,
        }
    }

    fn opts_no_subfields() -> IndexOptions {
        IndexOptions {
            auto_subfields: false,
            ..opts()
        }
    }

    #[test]
    fn index_body_is_dynamic_strict_with_disabled_refresh_and_shards() {
        let body = build_index_body(&[field("email", MappingType::Keyword, vec![])], &opts());
        assert_eq!(body["mappings"]["dynamic"], "strict");
        assert_eq!(body["settings"]["index"]["refresh_interval"], "-1");
        assert_eq!(body["settings"]["index"]["number_of_shards"], 1);
        assert_eq!(body["settings"]["index"]["number_of_replicas"], 1);
        assert_eq!(body["mappings"]["properties"]["email"]["type"], "keyword");
    }

    #[test]
    fn analysis_block_defines_the_flusso_analyzers() {
        let body = build_index_body(&[], &opts());
        let analysis = &body["settings"]["analysis"];
        assert_eq!(
            analysis["filter"]["flusso_word_delimiter"]["type"],
            "word_delimiter_graph"
        );
        assert_eq!(
            analysis["analyzer"]["flusso_code"]["tokenizer"],
            "whitespace"
        );
        // Built-in mode folds with asciifolding, not ICU.
        let code_filters = &analysis["analyzer"]["flusso_code"]["filter"];
        assert!(
            code_filters
                .as_array()
                .unwrap()
                .iter()
                .any(|f| f == "asciifolding")
        );
        assert_eq!(analysis["analyzer"]["flusso_text"]["tokenizer"], "standard");
        assert_eq!(analysis["normalizer"]["flusso_lowercase"]["type"], "custom");
    }

    #[test]
    fn icu_mode_swaps_in_icu_components() {
        let icu = IndexOptions {
            text_analysis: TextAnalysis::Icu,
            ..opts()
        };
        let body = build_index_body(&[], &icu);
        let analysis = &body["settings"]["analysis"];
        let code_filters = &analysis["analyzer"]["flusso_code"]["filter"];
        assert!(
            code_filters
                .as_array()
                .unwrap()
                .iter()
                .any(|f| f == "icu_folding")
        );
        assert_eq!(
            analysis["analyzer"]["flusso_text"]["tokenizer"],
            "icu_tokenizer"
        );
        assert_eq!(
            analysis["normalizer"]["flusso_lowercase"]["filter"][0],
            "icu_normalizer"
        );
    }

    #[test]
    fn text_field_gets_text_analyzer_and_subfields() {
        let body = build_index_body(&[field("name", MappingType::Text, vec![])], &opts());
        let name = &body["mappings"]["properties"]["name"];
        assert_eq!(name["type"], "text");
        assert_eq!(name["analyzer"], "flusso_text");
        assert_eq!(name["fields"]["keyword"]["type"], "keyword");
        assert_eq!(name["fields"]["keyword"]["ignore_above"], 256);
        assert_eq!(
            name["fields"]["keyword_lowercase"]["normalizer"],
            "flusso_lowercase"
        );
    }

    #[test]
    fn keyword_field_gets_text_and_lowercase_subfields() {
        let body = build_index_body(&[field("email", MappingType::Keyword, vec![])], &opts());
        let email = &body["mappings"]["properties"]["email"];
        assert_eq!(email["type"], "keyword");
        assert_eq!(email["fields"]["text"]["type"], "text");
        assert_eq!(email["fields"]["text"]["analyzer"], "flusso_code");
        assert_eq!(
            email["fields"]["keyword_lowercase"]["normalizer"],
            "flusso_lowercase"
        );
    }

    #[test]
    fn auto_subfields_off_leaves_string_fields_bare() {
        let body = build_index_body(
            &[field("name", MappingType::Text, vec![])],
            &opts_no_subfields(),
        );
        let name = &body["mappings"]["properties"]["name"];
        assert_eq!(name["type"], "text");
        assert!(name.get("fields").is_none());
        assert!(name.get("analyzer").is_none());
    }

    #[test]
    fn explicit_extra_overrides_the_auto_shape() {
        // A field that sets its own analyzer (e.g. `options: { analyzer: english }`)
        // keeps it over the auto default, and explicit `fields` replace the auto
        // subfields wholesale.
        let mut extra = BTreeMap::new();
        extra.insert(
            "analyzer".to_owned(),
            GenericValue::String("english".to_owned()),
        );
        let name = ResolvedField {
            name: FieldName::try_new("bio").unwrap(),
            mapping: Mapping {
                mapping_type: MappingType::Text,
                extra,
            },
            nullable: true,
            children: vec![],
        };
        let body = build_index_body(&[name], &opts());
        let bio = &body["mappings"]["properties"]["bio"];
        assert_eq!(bio["analyzer"], "english");
        // The auto subfields are still present (only `analyzer` was overridden).
        assert_eq!(bio["fields"]["keyword"]["type"], "keyword");
    }

    #[test]
    fn nested_field_recurses_into_properties() {
        let orders = field(
            "orders",
            MappingType::Nested,
            vec![
                field("id", MappingType::Long, vec![]),
                field("total", MappingType::Double, vec![]),
            ],
        );
        let body = build_index_body(&[orders], &opts());
        let orders = &body["mappings"]["properties"]["orders"];
        assert_eq!(orders["type"], "nested");
        assert_eq!(orders["properties"]["id"]["type"], "long");
        assert_eq!(orders["properties"]["total"]["type"], "double");
        // Numeric children get no string subfields.
        assert!(orders["properties"]["id"].get("fields").is_none());
    }

    #[test]
    fn extra_mapping_settings_pass_through() {
        let mut extra = BTreeMap::new();
        extra.insert("scaling_factor".to_owned(), GenericValue::Int(100));
        let amount = ResolvedField {
            name: FieldName::try_new("amount").unwrap(),
            mapping: Mapping {
                mapping_type: MappingType::ScaledFloat,
                extra,
            },
            nullable: true,
            children: vec![],
        };
        let body = build_index_body(&[amount], &opts());
        let amount = &body["mappings"]["properties"]["amount"];
        assert_eq!(amount["type"], "scaled_float");
        assert_eq!(amount["scaling_factor"], 100);
    }

    #[test]
    fn other_mapping_type_uses_its_raw_name() {
        assert_eq!(
            opensearch_type(&MappingType::Other("binary".to_owned())),
            "binary"
        );
    }
}
