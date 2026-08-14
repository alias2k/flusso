//! `#[derive(FlussoValue)]` — opts a Rust type into `flusso_query::FlussoValue<K>`,
//! so it may stand in for a field of kind `K` in a `FlussoDocument` struct.
//!
//! The kind is chosen with a `#[flusso(…)]` attribute and defaults to `keyword`:
//!
//! - `#[flusso(keyword)]` / `#[flusso(text)]` — an **enum** with only unit
//!   variants (`Pro`/`Enterprise`/`Free`, serializing to strings) or a
//!   **newtype** wrapper over a string;
//! - `#[flusso(number)]` / `#[flusso(date)]` — a **newtype** wrapper over a
//!   numeric / timestamp value (an enum serializes to a string, not a number).
//!
//! On success it emits `impl ::flusso_query::FlussoValue<#kind> for #ident {}`.
//! The leaf value's actual serde form is enforced by serde at the boundary;
//! this derive guarantees the *shape* fits the kind.

use proc_macro2::TokenStream;
use quote::quote;
use syn::spanned::Spanned;
use syn::{Data, DeriveInput, Fields};

/// The field kind a `FlussoValue` type stands in for. Numerics are split per
/// type so a value can't lossily cross kinds (a float into an integer field, an
/// `i64` into a `short`).
#[derive(Clone, Copy)]
pub(crate) enum Kind {
    Keyword,
    Text,
    Bool,
    Byte,
    Short,
    Integer,
    Long,
    Float,
    Double,
    Decimal,
    Date,
}

impl Kind {
    /// The `flusso_query::KindTag` this resolves to — the same kind as
    /// [`marker`](Kind::marker), as *data* a `const fn` can read. A fragment's
    /// check runs in const evaluation, where the schema kind is only a value, so
    /// it cannot name `FlussoValue<K>`; it reads this instead.
    pub(crate) fn tag(self) -> TokenStream {
        let name = match self {
            Kind::Keyword => quote! { Keyword },
            Kind::Text => quote! { Text },
            Kind::Bool => quote! { Bool },
            Kind::Byte => quote! { Byte },
            Kind::Short => quote! { Short },
            Kind::Integer => quote! { Integer },
            Kind::Long => quote! { Long },
            Kind::Float => quote! { Float },
            Kind::Double => quote! { Double },
            Kind::Decimal => quote! { Decimal },
            Kind::Date => quote! { Date },
        };
        quote! { ::flusso_query::KindTag::#name }
    }

    /// The `flusso_query::kind::…` marker this resolves to. The single place
    /// these marker paths are written — both this derive and the field-validation
    /// codegen route their kind through here.
    pub(crate) fn marker(self) -> TokenStream {
        match self {
            Kind::Keyword => quote! { ::flusso_query::kind::Keyword },
            Kind::Text => quote! { ::flusso_query::kind::Text },
            Kind::Bool => quote! { ::flusso_query::kind::Bool },
            Kind::Byte => quote! { ::flusso_query::kind::Byte },
            Kind::Short => quote! { ::flusso_query::kind::Short },
            Kind::Integer => quote! { ::flusso_query::kind::Integer },
            Kind::Long => quote! { ::flusso_query::kind::Long },
            Kind::Float => quote! { ::flusso_query::kind::Float },
            Kind::Double => quote! { ::flusso_query::kind::Double },
            Kind::Decimal => quote! { ::flusso_query::kind::Decimal },
            Kind::Date => quote! { ::flusso_query::kind::Date },
        }
    }

    /// Whether this kind is string-valued (and so accepts a unit enum).
    fn is_string(self) -> bool {
        matches!(self, Kind::Keyword | Kind::Text)
    }
}

pub(crate) fn expand(input: DeriveInput) -> TokenStream {
    if !input.generics.params.is_empty() {
        return syn::Error::new(
            input.generics.span(),
            "FlussoValue does not support generic types",
        )
        .to_compile_error();
    }

    let explicit = match kind_attr(&input) {
        Ok(kind) => kind,
        Err(error) => return error.to_compile_error(),
    };

    match build_impl(&input, explicit) {
        Ok(tokens) => tokens,
        Err(error) => error.to_compile_error(),
    }
}

/// The impl a `FlussoValue` derive emits. A **newtype with no explicit kind**
/// inherits *all* of its inner type's kinds (a blanket impl forwarding to the
/// field type) — so `struct Pippo(String)` is a keyword **and** text value, and
/// `struct Money(Decimal)` a decimal value, with no annotation. An explicit
/// `#[flusso(keyword | text)]` (or an enum, which defaults to keyword) restricts
/// to that single string kind.
fn build_impl(input: &DeriveInput, explicit: Option<Kind>) -> syn::Result<TokenStream> {
    let ident = &input.ident;
    match &input.data {
        Data::Enum(data) => {
            let kind = explicit.unwrap_or(Kind::Keyword);
            if !kind.is_string() {
                return Err(syn::Error::new(
                    input.ident.span(),
                    "an enum FlussoValue is string-valued — use `#[flusso(keyword)]` \
                     (the default) or `#[flusso(text)]`",
                ));
            }
            for variant in &data.variants {
                if !matches!(variant.fields, Fields::Unit) {
                    return Err(syn::Error::new(
                        variant.span(),
                        format!(
                            "FlussoValue requires unit variants — `{}` carries data, \
                             which serializes to an object/array, not a string",
                            variant.ident
                        ),
                    ));
                }
            }
            let marker = kind.marker();
            let tag = kind.tag();
            let variants = variant_keys(input, data)?;
            let leaf_check = leaf_check(ident);
            Ok(quote! {
                impl ::flusso_query::FlussoValue<#marker> for #ident {}
                impl ::flusso_query::FlussoValueMeta for #ident {
                    const KINDS: &'static [::flusso_query::KindTag] = &[#tag];
                    const VARIANTS: &'static [&'static str] = &[#(#variants),*];
                }
                #leaf_check
            })
        }
        Data::Struct(data) => {
            // The single field of a newtype tuple struct — pulled via the
            // iterator (not indexing, which the workspace lints forbid).
            let mut fields = match &data.fields {
                Fields::Unnamed(fields) => fields.unnamed.iter(),
                _ => return Err(newtype_required(input)),
            };
            let inner = match (fields.next(), fields.next()) {
                (Some(field), None) => &field.ty,
                _ => return Err(newtype_required(input)),
            };
            let leaf_check = leaf_check(ident);
            match explicit {
                // Restrict to one string kind (e.g. a keyword-only code wrapper).
                Some(kind) => {
                    let marker = kind.marker();
                    let tag = kind.tag();
                    Ok(quote! {
                        impl ::flusso_query::FlussoValue<#marker> for #ident {}
                        impl ::flusso_query::FlussoValueMeta for #ident {
                            const KINDS: &'static [::flusso_query::KindTag] = &[#tag];
                            const VARIANTS: &'static [&'static str] = &[];
                        }
                        #leaf_check
                    })
                }
                // Inherit every kind the inner type has — as a bound for the
                // type-level check, and as data for the const-level one.
                None => Ok(quote! {
                    impl<__FlussoK> ::flusso_query::FlussoValue<__FlussoK> for #ident
                    where #inner: ::flusso_query::FlussoValue<__FlussoK> {}
                    impl ::flusso_query::FlussoValueMeta for #ident {
                        const KINDS: &'static [::flusso_query::KindTag] =
                            <#inner as ::flusso_query::FlussoValueMeta>::KINDS;
                        const VARIANTS: &'static [&'static str] =
                            <#inner as ::flusso_query::FlussoValueMeta>::VARIANTS;
                    }
                    #leaf_check
                }),
            }
        }
        Data::Union(_) => Err(syn::Error::new(
            input.ident.span(),
            "FlussoValue cannot be derived for a union",
        )),
    }
}

