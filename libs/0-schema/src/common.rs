use std::path::PathBuf;

use nutype::nutype;

#[nutype(
    sanitize(trim, lowercase),
    validate(not_empty),
    derive(
        Debug,
        Clone,
        Display,
        AsRef,
        Deref,
        Clone,
        Hash,
        Eq,
        PartialEq,
        Serialize,
        Deserialize
    )
)]
pub struct IndexName(String);

#[nutype(derive(
    Debug,
    Clone,
    AsRef,
    Deref,
    Clone,
    Hash,
    Eq,
    PartialEq,
    Serialize,
    Deserialize
))]
pub struct SchemaPath(PathBuf);

impl AsRef<std::path::Path> for SchemaPath {
    fn as_ref(&self) -> &std::path::Path {
        (&self).as_path()
    }
}

#[nutype(
    sanitize(trim, lowercase),
    validate(not_empty),
    derive(
        Debug,
        Clone,
        Display,
        AsRef,
        Deref,
        Clone,
        Hash,
        Eq,
        PartialEq,
        Serialize,
        Deserialize
    )
)]
pub struct SinkName(String);

#[nutype(
    sanitize(trim),
    validate(not_empty),
    derive(
        Debug,
        Clone,
        Display,
        AsRef,
        Deref,
        Clone,
        Hash,
        Eq,
        PartialEq,
        Serialize,
        Deserialize
    )
)]
pub struct FieldName(String);

#[nutype(
    sanitize(trim),
    validate(not_empty),
    derive(
        Debug,
        Clone,
        Display,
        AsRef,
        Deref,
        Clone,
        Hash,
        Eq,
        PartialEq,
        Serialize,
        Deserialize
    )
)]
pub struct ColumnName(String);

#[nutype(
    sanitize(trim),
    validate(not_empty, len_char_min = 4),
    derive(
        Debug,
        Clone,
        Display,
        AsRef,
        Deref,
        Clone,
        Hash,
        Eq,
        PartialEq,
        Serialize,
        Deserialize
    )
)]
pub struct TableName(String);

#[nutype(
    sanitize(trim),
    validate(not_empty),
    derive(
        Debug,
        Clone,
        Display,
        AsRef,
        Deref,
        Clone,
        Hash,
        Eq,
        PartialEq,
        Serialize,
        Deserialize
    )
)]
pub struct RawFilterValue(String);
