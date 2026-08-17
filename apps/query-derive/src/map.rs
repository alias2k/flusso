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
//! The kind is chosen with `#[flusso(keyword)]` (the default), `#[flusso(text)]`,
//! `#[flusso(number)]`, or `#[flusso(date)]` — the same attribute as
//! `FlussoValue`. Any struct shape is accepted; on success it emits
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
    }
}

/// A `FlussoMap` type must be a **struct**. Usually a newtype over a map, but a
/// named-field struct is fine too — a type that serialises as a flat object of
/// same-kind values is a map on disk however it is spelled in Rust (language
/// keys plus a `fallback`, say). The value kind is checked at the use site
/// against the schema, so the shape here carries no extra information.
fn validate(input: &DeriveInput) -> syn::Result<()> {
    match &input.data {
        Data::Struct(_) => Ok(()),
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
