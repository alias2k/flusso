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
use design::DesignOptions;

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
}

pub(crate) async fn execute(args: DesignArgs) -> anyhow::Result<()> {
    design::serve(DesignOptions {
        config_path: args.config,
        address: args.address,
    })
    .await
    .context("running the designer")
}
