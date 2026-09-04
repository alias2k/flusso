use nutype::nutype;

#[nutype(
    sanitize(trim),
    validate(len_char_max = 63, regex = r"^[a-zA-Z_][a-zA-Z0-9_]*$"),
    derive(
        Debug,
        Clone,
        Display,
        AsRef,
        Deref,
        Hash,
        Eq,
        PartialEq,
        Serialize,
        Deserialize
    )
)]
pub struct FieldName(String);
