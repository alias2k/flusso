use schema_core::ParseFrom;

#[derive(thiserror::Error, Debug)]
pub enum ParseError {
    #[error("Invalid file format: {0}")]
    Serde(#[from] serde_yaml::Error),
}

impl<T: AsRef<str>> ParseFrom<T> for super::SchemaYaml {
    type Error = ParseError;

    fn try_parse(value: T) -> Result<Self, Self::Error> {
        let result = serde_yaml::from_str(value.as_ref())?;
        Ok(result)
    }
}
