//! Where a phase's time went, read from flusso's own `/metrics` (issue #399).
//!
//! `flusso.stage.duration` is one histogram per stage, labelled by `stage`,
//! `work`, `sink` and `index` where the engine knows them. Its `_sum` is the
//! seconds that stage was busy, so the difference between two scrapes, over
//! the phase's wall time, is the stage's **busy/wall**: the mean number of it
//! in flight. A stage a task runs one at a time stays at or under 1; one it
//! runs concurrently can pass it.
//!
//! Rows are grouped by **task**: the ingest task (`idle`, `page`, `resolve`,
//! `build`, `publish`, `snapshot`, and `lane_wait`, ingest time labelled with
//! the sink whose lane it waited on, overlapping `publish`), then each sink
//! (`idle`, `write`, `seed`, `reindex`). In the first phase each task opens
//! with a **start-up** row: the time before its loop ran (ingest: the
//! `live` start-up step; a sink: that less `prepare` and `open_live`, since
//! its loop starts as the source is prepared). Each task ends with an
//! **other** row: the wall time left (acks, confirmations, and the idle
//! stretch still open at the scrape, up to the one-second cut). Negative
//! once a task runs stages concurrently.
//!
//! ```text
//! | task | stage | work | sink | index | batches | busy s | busy/wall | ms/batch |
//! | ingest | build | backfill |  |  | 243 | 6.80 | 0.39 | 28.00 |
//! | ingest | other |  |  |  |  | 0.05 | 0.00 |  |
//! | sink primary | write | backfill | primary |  | 243 | 12.60 | 0.72 | 51.85 |
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

/// One stage's line in a phase: whose task it is, how often, how long in
/// all, busy seconds per wall second, and the mean per batch.
#[derive(Debug, Clone, Serialize)]
pub(crate) struct Row {
    pub(crate) task: String,
    #[serde(flatten)]
    pub(crate) labels: Labels,
    pub(crate) batches: f64,
    pub(crate) busy_seconds: f64,
    pub(crate) busy_per_wall: f64,
    pub(crate) ms_per_batch: f64,
}

/// What a task's stages leave of the wall time, after its start-up (the first
/// phase only: the time before the task's loop ran).
#[derive(Debug, Clone, Serialize)]
pub(crate) struct Other {
    pub(crate) task: String,
    pub(crate) startup_seconds: f64,
    pub(crate) seconds: f64,
    pub(crate) busy_per_wall: f64,
}

/// The ingest task's stages first, in pipeline order, then the sinks'.
const STAGE_ORDER: [&str; 10] = [
    "idle",
    "page",
    "resolve",
    "build",
    "publish",
    "lane_wait",
    "snapshot",
    "write",
    "seed",
    "reindex",
];

/// The task a series' time belongs to: a sink's own stages are that sink's;
/// `lane_wait` is the ingest task waiting on a sink's lane.
fn task_of(labels: &Labels) -> String {
    if labels.sink.is_empty() || labels.stage == "lane_wait" {
        "ingest".to_owned()
    } else {
        format!("sink {}", labels.sink)
    }
}

