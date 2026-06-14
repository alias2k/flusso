//! `#[derive(FlussoValue)]` — opts a Rust type into `flusso_search::FlussoValue<K>`,
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
//! On success it emits `impl ::flusso_search::FlussoValue<#kind> for #ident {}`.
//! The leaf value's actual serde form is enforced by serde at the boundary;
//! this derive guarantees the *shape* fits the kind.

use proc_macro2::TokenStream;
use quote::quote;
use syn::spanned::Spanned;
use syn::{Data, DeriveInput, Fields};

/// The field kind a `FlussoValue` type stands in for.
#[derive(Clone, Copy)]
pub(crate) enum Kind {
    Keyword,
    Text,
    Number,
    Date,
}

impl Kind {
    /// The `flusso_search::kind::…` marker this resolves to. The single place
    /// these marker paths are written — both this derive and the field-validation
    /// codegen route their kind through here.
    pub(crate) fn marker(self) -> TokenStream {
        match self {
            Kind::Keyword => quote! { ::flusso_search::kind::Keyword },
            Kind::Text => quote! { ::flusso_search::kind::Text },
            Kind::Number => quote! { ::flusso_search::kind::Number },
            Kind::Date => quote! { ::flusso_search::kind::Date },
        }
    }

    fn label(self) -> &'static str {
        match self {
            Kind::Keyword => "keyword",
            Kind::Text => "text",
            Kind::Number => "number",
            Kind::Date => "date",
        }
    }

    /// Whether this kind is string-valued (and so accepts enums).
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

    let kind = match parse_kind(&input) {
        Ok(kind) => kind,
        Err(error) => return error.to_compile_error(),
    };

    if let Err(error) = validate(&input, kind) {
        return error.to_compile_error();
    }

    let ident = &input.ident;
    let marker = kind.marker();
    quote! {
        impl ::flusso_search::FlussoValue<#marker> for #ident {}
    }
}

/// Read the kind from `#[flusso(keyword | number | date)]`; defaults to keyword.
fn parse_kind(input: &DeriveInput) -> syn::Result<Kind> {
    let mut kind = Kind::Keyword;
    for attr in &input.attrs {
        if !attr.path().is_ident("flusso") {
            continue;
        }
        attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("keyword") {
                kind = Kind::Keyword;
            } else if meta.path.is_ident("text") {
                kind = Kind::Text;
            } else if meta.path.is_ident("number") {
                kind = Kind::Number;
            } else if meta.path.is_ident("date") {
                kind = Kind::Date;
            } else {
                return Err(meta.error(
                    "unknown `flusso` kind (expected `keyword`, `text`, `number`, or `date`)",
                ));
            }
            Ok(())
        })?;
    }
    Ok(kind)
}

/// Ensure the type's shape fits the chosen kind.
fn validate(input: &DeriveInput, kind: Kind) -> syn::Result<()> {
    match &input.data {
        Data::Enum(data) => {
            // Only string-valued kinds accept enums — a unit variant serializes
            // to a string, which is wrong for a numeric/date field.
            if !kind.is_string() {
                return Err(syn::Error::new(
                    input.ident.span(),
                    format!(
                        "a `{}` FlussoValue requires a newtype wrapper (e.g. `struct Cents(i64)`) — \
                         an enum serializes to a string, not a {0}",
                        kind.label()
                    ),
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
            Ok(())
        }
        Data::Struct(data) => match &data.fields {
            Fields::Unnamed(fields) if fields.unnamed.len() == 1 => Ok(()),
            _ => Err(syn::Error::new(
                input.ident.span(),
                "FlussoValue on a struct requires a single-field tuple struct \
                 (a newtype wrapper, e.g. `struct Country(String)`)",
            )),
        },
        Data::Union(_) => Err(syn::Error::new(
            input.ident.span(),
            "FlussoValue cannot be derived for a union",
        )),
    }
}
