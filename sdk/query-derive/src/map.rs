//! `#[derive(FlussoMap)]` — opts a whole-map newtype wrapper into
//! `flusso_query::FlussoMap<K>`, so it may stand in for a `map` field of value
//! kind `K` in a `FlussoDocument` struct.
//!
//! Most map fields need nothing here — `HashMap<String, V>` already implements
//! `FlussoMap<K>` (via a blanket impl) when `V` is a `K` value. This derive is
//! for a type of your own that stands in for a map — a newtype around one, or a
//! named-field struct that serialises as a flat object of same-kind values:
//!
//! ```ignore
//! #[derive(serde::Deserialize, FlussoMap)]
//! #[flusso(text)]
//! struct Translations(std::collections::HashMap<String, String>);
//! ```
//!
//! The kind tag is **required** — `#[flusso(keyword)]` or `#[flusso(text)]`,
//! the same attribute as `FlussoValue`, matching the schema map's `values:`;
//! omitting it is a compile error, there is no default. Number/date maps
//! use `HashMap`/`BTreeMap` directly: their value kinds are split per type
//! (`Byte`…`Decimal`, `Date`), which a single tag can't name.
//! A map type reads one JSON object of values of its tag, so a shape that
//! certainly reads something else is refused, and what it holds is checked
//! against the tag: a wrapped map's value type, or each named field (see
//! [`reads_an_object`] and [`named_fields`]). It emits
//! `impl ::flusso_query::FlussoMap<#kind> for #ident {}` plus the marker a
//! fragment's embed check reads (`FlussoValueMeta` + a no-op `__flusso_check`),
//! which is why deriving beats a hand-written `impl FlussoMap<K>`: the hand-written
//! one carries no marker, so a fragment embedding it fails to compile.

use proc_macro2::TokenStream;
use quote::quote;
use syn::spanned::Spanned;
use syn::{Data, DeriveInput, Type};

use crate::value::{Kind, parse_kind};

pub(crate) fn expand(input: DeriveInput) -> TokenStream {
    if !input.generics.params.is_empty() {
        return syn::Error::new(
            input.generics.span(),
            "FlussoMap does not support generic types",
        )
        .to_compile_error();
    }

    let kind = match parse_kind(&input) {
        Ok(kind) => kind,
        Err(error) => return error.to_compile_error(),
    };

    if let Err(error) = validate(&input) {
        return error.to_compile_error();
    }
    if let Some(attr) = crate::serde_attr::converts_from(&input.attrs) {
        return syn::Error::new_spanned(
            attr,
            "`#[serde(from)]`/`try_from` makes serde read another type and convert it, so this \
             type's shape is never what serde reads; derive `FlussoMap` on the type serde reads, \
             and convert after reading",
        )
        .to_compile_error();
    }
    // A refused shape still gets its impls, so a field that embeds it adds
    // no second error about a missing `FlussoMap`.
    let object_check =
        reads_an_object(&input, kind).unwrap_or_else(|error| error.to_compile_error());

    let ident = &input.ident;
    let marker = kind.marker();
    let tag = kind.tag();
    quote! {
        impl ::flusso_query::FlussoMap<#marker> for #ident {}

        // A map wrapper is an `object` in the mapping, and a leaf as far as the
        // embed check goes — its keys are dynamic, so there is no sub-level to
        // walk. Both items exist so a parent can treat every custom type alike.
        impl ::flusso_query::FlussoValueMeta for #ident {
            const KINDS: &'static [::flusso_query::KindTag] =
                &[::flusso_query::KindTag::Object];
            const VARIANTS: &'static [&'static str] = &[];
            // Carries the declared value kind, so a fragment embedding this
            // checks the map's values, not just that the field is an object.
            const MAP_VALUES: &'static [::flusso_query::KindTag] = &[#tag];
        }

        impl #ident {
            #[doc(hidden)]
            pub const fn __flusso_check(_level: &[::flusso_query::FieldSpec]) {}
        }

        #object_check
    }
}

/// A `FlussoMap` type must be a **struct**: a newtype over a map, or a
/// named-field struct (language keys plus a `fallback`, say), which
/// [`named_fields`] checks field by field.
fn validate(input: &DeriveInput) -> syn::Result<()> {
    match &input.data {
        Data::Struct(data) => data.fields.iter().try_for_each(|field| {
            crate::value::unread_attr(
                &field.attrs,
                "a field of a `FlussoMap` type",
                "the value kind goes on the type (`#[flusso(text)] struct Translations(…)`), \
                 and every field holds a value of that kind",
            )
        }),
        Data::Enum(_) => Err(syn::Error::new(
            input.ident.span(),
            "FlussoMap cannot be derived for an enum — a map is an object of same-kind \
             values, not a set of variants",
        )),
        Data::Union(_) => Err(syn::Error::new(
            input.ident.span(),
            "FlussoMap cannot be derived for a union",
        )),
    }
}

