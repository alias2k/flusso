use nutype::nutype;

#[nutype(
    sanitize(trim, lowercase),
    validate(len_char_max = 63, regex = r"^[a-z_][a-z0-9_]*$"),
    derive(
        Debug,
        Clone,
        Display,
        AsRef,
        Deref,
        Hash,
        Eq,
        PartialEq,
        Ord,
        PartialOrd,
        Serialize,
        Deserialize
    )
)]
pub struct IndexName(String);

/// The identifier grammar the newtype validates, for the editor schema (nutype
/// has no schemars support, so this mirrors its `validate` attributes).
impl schemars::JsonSchema for IndexName {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        std::borrow::Cow::Borrowed("IndexName")
    }

    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({
            "type": "string",
            "pattern": "^[a-z_][a-z0-9_]*$",
            "maxLength": 63,
            "description": "An index name: lowercase letters, digits, and underscores, not starting with a digit, at most 63 characters."
        })
    }
}
