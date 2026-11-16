//! What a root's read-time fields generate: the context-taking entry points,
//! one handle per rule field, the `Computation` every fetched document goes
//! through, and the `doc()` namespace of rule handles.
//!
//! The runtime half is `flusso_query::rules`. This module decides, from the
//! schema alone, which stored fields a rule may read (see [`rule_handles`]):
//! the ones whose tests Rust decides exactly as the index does.

use proc_macro2::{Span, TokenStream};
use quote::quote;
use syn::ext::IdentExt;
use syn::spanned::Spanned;
use syn::{Ident, Type};

use config::{MappingType, ResolvedField};

use crate::doc::{
    DocField, ReadTime, generated_ident, has_namespace, kind_of, to_pascal_case, to_snake_case,
};

/// The error for a `rules` or `computed` field on a fragment.
pub(crate) fn read_time_in_fragment(read_time: &ReadTime) -> syn::Error {
    syn::Error::new(
        read_time.path().span(),
        format!(
            "`{}` decides a field from the root's context, and a fragment has none: it is \
             embedded wherever a root puts it — declare the field on the `FlussoRoot` struct, \
             and remove `{0}` here",
            read_time.attr()
        ),
    )
}

/// The names a root's read-time fields take from the query handles of its
/// stored fields.
#[derive(Default)]
pub(crate) struct ReadTimeNames {
    /// Document keys a read-time field writes. A stored field under one is
    /// replaced on every fetched document, so its handle moves to
    /// `stored_<name>()`: the plain name would query a value no document shows.
    keys: std::collections::HashSet<String>,
    /// Handle names taken by a rule field (`Contract::state(&cx)`).
    rule_handles: std::collections::HashSet<String>,
    /// The root has a context, so the [`ENTRY_POINTS`] are its own.
    entry_points: bool,
}

impl ReadTimeNames {
    pub(crate) fn of(fields: &[DocField], entry_points: bool) -> Self {
        let read_time = || fields.iter().filter(|field| field.read_time.is_some());
        Self {
            keys: read_time().map(|field| field.doc_key.clone()).collect(),
            rule_handles: read_time()
                .filter(|field| matches!(field.read_time, Some(ReadTime::Rules(_))))
                .filter_map(|field| field.ident.map(|ident| ident.unraw().to_string()))
                .collect(),
            entry_points,
        }
    }

    /// The handle name of the stored root field `key`, whose plain name is
    /// `name`: `stored_<name>` when the root itself already uses the plain
    /// one, for a read-time field or for an entry point.
    pub(crate) fn stored_handle(&self, key: &str, name: &str) -> String {
        let entry_point = self.entry_points && ENTRY_POINTS.contains(&name);
        if entry_point || self.keys.contains(key) || self.rule_handles.contains(name) {
            format!("stored_{name}")
        } else {
            name.to_owned()
        }
    }
}

/// What a root with a context generates for itself beside the query handles.
const ENTRY_POINTS: &[&str] = &["doc", "query", "get"];

/// What a root's `context`, `rules` and `computed` attributes generate.
pub(crate) struct ReadTimeItems {
    /// Items of its `FlussoRoot` impl.
    pub(crate) root: TokenStream,
    /// Inherent items, beside the query handles.
    pub(crate) inherent: TokenStream,
}

