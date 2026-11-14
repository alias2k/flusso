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
//! `FlussoValueMeta`, then recurse with
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

    for attr in input
        .attrs
        .iter()
        .filter(|attr| attr.path().is_ident("flusso"))
    {
        if let Err(error) = reject_location(attr) {
            return error.to_compile_error();
        }
    }

    if let Some(attr) = crate::serde_attr::converts_from(&input.attrs) {
        return syn::Error::new_spanned(attr, CONVERTS_FROM_FRAGMENT).to_compile_error();
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

    let rename_all = crate::serde_attr::rename_all(&input.attrs);
    let transparent = crate::serde_attr::transparent(&input.attrs);
    let fields = match doc::parse_fields(struct_fields, rename_all.as_deref(), transparent) {
        Ok(fields) => fields,
        Err(error) => return error.to_compile_error(),
    };

    let mut out = embeddable(&input.ident, &fields);
    for error in fields
        .iter()
        .filter(|field| !field.skip)
        .filter_map(|field| doc::non_string_key(field.ty))
    {
        out.extend(error.to_compile_error());
    }
    for field in fields.iter().filter(|field| !field.skip) {
        out.extend(doc::exhaustive_map_values(field.ty));
    }
    for error in fields
        .iter()
        .flat_map(|field| doc::misplaced_attrs(field, false))
    {
        out.extend(error.to_compile_error());
    }
    for renamed in fields.iter().filter_map(|field| field.scope.as_ref()) {
        out.extend(
            syn::Error::new(
                renamed.span(),
                format!(
                    "a fragment generates no query scopes, so `scope` here renames nothing — \
                     {}; remove `scope`",
                    doc::SCOPE_ALLOWED
                ),
            )
            .to_compile_error(),
        );
    }
    out
}

/// The error for `#[serde(from)]`/`try_from` on a fragment.
const CONVERTS_FROM_FRAGMENT: &str = "`#[serde(from)]`/`try_from` makes serde read another type \
     and convert it, so this struct's fields are never what serde reads, and checking them proves \
     nothing; derive `FlussoFragment` on the type serde reads, and convert after reading";

/// Make a **schema-bound** struct embeddable — a root, or a legacy `path =` child.
///
/// Its check is a no-op on purpose: it resolved the schema itself and its own
/// derive already validated every field against that level, so re-running the
/// checks when a parent embeds it would only duplicate work and pile a second,
/// worse error onto any failure. A fragment is the opposite case — it has no
/// schema of its own, so [`embeddable`] gives it real checks.
pub(crate) fn embeddable_leaf(ident: &Ident) -> TokenStream {
    quote! {
        impl ::flusso_query::FlussoValueMeta for #ident {
            const KINDS: &'static [::flusso_query::KindTag] = &[
                ::flusso_query::KindTag::Object,
                ::flusso_query::KindTag::Nested,
            ];
            const VARIANTS: &'static [&'static str] = &[];
        }

        impl #ident {
            #[doc(hidden)]
            pub const fn __flusso_check(_level: &[::flusso_query::FieldSpec]) {}
        }
    }
}

/// What makes a fragment embeddable: the metadata a parent reads to place it,
/// and the check it runs against the level it is placed at.
pub(crate) fn embeddable(ident: &Ident, fields: &[DocField]) -> TokenStream {
    let checks = fields.iter().map(|field| field_check(ident, field));
    let NewtypeArray {
        checks: declared,
        array,
    } = match fields {
        // Refused at macro time, the newtype meant an array: placed as one,
        // it adds no second error where it is used.
        [field] => newtype_array(ident, field).unwrap_or_else(|error| NewtypeArray {
            checks: error.to_compile_error(),
            array: quote! { const ARRAY: bool = true; },
        }),
        _ => NewtypeArray::default(),
    };
    let kinds = match fields {
        [field] => newtype_kinds(field),
        _ => None,
    }
    .unwrap_or_else(|| {
        quote! { &[::flusso_query::KindTag::Object, ::flusso_query::KindTag::Nested] }
    });
    let NewtypeMap { values, map_impl } = match fields {
        [field] => newtype_map(ident, field),
        _ => NewtypeMap::default(),
    };
    let value = match fields {
        [field] => newtype_over_a_value(ident, field),
        _ => TokenStream::new(),
    };
    quote! {
        impl ::flusso_query::FlussoValueMeta for #ident {
            const KINDS: &'static [::flusso_query::KindTag] = #kinds;
            const VARIANTS: &'static [&'static str] = &[];
            #array
            #values
        }

        #map_impl

        #declared
        #value

        impl #ident {
            /// Check this shape against one mapping level. Called by the root
            /// that embeds it, once per embedding site.
            #[doc(hidden)]
            pub const fn __flusso_check(level: &[::flusso_query::FieldSpec]) {
                // Silences the unused warning for a shape with no checkable
                // field (every field skipped/opaque).
                let _ = level;
                #(#checks)*
            }
        }
    }
}