/// The no-op `__flusso_check` every value type carries.
///
/// A fragment cannot tell a *value* type (`Money`) from a *sub-fragment*
/// (`Address`) at macro time — both are just path types. So it treats every
/// custom type the same way: read its kinds from `FlussoValueMeta`, then recurse
/// with `T::__flusso_check(children(level, "…"))`. A value type is a leaf, so its
/// sub-level is empty and there is nothing to check — but the function has to
/// exist for the uniform treatment to compile.
fn leaf_check(ident: &syn::Ident) -> TokenStream {
    quote! {
        impl #ident {
            #[doc(hidden)]
            pub const fn __flusso_check(_level: &[::flusso_query::FieldSpec]) {}
        }
    }
}

/// Every variant as the *document* spells it: `#[serde(rename = "…")]` wins,
/// then the container `#[serde(rename_all = "…")]`, else the variant name.
///
/// This is what makes the schema's declared `variants:` comparable to the Rust
/// enum — the schema lists document strings, not Rust identifiers.
fn variant_keys(input: &DeriveInput, data: &syn::DataEnum) -> syn::Result<Vec<String>> {
    let rename_all = container_rename_all(input);
    data.variants
        .iter()
        .map(|variant| {
            if let Some(renamed) = serde_rename(&variant.attrs) {
                return Ok(renamed);
            }
            let base = variant.ident.to_string();
            Ok(match rename_all.as_deref() {
                Some(rule) => rename_variant(&base, rule),
                None => base,
            })
        })
        .collect()
}

fn container_rename_all(input: &DeriveInput) -> Option<String> {
    let mut rename_all = None;
    for attr in &input.attrs {
        if !attr.path().is_ident("serde") {
            continue;
        }
        // Best-effort: pull `rename_all`, ignore every other serde attribute.
        let _ = attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("rename_all")
                && let Ok(value) = meta.value()
                && let Ok(lit) = value.parse::<syn::LitStr>()
            {
                rename_all = Some(lit.value());
            }
            Ok(())
        });
    }
    rename_all
}

