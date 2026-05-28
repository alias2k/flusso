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
        Clone,
        Hash,
        Eq,
        PartialEq,
        Serialize,
        Deserialize
    )
)]
pub struct IndexName(String);
