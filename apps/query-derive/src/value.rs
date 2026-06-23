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
            Ok(quote! { impl ::flusso_query::FlussoValue<#marker> for #ident {} })
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
            match explicit {
                // Restrict to one string kind (e.g. a keyword-only code wrapper).
                Some(kind) => {
                    let marker = kind.marker();
                    Ok(quote! { impl ::flusso_query::FlussoValue<#marker> for #ident {} })
                }
                // Inherit every kind the inner type has.
                None => Ok(quote! {
                    impl<__FlussoK> ::flusso_query::FlussoValue<__FlussoK> for #ident
                    where #inner: ::flusso_query::FlussoValue<__FlussoK> {}
                }),
            }
        }
        Data::Union(_) => Err(syn::Error::new(
            input.ident.span(),
            "FlussoValue cannot be derived for a union",
        )),
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
