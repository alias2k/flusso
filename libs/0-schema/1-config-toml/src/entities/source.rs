use serde::{Deserialize, Serialize};

use crate::common;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Source {
    Postgres(PostgresSource),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PostgresSource {
    pub connection_url: Option<ConnectionUrl>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ConnectionUrl {
    Url(common::ConnectionUrl),
    Parts {
        #[serde(default = "default_host")]
        host: String,
        #[serde(default = "default_port")]
        port: u16,
        #[serde(default = "default_user")]
        user: String,
        password: Option<String>,
        database: String,
    },
}

fn default_host() -> String {
    "127.0.0.1".to_owned()
}

fn default_port() -> u16 {
    5432
}

fn default_user() -> String {
    "postgres".to_owned()
}
