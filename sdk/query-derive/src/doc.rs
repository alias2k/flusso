//! Field parsing, validation, and codegen — everything that turns a struct +
//! a resolved mapping into the generated query surface (and precise errors).

use proc_macro2::{Span, TokenStream};
use quote::quote;
use syn::ext::IdentExt;
use syn::spanned::Spanned;
use syn::{Field, Fields, Ident, LitStr, Type};

use config::{Mapping, MappingType, ResolvedField};

use crate::resolve::PathSegment;
use crate::serde_attr::serde_metas;
use crate::value::Kind;

/// One struct field, with its resolved document key.
pub(crate) struct DocField<'a> {
    /// `None` for the single field of a `#[serde(transparent)]` newtype, which
    /// has no key of its own.
    pub(crate) ident: Option<&'a Ident>,
    pub(crate) ty: &'a Type,
    pub(crate) doc_key: String,
    pub(crate) skip: bool,
    /// `#[flusso(opaque)]` — this field's type is not a fragment, so don't check
    /// it against the schema. The escape hatch for a plain hand-written struct.
    /// Carries the attribute's span, for the error when it opts out of nothing.
    pub(crate) opaque: Option<Span>,
    /// The span of a `#[flusso(rename = "…")]`, for the error when the field
    /// has no key of its own to rename.
    pub(crate) renamed: Option<Span>,
    /// `#[serde(flatten)]` — this field's *keys* live at the enclosing level, so
    /// its type is checked against that level rather than a sub-level, and the
    /// container name itself is never looked up.
    pub(crate) flatten: bool,
    /// `#[flusso(scope = "Orders")]` — rename the namespace generated for this
    /// container level, and everything under it. Escapes a collision with a type
    /// the caller already has, and shortens a deep chain.
    pub(crate) scope: Option<Ident>,
}

/// Parse a struct's named fields, resolving each document key from serde's
/// `rename` / `rename_all` so validation matches what the document actually uses.
///
/// `transparent`: the struct is `#[serde(transparent)]`.
pub(crate) fn parse_fields<'a>(
    fields: &'a Fields,
    rename_all: Option<&str>,
    transparent: bool,
) -> syn::Result<Vec<DocField<'a>>> {
    if transparent {
        return transparent_field(fields).map(|field| vec![field]);
    }
    let named = match fields {
        Fields::Named(named) => &named.named,
        Fields::Unnamed(fields) if fields.unnamed.len() == 1 => {
            return fields
                .unnamed
                .iter()
                .map(newtype_field)
                .collect::<syn::Result<_>>();
        }
        _ => {
            return Err(syn::Error::new(
                fields.span(),
                "a flusso document is a struct with named fields, or a single-field \
                 tuple struct (a `#[serde(transparent)]` newtype)",
            ));
        }
    };

    let mut out = Vec::with_capacity(named.len());
    for field in named {
        let ident = field
            .ident
            .as_ref()
            .ok_or_else(|| syn::Error::new(field.span(), "field must be named"))?;
        let attrs = flusso_field_attr(field)?;
        let serde = serde_key(field);
        let flatten = crate::serde_attr::flatten(&field.attrs);
        let read = doc_key(ident, serde.rename.clone(), rename_all);
        if let Some(renamed) = &attrs.rename
            && !flatten
            && !crate::serde_attr::skips_deserializing(&field.attrs)
            && let Some(error) = unseen_rename(ident, renamed, &read, &serde)
        {
            return Err(error);
        }
        out.push(DocField {
            ident: Some(ident),
            ty: &field.ty,
            doc_key: attrs.rename.as_ref().map_or(read, LitStr::value),
            skip: attrs.skip,
            opaque: attrs.opaque,
            renamed: attrs.rename.as_ref().map(LitStr::span),
            flatten,
            scope: attrs.scope,
        });
    }
    Ok(out)
}

/// The error for a `#[flusso(…)]` on a field serde passes over in a
/// `#[serde(transparent)]` struct, the same from every derive.
pub(crate) const PASSED_OVER: &str = "serde passes over this field of a \
     `#[serde(transparent)]` struct, so the derive never checks it and its `#[flusso(…)]` is \
     never read; remove the attribute";

/// The one field serde reads a `#[serde(transparent)]` struct as: the one it
/// does not pass over (see [`crate::serde_attr::passed_over_by_transparent`]).
/// A `#[flusso(…)]` on a field it passes over is never read, so it is an error.
fn transparent_field(fields: &Fields) -> syn::Result<DocField<'_>> {
    let (passed_over, read): (Vec<&Field>, Vec<&Field>) = fields
        .iter()
        .partition(|field| crate::serde_attr::passed_over_by_transparent(field));
    if let Some(attr) = passed_over
        .iter()
        .flat_map(|field| &field.attrs)
        .find(|attr| attr.path().is_ident("flusso"))
    {
        return Err(syn::Error::new_spanned(attr, PASSED_OVER));
    }
    let fix = match read.as_slice() {
        [field] => return newtype_field(field),
        [] => "it has none — give it the one field serde reads",
        _ => "it has more — mark all but one of them `#[serde(skip)]`",
    };
    Err(syn::Error::new(
        fields.span(),
        format!(
            "serde reads a `#[serde(transparent)]` struct as its one field that is not `skip`, \
             `skip_deserializing`, `default` or a `PhantomData`, and the derive checks that \
             field; {fix}"
        ),
    ))
}

/// The key serde reads for a field: its `#[serde(rename)]`, else the container
/// `rename_all` applied to the field name, else the name itself.
fn doc_key(ident: &Ident, serde_rename: Option<String>, rename_all: Option<&str>) -> String {
    if let Some(renamed) = serde_rename {
        return renamed;
    }
    let base = ident.unraw().to_string();
    match rename_all {
        Some(rule) => apply_rename_all(&base, rule),
        None => base,
    }
}

/// What a field's `#[serde(…)]` attributes say about its key.
struct SerdeKey {
    /// The key serde reads: a plain `rename = "…"`, or the `deserialize` name
    /// of a `rename(serialize = …, deserialize = …)` list.
    rename: Option<String>,
    /// Every `alias`: a key serde reads besides the one the derive sees.
    aliases: Vec<String>,
}

/// Reads `rename` and `alias`; every other serde field attribute is stepped over.
fn serde_key(field: &Field) -> SerdeKey {
    let mut key = SerdeKey {
        rename: None,
        aliases: Vec::new(),
    };
    for meta in serde_metas(&field.attrs) {
        match meta.name.as_str() {
            "rename" => {
                if let Some(name) = meta.value.deserialize() {
                    key.rename = Some(name.to_owned());
                }
            }
            "alias" => {
                if let crate::serde_attr::SerdeValue::Str(alias) = &meta.value {
                    key.aliases.push(alias.clone());
                }
            }
            _ => {}
        }
    }
    key
}

