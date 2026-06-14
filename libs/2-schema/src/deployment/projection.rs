//! Projecting the enabled indexes of a [`Config`] into fully-typed mappings.
//!
//! The per-index projection (schema → mapping) is database-free and lives in
//! `schema-core` as [`IndexSchema::resolve`](schema_core::IndexSchema::resolve);
//! this is just the composition layer that walks the config's enabled indexes.

use schema_core::IndexMapping;

use super::Config;

impl Config {
    /// Project every **enabled** index into a fully-typed [`IndexMapping`],
    /// using only the declared schema. The engine runs it up front so each index
    /// is created from a complete description without touching the database.
    pub fn resolve_mappings(&self) -> Vec<IndexMapping> {
        self.indexes
            .iter()
            .filter(|(_, index)| index.enabled)
            .map(|(name, index)| index.schema.resolve(name.clone()))
            .collect()
    }
}
