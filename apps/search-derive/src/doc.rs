//! Field parsing, validation, and codegen — everything that turns a struct +
//! a resolved mapping into the generated query surface (and precise errors).

use proc_macro2::{Span, TokenStream};
use quote::quote;
use syn::spanned::Spanned;
use syn::{Field, Fields, Ident, LitStr, Type};

use schema::{MappingType, ResolvedField};

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

/// Read `#[serde(rename = "…")]` on a field, if present.
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

// ── validation ───────────────────────────────────────────────────────────────

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
    // `serde_json::Value` opts out of type checking.
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
        MappingType::Object if resolved.children.is_empty() => Ok(None), // json → anything
        MappingType::Object => {
            if vec_inner(inner).is_some() || is_primitive(leaf_ident(inner).as_deref()) {
                return Err(shape_error(field, os, "a struct (a sub-object)"));
            }
            Ok(None)
        }
        scalar => {
            let expected = expected_leaves(scalar);
            if expected.is_empty() {
                return Ok(None); // unrecognized OpenSearch type → accept anything
            }
            let found = leaf_ident(inner);
            if found.as_deref().is_some_and(|f| expected.contains(&f)) {
                return Ok(None);
            }
            // Hybrid: a user-defined type (a path that isn't a known primitive)
            // in a kind that supports custom values defers to a `FieldValue<K>`
            // bound — satisfied by `#[derive(FlussoValue)]`. Primitives (a real
            // mismatch like `i32` in a keyword) and non-path types still
            // hard-error here with the precise, schema-aware message.
            if let Some(kind) = value_kind(scalar)
                && found.as_deref().is_some_and(|f| !is_primitive(Some(f)))
            {
                return Ok(Some(value_assert(inner, kind)));
            }
            Err(scalar_error(field, os, &expected, found.as_deref()))
        }
    }
}

/// The `flusso_search::kind::…` marker a scalar mapping accepts custom values
/// for, or `None` for kinds without a `FlussoValue` escape hatch (`bool`, geo,
/// binary — a custom type there is almost always a mistake).
fn value_kind(mapping_type: &MappingType) -> Option<TokenStream> {
    match mapping_type {
        MappingType::Keyword => Some(quote! { ::flusso_search::kind::Keyword }),
        MappingType::Text => Some(quote! { ::flusso_search::kind::Text }),
        MappingType::Byte
        | MappingType::Short
        | MappingType::Integer
        | MappingType::Long
        | MappingType::Float
        | MappingType::HalfFloat
        | MappingType::Double
        | MappingType::ScaledFloat => Some(quote! { ::flusso_search::kind::Number }),
        MappingType::Date => Some(quote! { ::flusso_search::kind::Date }),
        _ => None,
    }
}

/// A zero-cost assertion that `ty` implements `FieldValue<kind>`, reported at
/// the field's type span if it doesn't (e.g. a missing `#[derive(FlussoValue)]`).
fn value_assert(ty: &Type, kind: TokenStream) -> TokenStream {
    quote::quote_spanned! {ty.span()=>
        const _: fn() = || {
            fn __assert_field_value<__T: ::flusso_search::FieldValue<#kind>>() {}
            __assert_field_value::<#ty>();
        };
    }
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
        MappingType::Double => vec!["f64"],
        MappingType::ScaledFloat => vec!["f64", "Decimal"],
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
                | "Decimal"
        )
    )
}

// ── codegen ──────────────────────────────────────────────────────────────────

