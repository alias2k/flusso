//! The run's result as data points: two files in `github-action-benchmark`'s
//! custom format (`smaller.json` for latencies and costs, `bigger.json` for
//! throughputs), plus `summary.json` with everything, and a log table.

use std::path::Path;

use anyhow::{Context, Result};
use serde::Serialize;

/// One data point in the `customSmallerIsBetter` / `customBiggerIsBetter` shape.
#[derive(Debug, Clone, Serialize)]
pub(crate) struct Point {
    pub(crate) name: String,
    pub(crate) unit: String,
    pub(crate) value: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) extra: Option<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct Report {
    pub(crate) scenario: String,
    pub(crate) scale: serde_json::Value,
    pub(crate) images: Images,
    pub(crate) phases: serde_json::Value,
    pub(crate) smaller: Vec<Point>,
    pub(crate) bigger: Vec<Point>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct Images {
    pub(crate) postgres: String,
    pub(crate) opensearch: String,
}

impl Report {
    fn series(&self) -> String {
        let scale = self
            .scale
            .get("name")
            .and_then(|n| n.as_str())
            .unwrap_or("unknown");
        format!("{}/{scale}", self.scenario)
    }

    fn extra(&self) -> String {
        serde_json::json!({ "images": self.images, "scale": self.scale }).to_string()
    }

    fn point(&self, metric: &str, unit: &str, value: f64) -> Point {
        Point {
            name: format!("{}/{metric}", self.series()),
            unit: unit.to_owned(),
            value,
            extra: Some(self.extra()),
        }
    }

    pub(crate) fn smaller(&mut self, metric: &str, unit: &str, value: f64) {
        let point = self.point(metric, unit, value);
        self.smaller.push(point);
    }

    pub(crate) fn bigger(&mut self, metric: &str, unit: &str, value: f64) {
        let point = self.point(metric, unit, value);
        self.bigger.push(point);
    }

    pub(crate) fn write(&self, dir: &Path) -> Result<()> {
        std::fs::create_dir_all(dir)?;
        let write = |name: &str, value: &dyn erased::Serialize| -> Result<()> {
            let path = dir.join(name);
            let json = value.to_pretty_json()?;
            std::fs::write(&path, json).with_context(|| format!("writing {}", path.display()))
        };
        write("smaller.json", &self.smaller)?;
        write("bigger.json", &self.bigger)?;
        write("summary.json", self)?;
        Ok(())
    }

    pub(crate) fn log(&self) {
        tracing::info!(series = %self.series(), "results");
        for point in self.smaller.iter().chain(self.bigger.iter()) {
            let metric = point.name.rsplit('/').next().unwrap_or(point.name.as_str());
            tracing::info!("  {metric:<28} {:>14.3} {}", point.value, point.unit);
        }
    }
}

/// A failed run, still written so the data point exists with a reason.
#[derive(Debug, Serialize)]
pub(crate) struct Failure {
    pub(crate) scenario: String,
    pub(crate) scale: String,
    pub(crate) phase: String,
    pub(crate) reason: String,
}

impl Failure {
    pub(crate) fn write(&self, dir: &Path) -> Result<()> {
        std::fs::create_dir_all(dir)?;
        let path = dir.join("failure.json");
        std::fs::write(&path, serde_json::to_string_pretty(self)?)
            .with_context(|| format!("writing {}", path.display()))
    }
}

/// Serialize behind a trait object so `write` takes any value.
mod erased {
    pub(crate) trait Serialize {
        fn to_pretty_json(&self) -> anyhow::Result<String>;
    }

    impl<T: serde::Serialize> Serialize for T {
        fn to_pretty_json(&self) -> anyhow::Result<String> {
            Ok(serde_json::to_string_pretty(self)?)
        }
    }
}

/// Prometheus text exposition → an estimate of a histogram's quantile, as
/// `histogram_quantile` computes it (linear interpolation within the bucket).
pub(crate) fn histogram_quantile(text: &str, metric: &str, sink: &str, q: f64) -> Option<f64> {
    let prefix = format!("{metric}_bucket{{");
    let mut buckets: Vec<(f64, f64)> = Vec::new();
    for line in text.lines() {
        let Some(rest) = line.strip_prefix(&prefix) else {
            continue;
        };
        let Some((labels, value)) = rest.split_once('}') else {
            continue;
        };
        let mut le = None;
        let mut matches_sink = false;
        for label in labels.split(',') {
            let Some((key, raw)) = label.split_once('=') else {
                continue;
            };
            let raw = raw.trim_matches('"');
            match key.trim() {
                "le" => {
                    le = Some(if raw == "+Inf" {
                        f64::INFINITY
                    } else {
                        raw.parse::<f64>().ok()?
                    });
                }
                "sink" if raw == sink => matches_sink = true,
                _ => {}
            }
        }
        if let (Some(le), true) = (le, matches_sink) {
            let count = value.trim().parse::<f64>().ok()?;
            buckets.push((le, count));
        }
    }
    if buckets.is_empty() {
        return None;
    }
    buckets.sort_by(|a, b| a.0.total_cmp(&b.0));
    let total = buckets.last()?.1;
    if total == 0.0 {
        return None;
    }
    let rank = q * total;
    let mut lower_bound = 0.0;
    let mut lower_count = 0.0;
    for &(le, count) in &buckets {
        if count >= rank {
            if le.is_infinite() {
                return Some(lower_bound);
            }
            let width = le - lower_bound;
            let share = if count > lower_count {
                (rank - lower_count) / (count - lower_count)
            } else {
                1.0
            };
            return Some(lower_bound + width * share);
        }
        lower_bound = le;
        lower_count = count;
    }
    buckets.last().map(|b| b.0)
}

#[cfg(test)]
mod tests {
    use super::histogram_quantile;

    #[test]
    fn interpolates_within_the_bucket() {
        let text = "\
flusso_flush_duration_seconds_bucket{sink=\"primary\",le=\"0.1\"} 5
flusso_flush_duration_seconds_bucket{sink=\"primary\",le=\"0.5\"} 9
flusso_flush_duration_seconds_bucket{sink=\"primary\",le=\"+Inf\"} 10
flusso_flush_duration_seconds_bucket{sink=\"audit\",le=\"+Inf\"} 100
";
        let p50 = histogram_quantile(text, "flusso_flush_duration_seconds", "primary", 0.5);
        assert_eq!(p50, Some(0.1));
        let p90 = histogram_quantile(text, "flusso_flush_duration_seconds", "primary", 0.9);
        assert_eq!(p90, Some(0.5));
        assert!(histogram_quantile(text, "flusso_flush_duration_seconds", "none", 0.5).is_none());
    }
}
