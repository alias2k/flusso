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

/// Resolve a **required** config value, with `reserved` as a deployment override
/// environment variable.
///
/// Precedence, highest first (see also [`resolve_optional`]):
///
/// 1. An explicit `{ env = "X" }` in the config names its own source and wins —
///    `reserved` is not consulted. If `X` is unset, that is an error.
/// 2. The `reserved` variable, if set — overriding a literal written in the
///    config. The override is logged so it is never silent.
/// 3. The literal value from the config.
pub(crate) fn resolve_required(
    config: EnvOrValue,
    reserved: &str,
) -> Result<String, EnvOrValueError> {
    match config {
        // An explicit `{ env = "X" }` reference names its own source and is not
        // overridden by the reserved variable.
        env @ EnvOrValue::Env { .. } => env.resolve(),
        EnvOrValue::Value(literal) => Ok(literal_or_override(literal, reserved)),
    }
}

/// Resolve an **optional** config value, with `reserved` as a deployment
/// override / fallback environment variable. Same precedence as
/// [`resolve_required`], plus: when the config omits the value entirely,
/// `reserved` fills it if set, otherwise the result is `None`.
pub(crate) fn resolve_optional(
    config: Option<EnvOrValue>,
    reserved: &str,
) -> Result<Option<String>, EnvOrValueError> {
    match config {
        Some(env @ EnvOrValue::Env { .. }) => env.resolve().map(Some),
        Some(EnvOrValue::Value(literal)) => Ok(Some(literal_or_override(literal, reserved))),
        None => Ok(match std::env::var(reserved) {
            Ok(value) => {
                tracing::debug!(var = %reserved, "config field resolved from environment");
                Some(value)
            }
            Err(_) => None,
        }),
    }
}

/// The `reserved` variable if set (logging that it overrides the config), else
/// the literal. Shared by [`resolve_required`] and [`resolve_optional`].
fn literal_or_override(literal: String, reserved: &str) -> String {
    match std::env::var(reserved) {
        Ok(value) => {
            tracing::warn!(
                var = %reserved,
                "environment variable overrides value set in config",
            );
            value
        }
        Err(_) => literal,
    }
}

#[derive(thiserror::Error, Debug)]
pub enum EnvOrValueError {
    #[error("environment variable '{0}' is not set")]
    NotSet(String),
}
