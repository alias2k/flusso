mod loader;

pub use loader::{LoadError, load};

// Re-export the canonical schema types so downstream crates depend only on
// `schema` rather than reaching into the sub-crates directly.
pub use schema_core::*;
