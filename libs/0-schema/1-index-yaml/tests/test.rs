#![allow(unused_crate_dependencies)]
use schema_core::ParseFrom;
use schema_index_yaml::SchemaYaml;

#[test]
fn parse_index_schema() {
    const FILE: &str = include_str!("user.schema.yml");

    SchemaYaml::try_parse(FILE).unwrap();
}
