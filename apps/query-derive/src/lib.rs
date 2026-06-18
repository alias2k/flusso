//! `#[derive(FlussoDocument)]` — the proc-macro behind `flusso-query`.
//!
//! It does **not** generate the document struct. The developer writes the struct;
//! this derive, at compile time:
//!
//! 1. discovers `flusso.toml` (up from `CARGO_MANIFEST_DIR`, or `config = "…"` /
//!    `FLUSSO_CONFIG`) and resolves the named index's mapping — no database;
//! 2. validates each declared field against that mapping (exists / type /
//!    nullability), reporting every problem at once with precise spans;
//! 3. generates the typed query surface (`Type::field()` handles, `get`/`query`,
//!    `SCHEMA_HASH`) that targets the `flusso-query` runtime.
//!
//! Two companion derives ship alongside it: [`FlussoValue`](derive_flusso_value)
//! (a Rust enum/newtype standing in for a leaf field) and
//! [`FlussoMultiDocument`](derive_flusso_multi_document) (the combined-search
//! union over several document types).
//!
//! Use them through `flusso-query`'s `derive` feature: `use flusso_query::FlussoDocument`.

use proc_macro::TokenStream;
use proc_macro2::{Span, TokenStream as TokenStream2};
use quote::quote;
use syn::spanned::Spanned;
use syn::{Data, DeriveInput, LitStr, parse_macro_input};

mod doc;
mod multi;
mod resolve;
mod value;

use resolve::Scope;

/// Derive the typed query surface for a flusso document struct.
///
/// ```ignore
/// #[derive(serde::Deserialize, FlussoDocument)]
/// #[flusso(index = "users")]
/// struct User { id: i32, email: String }
/// ```
#[proc_macro_derive(FlussoDocument, attributes(flusso))]
pub fn derive_flusso_document(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    expand(input).into()
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
    path: Option<String>,
    /// Span of the `path = "…"` value — where path-walk errors point.
    path_span: Span,
    config: Option<String>,
    rename_all: Option<String>,
}

impl Attrs {
    fn parse(input: &DeriveInput) -> syn::Result<Self> {
        let mut index: Option<String> = None;
        let mut index_span = input.ident.span();
        let mut path: Option<String> = None;
        let mut path_span = input.ident.span();
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
                        let lit: LitStr = meta.value()?.parse()?;
                        path_span = lit.span();
                        path = Some(lit.value());
                    } else if meta.path.is_ident("config") {
                        let lit: LitStr = meta.value()?.parse()?;
                        config = Some(lit.value());
                    } else {
                        return Err(meta.error(
                            "unknown `flusso` attribute (expected `index`, `path`, or `config`)",
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
            path_span,
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

    let level = match resolved.fields_at(attrs.path.as_deref()) {
        Ok(level) => level,
        Err(message) => return syn::Error::new(attrs.path_span, message).to_compile_error(),
    };

    // The scope this struct's handles live in: `Root` (untagged) at the root and
    // through flattened objects; the struct's own type under a `nested` array.
    let scope_tag = match resolved.scope_at(attrs.path.as_deref()) {
        Ok(Scope::Root) => quote! { ::flusso_query::Root },
        Ok(Scope::SelfTagged) => quote! { Self },
        Err(message) => return syn::Error::new(attrs.path_span, message).to_compile_error(),
    };

    let scope = match &attrs.path {
        Some(path) => format!("`{path}` in index `{}`", attrs.index),
        None => format!("index `{}`", attrs.index),
    };
    let prefix = attrs.path.as_deref().unwrap_or("");
    let is_root = attrs.path.is_none();
    let hash = resolved.mapping.hash.to_string();
    let tracked: Vec<String> = resolved
        .tracked
        .iter()
        .filter(|path| path.is_file())
        .map(|path| path.to_string_lossy().into_owned())
        .collect();

    // The query surface always generates (it follows the schema, not the
    // struct); field validation errors are reported alongside it.
    let items = doc::codegen(
        &input.ident,
        &attrs.index,
        &hash,
        prefix,
        is_root,
        &scope_tag,
        level,
        &fields,
        &tracked,
    );

    let mut out = items;
    let (errors, asserts) = doc::validate(level, &fields, &scope);
    out.extend(asserts);
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
