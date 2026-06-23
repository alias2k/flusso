//! Field parsing, validation, and codegen — everything that turns a struct +
//! a resolved mapping into the generated query surface (and precise errors).

use proc_macro2::{Span, TokenStream};
use quote::quote;
use syn::spanned::Spanned;
use syn::{Field, Fields, Ident, LitStr, Type};

use schema::{MappingType, ResolvedField};

use crate::value::Kind;

/// One struct field, with its resolved document key.
pub(crate) struct DocField<'a> {
    pub(crate) ident: &'a Ident,
    pub(crate) ty: &'a Type,
    pub(crate) doc_key: String,
    pub(crate) skip: bool,
}

/// Parse a struct's named fields, resolving each document key from serde's
/// `rename` / `rename_all` so validation matches what the document actually uses.
pub(crate) fn parse_fields<'a>(
    fields: &'a Fields,
    rename_all: Option<&str>,
) -> syn::Result<Vec<DocField<'a>>> {
    let named = match fields {
        Fields::Named(named) => &named.named,
        _ => {
            return Err(syn::Error::new(
                fields.span(),
                "FlussoDocument can only be derived for a struct with named fields",
            ));
        }
    };

    let mut out = Vec::with_capacity(named.len());
    for field in named {
        let ident = field
            .ident
            .as_ref()
            .ok_or_else(|| syn::Error::new(field.span(), "field must be named"))?;
        let (skip, rename) = flusso_field_attr(field)?;
        out.push(DocField {
            ident,
            ty: &field.ty,
            doc_key: doc_key(field, ident, rename, rename_all)?,
            skip,
        });
    }
    Ok(out)
}

/// The document key for a field, in precedence order: `#[flusso(rename = "…")]`,
/// then `#[serde(rename = "…")]`, then the container `rename_all` applied to the
/// field name, else the name itself.
fn doc_key(
    field: &Field,
    ident: &Ident,
    flusso_rename: Option<String>,
    rename_all: Option<&str>,
) -> syn::Result<String> {
    if let Some(renamed) = flusso_rename {
        return Ok(renamed);
    }
    if let Some(renamed) = serde_rename(field)? {
        return Ok(renamed);
    }
    let base = ident.to_string();
    Ok(match rename_all {
        Some(rule) => apply_rename_all(&base, rule),
        None => base,
    })
}

fn serde_rename(field: &Field) -> syn::Result<Option<String>> {
    let mut renamed = None;
    for attr in &field.attrs {
        if !attr.path().is_ident("serde") {
            continue;
        }
        // Best-effort: pull `rename`, ignore other serde attrs (flags, etc.).
        let _ = attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("rename")
                && let Ok(value) = meta.value()
                && let Ok(lit) = value.parse::<LitStr>()
            {
                renamed = Some(lit.value());
            }
            Ok(())
        });
    }
    Ok(renamed)
}

/// Read a field's `#[flusso(…)]` attributes: `skip` and/or `rename = "…"`.
fn flusso_field_attr(field: &Field) -> syn::Result<(bool, Option<String>)> {
    let mut skip = false;
    let mut rename = None;
    for attr in &field.attrs {
        if attr.path().is_ident("flusso") {
            attr.parse_nested_meta(|meta| {
                if meta.path.is_ident("skip") {
                    skip = true;
                    Ok(())
                } else if meta.path.is_ident("rename") {
                    rename = Some(meta.value()?.parse::<LitStr>()?.value());
                    Ok(())
                } else {
                    Err(meta
                        .error("unknown `flusso` field attribute (expected `skip` or `rename`)"))
                }
            })?;
        }
    }
    Ok((skip, rename))
}

/// Validate each declared field against the resolved fields at this level.
/// Returns *every* problem (not just the first) so one build surfaces them all,
/// plus any deferred type assertions to emit alongside the generated surface
/// (see [`check_type`] — a `KeywordValue` bound for user enums / newtypes).
pub(crate) fn validate(
    level: &[ResolvedField],
    fields: &[DocField],
    scope: &str,
) -> (Vec<syn::Error>, Vec<TokenStream>) {
    let mut errors = Vec::new();
    let mut asserts = Vec::new();
    for field in fields {
        if field.skip {
            continue;
        }
        let Some(resolved) = level.iter().find(|r| r.name.as_ref() == field.doc_key) else {
            errors.push(unknown_field(field, level, scope));
            continue;
        };
        if let Err(error) = check_nullability(field, resolved) {
            errors.push(error);
        }
        match check_type(field, resolved) {
            Ok(Some(assertion)) => asserts.push(assertion),
            Ok(None) => {}
            Err(error) => errors.push(error),
        }
    }
    (errors, asserts)
}

