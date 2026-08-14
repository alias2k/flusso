//! `#[derive(FlussoFragment)]` — a **location-free** document shape.
//!
//! A fragment names no index and no path, so it can be declared once (in a
//! shared crate, even) and embedded anywhere: at two paths in one index
//! (`billingAddress` and `shippingAddress`), or across several indexes.
//!
//! # How it gets validated without knowing where it lives
//!
//! It doesn't validate itself — the **root** does, once per embedding. A derive
//! expanding `User` cannot see `Address`'s tokens, so the two halves meet as
//! data during const evaluation:
//!
//! - the root bakes the resolved mapping level into a `&[FieldSpec]` and emits
//!   `const _: () = Address::__flusso_check(LEVEL);` spanned on its own field;
//! - this derive emits that `__flusso_check` — one assertion per declared field,
//!   each message **baked at macro time**, which is how it can name a field the
//!   root never saw.
//!
//! Embed the same fragment twice and it is checked twice, against each level.
//!
//! # Uniform treatment of custom types
//!
//! A fragment cannot tell a *value* type (`Money`) from a *sub-fragment*
//! (`Address`) at macro time — both are path types. So it treats every custom
//! type identically: read the acceptable kinds off
//! [`FlussoValueMeta`](flusso_query::FlussoValueMeta), then recurse with
//! `T::__flusso_check(children(level, "…"))`. Both derives emit both items; a
//! value type's sub-level is empty and its check is a no-op.
//!
//! ```ignore
//! #[derive(serde::Deserialize, FlussoFragment)]
//! struct Address { city: String, zip: Option<String> }
//! ```

use proc_macro2::TokenStream;
use quote::{quote, quote_spanned};
use syn::spanned::Spanned;
use syn::{Data, DeriveInput, Ident, Type};

use crate::doc::{self, DocField};

pub(crate) fn expand(input: DeriveInput) -> TokenStream {
    if !input.generics.params.is_empty() {
        return syn::Error::new(
            input.generics.span(),
            "FlussoFragment does not support generic structs",
        )
        .to_compile_error();
    }

    if let Some(attr) = input
        .attrs
        .iter()
        .find(|attr| attr.path().is_ident("flusso"))
        && let Err(error) = reject_location(attr)
    {
        return error.to_compile_error();
    }

    let struct_fields = match &input.data {
        Data::Struct(data) => &data.fields,
        _ => {
            return syn::Error::new(
                input.ident.span(),
                "FlussoFragment can only be derived for a struct",
            )
            .to_compile_error();
        }
    };

    let rename_all = doc::container_rename_all(&input);
    let fields = match doc::parse_fields(struct_fields, rename_all.as_deref()) {
        Ok(fields) => fields,
        Err(error) => return error.to_compile_error(),
    };

    let ident = &input.ident;
    let checks = fields.iter().map(|field| field_check(ident, field));

    quote! {
        impl ::flusso_query::FlussoValueMeta for #ident {
            const KINDS: &'static [::flusso_query::KindTag] = &[
                ::flusso_query::KindTag::Object,
                ::flusso_query::KindTag::Nested,
            ];
            const VARIANTS: &'static [&'static str] = &[];
        }

        impl #ident {
            /// Check this shape against one mapping level. Called by the root
            /// that embeds it, once per embedding site.
            #[doc(hidden)]
            pub const fn __flusso_check(level: &[::flusso_query::FieldSpec]) {
                // Silences the unused warning for a fragment with no checkable
                // field (every field skipped/opaque).
                let _ = level;
                #(#checks)*
            }
        }
    }
}

/// A fragment is location-free by definition — `index`/`path` here would make it
/// a second type that references the schema, which is exactly what the root/
/// fragment split exists to prevent.
fn reject_location(attr: &syn::Attribute) -> syn::Result<()> {
    attr.parse_nested_meta(|meta| {
        if meta.path.is_ident("index") || meta.path.is_ident("path") {
            return Err(meta.error(
                "a fragment has no location — drop `index`/`path`. It is validated by \
                 each root that embeds it, at every path it appears",
            ));
        }
        Err(meta.error("unknown `flusso` attribute on a fragment"))
    })
}

