use nutype::nutype;

#[nutype(
    sanitize(trim),
    validate(not_empty),
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
pub struct RawFilterValue(String);
