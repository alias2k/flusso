#![doc = include_str!("../README.md")]

use proc_macro::TokenStream;
use proc_macro2::{Span, TokenStream as TokenStream2};
use syn::spanned::Spanned;
use syn::{Data, DeriveInput, LitStr, parse_macro_input};

mod doc;
mod fragment;
mod map;
mod multi;
mod resolve;
mod spec;
mod value;

/// Derive the typed query surface for a flusso index — the **root**.
///
/// This is the only type bound to a schema, and it owns the *whole* surface: a
/// handle for every field at every level, reached through a generated namespace
/// per container (`User::account().tier()`, `UserOrders::total()`). It also
/// drives validation into every shape it embeds.
///
/// ```ignore
/// #[derive(serde::Deserialize, FlussoRoot)]
/// #[flusso(index = "users")]
/// struct User { id: i32, email: String }
/// ```
#[proc_macro_derive(FlussoRoot, attributes(flusso))]
pub fn derive_flusso_root(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    expand(input).into()
}

/// Deprecated alias for [`macro@FlussoRoot`].
///
/// The derive was renamed when the root/fragment split landed: a root binds to
/// an index, a [`macro@FlussoFragment`] describes a location-free shape. This
/// alias expands identically to `FlussoRoot`.
#[deprecated(
    since = "0.14.0",
    note = "renamed to `FlussoRoot`; a child shape now uses `#[derive(FlussoFragment)]` instead of `path = \"…\"`"
)]
#[proc_macro_derive(FlussoDocument, attributes(flusso))]
pub fn derive_flusso_document(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    expand(input).into()
}

/// Derive a **location-free** document shape — a fragment.
///
/// A fragment names no index and no path, so one declaration can be embedded at
/// several paths (`billingAddress` and `shippingAddress`) or across indexes, and
/// even live in a shared crate. Each root that embeds it validates it against
/// the mapping at *that* path, recursively into any fragment it contains.
///
/// ```ignore
/// #[derive(serde::Deserialize, FlussoFragment)]
/// struct Address { city: String, zip: Option<String> }
/// ```
#[proc_macro_derive(FlussoFragment, attributes(flusso))]
pub fn derive_flusso_fragment(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    fragment::expand(input).into()
}

/// Implement `flusso_query::FlussoValue<K>` for an enum or newtype wrapper, so
/// it may stand in for a field of kind `K` in a `FlussoDocument` struct. The
/// kind is chosen with `#[flusso(keyword)]` (the default), `#[flusso(text)]`,
/// `#[flusso(number)]`, or `#[flusso(date)]`.
///
/// ```ignore
/// #[derive(serde::Serialize, serde::Deserialize, FlussoValue)]
/// #[serde(rename_all = "camelCase")]
/// #[flusso(keyword)]
/// enum AccountTier { Pro, Enterprise, Free }
/// ```
#[proc_macro_derive(FlussoValue, attributes(flusso))]
pub fn derive_flusso_value(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    value::expand(input).into()
}

/// Implement `flusso_query::FlussoMap<K>` for a newtype wrapper over a map, so
/// it may stand in for a `map` field of value kind `K` in a `FlussoDocument`
/// struct. `HashMap<String, V>` already implements the trait directly — this is
/// only for wrapping it. The kind is chosen with `#[flusso(keyword)]` (the
/// default), `#[flusso(text)]`, `#[flusso(number)]`, or `#[flusso(date)]`.
///
/// ```ignore
/// #[derive(serde::Deserialize, FlussoMap)]
/// #[flusso(text)]
/// struct Translations(std::collections::HashMap<String, String>);
/// ```
#[proc_macro_derive(FlussoMap, attributes(flusso))]
pub fn derive_flusso_map(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    map::expand(input).into()
}

/// Implement `flusso_query::FlussoMultiDocument` for an enum of document
/// types — the combined-search union. Each variant is a single-field tuple
/// variant whose payload implements `FlussoDocument`; the generated impl
/// lists every variant's index (`TARGETS`) and decodes each hit into the
/// variant matching its physical `_index`. Purely syntactic — no
/// `flusso.toml`, no schema resolution.
///
/// ```ignore
/// #[derive(serde::Serialize, FlussoMultiDocument)]
/// enum SearchItem {
///     User(User),
///     Product(Product),
/// }
/// ```
#[proc_macro_derive(FlussoMultiDocument)]
pub fn derive_flusso_multi_document(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    multi::expand(input).into()
}

/// The `#[flusso(…)]` container attributes, plus serde's container `rename_all`.
struct Attrs {
    index: String,
    /// Span of the `index = "…"` value — where index-resolution errors point.
    index_span: Span,
    /// The **deprecated** `path = "…"`: still validates against that level, but
    /// generates nothing. Superseded by `#[derive(FlussoFragment)]`.
    path: Option<LitStr>,
    config: Option<String>,
    rename_all: Option<String>,
}

