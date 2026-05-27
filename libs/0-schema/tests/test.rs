#![allow(unused_crate_dependencies)]
use schema::traits::ParseFrom;

const CONFIG_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/config.toml");

#[test]
fn parse_config() {
    use schema::files::config_file::ConfigFile;

    const FILE: &str = include_str!("config.toml");

    ConfigFile::try_parse(FILE).unwrap();
}

#[test]
fn parse_index_schema() {
    use schema::files::schema_file::IndexSchemaFile;

    const FILE: &str = include_str!("user.schema.yml");

    IndexSchemaFile::try_parse(FILE).unwrap();
}

#[test]
fn load_config() {
    use schema::common::SinkName;
    use schema::config::Config;
    use schema::files::config_file::{SinkType, SourceType};

    let config = Config::try_from_path(CONFIG_PATH).unwrap();

    assert_eq!(config.source.source_type, SourceType::Postgres);

    assert_eq!(config.sinks.len(), 2);
    let primary = SinkName::try_new("primary").unwrap();
    let audit = SinkName::try_new("audit").unwrap();
    assert_eq!(config.sinks[&primary].sink_type, SinkType::Opensearch);
    assert_eq!(config.sinks[&audit].sink_type, SinkType::Stdout);

    assert_eq!(config.indexes.len(), 1);
    let users = &config.indexes[0];
    assert_eq!(users.name.as_ref(), "users");
    assert!(users.enabled);
    assert_eq!(users.schema.table, "users");
    assert_eq!(users.schema.db_schema, "public");
    assert!(!users.schema.fields.is_empty());
}