fn serde_rename(attrs: &[syn::Attribute]) -> Option<String> {
    let mut renamed = None;
    for attr in attrs {
        if !attr.path().is_ident("serde") {
            continue;
        }
        let _ = attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("rename")
                && let Ok(value) = meta.value()
                && let Ok(lit) = value.parse::<syn::LitStr>()
            {
                renamed = Some(lit.value());
            }
            Ok(())
        });
    }
    renamed
}

/// Apply a serde `rename_all` rule to a **variant** name.
///
/// Distinct from the field-side rule in `doc.rs`: a variant starts out
/// PascalCase (`NotFound`), a field snake_case (`not_found`), so the word split
/// differs. An unrecognized rule leaves the name alone, as serde does.
fn rename_variant(name: &str, rule: &str) -> String {
    let mut words: Vec<String> = Vec::new();
    for ch in name.chars() {
        if ch.is_ascii_uppercase() || words.is_empty() {
            words.push(String::new());
        }
        if let Some(word) = words.last_mut() {
            word.push(ch.to_ascii_lowercase());
        }
    }
    match rule {
        "lowercase" => words.concat(),
        "UPPERCASE" => words.concat().to_ascii_uppercase(),
        "PascalCase" => name.to_owned(),
        "camelCase" => words
            .iter()
            .enumerate()
            .map(|(i, word)| {
                if i == 0 {
                    word.clone()
                } else {
                    capitalize(word)
                }
            })
            .collect(),
        "snake_case" => words.join("_"),
        "SCREAMING_SNAKE_CASE" => words.join("_").to_ascii_uppercase(),
        "kebab-case" => words.join("-"),
        "SCREAMING-KEBAB-CASE" => words.join("-").to_ascii_uppercase(),
        _ => name.to_owned(),
    }
}

fn capitalize(word: &str) -> String {
    let mut chars = word.chars();
    match chars.next() {
        Some(first) => first.to_ascii_uppercase().to_string() + chars.as_str(),
        None => String::new(),
    }
}

fn newtype_required(input: &DeriveInput) -> syn::Error {
    syn::Error::new(
        input.ident.span(),
        "FlussoValue on a struct requires a single-field tuple struct \
         (a newtype wrapper, e.g. `struct Country(String)`)",
    )
}

/// Read an explicit `#[flusso(keyword | text)]` kind; `None` when absent. Only
/// the string kinds are nameable — numeric/date/bool newtypes inherit their
/// inner type's kinds instead (a single name can't capture lossless widening).
pub(crate) fn kind_attr(input: &DeriveInput) -> syn::Result<Option<Kind>> {
    let mut kind = None;
    for attr in &input.attrs {
        if !attr.path().is_ident("flusso") {
            continue;
        }
        attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("keyword") {
                kind = Some(Kind::Keyword);
            } else if meta.path.is_ident("text") {
                kind = Some(Kind::Text);
            } else {
                return Err(meta.error(
                    "unknown `flusso` kind (expected `keyword` or `text`; numeric/date \
                     newtypes inherit their inner type's kinds, so need no tag)",
                ));
            }
            Ok(())
        })?;
    }
    Ok(kind)
}

/// The kind for the `FlussoMap` derive — `#[flusso(keyword | text)]`, default
/// keyword. (Map value kinds beyond strings come from `HashMap<String, V>`'s `V`
/// via the blanket impl, not this derive.)
pub(crate) fn parse_kind(input: &DeriveInput) -> syn::Result<Kind> {
    Ok(kind_attr(input)?.unwrap_or(Kind::Keyword))
}

#[cfg(test)]
mod tests {
    use super::rename_variant;

    #[test]
    fn splits_a_pascal_case_variant_into_words() {
        assert_eq!(rename_variant("NotFound", "snake_case"), "not_found");
        assert_eq!(rename_variant("NotFound", "kebab-case"), "not-found");
        assert_eq!(rename_variant("NotFound", "camelCase"), "notFound");
    }

    #[test]
    fn handles_a_single_word_variant() {
        assert_eq!(rename_variant("Pro", "lowercase"), "pro");
        assert_eq!(rename_variant("Pro", "camelCase"), "pro");
        assert_eq!(rename_variant("Pro", "UPPERCASE"), "PRO");
    }

    #[test]
    fn pascal_case_keeps_the_rust_spelling() {
        assert_eq!(rename_variant("NotFound", "PascalCase"), "NotFound");
    }

    #[test]
    fn screaming_rules_upcase_the_separator_form() {
        assert_eq!(
            rename_variant("NotFound", "SCREAMING_SNAKE_CASE"),
            "NOT_FOUND"
        );
        assert_eq!(
            rename_variant("NotFound", "SCREAMING-KEBAB-CASE"),
            "NOT-FOUND"
        );
    }

    #[test]
    fn an_unknown_rule_leaves_the_name_alone() {
        assert_eq!(rename_variant("NotFound", "Train-Case"), "NotFound");
    }
}
