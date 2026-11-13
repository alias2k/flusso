//! Saving over an existing file: the model is written into the file's own text,
//! so its comments and formatting survive.
//!
//! [`schema_yaml`] and [`config_toml`] take the text the file holds now (the
//! **base**) and the model to save. The base is parsed into its model, both
//! models are rendered to the canonical tree ([`codegen`]), and the base is
//! patched only where those two trees differ:
//!
//! - what the edit did not touch stays byte for byte, comments, blank lines,
//!   quoting, flow lists and explicit defaults (`required: false`) included,
//!   and a default the file leaves out stays out;
//! - a comment belongs to what it describes: the file's header to the file, a
//!   comment block directly above a key, an item or a table to it, a comment at
//!   the end of a line to that line, a comment after a list's last item to the
//!   list. A comment a blank line separates from what follows belongs to
//!   nothing and stays where it is. The YAML and TOML modules spell out the
//!   rules;
//! - a comment follows its owner through a rename, a move and a type change,
//!   and goes when its owner (or a whole join) is removed. Which old item a new
//!   one is comes from the designer's ids ([`SchemaIdentity`], [`ConfigIdentity`]), else from its name:
//!   never from its slot;
//! - a value that changed is re-rendered in place in the canonical style, and a
//!   key the edit adds is written after the key before it.
//!
//! The result is re-parsed and must read back as exactly the model being saved.
//! When it does not, or the base has a layout the patcher does not follow, the
//! canonical form is written instead and [`Saved::notice`] says so. A file with
//! no base (a new one) is always the canonical form. Comments the patch had to
//! drop while what they describe still exists are counted in
//! [`Saved::dropped`].
//!
//! ```
//! # use config::yaml::SchemaYaml;
//! # use kernel::{FieldSource, IndexSchema, ParseFrom};
//! # fn demo() -> anyhow::Result<()> {
//! let base = "# Users.\nversion: 1\ntable: users\nprimary_key: id\nfields:\n  - keyword: email # login\n    required: false\n";
//! let mut schema = IndexSchema::try_from(SchemaYaml::try_parse(base)?)?;
//! if let FieldSource::Column(column) = &mut schema.fields[0].source {
//!     column.nullable = false;
//! }
//! let saved = design::keep::schema_yaml(Some(base), &schema, None)?;
//! assert_eq!(saved.text, base.replace("required: false", "required: true"));
//! assert!(saved.notice.is_none());
//! # Ok(())
//! # }
//! # demo().unwrap();
//! ```

mod pairing;
#[cfg(test)]
mod tests;
mod toml_doc;
mod yaml;

use anyhow::Result;
use config::toml::ConfigToml;
use config::yaml::SchemaYaml;
use kernel::{IndexSchema, ParseFrom};
use serde::Serialize;

use crate::codegen;
pub use pairing::{ConfigIdentity, FieldId, SchemaIdentity};

/// The text a save writes, and whether it had to give up the file's layout.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Saved {
    /// The bytes to write.
    pub text: String,
    /// Set when the existing file's comments and formatting were not kept.
    pub notice: Option<Notice>,
    /// How many of the file's comments the save could not keep although what
    /// they describe still exists (a value re-rendered in the canonical form,
    /// an item that may have been renamed). Zero with a notice.
    pub dropped: usize,
}

/// Why a save wrote the canonical form over an existing file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Notice {
    /// The file's comments and formatting could not be kept; it is written in
    /// the designer's own layout.
    LayoutReset,
}

impl Saved {
    fn generated(text: String, base: Option<&str>) -> Self {
        let notice = base
            .filter(|b| !b.trim().is_empty() && *b != text)
            .map(|_| Notice::LayoutReset);
        Self {
            text,
            notice,
            dropped: 0,
        }
    }
}

/// `schema` as a `*.schema.yml`, written into `base` when there is one.
/// `identity` says which field of `base` each field of `schema` is.
pub fn schema_yaml(
    base: Option<&str>,
    schema: &IndexSchema,
    identity: Option<&SchemaIdentity>,
) -> Result<Saved> {
    let canonical = codegen::schema_to_yaml(schema)?;
    let Some(text) = base.filter(|b| !b.trim().is_empty()) else {
        return Ok(Saved::generated(canonical, base));
    };
    let patched = parse_schema(text).and_then(|old| {
        let fields = pairing::fields(identity, &old.fields, &schema.fields);
        let old = codegen::schema_value(&old).ok()?;
        let new = codegen::schema_value(schema).ok()?;
        yaml::patch(text, &old, &new, fields.as_ref())
    });
    Ok(match patched {
        Some(yaml::Patched { text, dropped }) if reads_as(parse_schema(&text).as_ref(), schema) => {
            Saved {
                text,
                notice: None,
                dropped,
            }
        }
        _ => Saved::generated(canonical, base),
    })
}

/// `config` as a `flusso.toml`, written into `base` when there is one.
/// `identity` says which `[[index]]` entry of `base` each entry is.
pub fn config_toml(
    base: Option<&str>,
    config: &ConfigToml,
    identity: Option<&ConfigIdentity>,
) -> Result<Saved> {
    let canonical = codegen::config_to_toml(config)?;
    let Some(text) = base.filter(|b| !b.trim().is_empty()) else {
        return Ok(Saved::generated(canonical, base));
    };
    let patched = ConfigToml::try_parse(text).ok().and_then(|old| {
        let names: Vec<String> = old.index.iter().map(|e| e.name.to_string()).collect();
        let indexes = pairing::indexes(identity, &names, config.index.len());
        let old = toml::Table::try_from(&old).ok()?;
        let new = toml::Table::try_from(config).ok()?;
        toml_doc::patch(text, &old, &new, indexes.as_ref())
    });
    Ok(match patched {
        Some(yaml::Patched { text, dropped })
            if reads_as(ConfigToml::try_parse(&text).ok().as_ref(), config) =>
        {
            Saved {
                text,
                notice: None,
                dropped,
            }
        }
        _ => Saved::generated(canonical, base),
    })
}

fn parse_schema(text: &str) -> Option<IndexSchema> {
    IndexSchema::try_from(SchemaYaml::try_parse(text).ok()?).ok()
}

/// Whether `read` is the model `expected`, compared as serialized.
fn reads_as<T: Serialize>(read: Option<&T>, expected: &T) -> bool {
    match (
        read.map(serde_json::to_value),
        serde_json::to_value(expected),
    ) {
        (Some(Ok(read)), Ok(expected)) => read == expected,
        _ => false,
    }
}
