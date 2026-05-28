use schema_core::ParseFrom;

use crate::SUPPORTED_VERSIONS;

#[derive(thiserror::Error, Debug)]
pub enum ParseError {
    #[error("invalid file format: {0}")]
    Serde(#[from] serde_yaml::Error),
    #[error("unsupported schema version {got}; supported versions: {supported}")]
    UnsupportedVersion { got: u8, supported: &'static str },
}

impl<T: AsRef<str>> ParseFrom<T> for super::SchemaYaml {
    type Error = ParseError;

    fn try_parse(value: T) -> Result<Self, Self::Error> {
        let result: super::SchemaYaml = serde_yaml::from_str(value.as_ref())?;

        if !SUPPORTED_VERSIONS.contains(&result.version) {
            return Err(ParseError::UnsupportedVersion {
                got: result.version,
                supported: "1",
            });
        }

        Ok(result)
    }
}