fn unknown_field(field: &DocField, level: &[ResolvedField], scope: &str) -> syn::Error {
    let mut known: Vec<&str> = level.iter().map(|r| r.name.as_ref()).collect();
    known.sort_unstable();
    syn::Error::new(
        field.ident.span(),
        format!(
            "no field `{}` in {scope} — known fields: {}",
            field.doc_key,
            known.join(", ")
        ),
    )
}

fn check_nullability(field: &DocField, resolved: &ResolvedField) -> syn::Result<()> {
    let is_option = option_inner(field.ty).is_some();
    match (resolved.nullable, is_option) {
        (true, false) => Err(syn::Error::new(
            field.ty.span(),
            format!(
                "field `{}` is nullable in the schema — wrap its type in `Option<…>`",
                field.doc_key
            ),
        )),
        (false, true) => Err(syn::Error::new(
            field.ty.span(),
            format!(
                "field `{}` is required (non-null) in the schema — drop the `Option<…>`",
                field.doc_key
            ),
        )),
        _ => Ok(()),
    }
}

/// Check one field's Rust type against its resolved mapping. `Ok(None)` = fine,
/// `Ok(Some(tokens))` = fine *if* a deferred bound holds (emitted into the
/// generated code), `Err` = a definite mismatch reported with a precise span.
fn check_type(field: &DocField, resolved: &ResolvedField) -> syn::Result<Option<TokenStream>> {
    let inner = option_inner(field.ty).unwrap_or(field.ty);
    if leaf_ident(inner).as_deref() == Some("Value") {
        return Ok(None);
    }
    let os = resolved.mapping.mapping_type.name();
    match &resolved.mapping.mapping_type {
        MappingType::Nested => {
            if vec_inner(inner).is_none() {
                return Err(shape_error(field, os, "a `Vec<…>` (a nested array)"));
            }
            Ok(None)
        }
        MappingType::Object => {
            // A `map` field (dynamic-key object) → check its value kind; an
            // opaque `json` (no children) accepts anything; a group / to-one
            // object wants a struct.
            if let Some(values) = &resolved.mapping.map_values {
                return check_map_type(field, inner, values);
            }
            if resolved.children.is_empty() {
                return Ok(None);
            }
            if vec_inner(inner).is_some() || is_primitive(leaf_ident(inner).as_deref()) {
                return Err(shape_error(field, os, "a struct (a sub-object)"));
            }
            Ok(None)
        }
        scalar => {
            let expected = expected_leaves(scalar);
            if expected.is_empty() {
                return Ok(None);
            }
            // OpenSearch has no array type, so an array field's mapping type is
            // its element type — projected as `Vec<element>`. Peel one layer.
            let inner = if resolved.array {
                match vec_inner(inner) {
                    Some(elem) => elem,
                    None => return Err(shape_error(field, os, "a `Vec<…>` (a flat array)")),
                }
            } else {
                inner
            };
            let found = leaf_ident(inner);
            if found.as_deref().is_some_and(|f| expected.contains(&f)) {
                return Ok(None);
            }
            // Hybrid: a user-defined type (a path that isn't a known primitive)
            // in a kind that supports custom values defers to a `FlussoValue<K>`
            // bound — satisfied by `#[derive(FlussoValue)]`. Primitives (a real
            // mismatch like `i32` in a keyword) and non-path types still
            // hard-error here with the precise, schema-aware message.
            if let Some(kind) = kind_of(scalar, resolved.mapping.decimal)
                && found.as_deref().is_some_and(|f| !is_primitive(Some(f)))
            {
                return Ok(Some(value_assert(inner, kind.marker())));
            }
            Err(scalar_error(field, os, &expected, found.as_deref()))
        }
    }
}

