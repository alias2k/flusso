//! `#[derive(FlussoValue)]` — opts a Rust type into `flusso_query::FlussoValue<K>`,
//! so it may stand in for a field of kind `K` in a `FlussoDocument` struct.
//!
//! The kind comes from one of two places — there is **no default**:
//!
//! - `#[flusso(keyword)]` / `#[flusso(text)]` — **required** on an enum with
//!   only unit variants (`Pro`/`Enterprise`/`Free`, serializing to strings);
//!   on a newtype it restricts the wrapper to that single string kind.
//! - An untagged **newtype** inherits all of its inner type's kinds
//!   (`struct Money(Decimal)` is a decimal value). Numeric/date kinds are only
//!   reachable this way: they are split per type for lossless widening, which
//!   a single tag can't name.
//!
//! On success it emits `impl ::flusso_query::FlussoValue<#kind> for #ident {}`.
//! The leaf value's actual serde form is enforced by serde at the boundary;
//! this derive guarantees the *shape* fits the kind.
//!
//! An enum may add `exhaustive` (`#[flusso(keyword, exhaustive)]`): every
//! embedding then requires the enum to cover the schema's **whole** declared
//! variant set, not just a subset — a schema variant the enum misses becomes a
//! compile error instead of a silently unselectable value. The flag rides
//! `FlussoValueMeta::EXHAUSTIVE`; an untagged newtype forwards its inner
//! type's flag.

use proc_macro2::TokenStream;
use quote::{quote, quote_spanned};
use syn::ext::IdentExt;
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

    /// The tag as written in `#[flusso(…)]`, for the string kinds.
    pub(crate) fn name(self) -> &'static str {
        match self {
            Kind::Keyword => "keyword",
            Kind::Text => "text",
            _ => "",
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

    let attrs = match value_attrs(&input) {
        Ok(attrs) => attrs,
        Err(error) => return error.to_compile_error(),
    };

    match build_impl(&input, attrs) {
        Ok(tokens) => tokens,
        Err(error) => error.to_compile_error(),
    }
}

