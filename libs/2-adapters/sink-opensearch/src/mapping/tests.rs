use std::collections::BTreeMap;

use kernel::{FieldName, FlussoType, GenericValue, Mapping};

use super::*;

fn field(name: &str, mapping_type: MappingType, children: Vec<ResolvedField>) -> ResolvedField {
    ResolvedField {
        name: FieldName::try_new(name).unwrap(),
        mapping: Mapping {
            mapping_type,
            extra: BTreeMap::new(),
            map_values: None,
            decimal: false,
            enum_order: None,
            subfields: true,
            indexed: true,
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
        settings: IndexSettings::default(),
    }
}

fn opts_no_subfields() -> IndexOptions {
    IndexOptions {
        auto_subfields: false,
        ..opts()
    }
}

fn enum_field(name: &str, variants: &[&str]) -> ResolvedField {
    let mut f = field(name, MappingType::Keyword, vec![]);
    f.mapping.enum_order = Some(variants.iter().map(|v| (*v).to_owned()).collect());
    f
}

#[test]
fn ordered_enum_gets_a_sort_subfield_backed_by_a_rank_normalizer() {
    let body = build_index_body(&[enum_field("status", &["low", "medium", "high"])], &opts());
    // The field itself stays a plain keyword; the rank rides a `.sort` subfield.
    assert_eq!(body["mappings"]["properties"]["status"]["type"], "keyword");
    let sort = &body["mappings"]["properties"]["status"]["fields"]["sort"];
    assert_eq!(sort["type"], "keyword");
    let norm = sort["normalizer"].as_str().unwrap();
    // Its normalizer runs a `mapping` char filter that remaps each variant to a
    // zero-padded rank.
    let cf = &body["settings"]["analysis"]["char_filter"][norm];
    assert_eq!(cf["type"], "mapping");
    assert_eq!(
        cf["mappings"],
        serde_json::json!(["low => 0", "medium => 1", "high => 2"])
    );
    assert_eq!(
        body["settings"]["analysis"]["normalizer"][norm]["char_filter"][0],
        norm
    );
}

#[test]
fn ordered_enum_sort_subfield_survives_without_auto_subfields() {
    let body = build_index_body(&[enum_field("status", &["a", "b"])], &opts_no_subfields());
    assert_eq!(
        body["mappings"]["properties"]["status"]["fields"]["sort"]["type"],
        "keyword"
    );
}

#[test]
fn bare_keyword_has_no_sort_subfield_or_char_filter() {
    let body = build_index_body(&[field("kind", MappingType::Keyword, vec![])], &opts());
    assert!(
        body["mappings"]["properties"]["kind"]["fields"]
            .get("sort")
            .is_none()
    );
    assert!(body["settings"]["analysis"].get("char_filter").is_none());
}

#[test]
fn enum_sort_ranks_are_zero_padded_to_a_fixed_width() {
    let variants: Vec<String> = (0..12).map(|i| format!("v{i}")).collect();
    let m = enum_sort_mappings(&variants);
    assert_eq!(m[0], "v0 => 00");
    assert_eq!(m[11], "v11 => 11");
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
            enum_order: None,
            subfields: true,
            indexed: true,
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
            enum_order: None,
            subfields: true,
            indexed: true,
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

fn map_field(name: &str, values: &FlussoType) -> ResolvedField {
    let mut map = field(name, MappingType::Object, vec![]);
    map.mapping
        .extra
        .insert("dynamic".to_owned(), GenericValue::Bool(true));
    map.mapping.map_values = Some(Box::new(Mapping::of_type(values)));
    map
}

/// Every type a map's `values` may declare.
fn leaf_value_types() -> Vec<FlussoType> {
    vec![
        FlussoType::Text,
        FlussoType::Identifier,
        FlussoType::Keyword,
        FlussoType::Enum,
        FlussoType::Uuid,
        FlussoType::Short,
        FlussoType::Integer,
        FlussoType::Long,
        FlussoType::Float,
        FlussoType::Double,
        FlussoType::Decimal,
        FlussoType::Date,
        FlussoType::Timestamp,
    ]
}

fn every_option_set() -> Vec<IndexOptions> {
    [true, false]
        .into_iter()
        .flat_map(|auto_subfields| {
            [TextAnalysis::Builtin, TextAnalysis::Icu].map(|text_analysis| IndexOptions {
                auto_subfields,
                text_analysis,
                ..opts()
            })
        })
        .collect()
}

#[test]
fn a_map_key_is_mapped_like_a_plain_field_of_its_values_type() {
    for options in every_option_set() {
        for ty in leaf_value_types() {
            let mut plain = field("title", MappingType::Text, vec![]);
            plain.mapping = Mapping::of_type(&ty);
            let plain = build_index_body(&[plain], &options);
            let map = build_index_body(&[map_field("title", &ty)], &options);

            let template = &map["mappings"]["dynamic_templates"][0]["flusso_map:title"];
            assert_eq!(template["path_match"], "title.*", "{ty:?}");
            assert_eq!(
                template["mapping"], plain["mappings"]["properties"]["title"],
                "{ty:?} auto_subfields={} {:?}",
                options.auto_subfields, options.text_analysis,
            );
            assert_eq!(map["settings"], plain["settings"]);
        }
    }
}

#[test]
fn a_text_map_key_gets_the_text_analyzer_and_subfields() {
    let body = build_index_body(&[map_field("title", &FlussoType::Text)], &opts());
    let key = &body["mappings"]["dynamic_templates"][0]["flusso_map:title"]["mapping"];
    assert_eq!(key["type"], "text");
    assert_eq!(key["analyzer"], TEXT_ANALYZER);
    assert_eq!(key["fields"]["keyword"]["type"], "keyword");
    assert_eq!(
        key["fields"]["keyword_lowercase"]["normalizer"],
        LOWERCASE_NORMALIZER
    );
}

#[test]
fn a_map_object_stays_open_with_no_fixed_keys() {
    let body = build_index_body(&[map_field("title", &FlussoType::Text)], &opts());
    let title = &body["mappings"]["properties"]["title"];
    assert_eq!(title["type"], "object");
    assert_eq!(title["dynamic"], true);
    assert!(title.get("properties").is_none());
    assert_eq!(body["mappings"]["dynamic"], "strict");
}

#[test]
fn map_templates_match_the_full_path_in_field_order() {
    let orders = field(
        "orders",
        MappingType::Nested,
        vec![
            field("id", MappingType::Long, vec![]),
            map_field("labels", &FlussoType::Keyword),
        ],
    );
    let meta = field(
        "meta",
        MappingType::Object,
        vec![map_field("prices", &FlussoType::Double)],
    );
    let fields = [map_field("title", &FlussoType::Text), orders, meta];
    let body = build_index_body(&fields, &opts());

    let templates = body["mappings"]["dynamic_templates"].as_array().unwrap();
    let names: Vec<&str> = templates
        .iter()
        .map(|template| {
            template
                .as_object()
                .unwrap()
                .keys()
                .next()
                .unwrap()
                .as_str()
        })
        .collect();
    assert_eq!(
        names,
        [
            "flusso_map:title",
            "flusso_map:orders.labels",
            "flusso_map:meta.prices"
        ]
    );
    assert_eq!(
        templates[1]["flusso_map:orders.labels"]["path_match"],
        "orders.labels.*"
    );
    assert_eq!(
        templates[2]["flusso_map:meta.prices"]["mapping"]["type"],
        "double"
    );
    assert_eq!(body, build_index_body(&fields, &opts()));
}

#[test]
fn an_index_without_a_map_has_no_dynamic_templates() {
    let body = build_index_body(&[field("name", MappingType::Text, vec![])], &opts());
    assert!(body["mappings"].get("dynamic_templates").is_none());
}

#[test]
fn other_mapping_type_uses_its_raw_name() {
    assert_eq!(
        opensearch_type(&MappingType::Other("binary".to_owned())),
        "binary"
    );
}

fn settings(value: Value) -> IndexSettings {
    serde_json::from_value(value).unwrap()
}

#[test]
fn user_settings_merge_into_the_index_settings_over_the_typed_options() {
    let options = IndexOptions {
        settings: settings(json!({
            "mapping.total_fields.limit": 5000,
            "number_of_replicas": 2,
        })),
        ..opts()
    };
    let index = &build_index_body(&[], &options)["settings"]["index"];
    assert_eq!(index["mapping"]["total_fields"]["limit"], 5000);
    assert_eq!(
        index["number_of_replicas"], 2,
        "index_settings beat the typed option"
    );
    assert_eq!(
        index["number_of_shards"], 1,
        "an untouched typed option stays"
    );
}

#[test]
fn per_index_settings_beat_sink_wide_ones_and_flusso_keys_beat_both() {
    let sink_wide =
        settings(json!({ "mapping.total_fields.limit": 2000, "max_result_window": 20000 }));
    let per_index = settings(json!({ "mapping": { "total_fields": { "limit": 3000 } } }));
    let owned_attempt =
        IndexSettings::from_trusted([("index.refresh_interval".to_owned(), json!("1s"))]);
    let options = IndexOptions {
        settings: sink_wide.layered(&per_index).layered(&owned_attempt),
        ..opts()
    };
    let body = build_index_body(&[], &options);
    let index = &body["settings"]["index"];
    assert_eq!(index["mapping"]["total_fields"]["limit"], 3000);
    assert_eq!(index["max_result_window"], 20000);
    assert_eq!(index["refresh_interval"], "-1");
    assert!(body["settings"]["analysis"]["analyzer"]["flusso_code"].is_object());
}
