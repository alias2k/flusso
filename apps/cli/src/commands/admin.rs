//! `flusso indexes` / `flusso reindex` — the operator client.
//!
//! Thin HTTP clients to a *running* flusso's private control surface (HTTP Basic
//! auth). They hold no privilege the private API doesn't grant any caller — the
//! same endpoints a browser or `curl` would hit, just wrapped for ergonomics.

use std::io::Write;
use std::path::PathBuf;

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

    /// PEM bundle of CA certificates trusted on top of the built-in Mozilla
    /// roots, for an `https://` server behind a private CA.
    #[arg(long, env = "FLUSSO_CA_FILE")]
    ca_file: Option<PathBuf>,
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

    /// The certificates in `--ca-file`, which must hold at least one.
    fn private_roots(&self) -> anyhow::Result<Vec<reqwest::Certificate>> {
        let Some(path) = &self.ca_file else {
            return Ok(Vec::new());
        };
        let shown = path.display();
        let pem = std::fs::read(path).with_context(|| format!("reading --ca-file {shown}"))?;
        let certificates = reqwest::Certificate::from_pem_bundle(&pem)
            .with_context(|| format!("parsing --ca-file {shown}"))?;
        if certificates.is_empty() {
            bail!("--ca-file {shown} holds no PEM certificate (`-----BEGIN CERTIFICATE-----`)");
        }
        Ok(certificates)
    }

    /// A request to `path` on the private surface, with Basic auth applied.
    fn request(
        &self,
        method: reqwest::Method,
        path: &str,
    ) -> anyhow::Result<reqwest::RequestBuilder> {
        let roots = webpki_root_certs::TLS_SERVER_ROOT_CERTS
            .iter()
            .filter_map(|der| reqwest::Certificate::from_der(der).ok())
            .chain(self.private_roots()?);
        let client = reqwest::Client::builder()
            .tls_certs_only(roots)
            .build()
            .context("building the HTTP client")?;
        Ok(client
            .request(method, format!("{}{path}", self.base_url()))
            .basic_auth(&self.admin_user, Some(&self.admin_password)))
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

    /// Rebuild the index on this sink only. Omitted: on every sink.
    #[arg(long, env = "FLUSSO_SINK")]
    sink: Option<String>,

    #[command(flatten)]
    connect: ConnectArgs,
}

/// `flusso indexes` — list the server's indexes and their lifecycle state.
pub(crate) async fn indexes(args: IndexesArgs) -> anyhow::Result<()> {
    let resp = args
        .connect
        .request(reqwest::Method::GET, "/indexes")?
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
    let path = reindex_path(&args.index)?;
    let resp = args
        .connect
        .request(reqwest::Method::POST, &path)?
        .query(&args.sink.as_deref().map(|sink| [("sink", sink)]))
        .send()
        .await
        .with_context(|| format!("requesting {path}"))?;
    let status = resp.status();
    let body = resp.text().await.unwrap_or_default();
    if !status.is_success() {
        bail!("server returned {status}: {}", body.trim());
    }
    let mut out = std::io::stdout().lock();
    writeln!(out, "{}", queued_message(&body))?;
    Ok(())
}

/// What `flusso reindex` prints for the server's `202` body
/// (`{"index", "sink", "queued"}`); a body it can't read is printed as is.
fn queued_message(body: &str) -> String {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(body) else {
        return body.trim().to_owned();
    };
    let field = |name: &str| value.get(name).and_then(serde_json::Value::as_str);
    match (field("index"), field("sink")) {
        (Some(index), Some(sink)) => format!("reindex of {index} on {sink} queued"),
        (Some(index), None) => format!("reindex of {index} on every sink queued"),
        _ => body.trim().to_owned(),
    }
}

/// `POST /indexes/<index>/reindex`'s path; an index name has only
/// characters a path takes as they are, and anything else is refused here.
fn reindex_path(index: &str) -> anyhow::Result<String> {
    let index = kernel::IndexName::try_new(index.to_owned())
        .map_err(|_| anyhow::anyhow!("{index:?} is not an index name"))?;
    Ok(format!("/indexes/{}/reindex", index.as_ref()))
}

#[cfg(test)]
mod tests;
