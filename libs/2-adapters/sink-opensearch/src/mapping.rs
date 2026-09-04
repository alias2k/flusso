//! Translating resolved schema fields into an OpenSearch index body: the
//! `dynamic: strict` mapping, the `flusso_*` analysis definitions, and the
//! production-ready subfield enrichment for `text`/`keyword` fields.

use kernel::{MappingType, ResolvedField};

use crate::config::TextAnalysis;
use serde_json::{Map, Value, json};
use sink::to_json;

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
/// The subfield key holding an ordered enum's prebaked rank, for order-correct
/// sort. A `mapping` char-filter normalizer rewrites each declared variant to a
/// zero-padded rank, so a plain `keyword` sort on this subfield sorts by
/// declared order; out-of-set values pass through and sort after (by value).
const ENUM_SORT_SUBFIELD: &str = "sort";
/// Prefix for the per-field char-filter + normalizer names that back the enum
/// sort subfield.
const ENUM_SORT_PREFIX: &str = "flusso_enumsort";

/// The shared char-filter/normalizer name for an ordered enum at `path` — one
/// per field, keyed by its (sanitized) dotted path so several enums coexist.
fn enum_sort_name(path: &str) -> String {
    let sane: String = path
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect();
    format!("{ENUM_SORT_PREFIX}__{sane}")
}

/// The `mapping` char-filter rules for a variant list: `variant => zero-padded
/// rank`. Padding to a fixed width keeps the lexicographic keyword sort equal to
/// the numeric rank order even past ten variants.
fn enum_sort_mappings(variants: &[String]) -> Vec<String> {
    let width = variants.len().saturating_sub(1).to_string().len();
    variants
        .iter()
        .enumerate()
        .map(|(rank, variant)| format!("{variant} => {rank:0width$}"))
        .collect()
}

/// Collect every ordered-enum field as `(dotted_path, variants)`, recursing into
/// object/nested children — the inputs to the per-field sort char-filters and
/// normalizers.
fn collect_enum_sorts<'a>(
    fields: &'a [ResolvedField],
    prefix: &str,
    out: &mut Vec<(String, &'a [String])>,
) {
    for field in fields {
        let path = field_path(prefix, field);
        if let Some(order) = &field.mapping.enum_order {
            out.push((path.clone(), order));
        }
        collect_enum_sorts(&field.children, &path, out);
    }
}

/// A field's dotted path from the index root.
fn field_path(prefix: &str, field: &ResolvedField) -> String {
    if prefix.is_empty() {
        field.name.as_ref().to_owned()
    } else {
        format!("{prefix}.{}", field.name.as_ref())
    }
}

/// Build the `PUT /{index}` request body: a `dynamic: strict` mapping with one
/// typed property per field, the shard counts, `refresh_interval: -1` for bulk
/// seeding, and the `flusso_*` analysis definitions the field shapes reference.
pub(crate) fn build_index_body(fields: &[ResolvedField], options: &IndexOptions) -> Value {
    let mut enum_sorts = Vec::new();
    collect_enum_sorts(fields, "", &mut enum_sorts);
    json!({
        "settings": {
            "index": {
                "refresh_interval": "-1",
                "number_of_shards": options.number_of_shards,
                "number_of_replicas": options.number_of_replicas,
            },
            // Always emitted so an explicit `analyzer: flusso_text` works even
            // when `auto_subfields` is off; an unused analyzer is harmless.
            "analysis": build_analysis(options.text_analysis, &enum_sorts),
        },
        "mappings": {
            "dynamic": "strict",
            "properties": build_properties(fields, options, ""),
        },
    })
}

/// The `analysis` block defining the `flusso_*` analyzers, the code-splitting
/// token filter, and the lowercase normalizer. The folding components swap
/// between built-in (`asciifolding`) and ICU (`icu_folding`) per `mode`.
fn build_analysis(mode: TextAnalysis, enum_sorts: &[(String, &[String])]) -> Value {
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

    // One `mapping` char filter + custom normalizer per ordered enum: the char
    // filter rewrites each declared variant to its zero-padded rank, so the
    // enum's `.sort` subfield holds a rank-ordered keyword.
    let mut char_filters = Map::new();
    for (path, variants) in enum_sorts {
        let name = enum_sort_name(path);
        char_filters.insert(
            name.clone(),
            json!({ "type": "mapping", "mappings": enum_sort_mappings(variants) }),
        );
        normalizers.insert(
            name.clone(),
            json!({ "type": "custom", "char_filter": [name], "filter": [] }),
        );
    }

    let mut analysis = Map::new();
    analysis.insert(
        "filter".to_owned(),
        json!({
            "flusso_word_delimiter": {
                "type": "word_delimiter_graph",
                "catenate_all": true,
                "preserve_original": true,
            },
        }),
    );
    if !char_filters.is_empty() {
        analysis.insert("char_filter".to_owned(), Value::Object(char_filters));
    }
    analysis.insert("analyzer".to_owned(), Value::Object(analyzers));
    analysis.insert("normalizer".to_owned(), Value::Object(normalizers));
    Value::Object(analysis)
}

fn build_properties(fields: &[ResolvedField], options: &IndexOptions, prefix: &str) -> Value {
    let mut props = Map::new();
    for field in fields {
        props.insert(
            field.name.as_ref().to_owned(),
            build_property(field, options, &field_path(prefix, field)),
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
fn build_property(field: &ResolvedField, options: &IndexOptions, path: &str) -> Value {
    let mut prop = Map::new();
    prop.insert(
        "type".to_owned(),
        Value::String(opensearch_type(&field.mapping.mapping_type)),
    );

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

    // An ordered enum gets a `.sort` subfield regardless of `auto_subfields` —
    // it is the ordering mechanism, not an optional enrichment. It sits beside
    // any auto subfields; a custom `fields` in `extra` still overrides.
    if field.mapping.enum_order.is_some() {
        let sort = json!({
            "type": "keyword",
            "normalizer": enum_sort_name(path),
            "ignore_above": KEYWORD_IGNORE_ABOVE,
        });
        match prop.get_mut("fields").and_then(Value::as_object_mut) {
            Some(fields) => {
                fields.insert(ENUM_SORT_SUBFIELD.to_owned(), sort);
            }
            None => {
                let mut fields = Map::new();
                fields.insert(ENUM_SORT_SUBFIELD.to_owned(), sort);
                prop.insert("fields".to_owned(), Value::Object(fields));
            }
        }
    }

    for (key, value) in &field.mapping.extra {
        prop.insert(key.clone(), to_json(value));
    }

    if !field.children.is_empty() {
        prop.insert(
            "properties".to_owned(),
            build_properties(&field.children, options, path),
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
mod tests;
