//! Where a phase's time went, read from flusso's own `/metrics` (issue #399).
//!
//! `flusso.stage.duration` is one histogram per stage, labelled by `stage`,
//! `work`, `sink` and `index` where the engine knows them. Its `_sum` is the
//! seconds that stage was busy, so the difference between two scrapes, over
//! the phase's wall time, is the stage's **share** of the phase. Shares of one
//! task add up to at most 1 (the ingest task: `read` + `resolve` + `build` +
//! `publish`; a sink: `wait` + `apply` + `flush`); `lane_wait` is per sink and
//! overlaps `publish`.
//!
//! ```text
//! | stage   | work     | sink    | batches | busy s | share | ms/batch |
//! | build   | backfill |         |     243 |  6.80  | 39.1% |   28.0   |
//! | flush   | backfill | primary |     243 | 12.60  | 72.4% |   51.9   |
//! ```

use std::collections::BTreeMap;
use std::fmt::Write as _;

use serde::Serialize;

const STAGE: &str = "flusso_stage_duration_seconds";
const FLUSH_DOCUMENTS: &str = "flusso_flush_documents";
const FLUSH_SIZE: &str = "flusso_flush_size_bytes";
const STARTUP: &str = "flusso_startup_duration_seconds";

/// One series' labels that matter here; any other label is ignored, so a
/// label flusso adds later doesn't break the parse.
#[derive(Debug, Clone, Default, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub(crate) struct Labels {
    pub(crate) stage: String,
    pub(crate) work: String,
    pub(crate) sink: String,
    pub(crate) index: String,
}

/// A histogram's running `_sum` and `_count`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize)]
pub(crate) struct Totals {
    pub(crate) sum: f64,
    pub(crate) count: f64,
}

/// What one scrape says about stages and flush sizes.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct Scrape {
    stages: BTreeMap<Labels, Totals>,
    flush_documents: BTreeMap<Labels, Totals>,
    flush_size: BTreeMap<Labels, Totals>,
    startup: BTreeMap<Labels, f64>,
}

impl Scrape {
    /// Read a Prometheus text exposition.
    pub(crate) fn parse(text: &str) -> Self {
        let mut scrape = Self::default();
        for line in text.lines() {
            let Some((name, labels, value)) = sample(line) else {
                continue;
            };
            let (family, part) = match name.rsplit_once('_') {
                Some((family, part @ ("sum" | "count"))) => (family, part),
                _ => (name, ""),
            };
            let totals = match family {
                STAGE => scrape.stages.entry(labels).or_default(),
                FLUSH_DOCUMENTS => scrape.flush_documents.entry(labels).or_default(),
                FLUSH_SIZE => scrape.flush_size.entry(labels).or_default(),
                STARTUP if part.is_empty() => {
                    scrape.startup.insert(labels, value);
                    continue;
                }
                _ => continue,
            };
            match part {
                "sum" => totals.sum = value,
                "count" => totals.count = value,
                _ => {}
            }
        }
        scrape
    }
}

/// `name{labels} value` → its parts; `None` for a comment or another shape.
fn sample(line: &str) -> Option<(&str, Labels, f64)> {
    if line.starts_with('#') {
        return None;
    }
    let (series, value) = line.rsplit_once(' ')?;
    let value = value.trim().parse::<f64>().ok()?;
    let (name, rest) = series.split_once('{').unwrap_or((series, "}"));
    let raw = rest.strip_suffix('}')?;
    let mut labels = Labels::default();
    for pair in raw.split(',').filter(|pair| !pair.is_empty()) {
        let (key, quoted) = pair.split_once('=')?;
        let text = quoted.trim_matches('"').to_owned();
        match key.trim() {
            "stage" | "step" => labels.stage = text,
            "work" => labels.work = text,
            "sink" => labels.sink = text,
            "index" => labels.index = text,
            _ => {}
        }
    }
    Some((name, labels, value))
}

/// One stage's line in a phase: how often, how long in all, its share of the
/// phase, and the mean per batch.
#[derive(Debug, Clone, Serialize)]
pub(crate) struct Row {
    #[serde(flatten)]
    pub(crate) labels: Labels,
    pub(crate) batches: f64,
    pub(crate) busy_seconds: f64,
    pub(crate) share: f64,
    pub(crate) ms_per_batch: f64,
}

