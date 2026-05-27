use crate::traits::ParseFrom;

#[derive(thiserror::Error, Debug)]
pub enum ParseError {
    #[error("Invalid file format: {0}")]
    Serde(#[from] toml::de::Error),
}

impl<T: AsRef<str>> ParseFrom<T> for super::ConfigFile {
    type Error = ParseError;

    fn try_parse(value: T) -> Result<Self, Self::Error> {
        let result = toml::from_str(value.as_ref())?;
        Ok(result)
    }
}
