//! `flusso design` — open the visual schema designer.
//!
//! Serves a local web UI that introspects the configured source *through the
//! source abstraction*, edits `flusso.toml` and the referenced `*.schema.yml`
//! files in place, previews the resulting document/mapping, and validates
//! against the live database. The files stay the source of truth — the UI is a
//! round-tripping view, not a separate model. All the work lives in the
//! `flusso-design` crate; this is the thin CLI entry point.

use std::net::SocketAddr;
use std::path::PathBuf;

use anyhow::Context;
use clap::Args;
use design::{ConfigValidator, DesignOptions};

use crate::DEFAULT_CONFIG;

#[derive(Debug, Args)]
pub(crate) struct DesignArgs {
    /// Path to the configuration file to edit. Created on first save if it does
    /// not exist yet.
    #[arg(short, long, env = "FLUSSO_CONFIG", default_value = DEFAULT_CONFIG)]
    config: PathBuf,

    /// Local address to bind the designer's UI + API to.
    #[arg(long, env = "FLUSSO_DESIGN_ADDRESS", default_value = "127.0.0.1:7700")]
    address: SocketAddr,

    /// Don't open the designer in a browser on start (it opens by default).
    #[arg(long, env = "FLUSSO_DESIGN_NO_OPEN")]
    no_open: bool,
}

pub(crate) async fn execute(args: DesignArgs) -> anyhow::Result<()> {
    // Install the same stderr logging the daemon uses, so the designer's
    // "listening at <url>" line (and any warnings) actually reach the terminal.
    let _tracing = crate::telemetry::init_tracing();
    design::serve(DesignOptions {
        config_path: args.config,
        address: args.address,
        open_browser: !args.no_open,
        adapters: crate::adapters::descriptions().to_vec(),
        validate: ConfigValidator::new(crate::adapters::validate_draft),
    })
    .await
    .context("running the designer")
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use config::ParseFrom;
    use config::toml::ConfigToml;
    use design::api::{self, ValidateRequest};

    use super::*;

    async fn validate(index_sinks: &str) -> String {
        let draft = ConfigToml::try_parse(&format!(
            r#"
            [source]
            type = "postgres"

            [sinks.primary]
            type = "opensearch"
            url = "https://search:9200"

            [[index]]
            name = "orders"
            schema = "orders.schema.yml"
            enabled = true

            {index_sinks}
            "#
        ))
        .unwrap();
        let request = ValidateRequest {
            config: draft,
            indexes: Vec::new(),
        };
        let validator = ConfigValidator::new(crate::adapters::validate_draft);
        let response = api::validate(request, &validator).await;
        assert!(!response.db_reachable);
        response.error.unwrap()
    }

    #[tokio::test]
    async fn validate_rejects_an_owned_per_index_setting() {
        let error =
            validate("[index.sinks.primary]\nindex_settings = { refresh_interval = \"1s\" }").await;
        assert!(error.starts_with("invalid configuration"), "{error}");
        assert!(error.contains("index.refresh_interval"), "{error}");
    }

    #[tokio::test]
    async fn validate_rejects_per_index_options_for_an_undeclared_sink() {
        let error = validate("[index.sinks.audit]\nindex_settings = { x = 1 }").await;
        assert!(error.starts_with("invalid configuration"), "{error}");
        assert!(error.contains("no sink is named \"audit\""), "{error}");
    }
}