/// A `#[flusso(rename)]` that does not name one of the field's `alias`es.
///
/// The derive reads the key serde reads (`read`) by itself, so a `flusso`
/// rename is only ever needed to pick one of the `alias`es serde also accepts.
/// Naming `read` again is redundant; naming any other key checks the schema
/// under a key serde never reads, so every real document fails to deserialize.
fn unseen_rename(
    ident: &Ident,
    flusso: &LitStr,
    read: &str,
    serde: &SerdeKey,
) -> Option<syn::Error> {
    let named = flusso.value();
    if serde.aliases.contains(&named) {
        return None;
    }
    let message = if named == read {
        format!(
            "`#[flusso(rename = \"{named}\")]` repeats the key the derive already reads from \
             serde for `{ident}`; remove `#[flusso(rename)]`"
        )
    } else if serde.aliases.is_empty() {
        let fix = if serde.rename.is_some() {
            format!("change the `#[serde(rename)]` to `#[serde(rename = \"{named}\")]`")
        } else {
            format!("write `#[serde(rename = \"{named}\")]` instead")
        };
        format!(
            "serde reads `{ident}` from the key `{read}`, but `#[flusso(rename = \"{named}\")]` \
             checks it against the schema as `{named}`, so every real document would fail to \
             deserialize — {fix}, and remove `#[flusso(rename)]`, which only picks one of a \
             field's `#[serde(alias)]`es"
        )
    } else {
        format!(
            "serde reads `{ident}` from `{read}` or its aliases ({}), never from `{named}`, \
             but `#[flusso(rename = \"{named}\")]` checks it against the schema as `{named}` — \
             `#[flusso(rename)]` names one of the field's `#[serde(alias)]`es; name one, or \
             add `#[serde(alias = \"{named}\")]`",
            serde
                .aliases
                .iter()
                .map(|alias| format!("`{alias}`"))
                .collect::<Vec<_>>()
                .join(", "),
        )
    };
    Some(syn::Error::new(flusso.span(), message))
}

/// A field's `#[flusso(…)]` attributes: `skip`, `opaque`, `rename = "…"`,
/// `scope = "…"`, read from every `#[flusso(…)]` on the field.
struct FieldAttrs {
    skip: bool,
    opaque: Option<Span>,
    rename: Option<LitStr>,
    scope: Option<Ident>,
}

fn flusso_field_attr(field: &Field) -> syn::Result<FieldAttrs> {
    let mut skip = false;
    let mut opaque = None;
    let mut rename = None;
    let mut scope = None;
    for attr in field
        .attrs
        .iter()
        .filter(|attr| attr.path().is_ident("flusso"))
    {
        attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("skip") {
                crate::once(&meta, skip, "")?;
                skip = true;
            } else if meta.path.is_ident("opaque") {
                crate::once(&meta, opaque.is_some(), "")?;
                opaque = Some(meta.path.span());
            } else if meta.path.is_ident("rename") {
                let lit = meta.value()?.parse::<LitStr>()?;
                crate::once(&meta, rename.is_some(), "a field has one key")?;
                rename = Some(lit);
            } else if meta.path.is_ident("scope") {
                let lit: LitStr = meta.value()?.parse()?;
                crate::once(&meta, scope.is_some(), "a level takes one name")?;
                // It becomes a type name, so reject anything that isn't one
                // here rather than emitting code that won't parse.
                scope = Some(lit.parse::<Ident>().map_err(|_| {
                    syn::Error::new(
                        lit.span(),
                        "`scope` must be a valid type name, e.g. `scope = \"Orders\"`",
                    )
                })?);
            } else {
                return Err(meta.error(
                    "unknown `flusso` field attribute (expected `skip`, `opaque`, \
                     `rename`, or `scope`)",
                ));
            }
            Ok(())
        })?;
    }
    Ok(FieldAttrs {
        skip,
        opaque,
        rename,
        scope,
    })
}

