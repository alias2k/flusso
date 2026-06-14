//! [`SourceSpec`] — the source's own view of what to build.
//!
//! A source needs only a fraction of the top-level `Config`: the indexes it
//! must keep in sync and the shape of each (root table, field sources, filters).
//! It needs neither the sink list nor any connection or OpenSearch detail. That
//! genuine subset is [`SourceSpec`], expressed entirely in [`schema_core`] types
//! the source already speaks.
//!
//! The composition root translates the top-level config into a `SourceSpec` and
//! hands the backend the spec, so the backend knows nothing about how the
//! application is configured. `SourceSpec` is expressed purely in `schema-core`
//! vocabulary and never references the assembled `Config` (which lives a layer
//! up), so a source is reusable and unit-testable without constructing one, and
//! the `flusso.toml` shape can evolve without recompiling the backend.

use std::collections::BTreeMap;

use schema_core::{IndexMapping, IndexName, IndexSchema};

/// The enabled indexes a source must build, each paired with its schema.
///
/// Everything here is treated as live — disabled indexes are dropped during
/// translation (see [`from_config`](Self::from_config)), so the source never has
/// to re-check an `enabled` flag.
#[derive(Debug, Clone, Default)]
pub struct SourceSpec {
    indexes: BTreeMap<IndexName, IndexSchema>,
}

impl SourceSpec {
    /// Build a spec from an explicit set of `(index, schema)` pairs. Every entry
    /// is treated as live; the caller (the composition root) is responsible for
    /// having filtered out disabled indexes when translating from a config.
    pub fn new(indexes: BTreeMap<IndexName, IndexSchema>) -> Self {
        Self { indexes }
    }

    /// Iterate the indexes and their schemas, in index-name order.
    pub fn indexes(&self) -> impl Iterator<Item = (&IndexName, &IndexSchema)> {
        self.indexes.iter()
    }

    /// The schema for one index, if the spec carries it.
    pub fn schema(&self, index: &IndexName) -> Option<&IndexSchema> {
        self.indexes.get(index)
    }

    /// Project every index into its fully-typed [`IndexMapping`], using only the
    /// declared schema — the database-free counterpart the engine creates
    /// indexes from up front.
    pub fn index_mappings(&self) -> Vec<IndexMapping> {
        self.indexes
            .iter()
            .map(|(name, schema)| schema.resolve(name.clone()))
            .collect()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use std::collections::BTreeMap;

    use schema_core::{
        Column, DatabaseSchema, Field, FieldSource, FlussoType, IndexName, IndexSchema,
    };

    use super::SourceSpec;

    fn index_name(name: &str) -> IndexName {
        IndexName::try_new(name).unwrap()
    }

    /// A one-column schema over `public.<table>`, enough to resolve a mapping.
    fn schema(table: &str) -> IndexSchema {
        IndexSchema {
            version: 1,
            table: schema_core::TableName::try_new(table).unwrap(),
            db_schema: DatabaseSchema::try_new("public").unwrap(),
            primary_key: Some(schema_core::ColumnName::try_new("id").unwrap()),
            doc_id: None,
            soft_delete: None,
            filters: None,
            fields: vec![Field {
                field: schema_core::FieldName::try_new("id").unwrap(),
                options: Default::default(),
                source: FieldSource::Column(Column {
                    column: schema_core::ColumnName::try_new("id").unwrap(),
                    ty: FlussoType::Keyword,
                    nullable: false,
                    transforms: Vec::new(),
                    default: None,
                }),
            }],
        }
    }

    #[test]
    fn accessors_expose_indexes_in_name_order() {
        let mut indexes = BTreeMap::new();
        indexes.insert(index_name("b"), schema("bees"));
        indexes.insert(index_name("a"), schema("ants"));
        let spec = SourceSpec::new(indexes);

        let names: Vec<&str> = spec.indexes().map(|(name, _)| name.as_ref()).collect();
        assert_eq!(names, ["a", "b"]);
        assert!(spec.schema(&index_name("a")).is_some());
        assert!(spec.schema(&index_name("missing")).is_none());

        let mappings = spec.index_mappings();
        assert_eq!(mappings.len(), 2);
        assert_eq!(mappings.first().unwrap().index.as_ref(), "a");
    }
}