/// The assertions for one field, with every message baked in at macro time.
fn field_check(fragment: &Ident, field: &DocField) -> TokenStream {
    if field.skip || field.opaque {
        return quote! {};
    }

    let key = &field.doc_key;
    let span = field.ty.span();

    // A flattened group's keys belong to the *enclosing* level, so there is no
    // container field to look up — hand the same level straight through.
    if field.flatten {
        let ty = strip_option(field.ty);
        return quote_spanned! {span=> <#ty>::__flusso_check(level); };
    }

    let exists = message(
        fragment,
        key,
        "does not exist at the path this struct is embedded at",
    );
    let mut checks = quote_spanned! {span=>
        assert!(::flusso_query::exists(level, #key), #exists);
    };

    checks.extend(nullability_check(fragment, field, span));
    checks.extend(shape_check(fragment, field, span));
    checks
}

fn nullability_check(fragment: &Ident, field: &DocField, span: proc_macro2::Span) -> TokenStream {
    let key = &field.doc_key;
    if option_inner(field.ty).is_some() {
        let msg = message(
            fragment,
            key,
            "is declared `Option<…>`, but the schema field here is required",
        );
        quote_spanned! {span=> assert!(::flusso_query::nullable(level, #key), #msg); }
    } else {
        let msg = message(
            fragment,
            key,
            "is declared required, but the schema field here is nullable — wrap it in `Option<…>`",
        );
        quote_spanned! {span=> assert!(!::flusso_query::nullable(level, #key), #msg); }
    }
}

/// The type-shape half: what kind the schema field must be for this Rust type to
/// hold it, plus recursion into a sub-fragment.
fn shape_check(fragment: &Ident, field: &DocField, span: proc_macro2::Span) -> TokenStream {
    let key = &field.doc_key;
    let inner = strip_option(field.ty);

    // `serde_json::Value` opts out of type checking, exactly as on a root.
    if leaf_ident(inner).as_deref() == Some("Value") {
        return quote! {};
    }

    // A dynamic-key map: check the declared value kind, not the container.
    if let Some(value_ty) = hashmap_value(inner) {
        let msg = message(
            fragment,
            key,
            "is a `HashMap`, so the schema field here must be a `map` with a matching value type",
        );
        return match primitive_kinds(value_ty) {
            Some(tags) => quote_spanned! {span=>
                assert!(::flusso_query::map_value_is(level, #key, &[#(#tags),*]), #msg);
            },
            None => quote_spanned! {span=>
                assert!(
                    ::flusso_query::map_value_is(
                        level, #key,
                        <#value_ty as ::flusso_query::FlussoValueMeta>::KINDS,
                    ),
                    #msg
                );
            },
        };
    }

    // `Vec<…>` is either a `nested` array or a flat scalar array; peel it and
    // remember that the schema field has to be one of those.
    let (element, is_vec) = match vec_inner(inner) {
        Some(element) => (element, true),
        None => (inner, false),
    };

    let mut checks = TokenStream::new();
    if is_vec && primitive_kinds(element).is_some() {
        let msg = message(
            fragment,
            key,
            "is declared `Vec<…>`, but the schema field here is not an array",
        );
        checks.extend(quote_spanned! {span=>
            assert!(::flusso_query::array(level, #key), #msg);
        });
    }

    let kind_msg = message(
        fragment,
        key,
        "has a Rust type that cannot hold the schema field at this path",
    );
    match primitive_kinds(element) {
        // A known leaf type — its acceptable kinds are baked right here.
        Some(tags) => checks.extend(quote_spanned! {span=>
            assert!(::flusso_query::kind_is(level, #key, &[#(#tags),*]), #kind_msg);
        }),
        // Any other type is treated uniformly: kinds + variants from its
        // metadata, then recurse. A value type's check is a no-op.
        None => {
            let variant_msg = message(
                fragment,
                key,
                "declares a variant the schema does not list for this field",
            );
            checks.extend(quote_spanned! {span=>
                assert!(
                    ::flusso_query::kind_is(
                        level, #key,
                        <#element as ::flusso_query::FlussoValueMeta>::KINDS,
                    ),
                    #kind_msg
                );
                assert!(
                    ::flusso_query::variants_covered(
                        level, #key,
                        <#element as ::flusso_query::FlussoValueMeta>::VARIANTS,
                    ),
                    #variant_msg
                );
                <#element>::__flusso_check(::flusso_query::children(level, #key));
            });
        }
    }
    checks
}

/// `panic!` in a const context takes a literal, so every message is composed
/// here — at macro time, where the fragment and field names are known.
fn message(fragment: &Ident, key: &str, problem: &str) -> String {
    format!("fragment `{fragment}`: field `{key}` {problem}")
}

/// The [`KindTag`](flusso_query::KindTag)s a built-in leaf type may stand in
/// for — the reverse of the root's mapping-to-Rust table, so both directions
/// agree on what fits. `None` means "not a built-in leaf": a custom type, whose
/// kinds come from its own metadata instead.
fn primitive_kinds(ty: &Type) -> Option<Vec<TokenStream>> {
    let tags: &[&str] = match leaf_ident(ty).as_deref()? {
        "String" => &["Keyword", "Text", "Date", "GeoPoint", "Binary"],
        "bool" => &["Bool"],
        "i8" => &["Byte"],
        "i16" => &["Short"],
        "i32" => &["Integer"],
        "i64" => &["Long"],
        "f32" => &["Float"],
        "f64" => &["Double", "Decimal"],
        "NaiveDate" | "NaiveDateTime" | "DateTime" | "OffsetDateTime" | "PrimitiveDateTime"
        | "Date" => &["Date"],
        "GeoPoint" => &["GeoPoint"],
        _ => return None,
    };
    Some(
        tags.iter()
            .map(|tag| {
                let tag = Ident::new(tag, proc_macro2::Span::call_site());
                quote! { ::flusso_query::KindTag::#tag }
            })
            .collect(),
    )
}

fn strip_option(ty: &Type) -> &Type {
    option_inner(ty).unwrap_or(ty)
}

fn option_inner(ty: &Type) -> Option<&Type> {
    single_generic(ty, "Option")
}

fn vec_inner(ty: &Type) -> Option<&Type> {
    single_generic(ty, "Vec")
}

fn leaf_ident(ty: &Type) -> Option<String> {
    match ty {
        Type::Path(path) => path.path.segments.last().map(|s| s.ident.to_string()),
        _ => None,
    }
}

fn hashmap_value(ty: &Type) -> Option<&Type> {
    let Type::Path(path) = ty else {
        return None;
    };
    let segment = path.path.segments.last()?;
    if segment.ident != "HashMap" {
        return None;
    }
    let syn::PathArguments::AngleBracketed(args) = &segment.arguments else {
        return None;
    };
    let mut types = args.args.iter().filter_map(|arg| match arg {
        syn::GenericArgument::Type(inner) => Some(inner),
        _ => None,
    });
    let _key = types.next()?;
    types.next()
}

fn single_generic<'a>(ty: &'a Type, wrapper: &str) -> Option<&'a Type> {
    let Type::Path(path) = ty else {
        return None;
    };
    let segment = path.path.segments.last()?;
    if segment.ident != wrapper {
        return None;
    }
    let syn::PathArguments::AngleBracketed(args) = &segment.arguments else {
        return None;
    };
    args.args.iter().find_map(|arg| match arg {
        syn::GenericArgument::Type(inner) => Some(inner),
        _ => None,
    })
}
