use crate::common::{FieldName, IndexName};

use super::{ContentHash, Mapping};

/// A fully-resolved mapping for one index: every field typed and ready for a
/// sink to translate into its native mapping format.
///
/// A source produces this from the [`IndexSchema`](super::IndexSchema) — using
/// each field's explicit [`mapping`](super::Field::mapping) where one is given,
/// and the database's own column types where it is not. The result has a
/// concrete type for every field, which is what a sink needs to create the
/// index up front rather than leaving the destination to guess.
#[derive(Debug, Clone)]
pub struct IndexMapping {
    /// The logical index name (the config key) — the pipeline's stable identity.
    pub index: IndexName,
    /// Hash of the parsed index schema. A sink that owns a physical index folds
    /// this into the index's name (e.g. `users_3f2a1b9c`), so a structural
    /// schema change yields a new name — a fresh index that is re-seeded rather
    /// than written into the old shape.
    pub hash: ContentHash,
    pub fields: Vec<ResolvedField>,
}

/// One field within an [`IndexMapping`]: the document key it lands under, its
/// resolved [`Mapping`] (the `mapping_type` is always present), and the fields
/// nested under it for `object` / `nested` types.
#[derive(Debug, Clone)]
pub struct ResolvedField {
    pub name: FieldName,
    pub mapping: Mapping,
    pub children: Vec<ResolvedField>,
}