/// The [`Kind`] a scalar mapping accepts values for, or `None` for kinds without
/// a `FlussoValue` escape hatch (geo, binary — a custom type there is almost
/// always a mistake). Numerics split per type so values can't cross losslessly;
/// `decimal` carries the `double`-vs-`decimal` distinction the mapping type
/// erases. The marker tokens live on [`Kind::marker`]; this only classifies.
fn kind_of(mapping_type: &MappingType, decimal: bool) -> Option<Kind> {
    Some(match mapping_type {
        MappingType::Keyword => Kind::Keyword,
        MappingType::Text => Kind::Text,
        MappingType::Boolean => Kind::Bool,
        MappingType::Byte => Kind::Byte,
        MappingType::Short => Kind::Short,
        MappingType::Integer => Kind::Integer,
        MappingType::Long => Kind::Long,
        MappingType::Float | MappingType::HalfFloat => Kind::Float,
        MappingType::Double if decimal => Kind::Decimal,
        MappingType::Double => Kind::Double,
        MappingType::ScaledFloat => Kind::Decimal,
        MappingType::Date => Kind::Date,
        _ => return None,
    })
}

/// A zero-cost assertion that `ty` implements `FlussoValue<kind>`, reported at
/// the field's type span if it doesn't (e.g. a missing `#[derive(FlussoValue)]`).
fn value_assert(ty: &Type, kind: TokenStream) -> TokenStream {
    quote::quote_spanned! {ty.span()=>
        const _: fn() = || {
            fn __assert_field_value<__T: ::flusso_query::FlussoValue<#kind>>() {}
            __assert_field_value::<#ty>();
        };
    }
}

/// Validate a `map` field's Rust type against its declared value kind `values`.
/// The type is either `HashMap<String, V>` — peel to `V` and check it exactly
/// as a scalar value (a known primitive must match the kind; an unknown type
/// defers to `FlussoValue<kind>`) — or a whole-map newtype wrapper, which defers
/// a `FlussoMap<kind>` bound satisfied by `#[derive(FlussoMap)]`.
fn check_map_type(
    field: &DocField,
    inner: &Type,
    values: &MappingType,
) -> syn::Result<Option<TokenStream>> {
    let os = values.name();
    // Map value kinds carry no decimal flag (`map_values` is just a mapping
    // type), so a `double`-valued map keys to the `Double` kind.
    let kind = kind_of(values, false);
    match hashmap_value(inner) {
        Some(value_ty) => {
            let expected = expected_leaves(values);
            let found = leaf_ident(value_ty);
            if found.as_deref().is_some_and(|f| expected.contains(&f)) {
                return Ok(None);
            }
            if let Some(kind) = kind
                && found.as_deref().is_some_and(|f| !is_primitive(Some(f)))
            {
                return Ok(Some(value_assert(value_ty, kind.marker())));
            }
            Err(map_value_error(field, os, &expected, found.as_deref()))
        }
        None => match kind {
            Some(kind) => Ok(Some(map_assert(inner, kind.marker()))),
            None => Ok(None),
        },
    }
}

/// The value type `V` of a `HashMap<String, V>` (the last path segment must be
/// `HashMap`), else `None`. Peels a map field to its value type for checking.
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

/// A zero-cost assertion that `ty` implements `FlussoMap<kind>` (a whole-map
/// wrapper), reported at the field's type span if it doesn't.
fn map_assert(ty: &Type, kind: TokenStream) -> TokenStream {
    quote::quote_spanned! {ty.span()=>
        const _: fn() = || {
            fn __assert_field_map<__T: ::flusso_query::FlussoMap<#kind>>() {}
            __assert_field_map::<#ty>();
        };
    }
}

fn map_value_error(
    field: &DocField,
    os: &str,
    expected: &[&str],
    found: Option<&str>,
) -> syn::Error {
    let found = found.unwrap_or("a non-scalar type");
    syn::Error::new(
        field.ty.span(),
        format!(
            "field `{}` is a `{os}` map in the schema — its values must be `{}`, found `{found}`",
            field.doc_key,
            expected.join("` or `"),
        ),
    )
}

fn shape_error(field: &DocField, os: &str, expected: &str) -> syn::Error {
    syn::Error::new(
        field.ty.span(),
        format!(
            "field `{}` is `{os}` in the schema — expected {expected}",
            field.doc_key
        ),
    )
}

fn scalar_error(field: &DocField, os: &str, expected: &[&str], found: Option<&str>) -> syn::Error {
    let found = found.unwrap_or("a non-scalar type");
    syn::Error::new(
        field.ty.span(),
        format!(
            "field `{}` is `{os}` in the schema — expected `{}`, found `{found}`",
            field.doc_key,
            expected.join("` or `"),
        ),
    )
}

