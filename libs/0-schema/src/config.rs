mod aggregate;
mod field;
mod filter;
mod join;
mod sink;
mod soft_delete;
mod source;
mod transform;

use aggregate::*;
use field::*;
use filter::*;
use join::*;
use sink::*;
use soft_delete::*;
use source::*;
use transform::*;

use std::collections::HashMap;
use std::marker::PhantomData;
use std::path::Path;

use crate::common;
use crate::files::config_toml::ConfigToml;
use crate::files::schema_yaml::SchemaYaml;

#[derive(Debug, Clone)]
pub struct Config {
    pub source: Source,
    pub sinks: HashMap<common::SinkName, Sink>,
    pub indexes: Vec<Index>,
}

#[derive(Debug, Clone)]
pub struct Index {
    pub name: common::IndexName,
    pub enabled: bool,
    pub schema: IndexSchema,
}

#[derive(Debug, Clone)]
pub struct IndexSchema {
    pub version: u8,
    pub table: String,
    pub db_schema: String,
    pub primary_key: Option<String>,
    pub doc_id: Option<String>,
    pub soft_delete: Option<SoftDelete>,
    pub fields: Vec<Field>,
}

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("failed to parse config: {0}")]
    Config(#[from] crate::files::config_toml::ParseError),
    #[error("failed to parse index schema '{name}': {source}")]
    IndexSchema {
        name: common::IndexName,
        #[source]
        source: crate::files::schema_yaml::ParseError,
    },
}

#[expect(dead_code)]
#[derive(Debug, Clone, Copy)]
enum ConfigFileFormat {
    Toml(PhantomData<ConfigToml>),
}

#[expect(dead_code)]
#[derive(Debug, Clone, Copy)]
enum SchemaFileFormat {
    Yaml(PhantomData<SchemaYaml>),
}

impl Config {
    pub fn try_from_path(_path: impl AsRef<Path>) -> Result<Self, Error> {
        todo!()
        // let path = path.as_ref();
        // let base_dir = path.parent().unwrap_or(Path::new("."));
        //
        // let content = std::fs::read_to_string(path)?;
        // let file = ConfigToml::try_parse(content)?;
        //
        // let sinks = file.sinks.unwrap_or_default();
        //
        // let mut indexes = Vec::new();
        // for entry in file.index.unwrap_or_default() {
        //     let schema_path = base_dir.join(&entry.schema);
        //     let schema_content = std::fs::read_to_string(&schema_path)?;
        //     let schema_file = IndexSchemaFile::try_parse(&schema_content).map_err(|source| {
        //         Error::IndexSchema {
        //             name: entry.name.clone(),
        //             source,
        //         }
        //     })?;
        //
        //     let doc_id = schema_file
        //         .doc_id
        //         .or_else(|| schema_file.primary_key.clone());
        //
        //     indexes.push(Index {
        //         name: entry.name,
        //         enabled: entry.enabled,
        //         schema: IndexSchema {
        //             version: schema_file.version,
        //             table: schema_file.table,
        //             db_schema: schema_file.schema.unwrap_or_else(|| "public".to_string()),
        //             primary_key: schema_file.primary_key,
        //             doc_id,
        //             soft_delete: schema_file.soft_delete,
        //             fields: schema_file.fields,
        //         },
        //     });
        // }
        //
        // Ok(Config {
        //     source: file.source,
        //     sinks,
        //     indexes,
        // })
    }
}
