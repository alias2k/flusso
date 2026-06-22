//! `#[derive(FlussoMap)]` — opts a whole-map newtype wrapper into
//! `flusso_query::FlussoMap<K>`, so it may stand in for a `map` field of value
//! kind `K` in a `FlussoDocument` struct.
//!
//! Most map fields need nothing here — `HashMap<String, V>` already implements
//! `FlussoMap<K>` (via a blanket impl) when `V` is a `K` value. This derive is
//! for wrapping that map in a newtype:
//!
//! ```ignore
//! #[derive(serde::Deserialize, FlussoMap)]
//! #[flusso(text)]
//! struct Translations(std::collections::HashMap<String, String>);
//! ```
//!
//! The kind is chosen with `#[flusso(keyword)]` (the default), `#[flusso(text)]`,
//! `#[flusso(number)]`, or `#[flusso(date)]` — the same attribute as
//! `FlussoValue`. The type must be a single-field tuple struct (a newtype over a
//! map); on success it emits `impl ::flusso_query::FlussoMap<#kind> for #ident {}`.

use proc_macro2::TokenStream;
use quote::quote;
use syn::spanned::Spanned;
use syn::{Data, DeriveInput, Fields};

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
    quote! {
        impl ::flusso_query::FlussoMap<#marker> for #ident {}
    }
}

/// A `FlussoMap` type must be a single-field tuple struct wrapping a map — the
/// value kind is checked structurally by the trait bound at the use site, so
/// here we only enforce the newtype shape.
fn validate(input: &DeriveInput) -> syn::Result<()> {
    match &input.data {
        Data::Struct(data) => match &data.fields {
            Fields::Unnamed(fields) if fields.unnamed.len() == 1 => Ok(()),
            _ => Err(syn::Error::new(
                input.ident.span(),
                "FlussoMap requires a single-field tuple struct (a newtype over a map, \
                 e.g. `struct Translations(std::collections::HashMap<String, String>)`)",
            )),
        },
        Data::Enum(_) => Err(syn::Error::new(
            input.ident.span(),
            "FlussoMap cannot be derived for an enum — a map wraps `HashMap<String, V>`, \
             not a set of variants",
        )),
        Data::Union(_) => Err(syn::Error::new(
            input.ident.span(),
            "FlussoMap cannot be derived for a union",
        )),
    }
}