/// The Rust leaf type identifiers acceptable for a scalar mapping type. Empty
/// means "accept anything" (an unrecognized OpenSearch type). Lenient where a
/// field's Rust type depends on an optional feature (`decimal`, `chrono`).
fn expected_leaves(mapping_type: &MappingType) -> Vec<&'static str> {
    match mapping_type {
        MappingType::Text | MappingType::Keyword => vec!["String"],
        MappingType::Boolean => vec!["bool"],
        MappingType::Byte => vec!["i8"],
        MappingType::Short => vec!["i16"],
        MappingType::Integer => vec!["i32"],
        MappingType::Long => vec!["i64"],
        MappingType::Float | MappingType::HalfFloat => vec!["f32"],
        // `f64` is the primitive leaf for `double` and `scaled_float` (and a
        // `decimal` column, which also maps to `double`). A `Decimal` document
        // field isn't name-matched here — it's not a primitive, so it routes
        // through the deferred `FlussoValue<kind::Decimal>` bound (real type
        // checking, not an ident match).
        MappingType::Double | MappingType::ScaledFloat => vec!["f64"],
        MappingType::Date => vec![
            "String",
            "NaiveDate",
            "NaiveDateTime",
            "DateTime",
            "OffsetDateTime",
            "PrimitiveDateTime",
            "Date",
        ],
        MappingType::Other(name) if name == "geo_point" => vec!["GeoPoint", "String"],
        MappingType::Other(name) if name == "binary" => vec!["String"],
        // Object/Nested handled by the caller; any other Other → accept anything.
        _ => vec![],
    }
}

fn is_primitive(ident: Option<&str>) -> bool {
    matches!(
        ident,
        Some(
            "String"
                | "bool"
                | "char"
                | "i8"
                | "i16"
                | "i32"
                | "i64"
                | "i128"
                | "isize"
                | "u8"
                | "u16"
                | "u32"
                | "u64"
                | "u128"
                | "usize"
                | "f32"
                | "f64"
        )
    )
}

