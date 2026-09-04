#![allow(unused_crate_dependencies, clippy::unwrap_used, clippy::expect_used)]

//! The derive produces a usable `AdapterConfig`: example from attributes and
//! serde defaults, kind/port, and a description whose schema carries docs,
//! defaults, and the secret paths.

use std::path::PathBuf;

use kernel::{AdapterConfig, Options, Port, Secret};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
enum Mode {
    Prefer,
    VerifyFull,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema, AdapterConfig)]
#[serde(deny_unknown_fields)]
#[adapter(port = source, kind = "demo")]
struct Demo {
    /// The connection URL.
    #[adapter(example = "postgres://app@db/shop")]
    url: Secret,
    /// A password read from the environment.
    #[adapter(example = Secret::env("PGPASSWORD"))]
    password: Option<Secret>,
    /// Documents per request.
    #[serde(default = "default_batch")]
    batch: u32,
    /// Verify certificates.
    #[serde(default)]
    verify: bool,
    /// TLS mode.
    #[adapter(example = Mode::VerifyFull)]
    mode: Mode,
    /// A CA bundle path.
    #[adapter(example = "/etc/ssl/ca.pem")]
    root_cert: Option<PathBuf>,
    /// Never given an example: stays `None`.
    sni: Option<String>,
    /// Hand-wrapped option.
    #[adapter(example = Some(2))]
    replicas: Option<u32>,
}

fn default_batch() -> u32 {
    1000
}

#[test]
fn example_follows_attributes_then_serde_defaults() {
    let example = Demo::example();
    assert_eq!(example.url, Secret::Value("postgres://app@db/shop".into()));
    assert_eq!(example.password, Some(Secret::Env("PGPASSWORD".into())));
    assert_eq!(example.batch, 1000);
    assert!(!example.verify);
    assert_eq!(example.mode, Mode::VerifyFull);
    assert_eq!(example.root_cert, Some(PathBuf::from("/etc/ssl/ca.pem")));
    assert_eq!(example.sni, None);
    assert_eq!(example.replicas, Some(2));
}

#[test]
fn kind_port_and_description() {
    assert_eq!(Demo::KIND, "demo");
    assert_eq!(Demo::PORT, Port::Source);
    let description = Demo::description();
    assert_eq!(description.secrets, vec!["password", "url"]);
    let vars: Vec<String> = description.override_vars("source").collect();
    assert_eq!(vars, ["SOURCE_DEMO_PASSWORD", "SOURCE_DEMO_URL"]);
    let batch = description.schema.pointer("/properties/batch").unwrap();
    assert_eq!(batch.get("default"), Some(&serde_json::json!(1000)));
    assert_eq!(
        batch.get("description").and_then(|d| d.as_str()),
        Some("Documents per request.")
    );
    assert_eq!(
        description.example.get("mode").and_then(|v| v.as_str()),
        Some("verify-full")
    );
}

#[test]
fn example_round_trips_through_options() {
    let options = Options::from_serialize(&Demo::example()).unwrap();
    let back = Demo::from_options(options).unwrap();
    assert_eq!(back.batch, 1000);
    assert_eq!(back.url, Secret::Value("postgres://app@db/shop".into()));
}