fn stage_rank(stage: &str) -> usize {
    STAGE_ORDER
        .iter()
        .position(|known| *known == stage)
        .unwrap_or(STAGE_ORDER.len())
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
    pub(crate) other: Vec<Other>,
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
        let per_wall = |seconds: f64| {
            if wall_seconds > 0.0 {
                seconds / wall_seconds
            } else {
                0.0
            }
        };
        let mut stages: Vec<Row> = after
            .stages
            .iter()
            .filter_map(|(labels, now)| {
                let moved = delta(&before.stages, labels, *now);
                (moved.count > 0.0).then(|| Row {
                    task: task_of(labels),
                    labels: labels.clone(),
                    batches: moved.count,
                    busy_seconds: moved.sum,
                    busy_per_wall: per_wall(moved.sum),
                    ms_per_batch: moved.sum / moved.count * 1_000.0,
                })
            })
            .collect();
        stages.sort_by(|a, b| {
            (
                a.task != "ingest",
                &a.task,
                stage_rank(&a.labels.stage),
                &a.labels,
            )
                .cmp(&(
                    b.task != "ingest",
                    &b.task,
                    stage_rank(&b.labels.stage),
                    &b.labels,
                ))
        });
        let mut accounted: Vec<(String, f64)> = Vec::new();
        for row in stages.iter().filter(|row| row.labels.stage != "lane_wait") {
            match accounted.last_mut() {
                Some((task, seconds)) if *task == row.task => *seconds += row.busy_seconds,
                _ => accounted.push((row.task.clone(), row.busy_seconds)),
            }
        }
        let first_phase = before.startup.is_empty();
        let step = |name: &str| {
            after
                .startup
                .iter()
                .find(|(labels, _)| labels.stage == name && labels.sink.is_empty())
                .map_or(0.0, |(_, seconds)| *seconds)
        };
        // The ingest loop starts with the live stream; each sink's once every
        // sink is staged, as the source is prepared.
        let ingest_start = step("live");
        let sink_start = (ingest_start - step("prepare") - step("open_live")).max(0.0);
        let other = accounted
            .into_iter()
            .map(|(task, busy)| {
                let startup_seconds = match (first_phase, task.as_str()) {
                    (false, _) => 0.0,
                    (true, "ingest") => ingest_start,
                    (true, _) => sink_start,
                };
                let left = wall_seconds - busy - startup_seconds;
                Other {
                    task,
                    startup_seconds,
                    seconds: left,
                    busy_per_wall: per_wall(left),
                }
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
            other,
            flushes,
            startup,
        }
    }

    /// The attribution as Markdown, ready to paste into a pull request.
    pub(crate) fn markdown(&self) -> String {
        let mut out = String::new();
        let _ = writeln!(
            out,
            "**{}** ({:.2} s wall)\n\n| task | stage | work | sink | index | batches | busy s | busy/wall | ms/batch |\n| --- | --- | --- | --- | --- | ---: | ---: | ---: | ---: |",
            self.phase, self.wall_seconds
        );
        for other in &self.other {
            if other.startup_seconds > 0.0 {
                let _ = writeln!(
                    out,
                    "| {} | start-up |  |  |  |  | {:.2} | {:.2} |  |",
                    other.task,
                    other.startup_seconds,
                    other.startup_seconds / self.wall_seconds.max(f64::MIN_POSITIVE)
                );
            }
            for row in self.stages.iter().filter(|row| row.task == other.task) {
                let _ = writeln!(
                    out,
                    "| {} | {} | {} | {} | {} | {:.0} | {:.2} | {:.2} | {:.2} |",
                    row.task,
                    row.labels.stage,
                    row.labels.work,
                    row.labels.sink,
                    row.labels.index,
                    row.batches,
                    row.busy_seconds,
                    row.busy_per_wall,
                    row.ms_per_batch
                );
            }
            let _ = writeln!(
                out,
                "| {} | other |  |  |  |  | {:.2} | {:.2} |  |",
                other.task, other.seconds, other.busy_per_wall
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
flusso_stage_duration_seconds_sum{stage=\"idle\"} 2.5
flusso_stage_duration_seconds_count{stage=\"idle\"} 3
flusso_stage_duration_seconds_sum{sink=\"primary\",stage=\"lane_wait\",work=\"live\"} 0.5
flusso_stage_duration_seconds_count{sink=\"primary\",stage=\"lane_wait\",work=\"live\"} 20
flusso_stage_duration_seconds_sum{new_label=\"x\",sink=\"primary\",stage=\"write\",work=\"live\"} 3
flusso_stage_duration_seconds_count{new_label=\"x\",sink=\"primary\",stage=\"write\",work=\"live\"} 20
flusso_stage_duration_seconds_sum{sink=\"primary\",stage=\"idle\"} 0
flusso_stage_duration_seconds_count{sink=\"primary\",stage=\"idle\"} 0
flusso_flush_documents_sum{sink=\"primary\",work=\"live\"} 200
flusso_flush_documents_count{sink=\"primary\",work=\"live\"} 20
flusso_flush_size_bytes_sum{sink=\"primary\",work=\"live\"} 204800
flusso_flush_size_bytes_count{sink=\"primary\",work=\"live\"} 20
flusso_flush_duration_seconds_sum{sink=\"primary\"} 3
flusso_startup_duration_seconds{sink=\"primary\",step=\"stage\"} 0.3
flusso_startup_duration_seconds{step=\"live\"} 1.5
";

    #[test]
    fn a_phase_is_the_difference_between_two_scrapes_grouped_by_task() {
        let attribution =
            Attribution::between("burst", &Scrape::parse(BEFORE), &Scrape::parse(AFTER), 4.0);
        let rows: Vec<(&str, &str, f64, f64)> = attribution
            .stages
            .iter()
            .map(|row| {
                (
                    row.task.as_str(),
                    row.labels.stage.as_str(),
                    row.batches,
                    row.busy_per_wall,
                )
            })
            .collect();
        assert_eq!(
            rows,
            vec![
                ("ingest", "idle", 3.0, 0.625),
                ("ingest", "build", 20.0, 0.25),
                ("ingest", "lane_wait", 20.0, 0.125),
                ("sink primary", "write", 20.0, 0.75),
            ],
            "ingest first in pipeline order, lane_wait with the ingest task, a stage that didn't \
             run in the phase left out, an unknown label ignored"
        );
        assert!((attribution.stages[1].ms_per_batch - 50.0).abs() < 1e-9);
        let other: Vec<(&str, f64)> = attribution
            .other
            .iter()
            .map(|other| (other.task.as_str(), other.seconds))
            .collect();
        assert_eq!(
            other,
            vec![("ingest", 0.5), ("sink primary", 1.0)],
            "other is the wall time a task's stages leave, lane_wait not counted twice"
        );
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
    fn the_first_phase_carries_the_start_up_steps_and_busy_can_pass_the_wall() {
        let attribution =
            Attribution::between("backfill", &Scrape::default(), &Scrape::parse(AFTER), 2.0);
        assert_eq!(
            attribution.startup,
            BTreeMap::from([("live".to_owned(), 1.5), ("stage primary".to_owned(), 0.3)])
        );
        let table = attribution.markdown();
        assert!(
            table.contains(
                "| sink primary | write | live | primary |  | 20 | 3.00 | 1.50 | 150.00 |"
            ),
            "busy/wall is the mean number in flight, not capped at one:\n{table}"
        );
        assert!(
            table.contains("| ingest | start-up |  |  |  |  | 1.50 | 0.75 |  |"),
            "the first phase opens each task with the time before its loop ran:\n{table}"
        );
        assert!(table.contains("| sink primary | start-up |  |  |  |  | 1.50 | 0.75 |  |"));
        assert!(table.contains("| sink primary | other |  |  |  |  | -2.50 | -1.25 |  |"));
        assert!(table.contains("| | live | primary | 20 | 10.0 | 10.0 |"));
    }
}