/// The impl a `FlussoValue` derive emits. A **newtype with no explicit kind**
/// inherits *all* of its inner type's kinds (a blanket impl forwarding to the
/// field type) — so `struct Pippo(String)` is a keyword **and** text value, and
/// `struct Money(Decimal)` a decimal value, with no annotation. An explicit
/// `#[flusso(keyword | text)]` restricts to that single string kind; an
/// **enum** has no inner type to inherit from, so it must always carry one.
///
/// An `exhaustive` token is enum-only: it sets `FlussoValueMeta::EXHAUSTIVE`,
/// which every embedding turns into "this enum must cover the schema's whole
/// declared variant set". A newtype forwards its inner type's flag on the
/// untagged path; an explicit `exhaustive` on a struct is an error.
fn build_impl(input: &DeriveInput, attrs: ValueAttrs) -> syn::Result<TokenStream> {
    let ident = &input.ident;
    let explicit = attrs.kind;
    match &input.data {
        Data::Enum(data) => {
            let Some(kind) = explicit else {
                return Err(syn::Error::new(
                    input.ident.span(),
                    "an enum FlussoValue has no default kind — add `#[flusso(keyword)]` \
                     (exact string) or `#[flusso(text)]` (analyzed full-text)",
                ));
            };
            if !kind.is_string() {
                return Err(syn::Error::new(
                    input.ident.span(),
                    "an enum FlussoValue is string-valued — use `#[flusso(keyword)]` \
                     or `#[flusso(text)]`",
                ));
            }
            for variant in &data.variants {
                unread_attr(
                    &variant.attrs,
                    "an enum variant",
                    "the kind tag goes on the enum (`#[flusso(keyword)] enum …`), and a \
                     variant's document spelling comes from `#[serde(rename = \"…\")]`",
                )?;
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
            let exhaustive = attrs.exhaustive.map(|_| {
                quote! { const EXHAUSTIVE: bool = true; }
            });
            Ok(quote! {
                impl ::flusso_query::FlussoValue<#marker> for #ident {}
                impl ::flusso_query::FlussoValueMeta for #ident {
                    const KINDS: &'static [::flusso_query::KindTag] = &[#tag];
                    const VARIANTS: &'static [&'static str] = &[#(#variants),*];
                    #exhaustive
                }
                #leaf_check
            })
        }
        Data::Struct(data) => {
            if let Some(span) = attrs.exhaustive {
                return Err(syn::Error::new(
                    span,
                    "`exhaustive` applies to an enum — only an enum declares variants \
                     to cover; an untagged newtype inherits its inner type's \
                     exhaustiveness",
                ));
            }
            // A transparent struct is read as its one field serde does not
            // pass over; a plain tuple struct must have just the one.
            let transparent = crate::serde_attr::transparent(&input.attrs);
            if !transparent && !matches!(data.fields, Fields::Unnamed(_)) {
                return Err(newtype_required(input));
            }
            let mut fields = data.fields.iter().filter(|field| {
                !transparent || !crate::serde_attr::passed_over_by_transparent(field)
            });
            let inner = match (fields.next(), fields.next()) {
                (Some(field), None) => &field.ty,
                _ => return Err(newtype_required(input)),
            };
            for field in &data.fields {
                if transparent && crate::serde_attr::passed_over_by_transparent(field) {
                    if let Some(attr) = field.attrs.iter().find(|a| a.path().is_ident("flusso")) {
                        return Err(syn::Error::new_spanned(attr, crate::doc::PASSED_OVER));
                    }
                    continue;
                }
                unread_attr(
                    &field.attrs,
                    "the newtype's field",
                    "a kind tag goes on the type (`#[flusso(keyword)] struct Code(String)`)",
                )?;
            }
            let leaf_check = leaf_check(ident);
            if let Some(element) = crate::doc::vec_inner(inner) {
                return array_newtype(ident, element, explicit.is_some(), leaf_check);
            }
            if let Some(element) = crate::doc::option_inner(inner).and_then(crate::doc::vec_inner) {
                return Err(syn::Error::new(
                    inner.span(),
                    format!(
                        "a FlussoValue newtype over an array wraps the `Vec<…>` directly — write \
                         `struct {ident}(Vec<{}>)`, and put the `Option<…>` on the field \
                         (`Option<{ident}>`) when the schema field is nullable",
                        crate::fragment::render(element),
                    ),
                ));
            }
            let rendered = crate::fragment::render(inner);
            match explicit {
                // Restrict to one string kind (e.g. a keyword-only code wrapper).
                Some(kind) => {
                    let marker = kind.marker();
                    let tag = kind.tag();
                    let array_message = format!(
                        "`{ident}` has a kind tag, so it stands for one value, but `{rendered}` \
                         reads an array — drop the tag, and `{ident}` reads the array as \
                         `{rendered}` does"
                    );
                    let object_message = format!(
                        "`{ident}` has a kind tag, so it stands for one value, but `{rendered}` \
                         reads an object — derive `FlussoMap` on `{ident}` for a map, or \
                         `FlussoFragment` for a sub-document"
                    );
                    let array_check = quote_spanned! {inner.span()=>
                        const _: () = {
                            #[allow(unused_imports)]
                            use ::flusso_query::OpaqueWithoutMeta as _;
                            assert!(
                                !::flusso_query::OpaqueProbe::<#inner>::ARRAY,
                                #array_message
                            );
                            assert!(
                                !::flusso_query::OpaqueProbe::<#inner>::HAS_META
                                    || !::flusso_query::OpaqueProbe::<#inner>::AT_OBJECT,
                                #object_message
                            );
                        };
                    };
                    Ok(quote! {
                        impl ::flusso_query::FlussoValue<#marker> for #ident {}
                        impl ::flusso_query::FlussoValueMeta for #ident {
                            const KINDS: &'static [::flusso_query::KindTag] = &[#tag];
                            const VARIANTS: &'static [&'static str] = &[];
                        }
                        #array_check
                        #leaf_check
                    })
                }
                // Inherit every kind the inner type has — as a bound for the
                // type-level check, and as data for the const-level one. An
                // array newtype's `ARRAY` is forwarded with them, so the
                // wrapper is placed where the array newtype is; a sub-document
                // is refused, since a value's own check walks into nothing.
                None => {
                    let sub_document_message = format!(
                        "`{ident}` derives `FlussoValue`, but `{rendered}` is a sub-document, \
                         whose fields only a `FlussoFragment` checks — derive `FlussoFragment` \
                         on `{ident}` instead"
                    );
                    let sub_document_check = quote_spanned! {inner.span()=>
                        const _: () = {
                            #[allow(unused_imports)]
                            use ::flusso_query::OpaqueWithoutMeta as _;
                            assert!(
                                !::flusso_query::OpaqueProbe::<#inner>::HAS_META
                                    || !::flusso_query::OpaqueProbe::<#inner>::AT_NESTED,
                                #sub_document_message
                            );
                        };
                    };
                    Ok(quote! {
                        // Both bounds are higher-ranked, which the compiler
                        // checks only where the impl is used, not eagerly as a
                        // bound naming no generic: over an array newtype, which
                        // has no `FlussoValue<K>` (nor `Serialize`, the trait's
                        // supertrait), the newtype then needs no `Serialize`,
                        // as one over a `Vec` needs none.
                        impl<__FlussoK> ::flusso_query::FlussoValue<__FlussoK> for #ident
                        where
                            for<'__flusso> #inner: ::flusso_query::FlussoValue<__FlussoK>,
                            for<'__flusso> #ident: ::flusso_query::__serde::Serialize,
                        {}
                        impl ::flusso_query::FlussoValueMeta for #ident {
                            const KINDS: &'static [::flusso_query::KindTag] =
                                <#inner as ::flusso_query::FlussoValueMeta>::KINDS;
                            const VARIANTS: &'static [&'static str] =
                                <#inner as ::flusso_query::FlussoValueMeta>::VARIANTS;
                            const EXHAUSTIVE: bool =
                                <#inner as ::flusso_query::FlussoValueMeta>::EXHAUSTIVE;
                            const ARRAY: bool =
                                <#inner as ::flusso_query::FlussoValueMeta>::ARRAY;
                            const MAP_VALUES: &'static [::flusso_query::KindTag] =
                                <#inner as ::flusso_query::FlussoValueMeta>::MAP_VALUES;
                        }
                        // A newtype over a map type stands where the map does.
                        impl<__FlussoK> ::flusso_query::FlussoMap<__FlussoK> for #ident
                        where
                            for<'__flusso> #inner: ::flusso_query::FlussoMap<__FlussoK>,
                        {}
                        #sub_document_check
                        #leaf_check
                    })
                }
            }
        }
        Data::Union(_) => Err(syn::Error::new(
            input.ident.span(),
            "FlussoValue cannot be derived for a union",
        )),
    }
}

