#![doc = include_str!("../README.md")]
// The dev-dependencies (kernel, schemars, serde, trybuild) serve the integration
// tests only; the lib's own test build has nothing to use them for.
#![cfg_attr(test, allow(unused_crate_dependencies))]

use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::spanned::Spanned;
use syn::{
    Attribute, Data, DeriveInput, Expr, Fields, Ident, Lit, LitStr, Type, parse_macro_input,
};

/// Derive `kernel::AdapterConfig` for an adapter's configuration struct.
///
/// Struct attribute (both keys required):
///
/// ```ignore
/// #[adapter(port = sink, kind = "opensearch")]
/// ```
///
/// The struct must also carry `#[serde(deny_unknown_fields)]` (a typo in a
/// user's options must fail, not vanish) and derive `Serialize`,
/// `Deserialize`, and `schemars::JsonSchema`, which the trait bounds require.
///
/// Field attribute:
///
/// ```ignore
/// #[adapter(example = "https://search:9200")]
/// ```
///
/// `example()` builds the struct from these. A string literal is converted
/// with `Into`, so it fits `String`, `Secret`, or `PathBuf`; any other
/// expression is used verbatim (`example = 500`, `example = SslMode::Require`).
/// For an `Option<T>` field the value is wrapped in `Some`. A field with no
/// example uses its serde default (`#[serde(default)]` or `default = "fn"`);
/// an `Option` with neither is `None`; anything else is a compile error, so a
/// required option can never ship undocumented.
#[proc_macro_derive(AdapterConfig, attributes(adapter))]
pub fn derive_adapter_config(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    match expand(input) {
        Ok(tokens) => tokens.into(),
        Err(error) => error.to_compile_error().into(),
    }
}

fn expand(input: DeriveInput) -> syn::Result<TokenStream2> {
    let name = &input.ident;
    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();

    let ContainerAttrs { port, kind } = container_attrs(&input)?;
    if !has_serde_flag(&input.attrs, "deny_unknown_fields") {
        return Err(syn::Error::new(
            input.ident.span(),
            "an adapter config must carry `#[serde(deny_unknown_fields)]` so a misspelled option is rejected instead of ignored",
        ));
    }

    let Data::Struct(data) = &input.data else {
        return Err(syn::Error::new(
            input.span(),
            "`AdapterConfig` can only be derived for a struct with named fields",
        ));
    };
    let Fields::Named(fields) = &data.fields else {
        return Err(syn::Error::new(
            input.span(),
            "`AdapterConfig` can only be derived for a struct with named fields",
        ));
    };

    let mut initializers = Vec::new();
    for field in &fields.named {
        let ident = field
            .ident
            .as_ref()
            .ok_or_else(|| syn::Error::new(field.span(), "expected a named field"))?;
        let value = field_example(field, ident)?;
        initializers.push(quote! { #ident: #value });
    }

    let port = match port.to_string().as_str() {
        "source" => quote!(::kernel::Port::Source),
        "stream" => quote!(::kernel::Port::Stream),
        "sink" => quote!(::kernel::Port::Sink),
        other => {
            return Err(syn::Error::new(
                port.span(),
                format!("unknown port `{other}`; expected `source`, `stream`, or `sink`"),
            ));
        }
    };

    Ok(quote! {
        impl #impl_generics ::kernel::AdapterConfig for #name #ty_generics #where_clause {
            const PORT: ::kernel::Port = #port;
            const KIND: &'static str = #kind;

            fn example() -> Self {
                Self { #(#initializers,)* }
            }
        }
    })
}

struct ContainerAttrs {
    port: Ident,
    kind: LitStr,
}

fn container_attrs(input: &DeriveInput) -> syn::Result<ContainerAttrs> {
    let mut port = None;
    let mut kind = None;
    for attr in input.attrs.iter().filter(|a| a.path().is_ident("adapter")) {
        attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("port") {
                let value = meta.value()?;
                port = Some(value.parse::<Ident>()?);
                Ok(())
            } else if meta.path.is_ident("kind") {
                let value = meta.value()?;
                kind = Some(value.parse::<LitStr>()?);
                Ok(())
            } else {
                Err(meta.error("unknown `adapter` key; expected `port` or `kind`"))
            }
        })?;
    }
    let port = port.ok_or_else(|| {
        syn::Error::new(
            input.ident.span(),
            "missing `#[adapter(port = source|stream|sink, kind = \"…\")]`: `port` is required",
        )
    })?;
    let kind = kind.ok_or_else(|| {
        syn::Error::new(
            input.ident.span(),
            "missing `#[adapter(port = …, kind = \"…\")]`: `kind` is required (the `type = \"…\"` token in flusso.toml)",
        )
    })?;
    if kind.value().is_empty() {
        return Err(syn::Error::new(kind.span(), "`kind` must not be empty"));
    }
    Ok(ContainerAttrs { port, kind })
}

