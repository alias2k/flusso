use nutype::nutype;
use std::path::PathBuf;

#[nutype(derive(
    Debug,
    Clone,
    AsRef,
    Deref,
    Hash,
    Eq,
    PartialEq,
    Serialize,
    Deserialize
))]
pub struct SchemaPath(PathBuf);

impl AsRef<std::path::Path> for SchemaPath {
    fn as_ref(&self) -> &std::path::Path {
        self.as_path()
    }
}

/// A path to a `*.schema.yml`, relative to the config file.
impl schemars::JsonSchema for SchemaPath {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        std::borrow::Cow::Borrowed("SchemaPath")
    }

    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({
            "type": "string",
            "format": "uri-reference",
            "pattern": "^[^/][^:]*\\.ya?ml$",
            "description": "Path to the index schema YAML file, resolved relative to the config file's directory."
        })
    }
}