/// A newtype over `Vec<E>`: it stands for a flat array whose elements are `E`
/// values. It carries `E`'s kinds, variants and exhaustiveness as metadata
/// with `ARRAY` set, which an array field reads to accept it, and no
/// `FlussoValue<K>`, so query operators keep taking an element. A kind tag
/// would restrict nothing the element does not already say, so it is refused.
///
/// `E` must be a leaf value that reads no array: a `Vec` of arrays fits no
/// array leaf, and a sub-document's fields are checked only by a
/// `FlussoFragment` newtype, which a value's no-op check never walks into.
/// Both are const data, so they are asserted where the newtype is declared.
fn array_newtype(
    ident: &syn::Ident,
    element: &syn::Type,
    tagged: bool,
    leaf_check: TokenStream,
) -> syn::Result<TokenStream> {
    if tagged {
        return Err(syn::Error::new(
            ident.span(),
            "a FlussoValue newtype over a `Vec<…>` takes its kinds from the element — drop the \
             kind tag, and put it on the element type",
        ));
    }
    let rendered = crate::fragment::render(element);
    let nested_array = format!(
        "`{ident}` is a newtype over `Vec<{rendered}>`, an array of arrays — `{rendered}` already \
         reads an array; wrap its element type instead"
    );
    let sub_document = format!(
        "`{ident}` is a FlussoValue newtype over `Vec<{rendered}>`, but `{rendered}` is a \
         sub-document, not a value — for a `nested` array, write `Vec<{rendered}>` or derive \
         `FlussoFragment` on a `#[serde(transparent)]` newtype"
    );
    let object = format!(
        "`{ident}` is a FlussoValue newtype over `Vec<{rendered}>`, but `{rendered}` stands for \
         an `object`, not a leaf value — a `json` array takes `Vec<{rendered}>` as it is"
    );
    // Read through `OpaqueProbe` behind `HAS_META`: an element without
    // metadata already fails on `KINDS` below, with the trait's note.
    let element_check = quote_spanned! {element.span()=>
        const _: () = {
            #[allow(unused_imports)]
            use ::flusso_query::OpaqueWithoutMeta as _;
            let meta = ::flusso_query::OpaqueProbe::<#element>::HAS_META;
            assert!(
                !meta || !::flusso_query::OpaqueProbe::<#element>::ARRAY,
                #nested_array
            );
            assert!(
                !meta || !::flusso_query::OpaqueProbe::<#element>::AT_NESTED,
                #sub_document
            );
            assert!(!meta || !::flusso_query::OpaqueProbe::<#element>::AT_OBJECT, #object);
        };
    };
    Ok(quote! {
        #element_check
        impl ::flusso_query::FlussoValueMeta for #ident {
            const KINDS: &'static [::flusso_query::KindTag] =
                <#element as ::flusso_query::FlussoValueMeta>::KINDS;
            const VARIANTS: &'static [&'static str] =
                <#element as ::flusso_query::FlussoValueMeta>::VARIANTS;
            const EXHAUSTIVE: bool =
                <#element as ::flusso_query::FlussoValueMeta>::EXHAUSTIVE;
            const ARRAY: bool = true;
        }
        #leaf_check
    })
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
    let rename_all = crate::serde_attr::rename_all(&input.attrs);
    data.variants
        .iter()
        .map(|variant| {
            if let Some(renamed) = crate::serde_attr::variant_rename(&variant.attrs) {
                return Ok(renamed);
            }
            let base = variant.ident.unraw().to_string();
            Ok(match rename_all.as_deref() {
                Some(rule) => rename_variant(&base, rule),
                None => base,
            })
        })
        .collect()
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
         (a newtype wrapper, e.g. `struct Country(String)`), or a \
         `#[serde(transparent)]` struct with one field serde does not skip",
    )
}