/// One flush-size line: per sink and work, documents and bytes per flush.
#[derive(Debug, Clone, Serialize)]
pub(crate) struct FlushSize {
    #[serde(flatten)]
    pub(crate) labels: Labels,
    pub(crate) flushes: f64,
    pub(crate) documents_per_flush: f64,
    pub(crate) bytes_per_flush: Option<f64>,
}

/// Where one phase's time went.
#[derive(Debug, Clone, Serialize)]
pub(crate) struct Attribution {
    pub(crate) phase: String,
    pub(crate) wall_seconds: f64,
    pub(crate) stages: Vec<Row>,
    pub(crate) flushes: Vec<FlushSize>,
    /// Start-up steps in seconds (the backfill phase only).
    pub(crate) startup: BTreeMap<String, f64>,
}

impl Attribution {
    /// What happened between `before` and `after`, over `wall_seconds`.
    pub(crate) fn between(phase: &str, before: &Scrape, after: &Scrape, wall_seconds: f64) -> Self {
        let delta = |series: &BTreeMap<Labels, Totals>, labels: &Labels, now: Totals| {
            let then = series.get(labels).copied().unwrap_or_default();
            Totals {
                sum: now.sum - then.sum,
                count: now.count - then.count,
            }
        };
        let stages = after
            .stages
            .iter()
            .filter_map(|(labels, now)| {
                let moved = delta(&before.stages, labels, *now);
                (moved.count > 0.0).then(|| Row {
                    labels: labels.clone(),
                    batches: moved.count,
                    busy_seconds: moved.sum,
                    share: if wall_seconds > 0.0 {
                        moved.sum / wall_seconds
                    } else {
                        0.0
                    },
                    ms_per_batch: moved.sum / moved.count * 1_000.0,
                })
            })
            .collect();
        let flushes = after
            .flush_documents
            .iter()
            .filter_map(|(labels, now)| {
                let documents = delta(&before.flush_documents, labels, *now);
                (documents.count > 0.0).then(|| {
                    let bytes = after
                        .flush_size
                        .get(labels)
                        .map(|size| delta(&before.flush_size, labels, *size))
                        .filter(|size| size.count > 0.0)
                        .map(|size| size.sum / size.count);
                    FlushSize {
                        labels: labels.clone(),
                        flushes: documents.count,
                        documents_per_flush: documents.sum / documents.count,
                        bytes_per_flush: bytes,
                    }
                })
            })
            .collect();
        let startup = if before.startup.is_empty() {
            after
                .startup
                .iter()
                .map(|(labels, seconds)| {
                    let name = if labels.sink.is_empty() {
                        labels.stage.clone()
                    } else {
                        format!("{} {}", labels.stage, labels.sink)
                    };
                    (name, *seconds)
                })
                .collect()
        } else {
            BTreeMap::new()
        };
        Self {
            phase: phase.to_owned(),
            wall_seconds,
            stages,
            flushes,
            startup,
        }
    }

