//! `flusso indexes` / `flusso reindex` — the operator client.
//!
//! Thin HTTP clients to a *running* flusso's private control surface (HTTP Basic
//! auth). They hold no privilege the private API doesn't grant any caller — the
//! same endpoints a browser or `curl` would hit, just wrapped for ergonomics.

use std::io::Write;

use anyhow::{Context, bail};
use clap::Args;

use crate::http::{DEFAULT_ADMIN_PASSWORD, DEFAULT_ADMIN_USER};

/// Connection to a running flusso's private control surface, shared by the
/// client subcommands.
#[derive(Debug, Args)]
pub(crate) struct ConnectArgs {
    /// Address (or base URL) of the private control surface. A bare `host:port`
    /// is assumed to be `http://`.
    #[arg(long, env = "FLUSSO_SERVER", default_value = "127.0.0.1:9465")]
    server: String,

    /// Username for HTTP Basic auth.
    #[arg(long, env = "FLUSSO_ADMIN_USER", default_value = DEFAULT_ADMIN_USER)]
    admin_user: String,

    /// Password for HTTP Basic auth.
    #[arg(long, env = "FLUSSO_ADMIN_PASSWORD", default_value = DEFAULT_ADMIN_PASSWORD)]
    admin_password: String,
}

impl ConnectArgs {
    /// The base URL — a bare `host:port` defaults to the `http://` scheme.
    fn base_url(&self) -> String {
        let server = self.server.trim_end_matches('/');
        if server.starts_with("http://") || server.starts_with("https://") {
            server.to_owned()
        } else {
            format!("http://{server}")
        }
    }

    /// A request to `path` on the private surface, with Basic auth applied.
    fn request(&self, method: reqwest::Method, path: &str) -> reqwest::RequestBuilder {
        reqwest::Client::new()
            .request(method, format!("{}{path}", self.base_url()))
            .basic_auth(&self.admin_user, Some(&self.admin_password))
    }
}

#[derive(Debug, Args)]
pub(crate) struct IndexesArgs {
    #[command(flatten)]
    connect: ConnectArgs,
}

#[derive(Debug, Args)]
pub(crate) struct ReindexArgs {
    /// The logical index to rebuild from scratch.
    index: String,

    #[command(flatten)]
    connect: ConnectArgs,
}

/// `flusso indexes` — list the server's indexes and their lifecycle state.
pub(crate) async fn indexes(args: IndexesArgs) -> anyhow::Result<()> {
    let resp = args
        .connect
        .request(reqwest::Method::GET, "/indexes")
        .send()
        .await
        .context("requesting /indexes")?;
    let status = resp.status();
    let body = resp.text().await.unwrap_or_default();
    if !status.is_success() {
        bail!("server returned {status}: {}", body.trim());
    }
    let mut out = std::io::stdout().lock();
    match serde_json::from_str::<serde_json::Value>(&body) {
        Ok(value) => writeln!(out, "{}", serde_json::to_string_pretty(&value)?)?,
        Err(_) => writeln!(out, "{}", body.trim())?,
    }
    Ok(())
}

/// `flusso reindex <index>` — trigger a from-scratch rebuild of one index.
pub(crate) async fn reindex(args: ReindexArgs) -> anyhow::Result<()> {
    let resp = args
        .connect
        .request(reqwest::Method::POST, "/reindex")
        .query(&[("index", &args.index)])
        .send()
        .await
        .context("requesting /reindex")?;
    let status = resp.status();
    let body = resp.text().await.unwrap_or_default();
    if !status.is_success() {
        bail!("server returned {status}: {}", body.trim());
    }
    let mut out = std::io::stdout().lock();
    writeln!(out, "{}", body.trim())?;
    Ok(())
}

#[cfg(test)]
mod tests;
