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