/// The single field of a `#[serde(transparent)]` newtype, tuple or named. It
/// inherits the inner type's shape: there is no key of its own, so the inner
/// type is checked against this very level (like a flattened group), and a
/// `#[flusso(…)]` on it is never read, so it is an error.
fn newtype_field(field: &Field) -> syn::Result<DocField<'_>> {
    if let Some(attr) = field
        .attrs
        .iter()
        .find(|attr| attr.path().is_ident("flusso"))
    {
        return Err(syn::Error::new_spanned(
            attr,
            "the field of a `#[serde(transparent)]` newtype takes no `#[flusso(…)]` attribute \
             — it is checked as the newtype itself; put `opaque` or `skip` on the field that \
             embeds the newtype, and remove this attribute",
        ));
    }
    Ok(DocField {
        ident: None,
        ty: &field.ty,
        doc_key: String::new(),
        skip: false,
        opaque: None,
        renamed: None,
        flatten: true,
        scope: None,
    })
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
    let mut renamed_keys = std::collections::HashSet::new();
    for field in fields {
        let structured = level
            .iter()
            .find(|r| r.name.as_ref() == field.doc_key)
            .is_some_and(|resolved| !field.flatten && is_structured(resolved));
        let misplaced = misplaced_attrs(field, structured);
        let attrs_placed = misplaced.is_empty();
        errors.extend(misplaced);
        if let Some(error) = misplaced_scope(field, level, scope) {
            errors.push(error);
        } else if let Some(renamed) = &field.scope
            && !renamed_keys.insert(field.doc_key.as_str())
        {
            errors.push(syn::Error::new(
                renamed.span(),
                format!(
                    "`{}` already has a `scope` on another field, and a level takes one name; \
                     remove one of them",
                    field.doc_key
                ),
            ));
        }
        // A flattened group has no container field in the mapping — its keys
        // live at *this* level, so its type is checked against this level
        // instead (see `embed_checks`).
        if field.skip || field.flatten {
            continue;
        }
        let Some(resolved) = level.iter().find(|r| r.name.as_ref() == field.doc_key) else {
            errors.push(unknown_field(field, level, scope));
            continue;
        };
        if attrs_placed && let Some(error) = opaque_without_a_shape_check(field, resolved) {
            errors.push(error);
        }
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

/// Where `opaque` is allowed, said once for every rejection.
pub(crate) const OPAQUE_ALLOWED: &str = "`opaque` goes on a field whose type is a custom struct \
     or enum (bare, or in `Option<…>` or `Vec<…>`) that the derive would otherwise check against \
     the schema";

/// The `#[flusso(…)]` field attributes that do nothing where they are, by the
/// field's declaration alone. Shared by roots and fragments; a root also checks
/// `opaque` against the schema ([`opaque_without_a_shape_check`]).
///
/// `structured`: see [`crate::fragment::checked_type`].
pub(crate) fn misplaced_attrs(field: &DocField, structured: bool) -> Vec<syn::Error> {
    let name = field_label(field);
    let mut errors = Vec::new();
    if let Some(span) = field.renamed
        && field.flatten
    {
        errors.push(syn::Error::new(
            span,
            format!(
                "{name} is `#[serde(flatten)]`, so it has no key of its own to rename — \
                 `rename` goes on a field that has a key in the document; remove `rename`"
            ),
        ));
    }
    if let Some(span) = field.opaque {
        let message = if field.skip {
            Some(format!(
                "{name} is `#[flusso(skip)]`, which already turns off every check on it — \
                 {OPAQUE_ALLOWED}; remove `opaque`"
            ))
        } else if crate::fragment::checked_type(field, structured).is_none() {
            Some(format!(
                "{name} is {} — {OPAQUE_ALLOWED}; remove `opaque` (`skip` turns off every \
                 check on a field)",
                unshaped(field)
            ))
        } else {
            None
        };
        if let Some(message) = message {
            errors.push(syn::Error::new(span, message));
        }
    }
    errors
}

/// A field as an error names it: the Rust field name, plus the document key
/// when that differs.
fn field_label(field: &DocField) -> String {
    match field.ident {
        Some(ident) if ident.unraw() != field.doc_key && !field.flatten => {
            format!("`{ident}` (document key `{}`)", field.doc_key)
        }
        Some(ident) => format!("`{ident}`"),
        None => "the newtype's field".to_string(),
    }
}

/// Why a type has no sub-shape for `opaque` to opt out of, for its error.
fn unshaped(field: &DocField) -> String {
    let ty = field.ty;
    let inner = option_inner(ty).unwrap_or(ty);
    if field.flatten
        && (leaf_ident(inner).as_deref() == Some("Value") || map_container_value(inner).is_some())
    {
        return format!(
            "a flattened `{}`, which collects the keys no other field reads and is never \
             checked",
            crate::fragment::render(ty)
        );
    }
    if leaf_ident(vec_inner(inner).unwrap_or(inner)).as_deref() == Some("Value") {
        format!(
            "`{}`, which the derive never checks",
            crate::fragment::render(ty)
        )
    } else if map_container_value(inner).is_some() {
        "a `HashMap`/`BTreeMap`, checked as a `map` by its value type, so there is no shape to \
         opt out of"
            .to_string()
    } else {
        let name = crate::fragment::render(ty);
        format!(
            "`{name}`, which the derive treats as a built-in leaf type because of its name (a \
             type of your own named `{name}` is treated the same: rename it to have it checked), \
             so there is no shape to opt out of"
        )
    }
}

/// On a root, `opaque` turns off only the checks [`embed_checks`] drives: into
/// an `object`, `nested` or `json` field, and the variant checks at a `keyword`
/// or `text` field. Elsewhere a custom type is checked by its `FlussoValue` or
/// `FlussoMap` bound alone, which `opaque` leaves in place.
fn opaque_without_a_shape_check(field: &DocField, resolved: &ResolvedField) -> Option<syn::Error> {
    let span = field.opaque?;
    if resolved.mapping.map_values.is_none()
        && matches!(
            resolved.mapping.mapping_type,
            MappingType::Object | MappingType::Nested | MappingType::Keyword | MappingType::Text
        )
    {
        return None;
    }
    let (what, bound) = if resolved.mapping.map_values.is_some() {
        ("a `map`".to_string(), "FlussoMap")
    } else {
        (
            format!("a `{}` field", resolved.mapping.mapping_type.name()),
            "FlussoValue",
        )
    };
    Some(syn::Error::new(
        span,
        format!(
            "{} is {what} in the schema, where its type is checked only by its `{bound}` \
             bound, which `opaque` does not turn off — on a root, `opaque` goes on an `object`, \
             `nested`, `json`, `keyword` or `text` field; remove `opaque`",
            field_label(field)
        ),
    ))
}

/// Where `scope` is allowed, said once for every rejection.
pub(crate) const SCOPE_ALLOWED: &str =
    "`scope` goes on a `FlussoRoot` field whose schema field is a `nested` or `object` level";

/// A `scope` renames a generated namespace, which only a root field at an
/// indexed `nested` or `object` level has; anywhere else it would rename
/// nothing, so it is rejected rather than silently ignored.
///
/// A `skip` field still names its level by its key, so its `scope` works there.
fn misplaced_scope(field: &DocField, level: &[ResolvedField], scope: &str) -> Option<syn::Error> {
    let renamed = field.scope.as_ref()?;
    let key = &field.doc_key;
    let mut fix = "remove `scope`";
    let reason = if field.flatten {
        fix = "remove `scope`, or to rename a level the flattened type carries, put `scope` \
               on a `#[flusso(skip)]` field with that level's key";
        "is `#[serde(flatten)]`, so it has no level of its own to rename".to_string()
    } else {
        let resolved = level.iter().find(|r| r.name.as_ref() == *key);
        match resolved {
            // A field the schema doesn't have is reported by `validate` itself.
            None if !field.skip => return None,
            None => format!("is not a field of {scope}, so it has no level to rename"),
            Some(resolved) if !resolved.mapping.indexed => {
                "is `indexed: false` in the schema, so it has no query scope to rename".to_string()
            }
            Some(resolved) if has_namespace(resolved) => return None,
            Some(resolved) if resolved.mapping.map_values.is_some() => {
                "is a `map` in the schema, so it has no level to rename".to_string()
            }
            Some(resolved) if matches!(resolved.mapping.mapping_type, MappingType::Object) => {
                "is an opaque `json` object in the schema, so it has no level to rename".to_string()
            }
            Some(_) => "is a leaf field in the schema, so it has no level to rename".to_string(),
        }
    };
    Some(syn::Error::new(
        renamed.span(),
        format!("`{key}` {reason} — {SCOPE_ALLOWED}; {fix}"),
    ))
}

/// Whether a field is a level with fields of its own, where no built-in leaf
/// type could sit: a `nested` array, or an `object` with children that isn't a
/// `map`. Unlike [`has_namespace`], an unindexed level counts.
fn is_structured(resolved: &ResolvedField) -> bool {
    match resolved.mapping.mapping_type {
        MappingType::Nested => true,
        MappingType::Object => {
            resolved.mapping.map_values.is_none() && !resolved.children.is_empty()
        }
        _ => false,
    }
}

/// Whether an indexed field gets a namespace of its own: a `nested` array, or
/// an `object` with children that isn't a `map`.
fn has_namespace(resolved: &ResolvedField) -> bool {
    match resolved.mapping.mapping_type {
        MappingType::Nested => true,
        MappingType::Object => {
            resolved.mapping.map_values.is_none() && !resolved.children.is_empty()
        }
        _ => false,
    }
}

fn unknown_field(field: &DocField, level: &[ResolvedField], scope: &str) -> syn::Error {
    let mut known: Vec<&str> = level.iter().map(|r| r.name.as_ref()).collect();
    known.sort_unstable();
    syn::Error::new(
        field.ident.map_or_else(|| field.ty.span(), Ident::span),
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
    if let Some(element) = vec_inner(inner)
        && map_container_value(element).is_some()
    {
        return check_array_of_maps(field, inner, resolved);
    }
    match &resolved.mapping.mapping_type {
        MappingType::Nested => {
            if map_container_value(inner).is_some() {
                return Err(structured_map_error(field, inner, resolved));
            }
            // A custom type may be a newtype over the `Vec`; whether it is
            // is const data, which `embed_checks` asserts.
            let custom = leaf_ident(inner).is_some_and(|name| {
                !is_primitive(Some(&name)) && !matches!(name.as_str(), "Box" | "Arc" | "Rc" | "Cow")
            });
            if vec_inner(inner).is_none() && !custom {
                return Err(shape_error(field, os, "a `Vec<…>` (a nested array)"));
            }
            if let Some(element) = vec_inner(inner)
                && !resolved.children.is_empty()
                && leaf_ident(element).is_none_or(|name| is_primitive(Some(&name)))
            {
                return Err(shape_error(
                    field,
                    os,
                    "a `Vec<…>` of a struct, since each element has fields of its own",
                ));
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
                return json_array_check(field, inner, resolved);
            }
            if vec_inner(inner).is_some() || is_primitive(leaf_ident(inner).as_deref()) {
                return Err(shape_error(field, os, "a struct (a sub-object)"));
            }
            if map_container_value(inner).is_some() {
                return Err(structured_map_error(field, inner, resolved));
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
                    None => return array_value_check(field, inner, scalar, resolved),
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

/// A type that is not a `Vec` at a flat array of a scalar kind. A custom type
/// passes only as an array newtype (`FlussoValueMeta::ARRAY`, which
/// `#[derive(FlussoValue)]` sets on a newtype over a `Vec`) whose elements are
/// of the field's kind; both are const data, so they are asserted, each with its
/// own message.
fn array_value_check(
    field: &DocField,
    inner: &Type,
    scalar: &MappingType,
    resolved: &ResolvedField,
) -> syn::Result<Option<TokenStream>> {
    let os = scalar.name();
    let custom = leaf_ident(inner).filter(|name| !is_primitive(Some(name)));
    let (Some(_), Some(kind)) = (custom, kind_of(scalar, resolved.mapping.decimal)) else {
        return Err(shape_error(field, os, "a `Vec<…>` (a flat array)"));
    };
    let key = &field.doc_key;
    let rendered = crate::fragment::render(inner);
    let array_message = format!(
        "field `{key}` is a flat array of `{os}` in the schema — `{rendered}` does not read one; \
         declare it `Vec<…>`, or derive `FlussoValue` on a newtype over a `Vec<…>`"
    );
    let kind_message = format!(
        "field `{key}` is a flat array of `{os}` in the schema — the elements of `{rendered}` \
         are not a `{os}` value"
    );
    let tag = kind.tag();
    Ok(Some(quote::quote_spanned! {inner.span()=>
        const _: () = {
            #[allow(unused_imports)]
            use ::flusso_query::OpaqueWithoutMeta as _;
            assert!(::flusso_query::OpaqueProbe::<#inner>::ARRAY, #array_message);
            assert!(
                ::flusso_query::accepts_kind(::flusso_query::OpaqueProbe::<#inner>::KINDS, #tag),
                #kind_message
            );
        };
    }))
}

/// An opaque `json` field takes any type, unless it is an array (`array:
/// true`): then a `Vec<…>`, or a custom type that reads an array
/// (`FlussoValueMeta::ARRAY`, asserted since it is const data).
fn json_array_check(
    field: &DocField,
    inner: &Type,
    resolved: &ResolvedField,
) -> syn::Result<Option<TokenStream>> {
    if !resolved.array || vec_inner(inner).is_some() {
        return Ok(None);
    }
    let Some(_) = leaf_ident(inner).filter(|name| !is_primitive(Some(name))) else {
        return Err(shape_error(field, "json", "a `Vec<…>` (a `json` array)"));
    };
    let message = format!(
        "field `{}` is a `json` array in the schema — `{}` does not read one; declare it \
         `Vec<…>`",
        field.doc_key,
        crate::fragment::render(inner),
    );
    Ok(Some(quote::quote_spanned! {inner.span()=>
        const _: () = {
            #[allow(unused_imports)]
            use ::flusso_query::OpaqueWithoutMeta as _;
            assert!(::flusso_query::OpaqueProbe::<#inner>::ARRAY, #message);
        };
    }))
}

/// A map type, or an array of maps, at a level with fields of its own.
fn structured_map_error(field: &DocField, ty: &Type, resolved: &ResolvedField) -> syn::Error {
    let (level, fix) = if matches!(resolved.mapping.mapping_type, MappingType::Nested) {
        (
            "a `nested` array in the schema, whose elements have fields of their own",
            crate::fragment::DECLARE_AN_ELEMENT,
        )
    } else {
        (
            "an `object` with fields of its own in the schema",
            crate::fragment::DECLARE_A_STRUCT,
        )
    };
    syn::Error::new(
        field.ty.span(),
        format!(
            "field `{}` is {level}, and a `{}` is not that shape — {fix}",
            field.doc_key,
            crate::fragment::render(ty),
        ),
    )
}

/// An array of maps is opaque JSON: a `json` field (or a kind this derive does
/// not model) takes it, a level with fields of its own wants a struct, and any
/// other field is not that shape.
fn check_array_of_maps(
    field: &DocField,
    ty: &Type,
    resolved: &ResolvedField,
) -> syn::Result<Option<TokenStream>> {
    let mapping = &resolved.mapping;
    let takes_json = match &mapping.mapping_type {
        MappingType::Object => mapping.map_values.is_none() && resolved.children.is_empty(),
        other @ MappingType::Other(_) => expected_leaves(other).is_empty(),
        _ => false,
    };
    if takes_json {
        return Ok(None);
    }
    if is_structured(resolved) {
        return Err(structured_map_error(field, ty, resolved));
    }
    let what = if mapping.map_values.is_some() {
        "a `map`".to_string()
    } else {
        format!("`{}`", mapping.mapping_type.name())
    };
    Err(syn::Error::new(
        field.ty.span(),
        format!(
            "field `{}` is {what} in the schema, and a `{}` is an array of maps, which only a \
             `json` field takes",
            field.doc_key,
            crate::fragment::render(ty),
        ),
    ))
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
/// The type is either `HashMap<String, V>` / `BTreeMap<String, V>` — peel to `V`
/// and check it exactly as a scalar value (a known primitive must match the
/// kind; an unknown type defers to `FlussoValue<kind>`) — or a whole-map
/// wrapper, which defers a `FlussoMap<kind>` bound satisfied by
/// `#[derive(FlussoMap)]`.
fn check_map_type(
    field: &DocField,
    inner: &Type,
    values: &Mapping,
) -> syn::Result<Option<TokenStream>> {
    let os = values.mapping_type.name();
    let kind = kind_of(&values.mapping_type, values.decimal);
    match map_container_value(inner) {
        Some(value_ty) => {
            let expected = expected_leaves(&values.mapping_type);
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

/// The value type `V` of a `HashMap<String, V>` / `BTreeMap<String, V>` (by the
/// last path segment), else `None`. Peels a map field to its value type for
/// checking.
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

/// Generate the **whole** typed query surface for an index, from the root down.
///
/// The root is the only type bound to a schema, so it owns every handle in the
/// index — not just its own level. Walking the resolved mapping emits, per
/// container level, a namespace type carrying that level's handles:
///
/// - an **object** flattens into its enclosing scope, so its namespace chains
///   from the parent as a `&self` method: `User::account().tier()`;
/// - a **nested** array introduces its own scope, so its namespace is a named
///   type with associated fns: `UserOrders::total()`, and it implements
///   `FlussoScope` so a nesting-aware sort can read its `PATH`.
///
/// Namespaces live in a generated `flusso_<root>_query` module, named for their level
/// (`flusso_user_query::Orders`, `flusso_user_query::OrdersItems`), so nothing the derive emits
/// lands in the caller's namespace. The root additionally
/// implements `FlussoRoot` (`INDEX`/`SCHEMA_HASH`, inheriting `query`/`get`) —
/// the entry points a child level must not have.
#[allow(clippy::too_many_arguments)]
pub(crate) fn codegen(
    ident: &Ident,
    vis: &syn::Visibility,
    index: &str,
    hash: &str,
    scope_mod: Option<&Ident>,
    level: &[ResolvedField],
    fields: &[DocField],
    tracked: &[String],
    auto_subfields: bool,
) -> TokenStream {
    // A `#[flusso(scope = "…")]` on a root field renames that level's namespace
    // (and everything under it). Only the root's own fields can carry it — a
    // level the root doesn't project has nowhere to hang an attribute, so it
    // keeps the generated name.
    let renames = fields
        .iter()
        .filter_map(|field| Some((field.doc_key.clone(), field.scope.clone()?)))
        .collect();

    // Generated types live in their own module, never in the caller's namespace.
    // Emitting them beside the root made a name the user might reasonably pick
    // (a fragment named after the level it sits at) a redefinition — and the
    // derive's own `#[derive(Copy)]` then landed on *their* struct, so the real
    // error was buried under a cascade.
    //
    // `flusso_<root>_query` rather than something short: the module is the one
    // name the caller can still collide with, and this one reads as obviously
    // generated, so nobody picks it by accident.
    let module = scope_mod.cloned().unwrap_or_else(|| {
        Ident::new(
            &format!("flusso_{}_query", to_snake_case(&ident.to_string())),
            ident.span(),
        )
    });

    let mut ctx = Namespaces {
        auto_subfields,
        renames,
        module: module.clone(),
        scopes: std::collections::HashMap::new(),
        emitted: TokenStream::new(),
    };

    let root_scope = quote! { ::flusso_query::Root };
    let handles = ctx.emit_level(ident, &root_scope, "", &[], level, Receiver::Associated);

    let namespaces = ctx.emitted;
    let namespaces = if namespaces.is_empty() {
        scope_mod.map_or_else(TokenStream::new, |renamed| {
            syn::Error::new(
                renamed.span(),
                format!(
                    "index `{index}` has no `nested` or `object` level the index searches, so \
                     no module of query scopes is generated and `scope_mod` renames nothing — \
                     `scope_mod` goes on a root whose index has such a level; remove `scope_mod`"
                ),
            )
            .to_compile_error()
        })
    } else {
        let doc = format!(
            "Query scopes for the `{ident}` document — one per container level of \
             its index, generated from the schema.\n\nImport what you use: \
             `use {module}::Orders;`."
        );
        quote! {
            #[doc = #doc]
            #[allow(unreachable_pub)]
            #vis mod #module {
                #namespaces
            }
        }
    };
    let tracked = tracked.iter().map(|path| {
        let lit = LitStr::new(path, Span::call_site());
        quote! { const _: &[u8] = include_bytes!(#lit); }
    });

    quote! {
        #(#tracked)*

        impl ::flusso_query::FlussoScope for #ident {
            const PATH: &'static [::flusso_query::Segment] = &[];
        }
        impl ::flusso_query::FlussoRoot for #ident {
            const INDEX: &'static str = #index;
            const SCHEMA_HASH: &'static str = #hash;
        }
        impl #ident {
            #handles
        }

        #namespaces
    }
}

/// How a level's handles are reached.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Receiver {
    /// `User::email()` / `UserOrders::total()` — the level *is* the namespace.
    Associated,
    /// `User::account().tier()` — an object namespace, reached from its parent.
    Method,
}

/// Accumulates the namespace types emitted while walking the mapping.
struct Namespaces {
    auto_subfields: bool,
    /// Root-field scope renames, keyed by document key.
    renames: std::collections::HashMap<String, Ident>,
    /// The module the namespaces live in — referenced from the root's handles,
    /// but not from inside the module, where the names are already in scope.
    module: Ident,
    /// Every scope type generated so far, by name, with the level it is for:
    /// they share one module, so two levels must not generate one name.
    scopes: std::collections::HashMap<String, String>,
    emitted: TokenStream,
}

/// Record that the schema field at `path` generates the handle `name` on its
/// level, or the error naming the field that already did. An object's level
/// also has its presence check, `exists()`.
fn claim_handle(
    taken: &mut std::collections::HashMap<String, String>,
    name: &Ident,
    path: &str,
    prefix: &str,
    receiver: Receiver,
) -> syn::Result<()> {
    let generated = name.to_string();
    let level = if prefix.is_empty() {
        "the index root".to_string()
    } else {
        format!("`{prefix}`")
    };
    if receiver == Receiver::Method && generated == "exists" {
        return Err(syn::Error::new(
            Span::call_site(),
            format!(
                "the schema key `{path}` generates the query handle `exists()`, which the object \
                 {level} already has as its presence check — rename the field in the schema"
            ),
        ));
    }
    if let Some(other) = taken.get(&generated) {
        return Err(syn::Error::new(
            Span::call_site(),
            format!(
                "the schema keys `{other}` and `{path}` at {level} both generate the query \
                 handle `{generated}()` — rename one of them in the schema"
            ),
        ));
    }
    taken.insert(generated, path.to_owned());
    Ok(())
}

impl Namespaces {
    /// The handle fns for one level, emitting any namespace types beneath it.
    ///
    /// `base` names this level's type; a child namespace is `base` + the field
    /// name, so a rename at any level flows down to everything under it.
    fn emit_level(
        &mut self,
        base: &Ident,
        scope: &TokenStream,
        prefix: &str,
        segments: &[PathSegment],
        level: &[ResolvedField],
        receiver: Receiver,
    ) -> TokenStream {
        let mut handles = TokenStream::new();
        let mut handle_keys = std::collections::HashMap::new();
        // An `indexed: false` field has nothing in the index to query, so it
        // gets no handle; its value still deserializes into the document type.
        for resolved in level.iter().filter(|resolved| resolved.mapping.indexed) {
            let path = if prefix.is_empty() {
                resolved.name.to_string()
            } else {
                format!("{prefix}.{}", resolved.name)
            };
            let mut below = segments.to_vec();
            below.push(PathSegment {
                name: resolved.name.to_string(),
                nested: matches!(resolved.mapping.mapping_type, MappingType::Nested),
            });
            let key = resolved.name.as_ref();
            let (name, child) = match generated_ident(&to_snake_case(key), key)
                .and_then(|name| Ok((name, self.child_ident(base, key, segments.is_empty())?)))
                .and_then(|(name, child)| {
                    claim_handle(&mut handle_keys, &name, &path, prefix, receiver)?;
                    Ok((name, child))
                }) {
                Ok(names) => names,
                Err(error) => {
                    handles.extend(error.to_compile_error());
                    continue;
                }
            };
            let has_scope = matches!(resolved.mapping.mapping_type, MappingType::Nested)
                || has_namespace(resolved);
            if has_scope && let Err(error) = self.claim_scope(&child, &path) {
                handles.extend(error.to_compile_error());
                continue;
            }

            // At the root the namespace is one module away; inside the module
            // its siblings are already in scope.
            let child_path = if segments.is_empty() {
                let module = &self.module;
                quote! { #module::#child }
            } else {
                quote! { #child }
            };

            let container = match &resolved.mapping.mapping_type {
                // A `nested` array is its own scope: the child namespace tags
                // the handles inside it, and carries the PATH a sort reads.
                MappingType::Nested => {
                    self.emit_nested(&child, &path, &below, &resolved.children);
                    Some((
                        quote! { ::flusso_query::Nested<#scope, #child_path> },
                        quote! { ::flusso_query::Nested::<#scope, #child_path>::at(#path) },
                    ))
                }
                // A group / to-one join flattens into the enclosing scope, so
                // its namespace keeps `scope` and chains from here.
                MappingType::Object if has_namespace(resolved) => {
                    self.emit_object(&child, scope, &path, &below, &resolved.children);
                    Some((quote! { #child_path }, quote! { #child_path }))
                }
                _ => None,
            };

            let Some((ret, ctor)) =
                container.or_else(|| leaf_handle(resolved, &path, scope, self.auto_subfields))
            else {
                continue;
            };

            handles.extend(match receiver {
                Receiver::Associated => quote! { pub fn #name() -> #ret { #ctor } },
                Receiver::Method => quote! { pub fn #name(&self) -> #ret { #ctor } },
            });
        }
        handles
    }

    /// A `nested` level: its own scope tag, its own `PATH`, associated fns.
    fn emit_nested(
        &mut self,
        ident: &Ident,
        path: &str,
        segments: &[PathSegment],
        level: &[ResolvedField],
    ) {
        let scope = quote! { #ident };
        let handles = self.emit_level(ident, &scope, path, segments, level, Receiver::Associated);
        let path_segments = path_segments(segments);
        self.emitted.extend(quote! {
            /// A `nested` array's element scope: its handles, and the path a
            /// nesting-aware sort reads. Generated from the schema.
            #[derive(Debug, Clone, Copy, PartialEq, Eq)]
            pub struct #ident;

            impl ::flusso_query::FlussoScope for #ident {
                const PATH: &'static [::flusso_query::Segment] = &[ #(#path_segments),* ];
            }

            impl #ident {
                #handles
            }
        });
    }

    /// An `object` level: no scope of its own, reached by chaining.
    fn emit_object(
        &mut self,
        ident: &Ident,
        scope: &TokenStream,
        path: &str,
        segments: &[PathSegment],
        level: &[ResolvedField],
    ) {
        let handles = self.emit_level(ident, scope, path, segments, level, Receiver::Method);
        self.emitted.extend(quote! {
            /// A sub-document's handles — an `object` group or a to-one join.
            /// Flattened, so its fields query in the enclosing scope. Generated
            /// from the schema.
            #[derive(Debug, Clone, Copy, PartialEq, Eq)]
            pub struct #ident;

            impl #ident {
                /// The sub-document is present — most useful on a nullable
                /// to-one join.
                pub fn exists(&self) -> ::flusso_query::Query<#scope> {
                    ::flusso_query::Object::<#scope>::at(#path).exists()
                }
                #handles
            }
        });
    }

    /// Record that the level at `path` generates the scope type `child`, or
    /// the error naming the level that already did.
    fn claim_scope(&mut self, child: &Ident, path: &str) -> syn::Result<()> {
        let name = child.to_string();
        if let Some(other) = self.scopes.get(&name) {
            return Err(syn::Error::new(
                Span::call_site(),
                format!(
                    "the schema levels `{other}` and `{path}` both generate the query scope \
                     `{}::{name}` — rename one of them in the schema, or give the root field \
                     above one of them a `#[flusso(scope = \"…\")]`",
                    self.module,
                ),
            ));
        }
        self.scopes.insert(name, path.to_owned());
        Ok(())
    }

    /// The namespace type for a child level.
    ///
    /// Directly under the root it is named for the level alone (`Orders`) — the
    /// module already says which document it belongs to. Deeper levels append,
    /// so `orders.items` is `OrdersItems`, keeping siblings apart.
    ///
    /// At the root level a `#[flusso(scope = "…")]` replaces the name outright,
    /// and since it becomes the `base` for the recursion, everything under it
    /// follows.
    fn child_ident(&self, base: &Ident, field: &str, at_root: bool) -> syn::Result<Ident> {
        if at_root {
            if let Some(renamed) = self.renames.get(field) {
                return Ok(renamed.clone());
            }
            // Directly under the root the module supplies the qualification, so
            // the type is named for its level alone: `flusso_user_query::Orders`.
            return generated_ident(&to_pascal_case(field), field);
        }
        generated_ident(&format!("{base}{}", to_pascal_case(field)), field)
    }
}

fn path_segments(segments: &[PathSegment]) -> Vec<TokenStream> {
    segments
        .iter()
        .map(|segment| {
            let name = LitStr::new(&segment.name, Span::call_site());
            let kind = Ident::new(
                if segment.nested { "Nested" } else { "Object" },
                Span::call_site(),
            );
            quote! {
                ::flusso_query::Segment {
                    name: #name,
                    kind: ::flusso_query::SegmentKind::#kind,
                }
            }
        })
        .collect()
}

/// The handle type + constructor for a leaf field — everything that is not a
/// container level with its own namespace. `scope` is the enclosing query scope.
fn leaf_handle(
    resolved: &ResolvedField,
    path: &str,
    scope: &TokenStream,
    auto_subfields: bool,
) -> Option<(TokenStream, TokenStream)> {
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
    // `auto_subfields` on, a scalar field (no children) that did not opt out with
    // `subfields: false`, and no custom `fields` override (which replaces the
    // defaults). Otherwise stamp `NoSubfields` so the accessors are a compile
    // error rather than a runtime 400.
    let subfielded = auto_subfields
        && resolved.mapping.subfields
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

    match &resolved.mapping.mapping_type {
        // A keyword with a declared enum order gets the order-aware `Enum` handle
        // (sorts on the prebaked `.sort` subfield); a bare keyword stays `Keyword`.
        MappingType::Keyword if resolved.mapping.enum_order.is_some() => string_handle("Enum"),
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
        // An `object` reaching here is either a `map` (dynamic-key object → a
        // kind-typed map handle) or an opaque `json` with no children; a group
        // or to-one join was handled by its own namespace.
        MappingType::Object => match resolved
            .mapping
            .map_values
            .as_deref()
            .map(|values| &values.mapping_type)
        {
            // A string map's keys are mapped like a plain field of the value
            // kind, so they carry the same subfield type-state.
            Some(MappingType::Text) => string_handle("TextMap"),
            Some(MappingType::Keyword) => string_handle("KeywordMap"),
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
            ) => number_map(kind_of(
                value,
                resolved
                    .mapping
                    .map_values
                    .as_deref()
                    .is_some_and(|values| values.decimal),
            )?),
            // Any other value kind has no typed map handle (the conversion
            // rejects non-leaf map values, so this is defensive) — fall back to
            // the opaque object handle so the field is still addressable.
            Some(_) => simple("Json"),
            None => simple("Json"),
        },
        MappingType::Nested => None,
        MappingType::Other(_) => simple("Json"),
    }
}

/// Every Rust keyword, strict and reserved, in any edition: what a generated
/// name must not be spelled as bare. `syn` parses by its own edition, which
/// misses the newest (`gen`), so the set is spelled out.
const KEYWORDS: &[&str] = &[
    "abstract", "as", "async", "await", "become", "box", "break", "const", "continue", "crate",
    "do", "dyn", "else", "enum", "extern", "false", "final", "fn", "for", "gen", "if", "impl",
    "in", "let", "loop", "macro", "match", "mod", "move", "mut", "override", "priv", "pub", "ref",
    "return", "self", "Self", "static", "struct", "super", "trait", "true", "try", "type",
    "typeof", "unsafe", "unsized", "use", "virtual", "where", "while", "yield",
];

/// A name generated from the document key `key`, as an identifier. A Rust
/// keyword becomes a raw identifier (`type` → `r#type`, matching a field the
/// caller spells `r#type`); the few a raw identifier cannot spell (`self`,
/// `Self`, `super`, `crate`) take a trailing `_`. A name no identifier can
/// spell (key `_`) is an error naming the key.
fn generated_ident(name: &str, key: &str) -> syn::Result<Ident> {
    match name {
        "self" | "Self" | "super" | "crate" => {
            return Ok(Ident::new(&format!("{name}_"), Span::call_site()));
        }
        _ if KEYWORDS.contains(&name) => return Ok(Ident::new_raw(name, Span::call_site())),
        _ => {}
    }
    syn::parse_str::<Ident>(name).map_err(|_| {
        syn::Error::new(
            Span::call_site(),
            format!(
                "the schema key `{key}` gives the generated name `{name}`, which is not a Rust \
                 identifier, so it has no query handle — rename the field in the schema"
            ),
        )
    })
}

/// A document key as a type-name fragment: `orders` → `Orders`,
/// `billingAddress` → `BillingAddress`, `postal_code` → `PostalCode`.
fn to_pascal_case(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    let mut upper_next = true;
    for ch in name.chars() {
        if ch == '_' || ch == '-' {
            upper_next = true;
        } else if upper_next {
            out.extend(ch.to_uppercase());
            upper_next = false;
        } else {
            out.push(ch);
        }
    }
    out
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

pub(crate) fn vec_inner(ty: &Type) -> Option<&Type> {
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

/// At a root `keyword` or `text` field, `opaque` turns off the variant checks
/// below, which read the type's `FlussoValueMeta`. For a type with no variants
/// (a newtype over a `String`, a non-`exhaustive` enum where the schema declares
/// none) those checks pass anyway, so `opaque` there turns off nothing. Whether
/// the type has the metadata is only knowable at const evaluation, so this is a
/// const assertion through `OpaqueProbe`, which also lets a hand-written
/// `FlussoValue` without the metadata keep its `opaque`.
fn opaque_turns_off_something(
    field: &DocField,
    resolved: &ResolvedField,
    scope: &str,
) -> Option<TokenStream> {
    field.opaque?;
    if field.skip
        || resolved.mapping.map_values.is_some()
        || !matches!(
            resolved.mapping.mapping_type,
            MappingType::Keyword | MappingType::Text
        )
    {
        return None;
    }
    let element = crate::fragment::checked_type(field, false)?;
    let key = &field.doc_key;
    let label = field_label(field);
    let rendered = crate::fragment::render(element);
    let (probe, message) = match &resolved.mapping.enum_order {
        Some(order) => (
            quote! { SKIPS_VARIANTS },
            format!(
                "`opaque` on {label} turns off nothing — the schema field `{key}` in {scope} \
                 declares variants [{}], but `{rendered}` declares none (it is not an enum, nor \
                 a newtype over one), so there is no variant check to turn off; remove `opaque`",
                order.join(", "),
            ),
        ),
        None => (
            quote! { SKIPS_EXHAUSTIVE },
            format!(
                "`opaque` on {label} turns off nothing — the schema field `{key}` in {scope} \
                 declares no `variants:` and `{rendered}` is not `exhaustive`, so no variant \
                 check runs here; remove `opaque`"
            ),
        ),
    };
    let span = field.opaque.unwrap_or_else(|| field.ty.span());
    Some(quote::quote_spanned! {span=>
        {
            #[allow(unused_imports)]
            use ::flusso_query::OpaqueWithoutMeta as _;
            assert!(::flusso_query::OpaqueProbe::<#element>::#probe, #message);
        }
    })
}

/// At a level with fields of its own, whether a custom type that is not a
/// `Vec` reads an array (`FlussoValueMeta::ARRAY`: a newtype fragment over a
/// `Vec`) exactly where the schema field is a `nested` array, and that the
/// element of a `Vec` does not read one too. `check_type`
/// lets such a type through at a `nested` field, since only const evaluation
/// knows. Read through `OpaqueProbe`, so `opaque` keeps this check, as it keeps
/// the array shape of a `Vec`.
fn array_newtype_check(
    field: &DocField,
    resolved: &ResolvedField,
    scope: &str,
) -> Option<TokenStream> {
    if !is_structured(resolved) {
        return None;
    }
    let inner = option_inner(field.ty).unwrap_or(field.ty);
    let (ty, in_vec) = vec_inner(inner).map_or((inner, false), |element| (element, true));
    if map_container_value(ty).is_some()
        || leaf_ident(ty).is_none_or(|name| is_primitive(Some(&name)) || name == "Value")
    {
        return None;
    }
    let key = &field.doc_key;
    let rendered = crate::fragment::render(ty);
    let nested = matches!(resolved.mapping.mapping_type, MappingType::Nested);
    let span = field.ty.span();
    let (kind, stands) = if nested {
        ("a `nested` array", quote::quote! { AT_NESTED })
    } else {
        ("an `object`", quote::quote! { AT_OBJECT })
    };
    let value_message = format!(
        "field `{key}` in {scope} is {kind} with fields of its own — `{rendered}` is not a \
         sub-document; declare a struct for it with `#[derive(FlussoFragment)]`"
    );
    let sub_document = quote::quote_spanned! {span=>
        {
            #[allow(unused_imports)]
            use ::flusso_query::OpaqueWithoutMeta as _;
            assert!(::flusso_query::OpaqueProbe::<#ty>::#stands, #value_message);
        }
    };
    if in_vec {
        let message = format!(
            "field `{key}` in {scope} is `Vec<{rendered}>`, an array of arrays — `{rendered}` is \
             a newtype over a `Vec<…>`, which already reads the array; drop the outer `Vec<…>`"
        );
        return Some(quote::quote_spanned! {span=>
            #sub_document
            {
                #[allow(unused_imports)]
                use ::flusso_query::OpaqueWithoutMeta as _;
                assert!(!::flusso_query::OpaqueProbe::<#ty>::READS_ARRAY, #message);
            }
        });
    }
    let message = if nested {
        format!(
            "field `{key}` in {scope} is a `nested` array — `{rendered}` does not read one; \
             declare it `Vec<…>`, or make `{rendered}` a `#[serde(transparent)]` \
             `FlussoFragment` newtype over a `Vec<…>`"
        )
    } else {
        format!(
            "field `{key}` in {scope} is an `object`, not an array — `{rendered}` is a newtype \
             over a `Vec<…>`; declare a struct for it"
        )
    };
    Some(quote::quote_spanned! {span=>
        #sub_document
        {
            #[allow(unused_imports)]
            use ::flusso_query::OpaqueWithoutMeta as _;
            assert!(::flusso_query::OpaqueProbe::<#ty>::ARRAY == #nested, #message);
        }
    })
}

/// Drive the fragment check into every embedded field.
///
/// The root is the only type that resolves the schema, so it bakes the whole
/// resolved subtree once and hands slices of it down: `children(LEVEL, "…")` for
/// a normal field, the level itself for a `#[serde(flatten)]` group whose keys
/// live here. Each call is spanned on its own field, so a failure deep in a
/// fragment reports the embedding that reached it.
///
/// Returns nothing when no field embeds a checkable type — a document of plain
/// leaves bakes no constants at all.
pub(crate) fn embed_checks(
    level: &[ResolvedField],
    fields: &[DocField],
    scope: &str,
) -> TokenStream {
    let mut calls = TokenStream::new();
    for field in fields {
        let span = field.ty.span();
        if field.flatten {
            if let Some(element) = crate::fragment::embedded_type(field, false) {
                calls.extend(crate::fragment::flattened_check(
                    element,
                    span,
                    quote! { __FLUSSO_LEVEL },
                ));
            }
            continue;
        }
        let key = &field.doc_key;
        let Some(resolved) = level.iter().find(|r| r.name.as_ref() == *key) else {
            continue;
        };
        if !field.skip {
            calls.extend(array_newtype_check(field, resolved, scope));
        }
        if let Some(check) = opaque_turns_off_something(field, resolved, scope) {
            calls.extend(check);
            continue;
        }
        let Some(element) = crate::fragment::embedded_type(field, is_structured(resolved)) else {
            continue;
        };
        // Unlike a fragment, a root *has* the mapping — so it drives the check
        // only where there is a real sub-shape to walk into. A custom scalar
        // type (an enum at a keyword field) is already covered by the
        // `FlussoValue<K>` bound `check_type` emits; adding a no-op check there
        // would only pile a second, worse error onto a missing derive.
        if matches!(
            resolved.mapping.mapping_type,
            MappingType::Object | MappingType::Nested
        ) && resolved.mapping.map_values.is_none()
        {
            // The `map_kind_ok` assert mirrors what a fragment checks: a map
            // wrapper (its `MAP_VALUES` is non-empty) cannot stand in for a
            // structured object or a nested array — only for a `map` (already
            // handled by `check_type`, which never reaches here) or an opaque
            // `json`. Whether the type *is* a map is only knowable at const
            // evaluation, so this cannot be a macro-time error. Reading
            // `MAP_VALUES` also doubles as the embeddability probe: a plain
            // un-derived struct fails it with `FlussoValueMeta`'s directed
            // note ("add a derive, or mark the field opaque") rather than
            // only a bare "no `__flusso_check`".
            let map_message = format!(
                "field `{key}` in {scope} is not a `map` — `{}` is a map type, so it can \
                 only hold a `map` field with a matching value kind",
                crate::fragment::render(element),
            );
            calls.extend(quote::quote_spanned! {span=>
                assert!(
                    ::flusso_query::map_kind_ok(
                        __FLUSSO_LEVEL, #key,
                        <#element as ::flusso_query::FlussoValueMeta>::MAP_VALUES,
                    ),
                    #map_message
                );
                <#element>::__flusso_check(::flusso_query::children(__FLUSSO_LEVEL, #key));
            });
        }
        // Only an ordered enum has a variant set to disagree with. The messages
        // are baked here because this is the side that knows the schema's
        // variants. An `exhaustive` type additionally demands the whole
        // declared set — and a field that declares none at all, where the
        // guarantee would verify nothing.
        match &resolved.mapping.enum_order {
            Some(order) => {
                let covered_message = format!(
                    "field `{key}` in {scope} declares variants [{}] — the Rust type declares \
                     one that is not among them, so it could never match a document",
                    order.join(", "),
                );
                let exhausted_message = format!(
                    "field `{key}` in {scope} declares variants [{}] — the Rust type is marked \
                     `exhaustive` but does not cover them all",
                    order.join(", "),
                );
                calls.extend(quote::quote_spanned! {span=>
                    assert!(
                        ::flusso_query::variants_covered(
                            __FLUSSO_LEVEL, #key,
                            <#element as ::flusso_query::FlussoValueMeta>::VARIANTS,
                        ),
                        #covered_message
                    );
                    assert!(
                        !<#element as ::flusso_query::FlussoValueMeta>::EXHAUSTIVE
                            || ::flusso_query::variants_exhausted(
                                __FLUSSO_LEVEL, #key,
                                <#element as ::flusso_query::FlussoValueMeta>::VARIANTS,
                            ),
                        #exhausted_message
                    );
                });
            }
            // Only where an enum can actually sit (a string leaf) — reading
            // the meta on any other field would pile a second error onto a
            // missing derive, which `check_type`'s deferred bound already
            // reports better.
            None if matches!(
                resolved.mapping.mapping_type,
                MappingType::Keyword | MappingType::Text
            ) && resolved.mapping.map_values.is_none() =>
            {
                let message = format!(
                    "field `{key}` in {scope} declares no variants — the Rust type is marked \
                     `exhaustive`, but there is nothing to cover; declare `variants:` on the \
                     schema field or drop the marker",
                );
                calls.extend(quote::quote_spanned! {span=>
                    assert!(
                        !<#element as ::flusso_query::FlussoValueMeta>::EXHAUSTIVE,
                        #message
                    );
                });
            }
            None => {}
        }
    }

    if calls.is_empty() {
        return TokenStream::new();
    }
    let baked = crate::spec::bake_level(level);
    quote! {
        const _: () = {
            const __FLUSSO_LEVEL: &[::flusso_query::FieldSpec] = #baked;
            #calls
        };
    }
}
