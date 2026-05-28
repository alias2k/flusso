use nutype::nutype;

#[nutype(
    sanitize(trim),
    validate(regex = r"^https?://\S+$"),
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
pub struct HttpUrl(String);
