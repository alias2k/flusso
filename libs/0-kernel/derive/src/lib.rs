#![doc = include_str!("../README.md")]

use proc_macro::TokenStream;

/// Placeholder until the derive lands; see the kernel's `AdapterConfig` trait.
#[proc_macro_derive(AdapterConfig, attributes(adapter))]
pub fn derive_adapter_config(_input: TokenStream) -> TokenStream {
    TokenStream::new()
}
