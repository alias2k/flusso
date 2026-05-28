#![allow(unused_crate_dependencies)]
use schema_config_toml::ConfigToml;
use schema_core::ParseFrom;

#[test]
fn parse_config() {
    const FILE: &str = include_str!("config.toml");

    ConfigToml::try_parse(FILE).unwrap();
}
