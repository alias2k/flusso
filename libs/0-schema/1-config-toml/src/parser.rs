use schema_core::ParseFrom;

#[derive(thiserror::Error, Debug)]
pub enum ParseError {
    // `toml` already renders a precise, annotated snippet (line/column + a caret
    // under the offending span). Pass it through verbatim rather than prefixing
    // it — a prefix only mangles that snippet's first line.
    #[error("{0}")]
    Serde(#[from] toml::de::Error),
}

impl<T: AsRef<str>> ParseFrom<T> for super::ConfigToml {
    type Error = ParseError;

    fn try_parse(value: T) -> Result<Self, Self::Error> {
        let result = toml::from_str(value.as_ref())?;
        Ok(result)
    }
}
