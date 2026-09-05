//! The child `flusso` process: spawn, read `/status` and `/metrics`, sample its
//! resident set and CPU time, stop it.

use std::net::{SocketAddr, TcpListener};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use serde::Serialize;
use tokio::process::{Child, Command};

/// Peak resident set and CPU time so far, from `ps`.
#[derive(Debug, Clone, Copy, Default, Serialize)]
pub(crate) struct Resources {
    pub(crate) peak_rss_mib: f64,
    pub(crate) cpu_seconds: f64,
}

#[derive(Debug)]
pub(crate) struct Flusso {
    child: Child,
    pid: u32,
    public: SocketAddr,
    client: reqwest::Client,
    resources: Arc<Mutex<Resources>>,
    rss_breached: Arc<AtomicBool>,
    sampler: tokio::task::JoinHandle<()>,
    pub(crate) log_path: PathBuf,
}

fn free_port() -> Result<SocketAddr> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    Ok(listener.local_addr()?)
}

impl Flusso {
    /// Start `bin run` on `config`, with the source and sink pointed at the
    /// given URLs through their override variables. Logs go to
    /// `workdir/flusso.log`, the lock to `workdir/flusso.lock`.
    pub(crate) async fn spawn(
        bin: &Path,
        config: &Path,
        workdir: &Path,
        pg_url: &str,
        os_url: &str,
        rss_cap_mib: u64,
    ) -> Result<Self> {
        let public = free_port()?;
        let private = free_port()?;
        let log_path = workdir.join("flusso.log");
        let log = std::fs::File::create(&log_path)?;
        let child = Command::new(bin)
            .arg("run")
            .arg("--config")
            .arg(config)
            .arg("--lock")
            .arg(workdir.join("flusso.lock"))
            .arg("--public-address")
            .arg(public.to_string())
            .arg("--private-address")
            .arg(private.to_string())
            .env("SOURCE_POSTGRES_CONNECTION_URL", pg_url)
            .env("PRIMARY_OPENSEARCH_URL", os_url)
            .env(
                "RUST_LOG",
                std::env::var("FLUSSO_LOG").unwrap_or_else(|_| "info".into()),
            )
            .current_dir(workdir)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::from(log))
            .kill_on_drop(true)
            .spawn()
            .with_context(|| format!("spawning {}", bin.display()))?;
        let pid = child.id().context("the child has no pid")?;
        let resources = Arc::new(Mutex::new(Resources::default()));
        let rss_breached = Arc::new(AtomicBool::new(false));
        let sampler = tokio::spawn(sample_forever(
            pid,
            Arc::clone(&resources),
            Arc::clone(&rss_breached),
            rss_cap_mib as f64,
        ));
        tracing::info!(pid, %public, log = %log_path.display(), "flusso started");
        Ok(Self {
            child,
            pid,
            public,
            client: reqwest::Client::new(),
            resources,
            rss_breached,
            sampler,
            log_path,
        })
    }

    pub(crate) fn resources(&self) -> Resources {
        *self
            .resources
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Fails if the child exited or its resident set crossed the cap.
    fn check_alive(&mut self) -> Result<()> {
        if let Some(status) = self.child.try_wait()? {
            bail!(
                "flusso exited early with {status}; see {}",
                self.log_path.display()
            );
        }
        if self.rss_breached.load(Ordering::Relaxed) {
            bail!(
                "flusso crossed the RSS cap (peak {:.0} MiB)",
                self.resources().peak_rss_mib
            );
        }
        Ok(())
    }

    pub(crate) async fn status(&self) -> Result<serde_json::Value> {
        let url = format!("http://{}/status", self.public);
        Ok(self.client.get(url).send().await?.json().await?)
    }

    pub(crate) async fn metrics(&self) -> Result<String> {
        let url = format!("http://{}/metrics", self.public);
        Ok(self.client.get(url).send().await?.text().await?)
    }

    /// Wait until the deployment is `live` with every index seeded. Returns the
    /// time from spawn to that moment.
    pub(crate) async fn wait_seeded(
        &mut self,
        started: Instant,
        cap: Duration,
    ) -> Result<Duration> {
        loop {
            self.check_alive()?;
            if let Ok(status) = self.status().await {
                let live = status.get("phase").and_then(|p| p.as_str()) == Some("live");
                let seeded = status
                    .get("indexes")
                    .and_then(|i| i.as_object())
                    .is_some_and(|indexes| {
                        !indexes.is_empty()
                            && indexes.values().all(|s| s.as_str() == Some("seeded"))
                    });
                if live && seeded {
                    return Ok(started.elapsed());
                }
            }
            if started.elapsed() > cap {
                bail!("the backfill did not finish within {cap:?}");
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    }

    /// `changes_captured` right now — the baseline before a burst.
    pub(crate) async fn captured(&self) -> Result<u64> {
        let status = self.status().await?;
        Ok(status
            .get("changes_captured")
            .and_then(|c| c.as_u64())
            .unwrap_or(0))
    }

    /// Wait until at least `expected` more changes than `baseline` have been
    /// captured and every sink has nothing in flight. Returns the instant that
    /// first held.
    pub(crate) async fn wait_drained(
        &mut self,
        baseline: u64,
        expected: u64,
        cap: Duration,
    ) -> Result<Instant> {
        let started = Instant::now();
        loop {
            self.check_alive()?;
            let status = self.status().await?;
            let now = Instant::now();
            let captured = status
                .get("changes_captured")
                .and_then(|c| c.as_u64())
                .unwrap_or(0);
            let idle = status
                .get("sinks")
                .and_then(|s| s.as_object())
                .is_some_and(|sinks| {
                    sinks.values().all(|sink| {
                        sink.get("changes_in_flight").and_then(|v| v.as_u64()) == Some(0)
                    })
                });
            if captured >= baseline + expected && idle {
                return Ok(now);
            }
            if started.elapsed() > cap {
                bail!(
                    "the burst did not drain within {cap:?} (captured {} of {expected}, idle {idle})",
                    captured.saturating_sub(baseline)
                );
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }

    /// SIGTERM, wait up to ten seconds, then kill.
    pub(crate) async fn stop(mut self) -> Result<Resources> {
        self.sampler.abort();
        let _ = Command::new("kill")
            .arg("-TERM")
            .arg(self.pid.to_string())
            .status()
            .await;
        match tokio::time::timeout(Duration::from_secs(10), self.child.wait()).await {
            Ok(status) => {
                tracing::info!(?status, "flusso stopped");
            }
            Err(_) => {
                tracing::warn!("flusso ignored SIGTERM for 10s; killing it");
                self.child.kill().await?;
            }
        }
        Ok(self.resources())
    }
}

/// Every 500 ms: `ps -o rss= -o cputime= -p PID`, portable across macOS and
/// Linux. RSS is reported in KiB on both.
async fn sample_forever(
    pid: u32,
    resources: Arc<Mutex<Resources>>,
    breached: Arc<AtomicBool>,
    cap_mib: f64,
) {
    let mut ticker = tokio::time::interval(Duration::from_millis(500));
    loop {
        ticker.tick().await;
        let Ok(output) = Command::new("ps")
            .args(["-o", "rss=", "-o", "cputime=", "-p", &pid.to_string()])
            .output()
            .await
        else {
            continue;
        };
        let text = String::from_utf8_lossy(&output.stdout);
        let mut fields = text.split_whitespace();
        let (Some(rss), Some(cputime)) = (fields.next(), fields.next()) else {
            continue;
        };
        let rss_mib = rss.parse::<f64>().unwrap_or(0.0) / 1024.0;
        let cpu = parse_cputime(cputime);
        let mut guard = resources
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        guard.peak_rss_mib = guard.peak_rss_mib.max(rss_mib);
        guard.cpu_seconds = cpu;
        if rss_mib > cap_mib {
            breached.store(true, Ordering::Relaxed);
        }
    }
}

/// `[[dd-]hh:]mm:ss[.cc]` → seconds.
fn parse_cputime(text: &str) -> f64 {
    let (days, rest) = match text.split_once('-') {
        Some((d, rest)) => (d.parse::<f64>().unwrap_or(0.0), rest),
        None => (0.0, text),
    };
    let parts: Vec<f64> = rest
        .split(':')
        .map(|p| p.parse::<f64>().unwrap_or(0.0))
        .collect();
    let seconds = parts.iter().fold(0.0, |acc, part| acc * 60.0 + part);
    days * 86_400.0 + seconds
}

#[cfg(test)]
mod tests {
    use super::parse_cputime;

    #[test]
    fn cputime_forms() {
        assert_eq!(parse_cputime("00:05"), 5.0);
        assert_eq!(parse_cputime("01:02:03"), 3723.0);
        assert_eq!(parse_cputime("1-00:00:01"), 86_401.0);
        assert_eq!(parse_cputime("0:01.50"), 1.5);
    }
}