/// The expression that initializes one field of the example.
fn field_example(field: &syn::Field, ident: &Ident) -> syn::Result<TokenStream2> {
    let optional = is_option(&field.ty);
    if let Some(expr) = adapter_example(&field.attrs)? {
        let value = match &expr {
            Expr::Lit(lit) if matches!(lit.lit, Lit::Str(_)) => {
                quote!(::core::convert::Into::into(#expr))
            }
            _ => quote!(#expr),
        };
        return Ok(if optional && !is_option_literal(&expr) {
            quote!(::core::option::Option::Some(#value))
        } else {
            value
        });
    }
    match serde_default(&field.attrs)? {
        Some(SerdeDefault::Trait) => Ok(quote!(::core::default::Default::default())),
        Some(SerdeDefault::Path(path)) => Ok(quote!(#path())),
        None if optional => Ok(quote!(::core::option::Option::None)),
        None => Err(syn::Error::new(
            ident.span(),
            format!(
                "field `{ident}` has no default, so it needs `#[adapter(example = …)]` (the example is what the docs and the editor schema show)"
            ),
        )),
    }
}

fn adapter_example(attrs: &[Attribute]) -> syn::Result<Option<Expr>> {
    let mut example = None;
    for attr in attrs.iter().filter(|a| a.path().is_ident("adapter")) {
        attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("example") {
                let value = meta.value()?;
                example = Some(value.parse::<Expr>()?);
                Ok(())
            } else {
                Err(meta.error("unknown `adapter` field key; expected `example`"))
            }
        })?;
    }
    Ok(example)
}

enum SerdeDefault {
    Trait,
    Path(syn::ExprPath),
}

fn serde_default(attrs: &[Attribute]) -> syn::Result<Option<SerdeDefault>> {
    let mut found = None;
    for attr in attrs.iter().filter(|a| a.path().is_ident("serde")) {
        attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("default") {
                if meta.input.peek(syn::Token![=]) {
                    let lit: LitStr = meta.value()?.parse()?;
                    found = Some(SerdeDefault::Path(lit.parse()?));
                } else {
                    found = Some(SerdeDefault::Trait);
                }
            } else if meta.input.peek(syn::Token![=]) {
                let _: Expr = meta.value()?.parse()?;
            } else if meta.input.peek(syn::token::Paren) {
                let _ = meta.parse_nested_meta(|_| Ok(()));
            }
            Ok(())
        })?;
    }
    Ok(found)
}

fn has_serde_flag(attrs: &[Attribute], flag: &str) -> bool {
    let mut found = false;
    for attr in attrs.iter().filter(|a| a.path().is_ident("serde")) {
        let _ = attr.parse_nested_meta(|meta| {
            if meta.path.is_ident(flag) {
                found = true;
            } else if meta.input.peek(syn::Token![=]) {
                let _: Expr = meta.value()?.parse()?;
            } else if meta.input.peek(syn::token::Paren) {
                let _ = meta.parse_nested_meta(|_| Ok(()));
            }
            Ok(())
        });
    }
    found
}

fn is_option(ty: &Type) -> bool {
    match ty {
        Type::Path(path) => path
            .path
            .segments
            .last()
            .is_some_and(|segment| segment.ident == "Option"),
        _ => false,
    }
}

/// `Some(…)` / `None` written by hand; not re-wrapped.
fn is_option_literal(expr: &Expr) -> bool {
    match expr {
        Expr::Call(call) => {
            matches!(&*call.func, Expr::Path(p) if p.path.segments.last().is_some_and(|s| s.ident == "Some"))
        }
        Expr::Path(p) => p.path.segments.last().is_some_and(|s| s.ident == "None"),
        _ => false,
    }
}
