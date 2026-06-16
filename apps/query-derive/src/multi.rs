//! `#[derive(FlussoMultiDocument)]` — implements the combined-search union
//! trait for an enum of document types.
//!
//! Unlike `FlussoDocument`, this derive is **purely syntactic**: no
//! `flusso.toml` discovery, no schema resolution. Each variant's payload type
//! must itself implement `FlussoDocument` (usually via its own derive), and
//! the generated code only references that type's `INDEX` / `SCHEMA_HASH`
//! consts. The emitted impl supplies the trait's two members:
//!
//! - `TARGETS` — one `(INDEX, SCHEMA_HASH)` pair per variant, in declaration
//!   order;
//! - `decode` — dispatch on the hit's physical index name via
//!   `FlussoDocument::physical_index()`, deserializing the source into the
//!   matching variant; a hit from an index no variant claims is
//!   `Error::UnexpectedIndex`.

use proc_macro2::TokenStream;
use quote::{ToTokens, quote, quote_spanned};
use syn::spanned::Spanned;
use syn::{Data, DeriveInput, Fields, Ident, Type};

pub(crate) fn expand(input: DeriveInput) -> TokenStream {
    if !input.generics.params.is_empty() {
        return syn::Error::new(
            input.generics.span(),
            "FlussoMultiDocument does not support generic enums",
        )
        .to_compile_error();
    }

    let data = match &input.data {
        Data::Enum(data) => data,
        _ => {
            return syn::Error::new(
                input.ident.span(),
                "FlussoMultiDocument can only be derived for an enum \
                 (one single-field variant per document type)",
            )
            .to_compile_error();
        }
    };

    if data.variants.is_empty() {
        return syn::Error::new(
            input.ident.span(),
            "FlussoMultiDocument requires at least one variant — \
             an empty union has no index to search",
        )
        .to_compile_error();
    }

    // Collect each variant's payload type, accumulating shape errors so every
    // bad variant is reported at once (bon-style, like the other derives).
    let mut errors = TokenStream::new();
    let mut variants: Vec<(&Ident, &Type)> = Vec::new();
    for variant in &data.variants {
        match &variant.fields {
            Fields::Unnamed(fields) if fields.unnamed.len() == 1 => {
                if let Some(field) = fields.unnamed.first() {
                    variants.push((&variant.ident, &field.ty));
                }
            }
            _ => {
                errors.extend(
                    syn::Error::new(
                        variant.span(),
                        format!(
                            "FlussoMultiDocument requires single-field tuple variants \
                             (`{}(SomeDocument)`) — the payload is the document type \
                             this variant decodes into",
                            variant.ident
                        ),
                    )
                    .to_compile_error(),
                );
            }
        }
    }

    // Two variants with the same payload type would make the `_index` dispatch
    // ambiguous (the first always wins) — reject the repeat where it appears.
    let mut seen: Vec<String> = Vec::new();
    for (_, ty) in &variants {
        let token = ty.to_token_stream().to_string();
        if seen.contains(&token) {
            errors.extend(
                syn::Error::new(
                    ty.span(),
                    format!(
                        "duplicate document type `{token}` — each variant must decode a \
                         distinct type, or the index dispatch is ambiguous",
                    ),
                )
                .to_compile_error(),
            );
        } else {
            seen.push(token);
        }
    }

    if !errors.is_empty() {
        return errors;
    }

    let ident = &input.ident;

    // `quote_spanned` pins the `FlussoDocument` bound to the variant's payload
    // type, so "trait not satisfied" points at the offending variant.
    let targets = variants.iter().map(|(_, ty)| {
        quote_spanned! { ty.span() =>
            (
                <#ty as ::flusso_query::FlussoDocument>::INDEX,
                <#ty as ::flusso_query::FlussoDocument>::SCHEMA_HASH,
            )
        }
    });

    let arms = variants.iter().map(|(variant, ty)| {
        quote_spanned! { ty.span() =>
            if physical_index == <#ty as ::flusso_query::FlussoDocument>::physical_index() {
                return ::flusso_query::__serde_json::from_value::<#ty>(source)
                    .map(Self::#variant)
                    .map_err(::flusso_query::Error::from);
            }
        }
    });

    quote! {
        impl ::flusso_query::FlussoMultiDocument for #ident {
            const TARGETS: &'static [(&'static str, &'static str)] = &[ #(#targets),* ];

            fn decode(
                physical_index: &str,
                source: ::flusso_query::__serde_json::Value,
            ) -> ::flusso_query::Result<Self> {
                #(#arms)*
                ::core::result::Result::Err(::flusso_query::Error::UnexpectedIndex {
                    index: ::std::borrow::ToOwned::to_owned(physical_index),
                })
            }
        }
    }
}