/// With a context the entry points take it (`Contract::query(&cx)`,
/// `Contract::get(&cx, …)`), each rule field gets a handle taking it
/// (`Contract::state(&cx)`), and every fetched document gets its read-time
/// fields written into it before it decodes.
///
/// Each function is called with the field's own type spelled out, so a
/// function that returns another type is an error on the attribute naming it.
pub(crate) fn read_time_items(
    module: &Ident,
    level: &[ResolvedField],
    fields: &[DocField],
    context: Option<&Type>,
) -> ReadTimeItems {
    let Some(context) = context else {
        let orphans = fields.iter().filter_map(|field| {
            let read_time = field.read_time.as_ref()?;
            Some(
                syn::Error::new(
                    read_time.path().span(),
                    format!(
                        "a `{}` field is decided from request-time values, and this root names \
                         no type for them — add `context = YourType` to the struct's \
                         `#[flusso(…)]` (the type the function takes by reference)",
                        read_time.attr()
                    ),
                )
                .to_compile_error(),
            )
        });
        return ReadTimeItems {
            root: quote! { type Context = (); },
            inherent: quote! { #(#orphans)* },
        };
    };

    let mut steps = TokenStream::new();
    let mut handles = TokenStream::new();
    for field in fields {
        let key = &field.doc_key;
        let ty = field.ty;
        match &field.read_time {
            None => {}
            Some(ReadTime::Computed(function)) => {
                steps.extend(quote::quote_spanned! {function.span()=>
                    .computed::<_, #ty, _>(#key, #function)
                });
            }
            Some(ReadTime::Rules(function)) => {
                steps.extend(quote::quote_spanned! {function.span()=>
                    .rules::<#ty, _>(#key, #function)
                });
                let Some(name) = field.ident else { continue };
                if ENTRY_POINTS.contains(&name.unraw().to_string().as_str()) {
                    handles.extend(
                        syn::Error::new(
                            name.span(),
                            format!(
                                "the rule field `{name}` would generate `{name}(&cx)`, which a \
                                 root with a context already has — rename the field, and keep \
                                 its document key with `#[serde(rename = \"{key}\")]`"
                            ),
                        )
                        .to_compile_error(),
                    );
                    continue;
                }
                let variants = level
                    .iter()
                    .find(|resolved| resolved.name.as_ref() == key.as_str())
                    .and_then(|resolved| resolved.mapping.enum_order.as_ref())
                    .map(|order| order.iter().map(ToString::to_string).collect::<Vec<_>>())
                    .unwrap_or_default();
                let doc = format!(
                    "`{key}` as its rules decide it for `cx`: filter, sort and count on the \
                     value every fetched document shows."
                );
                // Only the call carries the attribute's span: a mismatch is
                // reported there, while the `pub fn` stays the derive's own
                // (a `pub` under the caller's span trips `unreachable_pub`).
                let rules = quote::quote_spanned! {function.span()=> #function(cx) };
                handles.extend(quote! {
                    #[doc = #doc]
                    pub fn #name(cx: &#context) -> ::flusso_query::rules::RuleField<#ty> {
                        ::flusso_query::rules::RuleField::<#ty>::new(#rules, &[ #(#variants),* ])
                    }
                });
            }
        }
    }

    ReadTimeItems {
        root: quote! {
            type Context = #context;

            fn computation(cx: &Self::Context) -> ::flusso_query::rules::Computation {
                ::flusso_query::rules::Computation::with(cx) #steps .finish()
            }
        },
        inherent: quote! {
            /// The stored fields a rule can read, each with the tests a rule
            /// can make: `Self::doc().ends_at().lt(cx.now)`.
            pub fn doc() -> #module::rules::Doc {
                #module::rules::Doc
            }

            /// Start a typed query. `cx` decides the read-time fields of every
            /// document it returns.
            pub fn query(cx: &#context) -> ::flusso_query::Search<Self> {
                <Self as ::flusso_query::FlussoRoot>::query_in(cx)
            }

            /// Fetch one document by id, its read-time fields decided by `cx`.
            pub fn get(
                cx: &#context,
                client: &::flusso_query::Client,
                id: impl ::std::fmt::Display,
            ) -> impl ::std::future::Future<Output = ::flusso_query::Result<Option<Self>>> {
                <Self as ::flusso_query::FlussoRoot>::get_in(cx, client, id)
            }

            #handles
        },
    }
}

/// How many values a stored field holds, counting the levels above it.
#[derive(Clone, Copy)]
pub(crate) enum Presence {
    Required,
    Optional,
    Array,
}

