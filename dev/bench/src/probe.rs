//! Visibility probes: when does a stamped root row become searchable?

use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use serde_json::json;

use crate::scenario::Probe;

/// How long a probe sleeps between two polls: fine enough that the midpoint's
/// error stays a few milliseconds, coarse enough that a probe costs
/// OpenSearch a handful of `_count` requests.
const POLL_EVERY: Duration = Duration::from_millis(5);

/// Poll `_count` on the index's convenience alias until the document carries
/// the probe's stamp or a later one (a second write to the same row inside the
/// latency window must not hide the first). Fails past `cap`.
///
/// The document became searchable after the last poll that missed it was
/// sent and before the poll that found it was: the visible latency is the
/// midpoint of the two, measured from `committed`. That leaves no bias from
/// the polling (a 20 ms poll, taking the hit's answer, added about 10 ms;
/// #387) and an error of at most half of [`POLL_EVERY`] plus one round trip.
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
    let mut missed_at = committed;
    loop {
        let sent = Instant::now();
        let response = client
            .post(&url)
            .json(&body)
            .send()
            .await
            .context("probing OpenSearch")?;
        if response.status().is_success() {
            let value: serde_json::Value = response.json().await?;
            if value.get("count").and_then(|c| c.as_u64()).unwrap_or(0) > 0 {
                let between = sent.duration_since(missed_at) / 2;
                return Ok(missed_at.duration_since(committed) + between);
            }
        }
        missed_at = sent;
        if committed.elapsed() > cap {
            bail!(
                "{}/{} with updatedAt={} never became visible within {cap:?}",
                probe.index,
                probe.id,
                probe.stamp
            );
        }
        tokio::time::sleep(POLL_EVERY).await;
    }
}
