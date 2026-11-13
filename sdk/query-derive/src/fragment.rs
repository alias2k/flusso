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
    let array = match fields {
        [field] => newtype_array(field),
        _ => None,
    };
    let kinds = match fields {
        [field] => newtype_kinds(field),
        _ => None,
    }
    .unwrap_or_else(|| {
        quote! { &[::flusso_query::KindTag::Object, ::flusso_query::KindTag::Nested] }
    });
    quote! {
        impl ::flusso_query::FlussoValueMeta for #ident {
            const KINDS: &'static [::flusso_query::KindTag] = #kinds;
            const VARIANTS: &'static [&'static str] = &[];
            #array
        }

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
/// that type's kinds: one over a value type is no sub-document.
fn newtype_kinds(field: &DocField) -> Option<TokenStream> {
    if field.ident.is_some() || field.skip {
        return None;
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

/// `FlussoValueMeta::ARRAY` for a newtype fragment that reads a JSON array of
/// sub-documents, so a parent places it where it would place the `Vec`.
///
/// The newtype's own check walks the element against the level it is handed
/// (its field is checked like a flattened one), so a parent hands it a `nested`
/// array's element level exactly as it would for `Vec<Element>`. Only a bare
/// `Vec<…>` of a custom type, or a newtype that is itself one, is an array: an
/// `Option<Vec<…>>` reads `null` too, and a `Vec` of a built-in leaf has no
/// sub-document to walk. The element has to be a sub-document that reads no
/// array itself (`OpaqueProbe::ARRAY_ELEMENT`): a `Vec` of an array newtype is
/// an array of arrays, and a `Vec` of a value type reads no sub-document.
///
/// `READS_ARRAY` is the wider fact a `Vec<…>` of the newtype is refused on:
/// an array anywhere down the chain of newtypes, `Option` or not.
fn newtype_array(field: &DocField) -> Option<TokenStream> {
    if field.ident.is_some() || field.skip {
        return None;
    }
    let nullable = option_inner(field.ty).is_some();
    let inner = strip_option(field.ty);
    let in_vec = vec_inner(inner).is_some();
    let Some(ty) = checked_type(field, false) else {
        return in_vec.then(|| quote! { const READS_ARRAY: bool = true; });
    };
    let (array, reads) = if in_vec {
        (quote! { ARRAY_ELEMENT }, quote! { true })
    } else {
        (
            quote! { ARRAY },
            quote! { ::flusso_query::OpaqueProbe::<#ty>::READS_ARRAY },
        )
    };
    let array = if nullable {
        quote! { false }
    } else {
        quote! { ::flusso_query::OpaqueProbe::<#ty>::#array }
    };
    Some(quote! {
        const ARRAY: bool = {
            #[allow(unused_imports)]
            use ::flusso_query::OpaqueWithoutMeta as _;
            #array
        };
        const READS_ARRAY: bool = {
            #[allow(unused_imports)]
            use ::flusso_query::OpaqueWithoutMeta as _;
            #reads
        };
    })
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
        return embedded_type(field, false).map_or_else(TokenStream::new, |ty| {
            flattened_check(ty, span, quote! { level })
        });
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
                            && ::flusso_query::OpaqueProbe::<#element>::READS_ARRAY),
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
             field here is not an array — declare its element type here"
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
                    ),
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
        checks.extend(match primitive_kinds(value_ty) {
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
    // A `json` array holds any element, as on a root.
    let json_array = quote! {
        (::flusso_query::is_json(level, #key) && ::flusso_query::array(level, #key))
    };
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
            // `map_kind_ok` only fails for a map wrapper (a type with declared
            // `MAP_VALUES` — always `keyword` or `text`, the derive allows no
            // other tag), so when the schema's `values:` kind is readable the
            // message can name the exact tag to put on it. A const panic takes
            // a literal, so this is one assert per possible `values:` kind,
            // each guarded to fire only for its own case.
            let map_text_msg = message(
                fragment,
                key,
                &format!(
                    "is `{}`, a map wrapper whose kind tag does not match — the schema \
                     field here is a `map` of `text` values, so the type needs \
                     `#[flusso(text)]`",
                    render(element)
                ),
            );
            let map_keyword_msg = message(
                fragment,
                key,
                &format!(
                    "is `{}`, a map wrapper whose kind tag does not match — the schema \
                     field here is a `map` of `keyword` values, so the type needs \
                     `#[flusso(keyword)]`",
                    render(element)
                ),
            );
            let map_other_msg = message(
                fragment,
                key,
                &format!(
                    "is `{}`, a map wrapper — but the schema field here is a `map` of \
                     number/date values, which a kind tag cannot name; use a \
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
                assert!(
                    ::flusso_query::map_kind_ok(
                        level, #key,
                        <#element as ::flusso_query::FlussoValueMeta>::MAP_VALUES,
                    ) || !matches!(
                        ::flusso_query::map_values_of(level, #key),
                        Some(::flusso_query::KindTag::Text),
                    ),
                    #map_text_msg
                );
                assert!(
                    ::flusso_query::map_kind_ok(
                        level, #key,
                        <#element as ::flusso_query::FlussoValueMeta>::MAP_VALUES,
                    ) || !matches!(
                        ::flusso_query::map_values_of(level, #key),
                        Some(::flusso_query::KindTag::Keyword),
                    ),
                    #map_keyword_msg
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
