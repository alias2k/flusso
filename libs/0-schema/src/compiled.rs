//! Compiling a configuration into a single, portable binary artifact.
//!
//! [`compile`] runs the whole load pipeline — read `config.toml`, parse every
//! referenced schema, validate and convert into a [`Config`] — and wraps the
//! result in a [`Compiled`] envelope. [`write`] serializes that envelope to
//! MessagePack; [`load_compiled`] reads it back.
//!
//! The point is deployment: a compiled artifact is one file that carries the
//! full, validated configuration with no scattered YAML and no source tree. It
//! holds no secret it wasn't given literally — `{ env = "VAR" }` references are
//! preserved and resolved where the artifact runs.

use std::path::Path;

use serde::{Deserialize, Serialize};

use schema_core::Config;

use crate::loader::{self, LoadError};

/// The artifact format version. Bumped on any incompatible change to the
/// serialized shape so a binary refuses an artifact it can't read, rather than
/// misinterpreting it.
pub const FORMAT_VERSION: u8 = 1;

/// A compiled configuration: the validated [`Config`] plus the provenance needed
/// to read it safely.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Compiled {
    /// Artifact format version, checked on load against [`FORMAT_VERSION`].
    pub format_version: u8,
    /// The `flusso` version that produced this artifact (informational).
    pub flusso_version: String,
    /// The fully-validated configuration.
    pub config: Config,
}

#[derive(thiserror::Error, Debug)]
pub enum CompileError {
    #[error(transparent)]
    Load(#[from] LoadError),
    #[error("failed to read compiled config `{path}`: {source}")]
    Read {
        path: std::path::PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to write compiled config `{path}`: {source}")]
    Write {
        path: std::path::PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to encode compiled config: {0}")]
    Encode(#[from] rmp_serde::encode::Error),
    #[error("failed to decode compiled config: {0}")]
    Decode(#[from] rmp_serde::decode::Error),
    #[error(
        "compiled config format version {got} is not supported by this build \
         (expected {expected}); recompile with a matching `flusso`"
    )]
    VersionMismatch { got: u8, expected: u8 },
}

/// Compile a `config.toml` (and the schemas it references) into a [`Compiled`]
/// envelope. Needs neither a database nor any secret to be set — schemas are
/// self-describing and secrets are deferred.
pub fn compile(config_path: impl AsRef<Path>) -> Result<Compiled, CompileError> {
    let config = loader::load(config_path)?;
    Ok(Compiled {
        format_version: FORMAT_VERSION,
        flusso_version: env!("CARGO_PKG_VERSION").to_owned(),
        config,
    })
}

/// Serialize a [`Compiled`] envelope to its MessagePack bytes.
pub fn to_bytes(compiled: &Compiled) -> Result<Vec<u8>, CompileError> {
    Ok(rmp_serde::to_vec_named(compiled)?)
}

/// Write a [`Compiled`] envelope to `path` as MessagePack.
pub fn write(compiled: &Compiled, path: impl AsRef<Path>) -> Result<(), CompileError> {
    let path = path.as_ref();
    let bytes = to_bytes(compiled)?;
    std::fs::write(path, bytes).map_err(|source| CompileError::Write {
        path: path.to_path_buf(),
        source,
    })
}

/// Decode a [`Compiled`] envelope from MessagePack bytes, checking the format
/// version.
pub fn from_bytes(bytes: &[u8]) -> Result<Config, CompileError> {
    let compiled: Compiled = rmp_serde::from_slice(bytes)?;
    if compiled.format_version != FORMAT_VERSION {
        return Err(CompileError::VersionMismatch {
            got: compiled.format_version,
            expected: FORMAT_VERSION,
        });
    }
    Ok(compiled.config)
}

/// Read a compiled artifact from `path` and return its [`Config`].
pub fn load_compiled(path: impl AsRef<Path>) -> Result<Config, CompileError> {
    let path = path.as_ref();
    let bytes = std::fs::read(path).map_err(|source| CompileError::Read {
        path: path.to_path_buf(),
        source,
    })?;
    from_bytes(&bytes)
}