impl Presence {
    fn below(self, resolved: &ResolvedField) -> Self {
        match self {
            Presence::Array => Presence::Array,
            _ if resolved.array => Presence::Array,
            Presence::Optional => Presence::Optional,
            Presence::Required if resolved.nullable => Presence::Optional,
            Presence::Required => Presence::Required,
        }
    }

    fn marker(self) -> TokenStream {
        match self {
            Presence::Required => quote! { ::flusso_query::rules::Required },
            Presence::Optional => quote! { ::flusso_query::rules::Optional },
            Presence::Array => quote! { ::flusso_query::rules::Array },
        }
    }
}

/// Mapping options that leave how a field's values match untouched. A field
/// with any other option of its own (a `normalizer`, a date `format`, a
/// `null_value`, `index: false`, …) may match differently in the index than
/// Rust decides from the document, so it gets no rule handle.
const RULE_SAFE_OPTIONS: &[&str] = &[
    "copy_to",
    "doc_values",
    "eager_global_ordinals",
    "fields",
    "meta",
    "store",
];

/// The rule handles of one object level, as a namespace type, with the
/// namespaces of the objects beneath it.
///
/// Only a field Rust decides exactly as the index does gets one: keyword,
/// boolean, whole-number, `double`, `decimal` and date leaves, and the plain
/// objects holding them. Text, 32-bit floats, geo points, maps, unindexed
/// fields and `nested` arrays get none, so reaching for one in a rule does not
/// compile.
pub(crate) fn rule_handles(
    ident: &Ident,
    prefix: &str,
    presence: Presence,
    level: &[ResolvedField],
) -> TokenStream {
    let mut methods = TokenStream::new();
    let mut below = TokenStream::new();
    let mut taken = std::collections::HashSet::new();
    for resolved in level.iter().filter(|resolved| resolved.mapping.indexed) {
        let key = resolved.name.as_ref();
        let path = if prefix.is_empty() {
            key.to_owned()
        } else {
            format!("{prefix}.{key}")
        };
        let presence = presence.below(resolved);
        let marker = presence.marker();
        // A name that is no identifier, or that two keys share, is reported
        // once, on the query handles of the same level.
        let Ok(name) = generated_ident(&to_snake_case(key), key) else {
            continue;
        };
        let simple = |ty: &str| {
            let ty = Ident::new(ty, Span::call_site());
            Some((
                quote! { ::flusso_query::rules::#ty<#marker> },
                quote! { ::flusso_query::rules::#ty::<#marker>::at(#path) },
            ))
        };
        let safe = resolved
            .mapping
            .extra
            .keys()
            .all(|option| RULE_SAFE_OPTIONS.contains(&option.as_str()));
        let handle = match &resolved.mapping.mapping_type {
            MappingType::Object if has_namespace(resolved) => {
                let Ok(child) = generated_ident(&format!("{ident}{}", to_pascal_case(key)), key)
                else {
                    continue;
                };
                below.extend(rule_handles(&child, &path, presence, &resolved.children));
                Some((quote! { #child }, quote! { #child }))
            }
            _ if !safe => None,
            MappingType::Keyword => simple("RuleKeyword"),
            MappingType::Boolean => simple("RuleBool"),
            MappingType::Date => simple("RuleDate"),
            MappingType::Byte
            | MappingType::Short
            | MappingType::Integer
            | MappingType::Long
            | MappingType::Double => {
                kind_of(&resolved.mapping.mapping_type, resolved.mapping.decimal).map(|kind| {
                    let kind = kind.marker();
                    (
                        quote! { ::flusso_query::rules::RuleNumber<#kind, #marker> },
                        quote! { ::flusso_query::rules::RuleNumber::<#kind, #marker>::at(#path) },
                    )
                })
            }
            _ => None,
        };
        if let Some((ret, ctor)) = handle
            && taken.insert(name.to_string())
        {
            methods.extend(quote! { pub fn #name(&self) -> #ret { #ctor } });
        }
    }
    quote! {
        /// The fields a rule can read at this level, each with the tests a
        /// rule can make. Generated from the schema.
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub struct #ident;

        impl #ident {
            #methods
        }

        #below
    }
}
