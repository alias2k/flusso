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
//! A map type reads one JSON object, so a shape that certainly reads something
//! else is refused (see [`reads_an_object`]); any other struct is accepted, and
//! on success it emits
//! `impl ::flusso_query::FlussoMap<#kind> for #ident {}` plus the marker a
//! fragment's embed check reads (`FlussoValueMeta` + a no-op `__flusso_check`),
//! which is why deriving beats a hand-written `impl FlussoMap<K>`: the hand-written
//! one carries no marker, so a fragment embedding it fails to compile.

use proc_macro2::TokenStream;
use quote::quote;
use syn::spanned::Spanned;
use syn::{Data, DeriveInput};

use crate::value::parse_kind;

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
    let object_check = match reads_an_object(&input) {
        Ok(check) => check,
        Err(error) => return error.to_compile_error(),
    };

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

/// A `FlussoMap` type must be a **struct**. Usually a newtype over a map, but a
/// named-field struct is fine too — a type that serialises as a flat object of
/// same-kind values is a map on disk however it is spelled in Rust (language
/// keys plus a `fallback`, say). The value kind is checked at the use site
/// against the schema, so the shape here carries no extra information.
fn validate(input: &DeriveInput) -> syn::Result<()> {
    match &input.data {
        Data::Struct(data) => data.fields.iter().try_for_each(|field| {
            crate::value::unread_attr(
                &field.attrs,
                "a field of a `FlussoMap` type",
                "the value kind goes on the type (`#[flusso(text)] struct Translations(…)`), \
                 and the fields are never checked one by one",
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

/// A map type reads one JSON object. The derive can't see what serde reads for
/// any struct, but it refuses what certainly reads something else: a unit
/// struct, a tuple struct of several fields, and a newtype (tuple or
/// `#[serde(transparent)]`) over an `Option`, a `Vec` or a primitive leaf at
/// macro time; over a value type or an array newtype through const data, the
/// assert spanned on the inner type, as over a derived sub-document, whose
/// fields serde requires where a map has dynamic keys. A newtype over a
/// `HashMap`/`BTreeMap`, another map type or `serde_json::Value`, and a struct
/// with named fields, are accepted.
fn reads_an_object(input: &DeriveInput) -> syn::Result<TokenStream> {
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
        (syn::Fields::Named(_), _) if !transparent => return Ok(TokenStream::new()),
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
    if crate::doc::map_container_value(inner).is_some() {
        return Ok(TokenStream::new());
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
         itself, beside a `#[serde(flatten)]` map for the rest"
    );
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
        };
    })
}
