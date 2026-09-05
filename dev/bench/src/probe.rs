//! Visibility probes: when does a stamped root row become searchable?

use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use serde_json::json;

use crate::scenario::Probe;

/// Poll `_count` on the index's convenience alias until the document carries
/// the probe's stamp or a later one (a second write to the same row inside the
/// latency window must not hide the first); the elapsed time since `committed`
/// is the visible latency. Fails past `cap`.
pub(crate) async fn wait_visible(
    client: &reqwest::Client,
    os_url: &str,
    probe: &Probe,
    committed: Instant,
    cap: Duration,
) -> Result<Duration> {
    let url = format!("{os_url}/{}/_count", probe.index);
    let body = json!({
        "query": {
            "bool": {
                "filter": [
                    { "term": { "id": probe.id } },
                    { "range": { "updatedAt": { "gte": probe.stamp } } }
                ]
            }
        }
    });
    loop {
        let response = client
            .post(&url)
            .json(&body)
            .send()
            .await
            .context("probing OpenSearch")?;
        if response.status().is_success() {
            let value: serde_json::Value = response.json().await?;
            if value.get("count").and_then(|c| c.as_u64()).unwrap_or(0) > 0 {
                return Ok(committed.elapsed());
            }
        }
        if committed.elapsed() > cap {
            bail!(
                "{}/{} with updatedAt={} never became visible within {cap:?}",
                probe.index,
                probe.id,
                probe.stamp
            );
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}
