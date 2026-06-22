use std::collections::BTreeMap;

use schema_core::{FieldName, GenericValue, Mapping};

use super::*;

fn field(name: &str, mapping_type: MappingType, children: Vec<ResolvedField>) -> ResolvedField {
    ResolvedField {
        name: FieldName::try_new(name).unwrap(),
        mapping: Mapping {
            mapping_type,
            extra: BTreeMap::new(),
            map_values: None,
            decimal: false,
        },
        nullable: true,
        array: false,
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
            map_values: None,
            decimal: false,
        },
        nullable: true,
        array: false,
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
            map_values: None,
            decimal: false,
        },
        nullable: true,
        array: false,
        children: vec![],
    };
    let body = build_index_body(&[amount], &opts());
    let amount = &body["mappings"]["properties"]["amount"];
    assert_eq!(amount["type"], "scaled_float");
    assert_eq!(amount["scaling_factor"], 100);
}

#[test]
fn map_field_renders_a_dynamic_object() {
    // A `map` field resolves to an `object` with `dynamic: true` in `extra` and
    // no children, so the dynamic keys stay searchable rather than being
    // rejected by the index's root `dynamic: strict`.
    let mut extra = BTreeMap::new();
    extra.insert("dynamic".to_owned(), GenericValue::Bool(true));
    let title = ResolvedField {
        name: FieldName::try_new("title").unwrap(),
        mapping: Mapping {
            mapping_type: MappingType::Object,
            extra,
            map_values: Some(MappingType::Text),
            decimal: false,
        },
        nullable: false,
        array: false,
        children: vec![],
    };
    let body = build_index_body(&[title], &opts());
    let title = &body["mappings"]["properties"]["title"];
    assert_eq!(title["type"], "object");
    assert_eq!(title["dynamic"], true);
    // No fixed sub-properties — the keys are dynamic.
    assert!(title.get("properties").is_none());
}

#[test]
fn other_mapping_type_uses_its_raw_name() {
    assert_eq!(
        opensearch_type(&MappingType::Other("binary".to_owned())),
        "binary"
    );
}