impl Attrs {
    fn parse(input: &DeriveInput) -> syn::Result<Self> {
        let mut index: Option<String> = None;
        let mut index_span = input.ident.span();
        let mut path: Option<LitStr> = None;
        let mut config: Option<String> = None;
        let mut rename_all: Option<String> = None;

        for attr in &input.attrs {
            if attr.path().is_ident("flusso") {
                attr.parse_nested_meta(|meta| {
                    if meta.path.is_ident("index") {
                        let lit: LitStr = meta.value()?.parse()?;
                        index_span = lit.span();
                        index = Some(lit.value());
                    } else if meta.path.is_ident("path") {
                        path = Some(meta.value()?.parse()?);
                    } else if meta.path.is_ident("config") {
                        let lit: LitStr = meta.value()?.parse()?;
                        config = Some(lit.value());
                    } else {
                        return Err(meta.error(
                            "unknown `flusso` attribute (expected `index`, `config`, \
                                 or the deprecated `path`)",
                        ));
                    }
                    Ok(())
                })?;
            } else if attr.path().is_ident("serde") {
                // Best-effort: read the container `rename_all`, ignore the rest.
                let _ = attr.parse_nested_meta(|meta| {
                    if meta.path.is_ident("rename_all")
                        && let Ok(value) = meta.value()
                        && let Ok(lit) = value.parse::<LitStr>()
                    {
                        rename_all = Some(lit.value());
                    }
                    Ok(())
                });
            }
        }

        let index = index.ok_or_else(|| {
            syn::Error::new(
                input.ident.span(),
                "missing `#[flusso(index = \"…\")]` — name the index this document binds to",
            )
        })?;

        Ok(Attrs {
            index,
            index_span,
            path,
            config,
            rename_all,
        })
    }
}

fn expand(input: DeriveInput) -> TokenStream2 {
    if !input.generics.params.is_empty() {
        return syn::Error::new(
            input.generics.span(),
            "FlussoDocument does not support generic structs",
        )
        .to_compile_error();
    }

    let attrs = match Attrs::parse(&input) {
        Ok(attrs) => attrs,
        Err(error) => return error.to_compile_error(),
    };

    let struct_fields = match &input.data {
        Data::Struct(data) => &data.fields,
        _ => {
            return syn::Error::new(
                input.ident.span(),
                "FlussoDocument can only be derived for a struct",
            )
            .to_compile_error();
        }
    };

    let fields = match doc::parse_fields(struct_fields, attrs.rename_all.as_deref()) {
        Ok(fields) => fields,
        Err(error) => return error.to_compile_error(),
    };

    let resolved = match resolve::resolve(&attrs.index, attrs.config.as_deref()) {
        Ok(resolved) => resolved,
        Err(message) => return syn::Error::new(attrs.index_span, message).to_compile_error(),
    };

    // The deprecated `path = "…"` form: validate against that level and stop.
    // It generates no handles and no entry points — the surface lives on the
    // root now — so it only keeps existing struct declarations compiling while
    // their call sites move.
    if let Some(path) = &attrs.path {
        let level = match resolved.fields_at(&path.value()) {
            Ok(level) => level,
            Err(message) => return syn::Error::new(path.span(), message).to_compile_error(),
        };
        let scope = format!("`{}` in index `{}`", path.value(), attrs.index);
        let mut out = deprecation_notice(&input.ident, path);
        let (errors, asserts) = doc::validate(level, &fields, &scope);
        out.extend(asserts);
        out.extend(doc::embed_checks(level, &fields, &scope));
        out.extend(fragment::embeddable_leaf(&input.ident));
        for error in errors {
            out.extend(error.to_compile_error());
        }
        return out;
    }

    let level = resolved.mapping.fields.as_slice();
    let scope = format!("index `{}`", attrs.index);
    let hash = resolved.mapping.hash.to_string();
    let tracked: Vec<String> = resolved
        .tracked
        .iter()
        .filter(|path| path.is_file())
        .map(|path| path.to_string_lossy().into_owned())
        .collect();

    // The query surface follows the *schema*, not the struct: every level of the
    // index gets handles, whether or not this projection deserializes it.
    let items = doc::codegen(
        &input.ident,
        &input.vis,
        &attrs.index,
        &hash,
        level,
        &fields,
        &tracked,
        resolved.auto_subfields,
    );

    let mut out = items;
    let (errors, asserts) = doc::validate(level, &fields, &scope);
    out.extend(asserts);

    // This struct is the only one here that resolves the schema, so it bakes the
    // level and drives the check into every shape it embeds — recursively, and
    // once per embedding site.
    out.extend(doc::embed_checks(level, &fields, &scope));
    // …and it is itself embeddable, though its check is a no-op: it already
    // validated every field against this level as part of its own expansion.
    out.extend(fragment::embeddable_leaf(&input.ident));

    for error in errors {
        out.extend(error.to_compile_error());
    }
    out
}

// Dev-dependencies are exercised only by the integration tests in `tests/`;
// reference them here so the lib's own test target doesn't flag them as unused.
#[cfg(test)]
mod dev_deps {
    use flusso_query as _;
    use serde as _;
    use serde_json as _;
    use trybuild as _;
}

/// A real deprecation warning for `#[flusso(path = "…")]`.
///
/// A proc macro can't warn directly on stable, but it can emit a `#[deprecated]`
/// item and use it. Unlike the enum-coverage case, the condition here is known
/// at macro time — the attribute is either written or it isn't — so the warning
/// fires exactly when it should.
fn deprecation_notice(ident: &syn::Ident, path: &LitStr) -> TokenStream2 {
    let note = format!(
        "`path = \"{}\"` is deprecated: drop the `#[flusso(…)]` attribute and derive \
         `FlussoFragment` instead. A fragment is validated wherever it is embedded, so one \
         declaration covers every path it appears at; handles for that level come from the \
         root's generated namespace. This form still validates, but generates no handles \
         and no `query`/`get`.",
        path.value(),
    );
    let flag = syn::Ident::new(
        &format!(
            "__FLUSSO_DEPRECATED_PATH_{}",
            ident.to_string().to_uppercase()
        ),
        path.span(),
    );
    quote::quote_spanned! {path.span()=>
        #[deprecated(note = #note)]
        #[allow(non_upper_case_globals)]
        const #flag: () = ();

        const _: () = {
            #[allow(clippy::let_unit_value)]
            let _ = #flag;
        };
    }
}
