#![doc = include_str!("../README.md")]

use proc_macro::TokenStream;
use proc_macro2::{Span, TokenStream as TokenStream2};
use syn::spanned::Spanned;
use syn::{Data, DeriveInput, Ident, LitStr, parse_macro_input};

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
/// it may stand in for a field of kind `K` in a `FlussoRoot` struct. The
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
    /// `scope_mod = "…"` — rename the module the generated scopes live in, for
    /// the one case the module itself can clash: the caller already has a module
    /// by that name.
    scope_mod: Option<Ident>,
    config: Option<String>,
    rename_all: Option<String>,
}

impl Attrs {
    fn parse(input: &DeriveInput) -> syn::Result<Self> {
        let mut index: Option<String> = None;
        let mut index_span = input.ident.span();
        let mut scope_mod: Option<Ident> = None;
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
                        return Err(meta.error(
                            "`path` no longer exists — the root generates handles for every \
                             level of its index. Make this a `#[derive(FlussoFragment)]` shape \
                             and embed it; it is validated at whatever path it lands on",
                        ));
                    } else if meta.path.is_ident("scope_mod") {
                        let lit: LitStr = meta.value()?.parse()?;
                        scope_mod = Some(lit.parse::<Ident>().map_err(|_| {
                            syn::Error::new(
                                lit.span(),
                                "`scope_mod` must be a valid module name, e.g. \
                                 `scope_mod = \"user_queries\"`",
                            )
                        })?);
                    } else if meta.path.is_ident("config") {
                        let lit: LitStr = meta.value()?.parse()?;
                        config = Some(lit.value());
                    } else {
                        return Err(meta.error(
                            "unknown `flusso` attribute (expected `index`, `config`, \
                             or `scope_mod`)",
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
            scope_mod,
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
        attrs.scope_mod.as_ref(),
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
