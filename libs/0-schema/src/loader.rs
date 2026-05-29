use std::path::{Path, PathBuf};

use schema_config_toml::ConfigToml;
use schema_core::common::IndexName;
use schema_core::{Config, Index, IndexSchema, ParseFrom};
use schema_index_yaml::SchemaYaml;

#[derive(thiserror::Error, Debug)]
pub enum LoadError {
    #[error("failed to read config `{path}`: {source}")]
    ReadConfig {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to parse config `{path}`: {source}")]
    ParseConfig {
        path: PathBuf,
        #[source]
        source: schema_config_toml::ParseError,
    },
    #[error("failed to convert config `{path}`: {source}")]
    ConvertConfig {
        path: PathBuf,
        #[source]
        source: schema_config_toml::ConversionError,
    },
    #[error("failed to read schema `{name}` from `{path}`: {source}")]
    ReadSchema {
        name: IndexName,
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to parse schema `{name}` from `{path}`: {source}")]
    ParseSchema {
        name: IndexName,
        path: PathBuf,
        #[source]
        source: schema_index_yaml::ParseError,
    },
    #[error("failed to convert schema `{name}` from `{path}`: {source}")]
    ConvertSchema {
        name: IndexName,
        path: PathBuf,
        #[source]
        source: schema_index_yaml::ConversionError,
    },
    #[error("duplicate index name `{0}`")]
    DuplicateIndex(IndexName),
}

/// Loads a full [`Config`] from a TOML config file at `config_path`.
///
/// Source and sinks come from the TOML itself; each `[[index]]` entry
/// references a YAML schema file, resolved relative to the config file's
/// directory, which is parsed and converted into [`Index`] entries.
pub fn load(config_path: impl AsRef<Path>) -> Result<Config, LoadError> {
    let config_path = config_path.as_ref();

    let raw = std::fs::read_to_string(config_path).map_err(|source| LoadError::ReadConfig {
        path: config_path.to_path_buf(),
        source,
    })?;

    let config_toml = ConfigToml::try_parse(&raw).map_err(|source| LoadError::ParseConfig {
        path: config_path.to_path_buf(),
        source,
    })?;

    let indexes = config_toml.index.clone();

    let mut config = Config::try_from(config_toml).map_err(|source| LoadError::ConvertConfig {
        path: config_path.to_path_buf(),
        source,
    })?;

    let base_dir = config_path.parent().unwrap_or(Path::new("."));

    for entry in indexes {
        let schema_path = resolve(base_dir, entry.schema.as_path());

        let raw =
            std::fs::read_to_string(&schema_path).map_err(|source| LoadError::ReadSchema {
                name: entry.name.clone(),
                path: schema_path.clone(),
                source,
            })?;

        let schema_yaml = SchemaYaml::try_parse(&raw).map_err(|source| LoadError::ParseSchema {
            name: entry.name.clone(),
            path: schema_path.clone(),
            source,
        })?;

        let schema =
            IndexSchema::try_from(schema_yaml).map_err(|source| LoadError::ConvertSchema {
                name: entry.name.clone(),
                path: schema_path.clone(),
                source,
            })?;

        let index = Index {
            enabled: entry.enabled,
            schema,
        };

        if config.indexes.insert(entry.name.clone(), index).is_some() {
            return Err(LoadError::DuplicateIndex(entry.name));
        }
    }

    Ok(config)
}

/// Resolves a schema path against the config's directory. Absolute paths are
/// used as-is.
fn resolve(base_dir: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        base_dir.join(path)
    }
}