/// Generate the field-handle `impl` (a handle per schema field at this level),
/// plus the `FlussoDocument` trait impl (the `PATH` metadata — for every struct),
/// plus — at the root only — the `FlussoIndex` impl (`INDEX`/`SCHEMA_HASH`,
/// inheriting `query`/`get`), plus rebuild tracking. `prefix` is the dotted path
/// of this level (empty at the root); `segments` is that path's container chain.
#[allow(clippy::too_many_arguments)]
pub(crate) fn codegen(
    ident: &Ident,
    index: &str,
    hash: &str,
    prefix: &str,
    is_root: bool,
    scope: &TokenStream,
    segments: &[crate::resolve::PathSegment],
    level: &[ResolvedField],
    fields: &[DocField],
    tracked: &[String],
    auto_subfields: bool,
) -> TokenStream {
    let handles = level
        .iter()
        .filter_map(|resolved| handle_fn(resolved, prefix, scope, fields, auto_subfields));

    // Every struct implements `FlussoDocument` carrying its path-from-root, so a
    // nesting-aware sort can read the `nested` boundaries above any field. The
    // root's `PATH` is empty; an object level adds to the path but isn't a
    // boundary.
    let path_segments = segments.iter().map(|segment| {
        let name = LitStr::new(&segment.name, Span::call_site());
        let kind = Ident::new(if segment.nested { "Nested" } else { "Object" }, Span::call_site());
        quote! {
            ::flusso_query::Segment {
                name: #name,
                kind: ::flusso_query::SegmentKind::#kind,
            }
        }
    });
    let doc_impl = quote! {
        impl ::flusso_query::FlussoDocument for #ident {
            const PATH: &'static [::flusso_query::Segment] = &[ #(#path_segments),* ];
        }
    };

    // Only the root binding implements `FlussoIndex`: it supplies the physical
    // index name = logical name + schema hash (exactly what the OpenSearch sink
    // writes), and inherits `query`/`get`. The derive bakes the hash in (a
    // structural schema change rotates it *and* forces a recompile), so it stays
    // hidden from callers — `Type::query()` just works. A child projection has no
    // `FlussoIndex`, so it cannot start a search.
    let index_impl = if is_root {
        quote! {
            impl ::flusso_query::FlussoIndex for #ident {
                const INDEX: &'static str = #index;
                const SCHEMA_HASH: &'static str = #hash;
            }
        }
    } else {
        quote! {}
    };
    let entry = quote! { #doc_impl #index_impl };

    let tracked = tracked.iter().map(|path| {
        let lit = LitStr::new(path, Span::call_site());
        quote! { const _: &[u8] = include_bytes!(#lit); }
    });

    quote! {
        #(#tracked)*
        #entry
        impl #ident {
            #(#handles)*
        }
    }
}

/// The handle fn for one schema field (every mapping kind has one now). `scope`
/// is this level's query scope tag (`::flusso_query::Root` or `Self`), baked
/// into every emitted handle so its queries land in the right scope.
fn handle_fn(
    resolved: &ResolvedField,
    prefix: &str,
    scope: &TokenStream,
    fields: &[DocField],
    auto_subfields: bool,
) -> Option<TokenStream> {
    let path = if prefix.is_empty() {
        resolved.name.to_string()
    } else {
        format!("{prefix}.{}", resolved.name)
    };
    let name = Ident::new(&to_snake_case(resolved.name.as_ref()), Span::call_site());

    let simple = |ty: &str| {
        let ty = Ident::new(ty, Span::call_site());
        Some((
            quote! { ::flusso_query::#ty<#scope> },
            quote! { ::flusso_query::#ty::<#scope>::at(#path) },
        ))
    };
    // A numeric handle carries its value kind: `Number<kind::Long, S>` etc., so a
    // value of the wrong numeric type is a compile error.
    let number = |kind: Kind| {
        let marker = kind.marker();
        Some((
            quote! { ::flusso_query::Number<#marker, #scope> },
            quote! { ::flusso_query::Number::<#marker, #scope>::at(#path) },
        ))
    };
    let number_map = |kind: Kind| {
        let marker = kind.marker();
        Some((
            quote! { ::flusso_query::NumberMap<#marker, #scope> },
            quote! { ::flusso_query::NumberMap::<#marker, #scope>::at(#path) },
        ))
    };
    // A `text`/`keyword` handle carries flusso's auto subfields (`.keyword()` /
    // `.text()` / `.keyword_lowercase()`) only when the sink provisions them:
    // `auto_subfields` on, a scalar field (no children), and no custom `fields`
    // override (which replaces the defaults). Otherwise stamp `NoSubfields` so
    // the accessors are a compile error rather than a runtime 400.
    let subfielded = auto_subfields
        && resolved.children.is_empty()
        && !resolved.mapping.extra.contains_key("fields");
    let string_handle = |ty: &str| {
        let ty = Ident::new(ty, Span::call_site());
        Some(if subfielded {
            (
                quote! { ::flusso_query::#ty<#scope, ::flusso_query::WithSubfields> },
                quote! { ::flusso_query::#ty::<#scope>::at(#path) },
            )
        } else {
            (
                quote! { ::flusso_query::#ty<#scope, ::flusso_query::NoSubfields> },
                quote! { ::flusso_query::#ty::<#scope, ::flusso_query::NoSubfields>::leaf(#path) },
            )
        })
    };

    let (ret, ctor) = match &resolved.mapping.mapping_type {
        MappingType::Keyword => string_handle("Keyword"),
        MappingType::Text => string_handle("Text"),
        MappingType::Boolean => simple("Bool"),
        // Each numeric mapping → a `Number` handle of its kind (`decimal` carries
        // the double-vs-decimal split), so values are type-checked per kind.
        MappingType::Byte
        | MappingType::Short
        | MappingType::Integer
        | MappingType::Long
        | MappingType::Float
        | MappingType::HalfFloat
        | MappingType::Double
        | MappingType::ScaledFloat => number(kind_of(
            &resolved.mapping.mapping_type,
            resolved.mapping.decimal,
        )?),
        MappingType::Date => simple("Date"),
        MappingType::Other(name) if name == "geo_point" => simple("Geo"),
        MappingType::Other(name) if name == "binary" => simple("Binary"),
        // An `object` mapping is one of three things, told apart by `map_values`
        // and children: a `map` (dynamic-key object → a kind-typed map handle),
        // an opaque `json` (no children), or a group / to-one-join sub-object.
        MappingType::Object => match &resolved.mapping.map_values {
            Some(MappingType::Text) => simple("TextMap"),
            Some(MappingType::Keyword) => simple("KeywordMap"),
            Some(MappingType::Date) => simple("DateMap"),
            Some(
                value @ (MappingType::Byte
                | MappingType::Short
                | MappingType::Integer
                | MappingType::Long
                | MappingType::Float
                | MappingType::HalfFloat
                | MappingType::Double
                | MappingType::ScaledFloat),
            ) => number_map(kind_of(value, false)?),
            // Any other value kind has no typed map handle (the conversion
            // rejects non-leaf map values, so this is defensive) — fall back to
            // the opaque object handle so the field is still addressable.
            Some(_) => simple("Json"),
            // A group / to-one-join object → an `Object<S>` handle (for
            // `.exists()`; sub-fields are queried via their own dotted-path child
            // handles). `S` is the enclosing scope, same as the leaf handles here.
            None if resolved.children.is_empty() => simple("Json"),
            None => Some((
                quote! { ::flusso_query::Object<#scope> },
                quote! { ::flusso_query::Object::<#scope>::at(#path) },
            )),
        },
        // A `nested` array → `Nested<EnclosingScope, ChildScope>`: queries lift
        // from the element scope up to this level. The child scope is the
        // projected element struct (which derives its own `SelfTagged` handles),
        // or the `Nested` default element type when un-projected.
        MappingType::Nested => Some(match nested_element(resolved, fields) {
            Some(elem) => (
                quote! { ::flusso_query::Nested<#scope, #elem> },
                quote! { ::flusso_query::Nested::<#scope, #elem>::at(#path) },
            ),
            None => (
                quote! { ::flusso_query::Nested<#scope> },
                quote! { ::flusso_query::Nested::<#scope>::at(#path) },
            ),
        }),
        MappingType::Other(_) => simple("Json"),
    }?;

    Some(quote! { pub fn #name() -> #ret { #ctor } })
}

/// The element type for a `Nested` handle: the struct's projected `Vec<Elem>`
/// element when present, else `None` (use the `Nested` default type).
fn nested_element(resolved: &ResolvedField, fields: &[DocField]) -> Option<TokenStream> {
    let field = fields
        .iter()
        .find(|f| f.doc_key == resolved.name.as_ref())?;
    let inner = option_inner(field.ty).unwrap_or(field.ty);
    let elem = vec_inner(inner)?;
    Some(quote! { #elem })
}

/// The last path segment ident of a type, e.g. `Option<String>` → `Option`,
/// `rust_decimal::Decimal` → `Decimal`.
fn leaf_ident(ty: &Type) -> Option<String> {
    match ty {
        Type::Path(path) => path.path.segments.last().map(|s| s.ident.to_string()),
        _ => None,
    }
}

fn option_inner(ty: &Type) -> Option<&Type> {
    single_generic(ty, "Option")
}

fn vec_inner(ty: &Type) -> Option<&Type> {
    single_generic(ty, "Vec")
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

/// Convert a (possibly camelCase) document key to a snake_case Rust fn name.
pub(crate) fn to_snake_case(name: &str) -> String {
    let mut out = String::with_capacity(name.len() + 4);
    for c in name.chars() {
        if c.is_ascii_uppercase() {
            if !out.is_empty() && !out.ends_with('_') {
                out.push('_');
            }
            out.push(c.to_ascii_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}

/// Apply a serde `rename_all` rule to a (snake_case) field name.
fn apply_rename_all(name: &str, rule: &str) -> String {
    let words: Vec<&str> = name.split('_').filter(|w| !w.is_empty()).collect();
    match rule {
        "camelCase" => words
            .iter()
            .enumerate()
            .map(|(i, w)| if i == 0 { w.to_string() } else { capitalize(w) })
            .collect(),
        "PascalCase" => words.iter().map(|w| capitalize(w)).collect(),
        "SCREAMING_SNAKE_CASE" => name.to_ascii_uppercase(),
        "kebab-case" => words.join("-"),
        "SCREAMING-KEBAB-CASE" => words.join("-").to_ascii_uppercase(),
        "lowercase" => words.concat().to_ascii_lowercase(),
        "UPPERCASE" => words.concat().to_ascii_uppercase(),
        _ => name.to_string(),
    }
}

fn capitalize(word: &str) -> String {
    let mut chars = word.chars();
    match chars.next() {
        Some(first) => first.to_ascii_uppercase().to_string() + chars.as_str(),
        None => String::new(),
    }
}