/// A newtype fragment stands where the type it wraps stands, so it reports
/// that type's kinds: one over a value type is no sub-document, and one over a
/// `HashMap`/`BTreeMap` is an `object` (see [`newtype_map`]).
fn newtype_kinds(field: &DocField) -> Option<TokenStream> {
    if field.ident.is_some() || field.skip {
        return None;
    }
    if map_container_value(strip_option(field.ty)).is_some() {
        return Some(quote! { &[::flusso_query::KindTag::Object] });
    }
    let Some(ty) = checked_type(field, false) else {
        let inner = strip_option(field.ty);
        let tags = primitive_kinds(vec_inner(inner).unwrap_or(inner))?;
        return Some(quote! { &[#(#tags),*] });
    };
    Some(quote! {
        {
            #[allow(unused_imports)]
            use ::flusso_query::OpaqueWithoutMeta as _;
            ::flusso_query::OpaqueProbe::<#ty>::KINDS
        }
    })
}

/// `FlussoValueMeta::ARRAY` for a newtype fragment, and the checks that keep
/// it honest, where the newtype is declared.
///
/// A fragment newtype that reads a JSON array is an array of sub-documents and
/// nothing else: over `Vec<Struct>`, or over another such newtype. A parent
/// places it where it would place the `Vec` (a `nested` field), and its own
/// check walks the element against the level it is handed. Every other way a
/// fragment newtype could read an array is refused here, so a derived type
/// reads an array exactly when `ARRAY` is set and a parent needs no other fact:
///
/// - over `Vec<E>`, `E` a leaf value (an array of values is a `FlussoValue`
///   newtype's), a map or `serde_json::Value` (opaque JSON, written at a `json`
///   field as it is), or an array itself (an array of arrays);
/// - over `Option<…>` of an array: it reads `null` too, which a `nested` field
///   never holds — put the `Option` on the field;
/// - over a `FlussoValue` array newtype, an array of the other kind.
///
/// Built-in leaves, maps and `Value` are refused at macro time; a custom
/// element is known only at const evaluation, so it gets one assert per case,
/// each spanned on the type it names.
fn newtype_array(ident: &Ident, field: &DocField) -> syn::Result<NewtypeArray> {
    if field.ident.is_some() || field.skip {
        return Ok(NewtypeArray::default());
    }
    if let Some(optional) = option_inner(field.ty) {
        if let Some(element) = vec_inner(optional) {
            return Err(syn::Error::new(
                field.ty.span(),
                format!(
                    "a FlussoFragment newtype over an array of sub-documents reads a `nested` \
                     field, which is never null (no rows read as an empty array) — drop the \
                     `Option`: `struct {ident}(Vec<{}>)`",
                    render(element),
                ),
            ));
        }
        let Some(ty) = checked_type(field, false) else {
            return Ok(NewtypeArray::default());
        };
        let rendered = render(ty);
        let message = format!(
            "`{ident}` is a FlussoFragment newtype over `Option<{rendered}>`, and `{rendered}` \
             reads an array, which a `nested` field holds and never as null (no rows read as an \
             empty array) — drop the `Option`: `struct {ident}({rendered})`"
        );
        let values = format!(
            "`{ident}` is a FlussoFragment newtype over `Option<{rendered}>`, and `{rendered}` \
             is an array of values — derive `FlussoValue` on a newtype over `{rendered}`, and put \
             the `Option<…>` on the field (`Option<{ident}>`) where it may be null"
        );
        return Ok(NewtypeArray {
            checks: quote_spanned! {ty.span()=>
                const _: () = {
                    #[allow(unused_imports)]
                    use ::flusso_query::OpaqueWithoutMeta as _;
                    assert!(
                        !::flusso_query::OpaqueProbe::<#ty>::ARRAY
                            || !::flusso_query::OpaqueProbe::<#ty>::AT_NESTED,
                        #message
                    );
                    assert!(
                        !::flusso_query::OpaqueProbe::<#ty>::ARRAY
                            || ::flusso_query::OpaqueProbe::<#ty>::AT_NESTED,
                        #values
                    );
                };
            },
            array: TokenStream::new(),
        });
    }
    let Some(element) = vec_inner(field.ty) else {
        let Some(ty) = checked_type(field, false) else {
            return Ok(NewtypeArray::default());
        };
        let rendered = render(ty);
        let message = format!(
            "`{ident}` is a FlussoFragment newtype over `{rendered}`, a `FlussoValue` newtype \
             over an array of values — derive `FlussoValue` on `{ident}` instead, so it goes \
             where `{rendered}` does"
        );
        return Ok(NewtypeArray {
            checks: quote_spanned! {ty.span()=>
                const _: () = {
                    #[allow(unused_imports)]
                    use ::flusso_query::OpaqueWithoutMeta as _;
                    assert!(
                        !::flusso_query::OpaqueProbe::<#ty>::ARRAY
                            || ::flusso_query::OpaqueProbe::<#ty>::AT_NESTED,
                        #message
                    );
                };
            },
            array: quote! {
                const ARRAY: bool = {
                    #[allow(unused_imports)]
                    use ::flusso_query::OpaqueWithoutMeta as _;
                    ::flusso_query::OpaqueProbe::<#ty>::ARRAY
                };
            },
        });
    };
    let rendered = render(element);
    let Some(ty) = checked_type(field, false) else {
        let (what, fix) = if leaf_ident(element).as_deref() == Some("Value")
            || map_container_value(element).is_some()
        {
            (
                "opaque JSON",
                format!(
                    "only a `json` field holds it, and takes `Vec<{rendered}>` as it is: declare \
                     the field `Vec<{rendered}>` and drop `{ident}`"
                ),
            )
        } else {
            (
                "an array of values",
                format!(
                    "derive `FlussoValue` on `{ident}` instead, which reads an array of values \
                     and goes at an array leaf or a `json` array"
                ),
            )
        };
        return Err(syn::Error::new(
            element.span(),
            format!(
                "a FlussoFragment newtype over a `Vec<…>` is an array of sub-documents, for a \
                 `nested` field, but `Vec<{rendered}>` is {what} — {fix}"
            ),
        ));
    };
    let nested_array = format!(
        "`{ident}` is a FlussoFragment newtype over `Vec<{rendered}>`, an array of arrays — \
         `{rendered}` already reads an array; wrap its element type instead"
    );
    let map = format!(
        "`{ident}` is a FlussoFragment newtype over `Vec<{rendered}>`, an array of maps, which is \
         opaque JSON — only a `json` field holds it, and takes `Vec<{rendered}>` as it is: declare \
         the field `Vec<{rendered}>` and drop `{ident}`"
    );
    let value = format!(
        "`{ident}` is a FlussoFragment newtype over `Vec<{rendered}>`, but `{rendered}` is a \
         value, not a sub-document — for an array of values, derive `FlussoValue` on `{ident}` \
         instead"
    );
    Ok(NewtypeArray {
        checks: quote_spanned! {ty.span()=>
            const _: () = {
                #[allow(unused_imports)]
                use ::flusso_query::OpaqueWithoutMeta as _;
                assert!(!::flusso_query::OpaqueProbe::<#ty>::ARRAY, #nested_array);
                assert!(
                    ::flusso_query::OpaqueProbe::<#ty>::AT_NESTED
                        || !::flusso_query::OpaqueProbe::<#ty>::AT_OBJECT,
                    #map
                );
                assert!(::flusso_query::OpaqueProbe::<#ty>::AT_NESTED, #value);
            };
        },
        array: quote! { const ARRAY: bool = true; },
    })
}

/// A newtype fragment over a leaf value (`String`, a number, a
/// `FlussoValue` type) is a value, not a sub-document: a root never takes it
/// at a leaf, since it has no `FlussoValue` impl, so a fragment must not
/// either. Refused where it is declared, at macro time for a built-in leaf,
/// through const data for a custom type.
fn newtype_over_a_value(ident: &Ident, field: &DocField) -> TokenStream {
    if field.ident.is_some() || field.skip {
        return TokenStream::new();
    }
    let inner = strip_option(field.ty);
    if vec_inner(inner).is_some()
        || map_container_value(inner).is_some()
        || leaf_ident(inner).as_deref() == Some("Value")
    {
        return TokenStream::new();
    }
    let rendered = render(inner);
    let message = format!(
        "`{ident}` is a FlussoFragment newtype over `{rendered}`, a value, not a sub-document — \
         derive `FlussoValue` on `{ident}` instead"
    );
    if primitive_kinds(inner).is_some() {
        return syn::Error::new(inner.span(), message).to_compile_error();
    }
    quote_spanned! {inner.span()=>
        const _: () = {
            #[allow(unused_imports)]
            use ::flusso_query::OpaqueWithoutMeta as _;
            assert!(
                !::flusso_query::OpaqueProbe::<#inner>::HAS_META
                    || ::flusso_query::OpaqueProbe::<#inner>::ARRAY
                    || ::flusso_query::OpaqueProbe::<#inner>::AT_OBJECT
                    || ::flusso_query::OpaqueProbe::<#inner>::AT_NESTED,
                #message
            );
        };
    }
}

/// What a newtype fragment carries of the map it wraps, so a parent places it
/// as it places that map, and checks its values.
///
/// An `Option` around what it wraps is looked through: the newtype then also
/// reads `null`, which a field that is never null never hands it, and a
/// nullable field wraps in `Option` itself.
///
/// - Over a `HashMap<String, V>`/`BTreeMap<String, V>`: `MAP_VALUES` holds
///   `V`'s kinds (`object` for `serde_json::Value`, which only a `json` field
///   takes), and the newtype is a
///   `FlussoMap<K>` exactly where the map is, which is what a root asks of a
///   type at a `map` field.
/// - Over any other type that is no `Vec`: both are forwarded from it, so a
///   newtype over a `FlussoMap` type is one too.
fn newtype_map(ident: &Ident, field: &DocField) -> NewtypeMap {
    if field.ident.is_some() || field.skip {
        return NewtypeMap::default();
    }
    let ty = strip_option(field.ty);
    if vec_inner(ty).is_some() {
        return NewtypeMap::default();
    }
    let value = map_container_value(ty).map(strip_option);
    let values = match value {
        // Any value reads `serde_json::Value`, but no `map` field's values are
        // `object`s: the newtype goes at a `json` field only, as the map does.
        Some(value) if leaf_ident(value).as_deref() == Some("Value") => {
            quote! { &[::flusso_query::KindTag::Object] }
        }
        // A value with no flusso metadata (a `Vec`, a map) has no kind any
        // `map` field holds: `object`, so the newtype goes at `json` only.
        Some(value) => match primitive_kinds(value) {
            Some(tags) => quote! { &[#(#tags),*] },
            None => quote! {
                {
                    #[allow(unused_imports)]
                    use ::flusso_query::OpaqueWithoutMeta as _;
                    if ::flusso_query::OpaqueProbe::<#value>::HAS_META {
                        ::flusso_query::OpaqueProbe::<#value>::KINDS
                    } else {
                        &[::flusso_query::KindTag::Object]
                    }
                }
            },
        },
        None if checked_type(field, false).is_none() => return NewtypeMap::default(),
        None => quote! {
            {
                #[allow(unused_imports)]
                use ::flusso_query::OpaqueWithoutMeta as _;
                ::flusso_query::OpaqueProbe::<#ty>::MAP_VALUES
            }
        },
    };
    // Over a map, the newtype is a `FlussoMap<K>` where a value is a `K`
    // value, as for the map written at the field; over a map type, where it is.
    let bound = match value {
        Some(value) => quote! { for<'__flusso> #value: ::flusso_query::FlussoValue<__FlussoK> },
        None => quote! { for<'__flusso> #ty: ::flusso_query::FlussoMap<__FlussoK> },
    };
    NewtypeMap {
        values: quote! { const MAP_VALUES: &'static [::flusso_query::KindTag] = #values; },
        map_impl: quote! {
            impl<__FlussoK> ::flusso_query::FlussoMap<__FlussoK> for #ident where #bound {}
        },
    }
}

/// What [`newtype_map`] emits: `MAP_VALUES` inside the newtype's
/// `FlussoValueMeta` impl, and its `FlussoMap` impl.
#[derive(Default)]
struct NewtypeMap {
    values: TokenStream,
    map_impl: TokenStream,
}

/// What [`newtype_array`] emits: the checks beside the newtype, and its
/// `ARRAY` inside its `FlussoValueMeta` impl.
#[derive(Default)]
struct NewtypeArray {
    checks: TokenStream,
    array: TokenStream,
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

/// The type a parent must drive a check into for this field, if any.
///
/// `None` for anything the parent already validates by itself: a skipped or
/// opaque field, a `serde_json::Value` escape hatch, a dynamic-key map, or a
/// built-in leaf. What is left is a custom type — a sub-fragment *or* a value
/// type, told apart only at const-evaluation time, where a value type's check
/// turns out to be a no-op.
///
/// `structured`: the schema field is a level with fields of its own (a root
/// knows this; a fragment never does), where no built-in leaf could sit.
pub(crate) fn embedded_type<'a>(field: &DocField<'a>, structured: bool) -> Option<&'a Type> {
    if field.skip || field.opaque.is_some() {
        return None;
    }
    checked_type(field, structured)
}

/// The custom type a field would hand a check to, `skip` and `opaque` aside —
/// the only kind of type `opaque` has anything to opt out of.
///
/// A built-in leaf is recognized by its name, so a type of the user's own named
/// `Date` is taken for one — except where a built-in leaf could never sit: a
/// flattened field (serde flattens a struct or a map only) and, on a root, a
/// `structured` field. There the name must be the user's own type, so it is
/// checked like any other. The one exception is a flattened `GeoPoint`:
/// flusso's own is a struct serde can flatten, and it carries no check.
pub(crate) fn checked_type<'a>(field: &DocField<'a>, structured: bool) -> Option<&'a Type> {
    let inner = strip_option(field.ty);
    if map_container_value(inner).is_some() {
        return None;
    }
    let element = vec_inner(inner).unwrap_or(inner);
    if leaf_ident(element).as_deref() == Some("Value") || map_container_value(element).is_some() {
        return None;
    }
    match primitive_kinds(element) {
        None => Some(element),
        Some(_) if field.flatten && leaf_ident(element).as_deref() == Some("GeoPoint") => None,
        Some(_) if (field.flatten || structured) && is_foreign_leaf(element) => Some(element),
        Some(_) => None,
    }
}

/// A built-in leaf from another crate (`chrono`, `time`, `uuid`, `rust_decimal`,
/// flusso's geo point), as opposed to a Rust primitive or `String`. Matched by
/// name, so a type of the user's own with one of these names matches too.
pub(crate) fn is_foreign_leaf(ty: &Type) -> bool {
    matches!(
        leaf_ident(ty).as_deref(),
        Some(
            "Decimal"
                | "Uuid"
                | "NaiveDate"
                | "NaiveDateTime"
                | "NaiveTime"
                | "DateTime"
                | "OffsetDateTime"
                | "PrimitiveDateTime"
                | "Date"
                | "GeoPoint"
        )
    )
}

/// The check of a flattened field's type against the enclosing `level`.
///
/// Reading `FlussoValueMeta` first is only for the error: a plain un-derived
/// struct then fails with the trait's note saying which derive to add (or
/// `opaque`), not just a bare "no `__flusso_check`".
pub(crate) fn flattened_check(
    ty: &Type,
    span: proc_macro2::Span,
    level: TokenStream,
) -> TokenStream {
    quote_spanned! {span=>
        let _ = <#ty as ::flusso_query::FlussoValueMeta>::KINDS;
        <#ty>::__flusso_check(#level);
    }
}

/// The check of a `#[serde(flatten)]` field (not a newtype's own field, which
/// serde reads as the newtype), on a root and in a fragment.
///
/// serde flattens only a struct or a map: anything else compiles and fails on
/// every read. A `Vec` or a built-in leaf is refused at macro time; a custom
/// type through const data, spanned on it: it must stand for an object
/// (`OpaqueProbe::AT_OBJECT`: a sub-document or a map type) and read no array.
/// `serde_json::Value`, a `HashMap`/`BTreeMap` (serde's catch-all) and flusso's
/// `GeoPoint` (a struct) are not checked; `opaque` keeps this check.
pub(crate) fn flattened_field(field: &DocField, level: TokenStream) -> TokenStream {
    if field.ident.is_none() || field.skip {
        return embedded_type(field, false).map_or_else(TokenStream::new, |ty| {
            flattened_check(ty, field.ty.span(), level)
        });
    }
    let inner = strip_option(field.ty);
    let rendered = render(field.ty);
    let refuse = |reads: &str| {
        syn::Error::new(
            field.ty.span(),
            format!(
                "`{rendered}` is `#[serde(flatten)]`, but it reads {reads}, and serde flattens \
                 only a struct or a map — every read would fail; drop `#[serde(flatten)]` and \
                 give the field its own key"
            ),
        )
        .to_compile_error()
    };
    if vec_inner(inner).is_some() {
        return refuse("an array");
    }
    if leaf_ident(inner).as_deref() == Some("Value") || map_container_value(inner).is_some() {
        return TokenStream::new();
    }
    if primitive_kinds(inner).is_some() && !is_foreign_leaf(inner) {
        return refuse("a single value");
    }
    // `opaque` turns off the walk into the type, never this: what serde can
    // flatten is no check of the type's own.
    let Some(ty) = checked_type(field, false) else {
        return TokenStream::new();
    };
    let shown = render(ty);
    let array = format!(
        "`{shown}` is `#[serde(flatten)]`, but it reads an array, and serde flattens only a \
         struct or a map — every read would fail; drop `#[serde(flatten)]` and give the field \
         its own key"
    );
    let value = format!(
        "`{shown}` is `#[serde(flatten)]`, but it reads a single value, and serde flattens only \
         a struct or a map — every read would fail; drop `#[serde(flatten)]` and give the field \
         its own key"
    );
    let span = ty.span();
    let mut checks = quote_spanned! {span=>
        {
            #[allow(unused_imports)]
            use ::flusso_query::OpaqueWithoutMeta as _;
            assert!(!::flusso_query::OpaqueProbe::<#ty>::ARRAY, #array);
            assert!(::flusso_query::OpaqueProbe::<#ty>::AT_OBJECT, #value);
        }
    };
    if embedded_type(field, false).is_some() {
        checks.extend(flattened_check(ty, span, level));
    }
    checks
}

/// The assertions for one field, with every message baked in at macro time.
///
/// `opaque` keeps what it keeps on a root: the field-exists, nullability and
/// array-shape checks, the kind of a type with flusso metadata at a leaf, and
/// the rejection where it turns off nothing (see [`opaque_check`]).
fn field_check(fragment: &Ident, field: &DocField) -> TokenStream {
    if field.skip {
        return quote! {};
    }

    let key = &field.doc_key;
    let span = field.ty.span();

    // A flattened group's keys belong to the *enclosing* level, so there is no
    // container field to look up — hand the same level straight through.
    // A flattened `serde_json::Value` or map is serde's catch-all for the
    // leftover keys, so there is nothing to check — the same rule a root follows.
    if field.flatten {
        return flattened_field(field, quote! { level });
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
    checks.extend(array_shape_check(fragment, field, span));
    if field.opaque.is_some() {
        checks.extend(opaque_check(fragment, field, span));
    } else {
        checks.extend(shape_check(fragment, field, span));
    }
    checks
}

/// Whether the field is a `Vec<…>` where the schema field is one, as a root
/// checks it. A `HashMap`/`BTreeMap`, an array of them and a
/// `serde_json::Value` are left to the shape check, which places them.
///
/// A custom type that is not a `Vec` may still read an array: a newtype
/// fragment over one (`FlussoValueMeta::ARRAY`). Which it is
/// is known only at const evaluation, so each case gets its own message, and
/// the guard picks the one that fires.
fn array_shape_check(fragment: &Ident, field: &DocField, span: proc_macro2::Span) -> TokenStream {
    let key = &field.doc_key;
    let inner = strip_option(field.ty);
    let element = vec_inner(inner).unwrap_or(inner);
    if leaf_ident(inner).as_deref() == Some("Value") || map_container_value(element).is_some() {
        return TokenStream::new();
    }
    let rendered = render(inner);
    if vec_inner(inner).is_some() {
        let msg = message(
            fragment,
            key,
            &format!(
                "is declared `{rendered}`, but the schema field here is not an array; drop the `Vec<…>`"
            ),
        );
        let mut checks = quote_spanned! {span=>
            assert!(::flusso_query::array_shape_ok(level, #key, true), #msg);
        };
        if primitive_kinds(element).is_none() || is_foreign_leaf(element) {
            let element_name = render(element);
            let nested_msg = message(
                fragment,
                key,
                &format!(
                    "is `{rendered}`, an array of arrays — `{element_name}` is a newtype over a \
                     `Vec<…>`, which already reads the array; drop the outer `Vec<…>`"
                ),
            );
            checks.extend(quote_spanned! {span=>
                {
                    #[allow(unused_imports)]
                    use ::flusso_query::OpaqueWithoutMeta as _;
                    // Read with the level, which the field-exists check already
                    // asserted, so the assert is not a constant in the user's
                    // crate (`clippy::assertions_on_constants`).
                    assert!(
                        !(::flusso_query::exists(level, #key)
                            && !::flusso_query::is_json(level, #key)
                            && ::flusso_query::OpaqueProbe::<#element>::ARRAY),
                        #nested_msg
                    );
                }
            });
        }
        return checks;
    }
    let msg = message(
        fragment,
        key,
        &format!(
            "is `{rendered}`, but the schema field here is an array (a `nested` array or a \
             flat array) — declare it `Vec<…>`"
        ),
    );
    if primitive_kinds(inner).is_some() && !is_foreign_leaf(inner) {
        return quote_spanned! {span=>
            assert!(::flusso_query::array_shape_ok(level, #key, false), #msg);
        };
    }
    let nested_msg = message(
        fragment,
        key,
        &format!(
            "is `{rendered}`, but the schema field here is a `nested` array — declare it \
             `Vec<…>`, or make `{rendered}` a `#[serde(transparent)]` `FlussoFragment` newtype \
             over one"
        ),
    );
    let array_msg = message(
        fragment,
        key,
        &format!(
            "is `{rendered}`, a newtype over a `Vec<…>`, which reads an array, but the schema \
             field here is not an array — it takes one value, so declare a type that reads one"
        ),
    );
    let values_at_nested_msg = message(
        fragment,
        key,
        &format!(
            "is `{rendered}`, a `FlussoValue` newtype over a `Vec<…>` of values, but the schema \
             field here is a `nested` array, whose elements have fields of their own — declare \
             `Vec<Struct>`, or a `#[serde(transparent)]` `FlussoFragment` newtype over one"
        ),
    );
    let sub_documents_at_a_leaf_msg = message(
        fragment,
        key,
        &format!(
            "is `{rendered}`, which reads an array of sub-documents, but the schema field here is \
             not a `nested` array, the only field that holds them; declare `Vec<…>` of the \
             element type, or a `FlussoValue` newtype over one"
        ),
    );
    quote_spanned! {span=>
        {
            #[allow(unused_imports)]
            use ::flusso_query::OpaqueWithoutMeta as _;
            let array = ::flusso_query::OpaqueProbe::<#inner>::ARRAY;
            let values = ::flusso_query::OpaqueProbe::<#inner>::LEAF_ARRAY;
            let shaped = array || ::flusso_query::array_shape_ok(level, #key, false);
            assert!(shaped || !::flusso_query::is_nested(level, #key), #nested_msg);
            assert!(shaped, #msg);
            assert!(!array || ::flusso_query::array_shape_ok(level, #key, true), #array_msg);
            assert!(!values || !::flusso_query::is_nested(level, #key), #values_at_nested_msg);
            assert!(
                !array
                    || values
                    || !::flusso_query::exists(level, #key)
                    || ::flusso_query::is_nested(level, #key),
                #sub_documents_at_a_leaf_msg
            );
        }
    }
}

/// What a fragment keeps under `opaque`, matching a root: at a `keyword` or
/// `text` field, `opaque` turns off the variant checks only; at an `object` or
/// `nested` field, the walk into the type. Anywhere else, and wherever those
/// checks would pass anyway, it turns off nothing and is rejected.
///
/// The fragment cannot see the schema at macro time, so every case is a const
/// assertion. At a leaf the type's kind is still checked, as a root checks its
/// bound, and a fragment reads the kind from its metadata, so a type without it
/// is rejected there: `OpaqueProbe` tells the two apart without failing to
/// compile on the type that has none.
fn opaque_check(fragment: &Ident, field: &DocField, span: proc_macro2::Span) -> TokenStream {
    let Some(element) = checked_type(field, false) else {
        return TokenStream::new();
    };
    let key = &field.doc_key;
    let rendered = render(element);
    let kind_msg = message(
        fragment,
        key,
        &format!(
            "is `{rendered}`, whose flusso kind does not match the schema field's type at this path"
        ),
    );
    let map_msg = message(
        fragment,
        key,
        "is `#[flusso(opaque)]`, but the schema field here is a `map`, where its type is \
         checked only by its value kind, which `opaque` does not turn off; remove `opaque`",
    );
    let no_meta_msg = message(
        fragment,
        key,
        &format!(
            "is `#[flusso(opaque)]` on `{rendered}`, which has no flusso metadata, so a fragment \
             cannot check its kind at this leaf, as a root checks it — derive `FlussoValue` for \
             it, or for a hand-written `FlussoValue` implement `FlussoValueMeta` and call \
             `flusso_query::embeddable!({rendered})`; wrap a type from another crate in a newtype \
             that derives `FlussoValue`"
        ),
    );
    let leaf_msg = message(
        fragment,
        key,
        &format!(
            "is `#[flusso(opaque)]`, but the schema field here is a leaf other than `keyword` \
             or `text`, where `{rendered}` is checked only by its kind, which `opaque` does not \
             turn off; remove `opaque`"
        ),
    );
    let variants_msg = message(
        fragment,
        key,
        &format!(
            "is `#[flusso(opaque)]`, which turns off nothing here — the schema field declares \
             variants, but `{rendered}` declares none (it is not an enum, nor a newtype over \
             one), so there is no variant check to turn off; remove `opaque`"
        ),
    );
    let exhaustive_msg = message(
        fragment,
        key,
        &format!(
            "is `#[flusso(opaque)]`, which turns off nothing here — the schema field declares \
             no `variants:` and `{rendered}` is not `exhaustive`, so no variant check runs; \
             remove `opaque`"
        ),
    );
    quote_spanned! {span=>
        {
            #[allow(unused_imports)]
            use ::flusso_query::OpaqueWithoutMeta as _;
            assert!(
                (::flusso_query::is_container(level, #key)
                    && !::flusso_query::OpaqueProbe::<#element>::HAS_META)
                    || ::flusso_query::kind_is(
                        level, #key,
                        ::flusso_query::OpaqueProbe::<#element>::KINDS,
                    )
                    || (::flusso_query::is_json(level, #key) && ::flusso_query::array(level, #key)),
                #kind_msg
            );
            assert!(::flusso_query::map_values_of(level, #key).is_none(), #map_msg);
            assert!(
                ::flusso_query::is_container(level, #key)
                    || ::flusso_query::OpaqueProbe::<#element>::HAS_META,
                #no_meta_msg
            );
            assert!(
                ::flusso_query::is_container(level, #key) || ::flusso_query::is_string(level, #key),
                #leaf_msg
            );
            assert!(
                !::flusso_query::is_string(level, #key)
                    || !::flusso_query::has_variants(level, #key)
                    || ::flusso_query::OpaqueProbe::<#element>::SKIPS_VARIANTS,
                #variants_msg
            );
            assert!(
                !::flusso_query::is_string(level, #key)
                    || ::flusso_query::has_variants(level, #key)
                    || ::flusso_query::OpaqueProbe::<#element>::SKIPS_EXHAUSTIVE,
                #exhaustive_msg
            );
        }
    }
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
    if let Some(value_ty) = map_container_value(inner) {
        // One `Option` level is the value's own: a map value can be `null`.
        let value_ty = strip_option(value_ty);
        let container = leaf_ident(inner).unwrap_or_else(|| "HashMap".to_string());
        let mut checks = not_at_a_level(fragment, key, &format!("a `{container}`"), span);
        // `serde_json::Value` has no kind: it holds an opaque `json` field's
        // values, and on a `map` a root checks its `FlussoValue` bound, which
        // it does not have.
        if leaf_ident(value_ty).as_deref() == Some("Value") {
            let msg = message(
                fragment,
                key,
                &format!(
                    "is a `{container}` of `serde_json::Value`, which only a `json` field takes, \
                     and the schema field here is not one — at a `map`, give the map its value \
                     type (`{container}<String, V>`)"
                ),
            );
            checks.extend(quote_spanned! {span=>
                assert!(::flusso_query::is_json(level, #key), #msg);
            });
            checks.extend(not_a_json_array(fragment, key, inner, span));
            return checks;
        }
        let msg = message(
            fragment,
            key,
            &format!(
                "is a `{container}` of `{}`, so the schema field here must be a `map` with \
                 matching value type",
                render(value_ty)
            ),
        );
        checks.extend(not_a_json_array(fragment, key, inner, span));
        checks.extend(match primitive_kinds(value_ty) {
            Some(tags) => quote_spanned! {span=>
                assert!(::flusso_query::map_value_is(level, #key, &[#(#tags),*]), #msg);
            },
            // Read through the probe: a value type with no flusso metadata
            // (`char`, a `Vec`, a map) fits a `json` field, as on a root, and
            // no `map`.
            None => quote_spanned! {span=>
                {
                    #[allow(unused_imports)]
                    use ::flusso_query::OpaqueWithoutMeta as _;
                    assert!(
                        ::flusso_query::is_json(level, #key)
                            || (::flusso_query::OpaqueProbe::<#value_ty>::HAS_META
                                && ::flusso_query::map_value_is(
                                    level, #key,
                                    ::flusso_query::OpaqueProbe::<#value_ty>::KINDS,
                                )),
                        #msg
                    );
                }
            },
        });
        return checks;
    }

    // The array shape is `array_shape_check`'s; the kind is the element's.
    let element = vec_inner(inner).unwrap_or(inner);
    if leaf_ident(element).as_deref() == Some("Value") {
        return quote! {};
    }

    // An array of maps is opaque JSON: only a `json` field takes it.
    if map_container_value(element).is_some() {
        let rendered = render(inner);
        let mut checks = not_at_a_level(
            fragment,
            key,
            &format!("`{rendered}`, an array of maps"),
            span,
        );
        let json_msg = message(
            fragment,
            key,
            &format!(
                "is `{rendered}`, an array of maps, which only a `json` field takes, and the \
                 schema field here is not one"
            ),
        );
        checks.extend(quote_spanned! {span=>
            assert!(::flusso_query::is_json(level, #key), #json_msg);
        });
        return checks;
    }

    let mut checks = TokenStream::new();

    let named_like_a_leaf = if is_foreign_leaf(element) {
        let name = leaf_ident(element).unwrap_or_default();
        format!(
            " (a fragment takes `{name}` for the built-in leaf type by its name, so a type of \
             your own named `{name}` needs another name to be checked here)"
        )
    } else {
        String::new()
    };
    let kind_msg = message(
        fragment,
        key,
        &format!(
            "is `{}`, which cannot hold the schema field at this path{named_like_a_leaf}",
            render(element)
        ),
    );
    // A `json` field holds any value, as on a root: its array shape is
    // `array_shape_check`'s, and a struct is still walked into, against the
    // no fields a `json` field declares.
    let json_array = quote! { ::flusso_query::is_json(level, #key) };
    match primitive_kinds(element) {
        // A known leaf type — its acceptable kinds are baked right here.
        Some(tags) => checks.extend(quote_spanned! {span=>
            assert!(
                ::flusso_query::kind_is(level, #key, &[#(#tags),*]) || #json_array,
                #kind_msg
            );
        }),
        // Any other type is treated uniformly: kinds + variants from its
        // metadata, then recurse. A value type's check is a no-op.
        None => {
            let variant_msg = message(
                fragment,
                key,
                "declares a variant the schema does not list for this field",
            );
            let exhausted_msg = message(
                fragment,
                key,
                &format!(
                    "is `{}`, marked `exhaustive` — but it does not cover every variant \
                     the schema declares for this field",
                    render(element)
                ),
            );
            let no_variants_msg = message(
                fragment,
                key,
                &format!(
                    "is `{}`, marked `exhaustive` — but the schema field here declares no \
                     variants, so there is nothing to cover; declare `variants:` on it or \
                     drop the marker",
                    render(element)
                ),
            );
            // `map_kind_ok` only fails for a map type (a type with declared
            // `MAP_VALUES`: a `FlussoMap` type, tagged `keyword` or `text`, or
            // a newtype over a map), so when the schema's `values:` kind is
            // readable the message can name the fix. A const panic takes
            // a literal, so this is one assert per possible `values:` kind,
            // each guarded to fire only for its own case.
            let retag_msg = |values: &str, other: &str| {
                message(
                    fragment,
                    key,
                    &format!(
                        "is `{}`, a map of `{other}` values, and the schema field here holds \
                         `{values}` values — tag the `FlussoMap` type `#[flusso({values})]`",
                        render(element)
                    ),
                )
            };
            let strings_msg = |values: &str| {
                message(
                    fragment,
                    key,
                    &format!(
                        "is `{}`, a map whose values are not strings, and the schema field here \
                         holds `{values}` values — give it `String` values",
                        render(element)
                    ),
                )
            };
            let (map_text_retag, map_text_strings) =
                (retag_msg("text", "keyword"), strings_msg("text"));
            let (map_keyword_retag, map_keyword_strings) =
                (retag_msg("keyword", "text"), strings_msg("keyword"));
            let map_other_msg = message(
                fragment,
                key,
                &format!(
                    "is `{}`, a map type whose values are not the number/date values the \
                     schema field here holds, which a kind tag cannot name; use a \
                     `HashMap`/`BTreeMap` of the matching value type instead",
                    render(element)
                ),
            );
            let map_msg = message(
                fragment,
                key,
                &format!(
                    "is `{}`, a map type — so the schema field here must be a `map` with a \
                     matching value kind",
                    render(element)
                ),
            );
            checks.extend(quote_spanned! {span=>
                assert!(
                    ::flusso_query::kind_is(
                        level, #key,
                        <#element as ::flusso_query::FlussoValueMeta>::KINDS,
                    ) || #json_array,
                    #kind_msg
                );
                // A map wrapper carries its value kind, so check that too —
                // otherwise this would only prove the field is object-ish.
                let map_ok = ::flusso_query::map_kind_ok(
                    level, #key,
                    <#element as ::flusso_query::FlussoValueMeta>::MAP_VALUES,
                );
                // A map of strings that fails here is tagged for the other
                // string kind: only a retag helps. Any other map needs strings.
                let strings = ::flusso_query::accepts_kind(
                    <#element as ::flusso_query::FlussoValueMeta>::MAP_VALUES,
                    ::flusso_query::KindTag::Keyword,
                ) || ::flusso_query::accepts_kind(
                    <#element as ::flusso_query::FlussoValueMeta>::MAP_VALUES,
                    ::flusso_query::KindTag::Text,
                );
                assert!(
                    map_ok
                        || !strings
                        || !matches!(
                            ::flusso_query::map_values_of(level, #key),
                            Some(::flusso_query::KindTag::Text),
                        ),
                    #map_text_retag
                );
                assert!(
                    map_ok
                        || strings
                        || !matches!(
                            ::flusso_query::map_values_of(level, #key),
                            Some(::flusso_query::KindTag::Text),
                        ),
                    #map_text_strings
                );
                assert!(
                    map_ok
                        || !strings
                        || !matches!(
                            ::flusso_query::map_values_of(level, #key),
                            Some(::flusso_query::KindTag::Keyword),
                        ),
                    #map_keyword_retag
                );
                assert!(
                    map_ok
                        || strings
                        || !matches!(
                            ::flusso_query::map_values_of(level, #key),
                            Some(::flusso_query::KindTag::Keyword),
                        ),
                    #map_keyword_strings
                );
                assert!(
                    ::flusso_query::map_kind_ok(
                        level, #key,
                        <#element as ::flusso_query::FlussoValueMeta>::MAP_VALUES,
                    ) || ::flusso_query::map_values_of(level, #key).is_none(),
                    #map_other_msg
                );
                assert!(
                    ::flusso_query::map_kind_ok(
                        level, #key,
                        <#element as ::flusso_query::FlussoValueMeta>::MAP_VALUES,
                    ),
                    #map_msg
                );
                assert!(
                    ::flusso_query::variants_covered(
                        level, #key,
                        <#element as ::flusso_query::FlussoValueMeta>::VARIANTS,
                    ),
                    #variant_msg
                );
                assert!(
                    !<#element as ::flusso_query::FlussoValueMeta>::EXHAUSTIVE
                        || ::flusso_query::has_variants(level, #key),
                    #no_variants_msg
                );
                assert!(
                    !<#element as ::flusso_query::FlussoValueMeta>::EXHAUSTIVE
                        || ::flusso_query::variants_exhausted(
                            level, #key,
                            <#element as ::flusso_query::FlussoValueMeta>::VARIANTS,
                        ),
                    #exhausted_msg
                );
                <#element>::__flusso_check(::flusso_query::children(level, #key));
            });
        }
    }
    checks
}

/// A map reads one object, so it never holds a `json` array: as on a root.
fn not_a_json_array(
    fragment: &Ident,
    key: &str,
    ty: &Type,
    span: proc_macro2::Span,
) -> TokenStream {
    let rendered = render(ty);
    let msg = message(
        fragment,
        key,
        &format!(
            "is `{rendered}`, which reads one object, but the schema field here is a `json` \
             array — declare it `Vec<…>`"
        ),
    );
    quote_spanned! {span=>
        assert!(
            !(::flusso_query::is_json(level, #key) && ::flusso_query::array(level, #key)),
            #msg
        );
    }
}

/// The fix for a map type, or an array of maps, at an `object` with fields of
/// its own: the same on a root and in a fragment.
pub(crate) const DECLARE_A_STRUCT: &str = "declare a struct for it (a \
     `#[derive(FlussoFragment)]`, or a plain struct marked `#[flusso(opaque)]`); a map type \
     goes on a `map` or `json` field";

/// [`DECLARE_A_STRUCT`] at a `nested` array.
pub(crate) const DECLARE_AN_ELEMENT: &str = "declare a struct for an element (a \
     `#[derive(FlussoFragment)]`, or a plain struct marked `#[flusso(opaque)]`) and write \
     `Vec<Struct>`; a map type goes on a `map` or `json` field";

/// The asserts that a map type, or an array of maps (`what`), is not at a
/// level with fields of its own, one message per kind of level.
fn not_at_a_level(fragment: &Ident, key: &str, what: &str, span: proc_macro2::Span) -> TokenStream {
    let object_msg = message(
        fragment,
        key,
        &format!(
            "is {what}, but the schema field here is an `object` with fields of its own — \
             {DECLARE_A_STRUCT}"
        ),
    );
    let nested_msg = message(
        fragment,
        key,
        &format!(
            "is {what}, but the schema field here is a `nested` array, whose elements have \
             fields of their own — {DECLARE_AN_ELEMENT}"
        ),
    );
    quote_spanned! {span=>
        assert!(
            !::flusso_query::has_fields(level, #key) || ::flusso_query::is_nested(level, #key),
            #object_msg
        );
        assert!(!::flusso_query::is_nested(level, #key), #nested_msg);
    }
}

/// `panic!` in a const context takes a literal, so every message is composed
/// here — at macro time, where the fragment and field names are known.
///
/// The schema's own type can't be *interpolated*: this side never sees the
/// mapping, and a const message can't be built from the level it is handed.
/// Naming the *Rust* type is the half that is knowable here. Where the schema
/// side should still steer the message, the caller bakes one literal per case
/// and guards each assert with a const-readable probe of the level (see the
/// map value-kind chain in `shape_check`).
fn message(fragment: &Ident, key: &str, problem: &str) -> String {
    format!("fragment `{fragment}`: field `{key}` {problem}")
}

/// A type as a reader would write it — `quote` renders `Vec < Item >`.
pub(crate) fn render(ty: &Type) -> String {
    quote!(#ty)
        .to_string()
        .replace(" < ", "<")
        .replace(" > ", ">")
        .replace(" >", ">")
        .replace(" ,", ",")
        .replace(":: ", "::")
        .replace(" ::", "::")
}

/// The `KindTag`s a built-in leaf type may stand in
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
        // Foreign leaf types behind a feature: no derive of their own, so they
        // are recognised here rather than via `FlussoValueMeta`.
        "Decimal" => &["Decimal", "Double"],
        "Uuid" => &["Keyword"],
        "NaiveDate" | "NaiveDateTime" | "NaiveTime" | "DateTime" | "OffsetDateTime"
        | "PrimitiveDateTime" | "Date" => &["Date"],
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

fn map_container_value(ty: &Type) -> Option<&Type> {
    let Type::Path(path) = ty else {
        return None;
    };
    let segment = path.path.segments.last()?;
    if segment.ident != "HashMap" && segment.ident != "BTreeMap" {
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
