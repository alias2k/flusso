use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum EnvOrValue {
    Env { env: String },
    Value(String),
}

impl EnvOrValue {
    pub fn resolve(self) -> Result<String, EnvOrValueError> {
        match self {
            EnvOrValue::Value(v) => Ok(v),
            EnvOrValue::Env { env } => {
                std::env::var(&env).map_err(|_| EnvOrValueError::NotSet(env))
            }
        }
    }
}

#[derive(thiserror::Error, Debug)]
pub enum EnvOrValueError {
    #[error("environment variable '{0}' is not set")]
    NotSet(String),
}
