use nutype::nutype;

#[nutype(
    sanitize(trim, lowercase),
    validate(len_char_max = 63, regex = r"^[a-z_][a-z0-9_]*$"),
    derive(
        Debug,
        Clone,
        Display,
        Default,
        AsRef,
        Deref,
        Hash,
        Eq,
        PartialEq,
        Serialize,
        Deserialize
    ),
    default = "public"
)]
pub struct DatabaseSchema(String);