/// Generate the `impl` block: a handle per schema field at this level, plus
/// entry points (`get`/`search`/`SCHEMA_HASH`) at the root, plus rebuild
/// tracking. `prefix` is the dotted path of this level (empty at the root).
#[allow(clippy::too_many_arguments)]
pub(crate) fn codegen(
    ident: &Ident,
    index: &str,
    hash: &str,
    prefix: &str,
    is_root: bool,
    scope: &TokenStream,
    level: &[ResolvedField],
    fields: &[DocField],
    tracked: &[String],
) -> TokenStream {
    let handles = level
        .iter()
        .filter_map(|resolved| handle_fn(resolved, prefix, scope, fields));

    // The physical index name = logical name + schema hash, exactly what the
    // OpenSearch sink writes. The derive bakes it in (a structural schema change
    // rotates the hash *and* forces this binding to be recompiled), so the hash
    // stays hidden from callers — `Type::search(&client)` just works.
    let entry = if is_root {
        quote! {
            /// The physical index this binding queries — the logical name plus
            /// the schema hash, matching what the engine's OpenSearch sink writes.
            pub const INDEX: &'static str = #index;

            /// The schema hash this binding was generated from (the `INDEX` suffix).
            pub const SCHEMA_HASH: &'static str = #hash;

            /// Start a typed search against this index.
            pub fn search(client: &::flusso_search::Client) -> ::flusso_search::Search<'_, Self> {
                ::flusso_search::Search::new(client, Self::INDEX, Self::SCHEMA_HASH)
            }

            /// Fetch one document by id; `None` when absent.
            pub async fn get(
                client: &::flusso_search::Client,
                id: impl ::core::fmt::Display,
            ) -> ::flusso_search::Result<::core::option::Option<Self>> {
                client.get_doc::<Self>(Self::INDEX, Self::SCHEMA_HASH, id).await
            }
        }
    } else {
        quote! {}
    };

    let tracked = tracked.iter().map(|path| {
        let lit = LitStr::new(path, Span::call_site());
        quote! { const _: &[u8] = include_bytes!(#lit); }
    });

    quote! {
        #(#tracked)*
        impl #ident {
            #entry
            #(#handles)*
        }
    }
}

/// The handle fn for one schema field (every mapping kind has one now). `scope`
/// is this level's query scope tag (`::flusso_search::Root` or `Self`), baked
/// into every emitted handle so its queries land in the right scope.
fn handle_fn(
    resolved: &ResolvedField,
    prefix: &str,
    scope: &TokenStream,
    fields: &[DocField],
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
            quote! { ::flusso_search::#ty<#scope> },
            quote! { ::flusso_search::#ty::<#scope>::at(#path) },
        ))
    };
    let number = |inner: &str| {
        let inner = Ident::new(inner, Span::call_site());
        Some((
            quote! { ::flusso_search::Number<#inner, #scope> },
            quote! { ::flusso_search::Number::<#inner, #scope>::at(#path) },
        ))
    };

    let (ret, ctor) = match &resolved.mapping.mapping_type {
        MappingType::Keyword => simple("Keyword"),
        MappingType::Text => simple("Text"),
        MappingType::Boolean => simple("Bool"),
        MappingType::Byte => number("i8"),
        MappingType::Short => number("i16"),
        MappingType::Integer => number("i32"),
        MappingType::Long => number("i64"),
        MappingType::Float | MappingType::HalfFloat => number("f32"),
        MappingType::Double | MappingType::ScaledFloat => number("f64"),
        MappingType::Date => simple("Date"),
        MappingType::Other(name) if name == "geo_point" => simple("Geo"),
        MappingType::Other(name) if name == "binary" => simple("Binary"),
        MappingType::Object if resolved.children.is_empty() => simple("Json"),
        // A group / one_to_one object → an `Object<S>` handle (for `.exists()`;
        // sub-fields are queried via their own dotted-path child handles). `S` is
        // the enclosing scope, same as the leaf handles at this level.
        MappingType::Object => Some((
            quote! { ::flusso_search::Object<#scope> },
            quote! { ::flusso_search::Object::<#scope>::at(#path) },
        )),
        // A `nested` array → `Nested<EnclosingScope, ChildScope>`: queries lift
        // from the element scope up to this level. The child scope is the
        // projected element struct (which derives its own `SelfTagged` handles),
        // or the `Nested` default element type when un-projected.
        MappingType::Nested => Some(match nested_element(resolved, fields) {
            Some(elem) => (
                quote! { ::flusso_search::Nested<#scope, #elem> },
                quote! { ::flusso_search::Nested::<#scope, #elem>::at(#path) },
            ),
            None => (
                quote! { ::flusso_search::Nested<#scope> },
                quote! { ::flusso_search::Nested::<#scope>::at(#path) },
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

// ── type helpers ─────────────────────────────────────────────────────────────

/// The last path segment ident of a type, e.g. `Option<String>` → `Option`,
/// `rust_decimal::Decimal` → `Decimal`.
fn leaf_ident(ty: &Type) -> Option<String> {
    match ty {
        Type::Path(path) => path.path.segments.last().map(|s| s.ident.to_string()),
        _ => None,
    }
}

/// The `T` of an `Option<T>`.
fn option_inner(ty: &Type) -> Option<&Type> {
    single_generic(ty, "Option")
}

/// The `T` of a `Vec<T>`.
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
        // "snake_case" and anything unrecognized → unchanged.
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