/// The type-site `#[flusso(…)]` tokens a value type may carry.
pub(crate) struct ValueAttrs {
    /// The explicit string kind (`keyword` | `text`), when tagged.
    pub(crate) kind: Option<Kind>,
    /// The span of an `exhaustive` token, when present.
    pub(crate) exhaustive: Option<proc_macro2::Span>,
}

/// Read the type-site attr: an explicit `#[flusso(keyword | text)]` kind (only
/// the string kinds are nameable — numeric/date/bool newtypes inherit their
/// inner type's kinds instead, a single name can't capture lossless widening)
/// plus an optional `exhaustive` token (full variant coverage, enums only).
pub(crate) fn value_attrs(input: &DeriveInput) -> syn::Result<ValueAttrs> {
    let mut attrs = ValueAttrs {
        kind: None,
        exhaustive: None,
    };
    for attr in &input.attrs {
        if !attr.path().is_ident("flusso") {
            continue;
        }
        attr.parse_nested_meta(|meta| {
            let kind = if meta.path.is_ident("keyword") {
                Kind::Keyword
            } else if meta.path.is_ident("text") {
                Kind::Text
            } else if meta.path.is_ident("exhaustive") {
                crate::once(&meta, attrs.exhaustive.is_some(), "")?;
                attrs.exhaustive = Some(meta.path.span());
                return Ok(());
            } else {
                return Err(meta.error(
                    "unknown `flusso` token (expected `keyword` or `text` — numeric/date \
                     newtypes inherit their inner type's kinds, so need no tag — or \
                     `exhaustive` on an enum)",
                ));
            };
            match attrs.kind {
                Some(seen) if seen.name() != kind.name() => {
                    return Err(meta.error(format!(
                        "`{}` and `{}` are both given, and a type has one kind — keep the \
                         one the schema field is (`keyword` for an exact string, `text` for \
                         analyzed full text)",
                        seen.name(),
                        kind.name(),
                    )));
                }
                seen => crate::once(&meta, seen.is_some(), "")?,
            }
            attrs.kind = Some(kind);
            Ok(())
        })?;
    }
    Ok(attrs)
}

/// A `#[flusso(…)]` among `attrs` of a part of the type the derive never reads
/// (a newtype's field, an enum variant), as an error saying where it goes.
pub(crate) fn unread_attr(attrs: &[syn::Attribute], what: &str, instead: &str) -> syn::Result<()> {
    match attrs.iter().find(|attr| attr.path().is_ident("flusso")) {
        Some(attr) => Err(syn::Error::new_spanned(
            attr,
            format!("`#[flusso(…)]` on {what} is never read — {instead}; remove it"),
        )),
        None => Ok(()),
    }
}

/// The kind for the `FlussoMap` derive — a **required** `#[flusso(keyword | text)]`
/// matching the schema map's `values:`. (Map value kinds beyond strings come from
/// `HashMap<String, V>`'s `V` via the blanket impl, not this derive.)
pub(crate) fn parse_kind(input: &DeriveInput) -> syn::Result<Kind> {
    let attrs = value_attrs(input)?;
    if let Some(span) = attrs.exhaustive {
        return Err(syn::Error::new(
            span,
            "`exhaustive` applies to a FlussoValue enum — a map has no variant \
             set to cover",
        ));
    }
    attrs.kind.ok_or_else(|| {
        syn::Error::new(
            input.ident.span(),
            "FlussoMap has no default value kind — add `#[flusso(keyword)]` or \
             `#[flusso(text)]`, matching the schema map's `values:`",
        )
    })
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