/// A map type reads one JSON object of values of its tag. The derive refuses
/// what certainly reads something else: a unit struct, a tuple struct of
/// several fields, and a newtype (tuple or `#[serde(transparent)]`) over an
/// `Option`, a `Vec` or a primitive leaf at macro time; over a value type, an
/// array newtype or a sub-document through const data, the assert spanned on
/// the inner type. What is left must hold values of the tag:
///
/// - over a `HashMap<String, V>`/`BTreeMap<String, V>`, `V` is a value of the
///   tag (a `FlussoValue` bound);
/// - over any other type, that type is a map of the tag (its
///   `FlussoValueMeta::MAP_VALUES`): a type without flusso metadata (`Box<…>`,
///   a plain struct) is one the derive cannot see into, so it is refused;
/// - over `serde_json::Value`, anything goes.
///
/// A struct with named fields is checked by [`named_fields`].
fn reads_an_object(input: &DeriveInput, kind: Kind) -> syn::Result<TokenStream> {
    let ident = &input.ident;
    let Data::Struct(data) = &input.data else {
        return Ok(TokenStream::new());
    };
    let transparent = crate::serde_attr::transparent(&input.attrs);
    let read: Vec<&syn::Field> = data
        .fields
        .iter()
        .filter(|field| !transparent || !crate::serde_attr::passed_over_by_transparent(field))
        .collect();
    let refuse = |span: proc_macro2::Span, reads: &str| {
        syn::Error::new(
            span,
            format!(
                "a FlussoMap type reads one JSON object, but `{ident}` reads {reads} — wrap a \
                 `HashMap<String, V>`/`BTreeMap<String, V>`, or declare named fields"
            ),
        )
    };
    let inner = match (&data.fields, read.as_slice()) {
        (syn::Fields::Unit, _) => return Err(refuse(ident.span(), "no value")),
        (syn::Fields::Named(_), _) if !transparent => return named_fields(input, data, kind),
        (_, [field]) => &field.ty,
        (syn::Fields::Unnamed(_), _) => return Err(refuse(data.fields.span(), "an array")),
        _ => return Ok(TokenStream::new()),
    };
    if crate::doc::option_inner(inner).is_some() {
        return Err(refuse(inner.span(), "`null` too, through its `Option`"));
    }
    if crate::doc::vec_inner(inner).is_some() {
        return Err(refuse(inner.span(), "an array"));
    }
    if let Some(error) = crate::doc::non_string_key(inner) {
        return Err(error);
    }
    if let Some(value) = crate::doc::map_container_value(inner) {
        return Ok(value_check(value, kind));
    }
    let name = crate::doc::leaf_ident(inner);
    if name.as_deref() == Some("Value") {
        return Ok(TokenStream::new());
    }
    if crate::doc::is_primitive(name.as_deref()) {
        return Err(refuse(inner.span(), "a single value"));
    }
    let rendered = crate::fragment::render(inner);
    let array = format!(
        "a FlussoMap type reads one JSON object, but `{ident}` wraps `{rendered}`, which reads an \
         array — wrap a `HashMap<String, V>`/`BTreeMap<String, V>`, or declare named fields"
    );
    let value = format!(
        "a FlussoMap type reads one JSON object, but `{ident}` wraps `{rendered}`, which reads a \
         single value — wrap a `HashMap<String, V>`/`BTreeMap<String, V>`, or declare named \
         fields"
    );
    let sub_document = format!(
        "a FlussoMap type reads one JSON object of dynamic keys, but `{ident}` wraps `{rendered}`, \
         a sub-document, whose fields serde requires — declare the fields you need on `{ident}` \
         itself as `Option<…>` (a map may lack any key), beside a `#[serde(flatten)]` map for \
         the rest"
    );
    let map = not_a_map_of(&format!("`{ident}` wraps `{rendered}`"), kind);
    let tag = kind.tag();
    Ok(quote::quote_spanned! {inner.span()=>
        const _: () = {
            #[allow(unused_imports)]
            use ::flusso_query::OpaqueWithoutMeta as _;
            assert!(!::flusso_query::OpaqueProbe::<#inner>::ARRAY, #array);
            assert!(::flusso_query::OpaqueProbe::<#inner>::AT_OBJECT, #value);
            assert!(
                !::flusso_query::OpaqueProbe::<#inner>::HAS_META
                    || !::flusso_query::OpaqueProbe::<#inner>::AT_NESTED,
                #sub_document
            );
            assert!(
                ::flusso_query::accepts_kind(::flusso_query::OpaqueProbe::<#inner>::MAP_VALUES, #tag),
                #map
            );
        };
    })
}

/// A named-field `FlussoMap` struct reads one object whose keys are the
/// field names, so each field holds one of the map's values. A map may lack
/// any key and hold any other, so:
///
/// - `#[serde(deny_unknown_fields)]` is refused (it fails on any other key),
///   and so is a field's `#[serde(alias)]` (a map may hold both keys);
/// - every map key is `String` (see [`crate::doc::non_string_key`]);
/// - a field reads `Option<V>`, `V` a value of the tag (`serde_json::Value`
///   takes anything);
/// - a `#[serde(flatten)]` field takes the keys no other field reads, so it
///   is a map of the tag, checked as a newtype's map is.
///
/// A field serde never reads (`skip`) or reads through a function of its own
/// (`with`, `deserialize_with`) is not checked.
fn named_fields(
    input: &DeriveInput,
    data: &syn::DataStruct,
    kind: Kind,
) -> syn::Result<TokenStream> {
    let ident = &input.ident;
    if let Some(attr) = crate::serde_attr::denies_unknown_fields(&input.attrs) {
        return Err(syn::Error::new_spanned(
            attr,
            format!(
                "a map holds any key, and `deny_unknown_fields` makes `{ident}` fail on every key \
                 it does not name — remove it; a `#[serde(flatten)]` \
                 `HashMap<String, String>` field keeps the other keys"
            ),
        ));
    }
    let mut checks = TokenStream::new();
    let mut errors: Option<syn::Error> = None;
    let mut push = |error: syn::Error| match &mut errors {
        Some(errors) => errors.combine(error),
        None => errors = Some(error),
    };
    for field in &data.fields {
        if crate::serde_attr::skips_deserializing(&field.attrs)
            || crate::serde_attr::reads_with(&field.attrs)
        {
            continue;
        }
        let name = field
            .ident
            .as_ref()
            .map_or_else(String::new, ToString::to_string);
        let rendered = crate::fragment::render(&field.ty);
        if let Some(error) = crate::doc::non_string_key(&field.ty) {
            push(error);
            continue;
        }
        if let Some(alias) = crate::serde_attr::alias(&field.attrs) {
            push(syn::Error::new_spanned(
                alias,
                format!(
                    "field `{name}` of the FlussoMap type `{ident}` reads its key and an alias, \
                     but a map may hold both keys, and serde then fails with `duplicate field` — \
                     remove the alias; a `#[serde(flatten)]` `HashMap<String, String>` field \
                     keeps the other key"
                ),
            ));
            continue;
        }
        if crate::serde_attr::flatten(&field.attrs) {
            match flattened_map(ident, &field.ty, kind) {
                Ok(check) => checks.extend(check),
                Err(error) => push(error),
            }
            continue;
        }
        let Some(value) = crate::doc::option_inner(&field.ty) else {
            push(syn::Error::new(
                field.ty.span(),
                format!(
                    "field `{name}` of the FlussoMap type `{ident}` holds one key of a map, and a \
                     map may lack any key — declare it `Option<{rendered}>`"
                ),
            ));
            continue;
        };
        checks.extend(value_check(value, kind));
    }
    match errors {
        Some(errors) => Err(errors),
        None => Ok(checks),
    }
}

/// A `#[serde(flatten)]` field of a named-field map type: it reads every key
/// no other field reads, so it is a map of the tag.
fn flattened_map(ident: &syn::Ident, ty: &Type, kind: Kind) -> syn::Result<TokenStream> {
    let inner = crate::doc::option_inner(ty).unwrap_or(ty);
    if let Some(value) = crate::doc::map_container_value(inner) {
        return Ok(value_check(value, kind));
    }
    let name = crate::doc::leaf_ident(inner);
    if name.as_deref() == Some("Value") {
        return Ok(TokenStream::new());
    }
    let rendered = crate::fragment::render(inner);
    if crate::doc::vec_inner(inner).is_some() || crate::doc::is_primitive(name.as_deref()) {
        return Err(syn::Error::new(
            inner.span(),
            format!(
                "`{rendered}` is `#[serde(flatten)]` in the FlussoMap type `{ident}`, where it \
                 reads the keys no other field reads, and serde flattens only a struct or a \
                 map — declare it `HashMap<String, String>`"
            ),
        ));
    }
    let map = not_a_map_of(
        &format!("`{ident}` flattens `{rendered}`, which reads the keys no other field reads"),
        kind,
    );
    let tag = kind.tag();
    Ok(quote::quote_spanned! {inner.span()=>
        const _: () = {
            #[allow(unused_imports)]
            use ::flusso_query::OpaqueWithoutMeta as _;
            assert!(
                ::flusso_query::accepts_kind(::flusso_query::OpaqueProbe::<#inner>::MAP_VALUES, #tag),
                #map
            );
        };
    })
}

/// The message for a type in a map that is no map of the tag's values.
fn not_a_map_of(what: &str, kind: Kind) -> String {
    let tag = kind.name();
    format!(
        "a `#[flusso({tag})]` FlussoMap type holds `{tag}` values, but {what}, which is not a \
         map of `{tag}` values the derive can see into — use a `HashMap<String, String>`, or a \
         type that derives `FlussoMap` with `#[flusso({tag})]`"
    )
}

/// That `value`, one value of the map, is a value of the tag: a
/// `FlussoValue` bound, spanned on it. `serde_json::Value` takes anything.
fn value_check(value: &Type, kind: Kind) -> TokenStream {
    if crate::doc::leaf_ident(value).as_deref() == Some("Value") {
        return TokenStream::new();
    }
    let marker = kind.marker();
    quote::quote_spanned! {value.span()=>
        const _: fn() = || {
            fn __flusso_map_value<__T: ::flusso_query::FlussoValue<#marker>>() {}
            __flusso_map_value::<#value>();
        };
    }
}
