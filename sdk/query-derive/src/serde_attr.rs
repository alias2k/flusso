//! Reading serde's own attributes, the one place the derives do it.
//!
//! The derives need four things serde decides: a container's `rename_all`, a
//! field's or variant's `rename`, an `alias`, and `flatten`. Everything else in
//! `#[serde(…)]` is stepped over, so an attribute the derive doesn't care
//! about (`bound = ""`, `default = "…"`, `with = "…"`, `from = "…"`) never
//! stops it from reading the ones it does.
//!
//! ```ignore
//! #[serde(bound = "", rename_all(deserialize = "camelCase"))]
//! //       ^ stepped over   ^ read: the rule deserialization applies
//! ```

use proc_macro2::TokenStream;
use syn::Attribute;

/// One item of a `#[serde(…)]` list.
pub(crate) struct SerdeMeta {
    pub(crate) name: String,
    pub(crate) value: SerdeValue,
}

pub(crate) enum SerdeValue {
    /// `flatten`
    Flag,
    /// `rename = "x"`
    Str(String),
    /// `rename(serialize = "x", deserialize = "y")`: each `key = "…"` pair.
    List(Vec<(String, String)>),
    /// Anything else: `default = some::path`, a list that isn't `key = "…"`.
    Other,
}

impl SerdeValue {
    /// The name deserialization uses: a plain string, or the `deserialize`
    /// entry of a list.
    pub(crate) fn deserialize(&self) -> Option<&str> {
        match self {
            SerdeValue::Str(value) => Some(value),
            SerdeValue::List(pairs) => pairs
                .iter()
                .find(|(key, _)| key == "deserialize")
                .map(|(_, value)| value.as_str()),
            SerdeValue::Flag | SerdeValue::Other => None,
        }
    }
}

/// Every item of every `#[serde(…)]` attribute, in order.
pub(crate) fn serde_metas(attrs: &[Attribute]) -> Vec<SerdeMeta> {
    let mut metas = Vec::new();
    for attr in attrs.iter().filter(|attr| attr.path().is_ident("serde")) {
        // Malformed serde syntax is serde's to report; whatever parsed before
        // it is still read.
        let _ = attr.parse_nested_meta(|meta| {
            let name = meta
                .path
                .get_ident()
                .map_or_else(String::new, ToString::to_string);
            let value = if meta.input.peek(syn::Token![=]) {
                match meta.value()?.parse::<syn::Expr>()? {
                    syn::Expr::Lit(syn::ExprLit {
                        lit: syn::Lit::Str(lit),
                        ..
                    }) => SerdeValue::Str(lit.value()),
                    _ => SerdeValue::Other,
                }
            } else if meta.input.peek(syn::token::Paren) {
                let content;
                syn::parenthesized!(content in meta.input);
                let tokens: TokenStream = content.parse()?;
                list(tokens)
            } else {
                SerdeValue::Flag
            };
            metas.push(SerdeMeta { name, value });
            Ok(())
        });
    }
    metas
}

fn list(tokens: TokenStream) -> SerdeValue {
    let parser =
        syn::punctuated::Punctuated::<syn::MetaNameValue, syn::Token![,]>::parse_terminated;
    let Ok(items) = syn::parse::Parser::parse2(parser, tokens) else {
        return SerdeValue::Other;
    };
    let pairs = items
        .iter()
        .filter_map(|item| {
            let key = item.path.get_ident()?.to_string();
            match &item.value {
                syn::Expr::Lit(syn::ExprLit {
                    lit: syn::Lit::Str(lit),
                    ..
                }) => Some((key, lit.value())),
                _ => None,
            }
        })
        .collect();
    SerdeValue::List(pairs)
}

/// The container `rename_all` deserialization applies, if any.
pub(crate) fn rename_all(attrs: &[Attribute]) -> Option<String> {
    serde_metas(attrs)
        .iter()
        .rev()
        .filter(|meta| meta.name == "rename_all")
        .find_map(|meta| meta.value.deserialize().map(str::to_owned))
}

/// Whether `#[serde(flatten)]` is set.
pub(crate) fn flatten(attrs: &[Attribute]) -> bool {
    serde_metas(attrs)
        .iter()
        .any(|meta| meta.name == "flatten" && matches!(meta.value, SerdeValue::Flag))
}

/// The key deserialization reads for a variant: its `rename`, plain or the
/// `deserialize` entry.
pub(crate) fn variant_rename(attrs: &[Attribute]) -> Option<String> {
    serde_metas(attrs)
        .iter()
        .rev()
        .filter(|meta| meta.name == "rename")
        .find_map(|meta| meta.value.deserialize().map(str::to_owned))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::{flatten, rename_all, variant_rename};

    fn attrs(source: &str) -> Vec<syn::Attribute> {
        let item: syn::DeriveInput = syn::parse_str(&format!("{source} struct S;")).unwrap();
        item.attrs
    }

    #[test]
    fn rename_all_is_read_after_any_other_item() {
        for before in [
            r#"bound = """#,
            r#"default = "Self::new""#,
            r#"from = "Raw""#,
            r#"try_from = "Raw""#,
            r#"crate = "serde""#,
            r#"expecting = "a user""#,
            "deny_unknown_fields",
            r#"bound(deserialize = "T: Clone")"#,
        ] {
            let attrs = attrs(&format!(r#"#[serde({before}, rename_all = "camelCase")]"#));
            assert_eq!(rename_all(&attrs).as_deref(), Some("camelCase"), "{before}");
        }
    }

    #[test]
    fn rename_all_takes_the_deserialize_rule_of_a_list() {
        let both =
            attrs(r#"#[serde(rename_all(serialize = "snake_case", deserialize = "camelCase"))]"#);
        assert_eq!(rename_all(&both).as_deref(), Some("camelCase"));
        let serialize_only = attrs(r#"#[serde(rename_all(serialize = "camelCase"))]"#);
        assert_eq!(rename_all(&serialize_only), None);
    }

    #[test]
    fn flatten_and_rename_are_read_after_any_other_item() {
        assert!(flatten(&attrs(r#"#[serde(with = "module", flatten)]"#)));
        assert_eq!(
            variant_rename(&attrs(r#"#[serde(alias = "p", rename = "pro")]"#)).as_deref(),
            Some("pro")
        );
        assert_eq!(
            variant_rename(&attrs(r#"#[serde(rename(deserialize = "pro"))]"#)).as_deref(),
            Some("pro")
        );
    }
}
