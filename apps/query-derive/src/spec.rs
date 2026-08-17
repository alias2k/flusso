//! Baking a resolved mapping level into the `&[FieldSpec]` a root hands to the
//! fragments it embeds.
//!
//! This is the root's half of the compile-time channel: it is the only place
//! that reads the schema, so it turns what it knows into plain const data and
//! passes it down. The whole subtree is baked once — `children` navigates it,
//! so a fragment three levels deep is reached without the root ever naming it.

use proc_macro2::TokenStream;
use quote::quote;

use schema::{MappingType, ResolvedField};

/// The `&[FieldSpec]` literal for one level, children and all.
pub(crate) fn bake_level(level: &[ResolvedField]) -> TokenStream {
    let fields = level.iter().map(bake_field);
    quote! { &[ #(#fields),* ] }
}

fn bake_field(field: &ResolvedField) -> TokenStream {
    let name = field.name.as_ref();
    let kind = tag(&field.mapping.mapping_type, field.mapping.decimal);
    let nullable = field.nullable;
    let array = field.array;
    let variants = match &field.mapping.enum_order {
        Some(order) => {
            let variants = order.iter();
            quote! { &[ #(#variants),* ] }
        }
        None => quote! { &[] },
    };
    // A map's value kind carries no decimal flag — `map_values` is only ever a
    // mapping type — so a `double`-valued map keys to the `Double` tag.
    let map_values = match &field.mapping.map_values {
        Some(values) => {
            let values = tag(values, false);
            quote! { ::core::option::Option::Some(#values) }
        }
        None => quote! { ::core::option::Option::None },
    };
    let children = bake_level(&field.children);
    quote! {
        ::flusso_query::FieldSpec {
            name: #name,
            kind: #kind,
            nullable: #nullable,
            array: #array,
            variants: #variants,
            map_values: #map_values,
            children: #children,
        }
    }
}

/// A mapping type as the const-readable `KindTag`.
///
/// `decimal` splits the two things OpenSearch stores as `double`, the same way
/// the handle codegen does, so a `Decimal` value type is accepted on a `decimal`
/// column and rejected on a true `double`.
fn tag(mapping_type: &MappingType, decimal: bool) -> TokenStream {
    let name = match mapping_type {
        MappingType::Keyword => quote! { Keyword },
        MappingType::Text => quote! { Text },
        MappingType::Boolean => quote! { Bool },
        MappingType::Byte => quote! { Byte },
        MappingType::Short => quote! { Short },
        MappingType::Integer => quote! { Integer },
        MappingType::Long => quote! { Long },
        MappingType::Float | MappingType::HalfFloat => quote! { Float },
        MappingType::Double if decimal => quote! { Decimal },
        MappingType::Double => quote! { Double },
        MappingType::ScaledFloat => quote! { Decimal },
        MappingType::Date => quote! { Date },
        MappingType::Object => quote! { Object },
        MappingType::Nested => quote! { Nested },
        MappingType::Other(name) if name == "geo_point" => quote! { GeoPoint },
        MappingType::Other(name) if name == "binary" => quote! { Binary },
        // Anything this vocabulary doesn't model must never become a spurious
        // compile error, so it matches everything.
        MappingType::Other(_) => quote! { Other },
    };
    quote! { ::flusso_query::KindTag::#name }
}