    /// The attribution as Markdown, ready to paste into a pull request.
    pub(crate) fn markdown(&self) -> String {
        let mut out = String::new();
        let _ = writeln!(
            out,
            "**{}** ({:.2} s wall)\n\n| stage | work | sink | index | batches | busy s | share | ms/batch |\n| --- | --- | --- | --- | ---: | ---: | ---: | ---: |",
            self.phase, self.wall_seconds
        );
        for row in &self.stages {
            let _ = writeln!(
                out,
                "| {} | {} | {} | {} | {:.0} | {:.2} | {:.1}% | {:.2} |",
                row.labels.stage,
                row.labels.work,
                row.labels.sink,
                row.labels.index,
                row.batches,
                row.busy_seconds,
                row.share * 100.0,
                row.ms_per_batch
            );
        }
        if !self.flushes.is_empty() {
            let _ = writeln!(
                out,
                "\n| flushes | work | sink | count | documents/flush | KiB/flush |\n| --- | --- | --- | ---: | ---: | ---: |"
            );
            for flush in &self.flushes {
                let kib = flush
                    .bytes_per_flush
                    .map_or_else(|| "—".to_owned(), |bytes| format!("{:.1}", bytes / 1024.0));
                let _ = writeln!(
                    out,
                    "| | {} | {} | {:.0} | {:.1} | {kib} |",
                    flush.labels.work, flush.labels.sink, flush.flushes, flush.documents_per_flush
                );
            }
        }
        if !self.startup.is_empty() {
            let _ = writeln!(out, "\n| start-up step | seconds |\n| --- | ---: |");
            for (step, seconds) in &self.startup {
                let _ = writeln!(out, "| {step} | {seconds:.3} |");
            }
        }
        out
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    const BEFORE: &str = "\
# HELP flusso_stage_duration_seconds Time each pipeline stage took
# TYPE flusso_stage_duration_seconds histogram
flusso_stage_duration_seconds_bucket{stage=\"build\",work=\"live\",le=\"0.1\"} 4
flusso_stage_duration_seconds_sum{stage=\"build\",work=\"live\"} 0.2
flusso_stage_duration_seconds_count{stage=\"build\",work=\"live\"} 4
flusso_startup_duration_seconds{sink=\"primary\",step=\"stage\"} 0.3
";

    const AFTER: &str = "\
flusso_stage_duration_seconds_sum{stage=\"build\",work=\"live\"} 1.2
flusso_stage_duration_seconds_count{stage=\"build\",work=\"live\"} 24
flusso_stage_duration_seconds_sum{new_label=\"x\",sink=\"primary\",stage=\"flush\",work=\"live\"} 3
flusso_stage_duration_seconds_count{new_label=\"x\",sink=\"primary\",stage=\"flush\",work=\"live\"} 20
flusso_stage_duration_seconds_sum{sink=\"primary\",stage=\"wait\"} 0
flusso_stage_duration_seconds_count{sink=\"primary\",stage=\"wait\"} 0
flusso_flush_documents_sum{sink=\"primary\",work=\"live\"} 200
flusso_flush_documents_count{sink=\"primary\",work=\"live\"} 20
flusso_flush_size_bytes_sum{sink=\"primary\",work=\"live\"} 204800
flusso_flush_size_bytes_count{sink=\"primary\",work=\"live\"} 20
flusso_flush_duration_seconds_sum{sink=\"primary\"} 3
flusso_startup_duration_seconds{sink=\"primary\",step=\"stage\"} 0.3
flusso_startup_duration_seconds{step=\"live\"} 1.5
";

    #[test]
    fn a_phase_is_the_difference_between_two_scrapes_over_its_wall_time() {
        let attribution =
            Attribution::between("burst", &Scrape::parse(BEFORE), &Scrape::parse(AFTER), 4.0);
        let rows: Vec<(&str, &str, f64, f64)> = attribution
            .stages
            .iter()
            .map(|row| {
                (
                    row.labels.stage.as_str(),
                    row.labels.sink.as_str(),
                    row.batches,
                    row.share,
                )
            })
            .collect();
        assert_eq!(
            rows,
            vec![("build", "", 20.0, 0.25), ("flush", "primary", 20.0, 0.75)],
            "a stage that didn't run in the phase is left out; an unknown label is ignored"
        );
        assert!((attribution.stages[0].ms_per_batch - 50.0).abs() < 1e-9);
        let flush = &attribution.flushes[0];
        assert_eq!(
            (
                flush.flushes,
                flush.documents_per_flush,
                flush.bytes_per_flush
            ),
            (20.0, 10.0, Some(10240.0))
        );
        assert!(
            attribution.startup.is_empty(),
            "start-up belongs to the phase that saw it first"
        );
    }

    #[test]
    fn the_first_phase_carries_the_start_up_steps() {
        let attribution =
            Attribution::between("backfill", &Scrape::default(), &Scrape::parse(AFTER), 2.0);
        assert_eq!(
            attribution.startup,
            BTreeMap::from([("live".to_owned(), 1.5), ("stage primary".to_owned(), 0.3)])
        );
        let table = attribution.markdown();
        assert!(table.contains("| flush | live | primary |  | 20 | 3.00 | 150.0% | 150.00 |"));
        assert!(table.contains("| | live | primary | 20 | 10.0 | 10.0 |"));
    }
}
