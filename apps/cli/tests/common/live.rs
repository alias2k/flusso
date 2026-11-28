//! The live harness of the detach e2e binaries (ADR 0012 stage 10, slice 3):
//! the real `flusso run` binary, one or two processes, over a Postgres and an
//! OpenSearch container, with a proxy in front of each so a test can refuse,
//! hold or freeze exactly the requests a promise is about.
//!
//! - [`OsProxy`]: HTTP/1.1 in front of OpenSearch; a rule picks, per request,
//!   to forward it, answer `503`, hold it until released, or never answer.
//! - [`PgProxy`]: TCP in front of Postgres; `freeze` stops every byte both
//!   ways while keeping the sockets open, as a network that drops packets
//!   does (the server's walsender then holds the slot until
//!   `wal_sender_timeout`).
//! - [`Project`] writes a one-index deployment over table `t`, [`Flusso`]
//!   runs it, and the free functions read the meta index, `/status`,
//!   `/wait` and the slot.

#![allow(dead_code, clippy::unwrap_used, clippy::expect_used)]

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;
use testcontainers_modules::testcontainers::ImageExt;
use testcontainers_modules::testcontainers::runners::AsyncRunner;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::watch;

/// The table every project indexes.
pub(crate) const TABLE: &str = "CREATE TABLE t (id int PRIMARY KEY, name text)";

/// A Postgres with logical replication and `extra` server settings
/// (`name=value`), the table `t` created, and its URL.
pub(crate) async fn postgres(extra: &[&str]) -> (test_postgres::Container, String) {
    let mut cmd = vec![
        "postgres".to_owned(),
        "-c".to_owned(),
        "wal_level=logical".to_owned(),
    ];
    for setting in extra {
        cmd.push("-c".to_owned());
        cmd.push((*setting).to_owned());
    }
    let container = test_postgres::image().with_cmd(cmd).start().await.unwrap();
    let port = container.get_host_port_ipv4(5432).await.unwrap();
    let url = format!("postgres://postgres:postgres@127.0.0.1:{port}/postgres");
    let pool = pool(&url).await;
    sqlx::query(TABLE).execute(&pool).await.unwrap();
    (container, url)
}

pub(crate) async fn pool(url: &str) -> PgPool {
    PgPoolOptions::new()
        .max_connections(4)
        .connect(url)
        .await
        .unwrap()
}

// ---------------------------------------------------------------------------
// OpenSearch proxy

/// One request as the proxy saw it.
#[derive(Debug, Clone)]
pub(crate) struct Seen {
    pub(crate) method: String,
    pub(crate) path: String,
    pub(crate) body: String,
}

impl Seen {
    /// Whether this is a `_bulk` that writes to `index` (by name in its
    /// action lines).
    pub(crate) fn bulk_to(&self, index: &str) -> bool {
        self.path.starts_with("/_bulk") && self.body.contains(&format!("\"_index\":\"{index}\""))
    }

    /// Whether this request touches the meta index.
    pub(crate) fn meta(&self) -> bool {
        self.path.contains("flusso_meta")
    }
}

/// What the proxy does with one request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Act {
    /// Forward it.
    Pass,
    /// Answer `503` without forwarding it.
    Refuse,
    /// Hold it until [`OsProxy::release`], then forward it.
    Hold,
    /// Never answer it (the connection stays open, nothing comes back).
    Hang,
    /// Answer a `_bulk` as OpenSearch does when its target is gone: every
    /// item `index_not_found_exception`, nothing forwarded.
    Missing,
    /// Forward it this many milliseconds late.
    Delay(u64),
}

type Rule = Box<dyn Fn(&Seen) -> Act + Send + Sync>;

struct ProxyState {
    seen: Mutex<Vec<Seen>>,
    rule: Mutex<Option<Rule>>,
    released: watch::Sender<u64>,
}

/// An HTTP/1.1 proxy in front of OpenSearch. Enough HTTP for flusso's
/// client: one request at a time per connection, bodies sized by
/// `Content-Length`, keep-alive.
#[derive(Clone)]
pub(crate) struct OsProxy {
    pub(crate) url: String,
    state: Arc<ProxyState>,
}

impl OsProxy {
    pub(crate) async fn start(target: &str) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let state = Arc::new(ProxyState {
            seen: Mutex::new(Vec::new()),
            rule: Mutex::new(None),
            released: watch::channel(0).0,
        });
        let client = reqwest::Client::new();
        let target = target.to_owned();
        let accepting = Arc::clone(&state);
        tokio::spawn(async move {
            while let Ok((stream, _)) = listener.accept().await {
                let state = Arc::clone(&accepting);
                let client = client.clone();
                let target = target.clone();
                tokio::spawn(async move {
                    let _ = serve_http(stream, &state, &client, &target).await;
                });
            }
        });
        Self { url, state }
    }

    /// Decide every request from now on with `rule`.
    pub(crate) fn rule(&self, rule: impl Fn(&Seen) -> Act + Send + Sync + 'static) {
        *self.state.rule.lock().unwrap() = Some(Box::new(rule));
    }

    /// Forward everything from now on (held requests stay held until
    /// [`release`](Self::release)).
    pub(crate) fn heal(&self) {
        *self.state.rule.lock().unwrap() = None;
    }

    /// Forward every request held so far.
    pub(crate) fn release(&self) {
        self.state.released.send_modify(|n| *n += 1);
    }

    /// Every request seen so far.
    pub(crate) fn seen(&self) -> Vec<Seen> {
        self.state.seen.lock().unwrap().clone()
    }

    /// How many requests seen so far `pick` matches.
    pub(crate) fn count(&self, pick: impl Fn(&Seen) -> bool) -> usize {
        self.state
            .seen
            .lock()
            .unwrap()
            .iter()
            .filter(|s| pick(s))
            .count()
    }
}

async fn serve_http(
    mut stream: TcpStream,
    state: &ProxyState,
    client: &reqwest::Client,
    target: &str,
) -> std::io::Result<()> {
    let mut buffer = Vec::new();
    loop {
        let head_end = loop {
            if let Some(end) = find(&buffer, b"\r\n\r\n") {
                break end;
            }
            let mut chunk = [0u8; 8192];
            let n = stream.read(&mut chunk).await?;
            if n == 0 {
                return Ok(());
            }
            buffer.extend_from_slice(chunk.get(..n).unwrap());
        };
        let head = String::from_utf8_lossy(buffer.get(..head_end).unwrap()).into_owned();
        let mut lines = head.split("\r\n");
        let mut request_line = lines.next().unwrap_or_default().split(' ');
        let method = request_line.next().unwrap_or_default().to_owned();
        let path = request_line.next().unwrap_or_default().to_owned();
        let mut length = 0usize;
        let mut headers = Vec::new();
        for line in lines {
            if let Some((name, value)) = line.split_once(':') {
                let lower = name.trim().to_ascii_lowercase();
                if lower == "content-length" {
                    length = value.trim().parse().unwrap_or(0);
                } else if matches!(lower.as_str(), "content-type" | "authorization") {
                    headers.push((name.trim().to_owned(), value.trim().to_owned()));
                }
            }
        }
        let body_start = head_end + 4;
        while buffer.len() < body_start + length {
            let mut chunk = [0u8; 8192];
            let n = stream.read(&mut chunk).await?;
            if n == 0 {
                return Ok(());
            }
            buffer.extend_from_slice(chunk.get(..n).unwrap());
        }
        let body = buffer
            .get(body_start..body_start + length)
            .unwrap()
            .to_vec();
        buffer.drain(..body_start + length);

        let seen = Seen {
            method: method.clone(),
            path: path.clone(),
            body: String::from_utf8_lossy(&body).into_owned(),
        };
        let act = state
            .rule
            .lock()
            .unwrap()
            .as_ref()
            .map_or(Act::Pass, |rule| rule(&seen));
        state.seen.lock().unwrap().push(seen);
        match act {
            Act::Hang => {
                std::future::pending::<()>().await;
            }
            Act::Hold => {
                let mut released = state.released.subscribe();
                let at = *released.borrow();
                while *released.borrow() == at {
                    if released.changed().await.is_err() {
                        return Ok(());
                    }
                }
            }
            Act::Delay(millis) => tokio::time::sleep(Duration::from_millis(millis)).await,
            Act::Pass | Act::Refuse | Act::Missing => {}
        }
        let (status, reply_type, reply) = if act == Act::Missing {
            (
                200u16,
                "application/json".to_owned(),
                missing_items(&String::from_utf8_lossy(&body)).into_bytes(),
            )
        } else if act == Act::Refuse {
            (
                503u16,
                "application/json".to_owned(),
                br#"{"error":"refused by the test's proxy"}"#.to_vec(),
            )
        } else {
            let method = reqwest::Method::from_bytes(method.as_bytes()).unwrap();
            let mut forward = client.request(method, format!("{target}{path}")).body(body);
            for (name, value) in &headers {
                forward = forward.header(name.as_str(), value.as_str());
            }
            let response = forward.send().await.map_err(std::io::Error::other)?;
            let status = response.status().as_u16();
            let reply_type = response
                .headers()
                .get("content-type")
                .and_then(|value| value.to_str().ok())
                .unwrap_or("application/json")
                .to_owned();
            let reply = response.bytes().await.map_err(std::io::Error::other)?;
            (status, reply_type, reply.to_vec())
        };
        let head = format!(
            "HTTP/1.1 {status} X\r\ncontent-type: {reply_type}\r\ncontent-length: {}\r\n\r\n",
            reply.len()
        );
        stream.write_all(head.as_bytes()).await?;
        stream.write_all(&reply).await?;
    }
}

/// A `_bulk` answer naming every action of `body` refused as missing.
fn missing_items(body: &str) -> String {
    let items: Vec<Value> = body
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .filter_map(|line| {
            let (op, meta) = line.as_object()?.iter().next()?;
            matches!(op.as_str(), "index" | "create" | "delete" | "update").then(|| {
                let index = meta["_index"].as_str().unwrap_or_default();
                json!({ op.clone(): {
                    "_index": index,
                    "_id": meta["_id"],
                    "status": 404,
                    "error": {
                        "type": "index_not_found_exception",
                        "reason": format!("no such index [{index}]"),
                        "index": index,
                    },
                }})
            })
        })
        .collect();
    json!({"took": 0, "errors": true, "items": items}).to_string()
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

// ---------------------------------------------------------------------------
// Postgres proxy

/// A TCP proxy in front of Postgres that can go silent both ways.
#[derive(Clone)]
pub(crate) struct PgProxy {
    pub(crate) port: u16,
    frozen: Arc<AtomicBool>,
    thawed: watch::Sender<u64>,
}

impl PgProxy {
    pub(crate) async fn start(target_port: u16) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let frozen = Arc::new(AtomicBool::new(false));
        let thawed = watch::channel(0).0;
        let proxy = Self {
            port,
            frozen,
            thawed,
        };
        let accepting = proxy.clone();
        tokio::spawn(async move {
            while let Ok((client, _)) = listener.accept().await {
                let proxy = accepting.clone();
                tokio::spawn(async move {
                    let Ok(server) = TcpStream::connect(("127.0.0.1", target_port)).await else {
                        return;
                    };
                    let (client_read, client_write) = client.into_split();
                    let (server_read, server_write) = server.into_split();
                    let up = tokio::spawn(pump(client_read, server_write, proxy.clone()));
                    let down = tokio::spawn(pump(server_read, client_write, proxy));
                    let _ = tokio::join!(up, down);
                });
            }
        });
        proxy
    }

    /// The URL of the proxied database.
    pub(crate) fn url(&self) -> String {
        format!(
            "postgres://postgres:postgres@127.0.0.1:{}/postgres",
            self.port
        )
    }

    /// Stop every byte both ways, sockets kept open.
    pub(crate) fn freeze(&self) {
        self.frozen.store(true, Ordering::SeqCst);
    }

    /// Let bytes through again.
    pub(crate) fn thaw(&self) {
        self.frozen.store(false, Ordering::SeqCst);
        self.thawed.send_modify(|n| *n += 1);
    }

    async fn wait_thawed(&self) {
        let mut thawed = self.thawed.subscribe();
        while self.frozen.load(Ordering::SeqCst) {
            if thawed.changed().await.is_err() {
                return;
            }
        }
    }
}

async fn pump(
    mut from: tokio::net::tcp::OwnedReadHalf,
    mut to: tokio::net::tcp::OwnedWriteHalf,
    proxy: PgProxy,
) {
    let mut chunk = [0u8; 16384];
    loop {
        proxy.wait_thawed().await;
        let Ok(n) = from.read(&mut chunk).await else {
            return;
        };
        if n == 0 {
            return;
        }
        proxy.wait_thawed().await;
        if to.write_all(chunk.get(..n).unwrap()).await.is_err() {
            return;
        }
    }
}

// ---------------------------------------------------------------------------
// The project and the process

/// A one-index deployment over `t` in its own folder.
#[derive(Debug, Clone)]
pub(crate) struct Project {
    pub(crate) dir: PathBuf,
}

impl Project {
    /// A project named `name`: Postgres at `pg_url`, the slot `s_{name}`, an
    /// OpenSearch sink `primary` at `os_url`, with `sink_extra` lines in its
    /// table (`backfill = false`, say).
    pub(crate) fn new(name: &str, pg_url: &str, os_url: &str, sink_extra: &str) -> Self {
        let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!(
            "flusso-detach-{}-{name}-{}",
            std::process::id(),
            unique()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("t.schema.yml"),
            "version: 1\ntable: t\nprimary_key: id\nfields:\n  - integer: id\n    required: true\n  - keyword: name\n    required: false\n",
        )
        .unwrap();
        let project = Self { dir };
        project.write(name, pg_url, os_url, sink_extra);
        project
    }

    /// Write the config again with these settings (same slot and names).
    pub(crate) fn write(&self, name: &str, pg_url: &str, os_url: &str, sink_extra: &str) {
        std::fs::write(
            self.dir.join("flusso.toml"),
            format!(
                "[source]\ntype = \"postgres\"\nconnection_url = \"{pg_url}\"\nslot = \"s_{name}\"\n\
                 publication = \"p_{name}\"\n\n[sinks.primary]\ntype = \"opensearch\"\n\
                 url = \"{os_url}\"\nnumber_of_replicas = 0\n{sink_extra}\n\n[[index]]\n\
                 name = \"t\"\nschema = \"t.schema.yml\"\nenabled = true\n"
            ),
        )
        .unwrap();
    }
}

/// A `flusso run` child of the test, logging to a file.
pub(crate) struct Flusso {
    child: Child,
    log: PathBuf,
    pub(crate) public: SocketAddr,
    pub(crate) private: SocketAddr,
}

impl Flusso {
    /// Start `flusso run` over `project` and wait until both HTTP surfaces
    /// listen.
    pub(crate) fn spawn(project: &Project, extra: &[&str]) -> Self {
        let log = project.dir.join(format!("stderr-{}.log", unique()));
        let child = Command::new(env!("CARGO_BIN_EXE_flusso"))
            .current_dir(&project.dir)
            .env("NO_COLOR", "1")
            .env("RUST_LOG", "info")
            .args([
                "run",
                "--config",
                "flusso.toml",
                "--public-address",
                "127.0.0.1:0",
                "--private-address",
                "127.0.0.1:0",
            ])
            .args(extra)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(std::fs::File::create(&log).unwrap())
            .spawn()
            .unwrap();
        let mut flusso = Self {
            child,
            log,
            public: SocketAddr::from(([127, 0, 0, 1], 0)),
            private: SocketAddr::from(([127, 0, 0, 1], 0)),
        };
        let deadline = Instant::now() + Duration::from_secs(60);
        loop {
            let stderr = flusso.stderr();
            if let (Some(public), Some(private)) =
                (listening(&stderr, "public"), listening(&stderr, "private"))
            {
                flusso.public = public;
                flusso.private = private;
                return flusso;
            }
            assert!(flusso.running(), "flusso exited at start: {stderr}");
            assert!(Instant::now() < deadline, "flusso never listened: {stderr}");
            std::thread::sleep(Duration::from_millis(100));
        }
    }

    /// [`spawn`](Self::spawn), then wait until it follows live changes.
    pub(crate) async fn start(project: &Project, extra: &[&str]) -> Self {
        let mut flusso = Self::spawn(project, extra);
        flusso
            .until_log("following live changes", Duration::from_secs(90))
            .await;
        flusso
    }

    pub(crate) fn stderr(&self) -> String {
        std::fs::read_to_string(&self.log).unwrap_or_default()
    }

    pub(crate) fn pid(&self) -> u32 {
        self.child.id()
    }

    pub(crate) fn running(&mut self) -> bool {
        self.child.try_wait().unwrap().is_none()
    }

    /// Wait until the log holds `text` (the process still running).
    pub(crate) async fn until_log(&mut self, text: &str, limit: Duration) {
        let deadline = Instant::now() + limit;
        loop {
            let stderr = self.stderr();
            if stderr.contains(text) {
                return;
            }
            assert!(self.running(), "flusso exited before {text:?}: {stderr}");
            assert!(Instant::now() < deadline, "no {text:?} in: {stderr}");
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }

    /// Wait until `check` holds of `/status`, or fail with what it last said.
    pub(crate) async fn until_status(
        &mut self,
        what: &str,
        limit: Duration,
        check: impl Fn(&Value) -> bool,
    ) -> Value {
        let deadline = Instant::now() + limit;
        loop {
            if let Some(status) = status(self.public).await
                && check(&status)
            {
                return status;
            }
            assert!(
                self.running(),
                "flusso exited before {what}: {}",
                self.stderr()
            );
            assert!(
                Instant::now() < deadline,
                "never {what}: status {:#}\n{}",
                status(self.public).await.unwrap_or_default(),
                self.stderr()
            );
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
    }

    /// Wait until `t` is seeded on `primary`.
    pub(crate) async fn until_seeded(&mut self) -> Value {
        self.until_status("seeded", Duration::from_secs(90), |s| {
            s["sinks"]["primary"]["indexes"]["t"] == "seeded"
        })
        .await
    }

    /// Wait until `t`'s lane on `primary` has health `health`.
    pub(crate) async fn until_lane(&mut self, health: &str) -> Value {
        self.until_status(&format!("lane {health}"), Duration::from_secs(90), |s| {
            s["sinks"]["primary"]["lanes"]["t"]["health"] == health
        })
        .await
    }

    /// Send `signal` (`TERM`, `KILL`, …).
    pub(crate) fn signal(&self, signal: &str) {
        let sent = Command::new("kill")
            .args([&format!("-{signal}"), &self.child.id().to_string()])
            .status()
            .unwrap();
        assert!(sent.success(), "kill -{signal}");
    }

    /// The exit status, or a failed test (after a SIGKILL) past `limit`.
    pub(crate) fn exit_within(&mut self, limit: Duration) -> ExitStatus {
        let deadline = Instant::now() + limit;
        loop {
            if let Some(status) = self.child.try_wait().unwrap() {
                return status;
            }
            if Instant::now() > deadline {
                let _ = self.child.kill();
                let _ = self.child.wait();
                panic!("flusso still ran {limit:?} later: {}", self.stderr());
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    /// Wait until it exits by itself, within `limit`.
    pub(crate) async fn exited(&mut self, limit: Duration) -> ExitStatus {
        let deadline = Instant::now() + limit;
        loop {
            if let Some(status) = self.child.try_wait().unwrap() {
                return status;
            }
            assert!(
                Instant::now() < deadline,
                "still running: {}",
                self.stderr()
            );
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }

    /// Stop it gracefully and require exit 0.
    pub(crate) fn stop(&mut self) {
        self.signal("TERM");
        let status = self.exit_within(Duration::from_secs(60));
        assert!(status.success(), "{status}: {}", self.stderr());
    }

    /// Kill it, nothing drained.
    pub(crate) fn kill(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }

    /// Run `flusso <args…> --server <private>` and return its exit status,
    /// stdout and stderr.
    pub(crate) fn admin(&self, args: &[&str]) -> (bool, String, String) {
        let output = Command::new(env!("CARGO_BIN_EXE_flusso"))
            .env("NO_COLOR", "1")
            .args(args)
            .args(["--server", &self.private.to_string()])
            .output()
            .unwrap();
        (
            output.status.success(),
            String::from_utf8_lossy(&output.stdout).into_owned(),
            String::from_utf8_lossy(&output.stderr).into_owned(),
        )
    }

    /// `/wait` for `position` at `level`, with a short timeout: its status
    /// and body (status 0 when nothing listens).
    pub(crate) async fn wait(&self, position: &str, level: &str) -> (u16, Value) {
        wait_at(self.public, position, level, 3_000).await
    }
}

impl Drop for Flusso {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// The address a surface logged it listens on.
fn listening(stderr: &str, surface: &str) -> Option<SocketAddr> {
    stderr
        .lines()
        .filter(|line| line.contains("HTTP surface listening"))
        .find(|line| line.contains(&format!("surface=\"{surface}\"")))?
        .split_whitespace()
        .find_map(|word| word.strip_prefix("addr="))?
        .parse()
        .ok()
}

pub(crate) fn unique() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos()
}

/// `/status`, when it answers.
pub(crate) async fn status(public: SocketAddr) -> Option<Value> {
    reqwest::get(format!("http://{public}/status"))
        .await
        .ok()?
        .json()
        .await
        .ok()
}

pub(crate) async fn wait_at(
    public: SocketAddr,
    position: &str,
    level: &str,
    timeout_ms: u64,
) -> (u16, Value) {
    let Ok(response) = reqwest::Client::new()
        .get(format!("http://{public}/wait"))
        .query(&[
            ("position", position),
            ("until", level),
            ("timeout_ms", &timeout_ms.to_string()),
        ])
        .send()
        .await
    else {
        // Nothing listens: the process is gone.
        return (0, Value::Null);
    };
    let code = response.status().as_u16();
    (code, response.json().await.unwrap_or(Value::Null))
}

/// The `reason` of `t`'s lane on `primary` in `status`, or "".
pub(crate) fn lane_reason(status: &Value) -> String {
    status["sinks"]["primary"]["lanes"]["t"]["reason"]
        .as_str()
        .unwrap_or_default()
        .to_owned()
}

// ---------------------------------------------------------------------------
// OpenSearch reads and the meta index

/// The hash alias of index `t` (`t_<hash>`), from the generations it has.
pub(crate) async fn hash_alias(os_url: &str) -> String {
    let aliases: Value = reqwest::get(format!("{os_url}/_alias"))
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let mut names: Vec<String> = aliases
        .as_object()
        .unwrap()
        .values()
        .filter_map(|index| index["aliases"].as_object())
        .flat_map(|aliases| aliases.keys().cloned())
        .filter(|alias| alias.starts_with("t_"))
        .collect();
    names.sort();
    names.dedup();
    assert_eq!(names.len(), 1, "one hash alias: {aliases}");
    names.remove(0)
}

/// The generations behind `hash_alias`'s name (`{hash_alias}_{n}`), sorted.
pub(crate) async fn generations(os_url: &str, hash_alias: &str) -> Vec<String> {
    let indices: Value = reqwest::get(format!("{os_url}/_cat/indices/{hash_alias}_*?format=json"))
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let mut names: Vec<String> = indices
        .as_array()
        .map(|all| {
            all.iter()
                .filter_map(|index| index["index"].as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default();
    names.sort();
    names
}

/// What the alias `target` serves: id → `name`, refreshed, paged by id.
pub(crate) async fn documents(
    os_url: &str,
    target: &str,
) -> std::collections::BTreeMap<i32, Option<String>> {
    let http = reqwest::Client::new();
    http.post(format!("{os_url}/{target}/_refresh"))
        .send()
        .await
        .unwrap();
    let mut read = std::collections::BTreeMap::new();
    let mut after: Option<Value> = None;
    loop {
        let mut query = json!({"size": 5000, "query": {"match_all": {}}, "sort": [{"id": "asc"}]});
        if let Some(after) = &after {
            query["search_after"] = after.clone();
        }
        let body: Value = http
            .post(format!("{os_url}/{target}/_search"))
            .json(&query)
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        let hits = body["hits"]["hits"].as_array().cloned().unwrap_or_default();
        let Some(last) = hits.last() else {
            return read;
        };
        after = Some(last["sort"].clone());
        for hit in &hits {
            read.insert(
                hit["_source"]["id"].as_i64().unwrap() as i32,
                hit["_source"]["name"].as_str().map(str::to_owned),
            );
        }
    }
}

/// Where `index` and `table` differ, at most ten ids of each kind; empty
/// when they are equal.
pub(crate) fn differences(
    index: &std::collections::BTreeMap<i32, Option<String>>,
    table: &std::collections::BTreeMap<i32, Option<String>>,
) -> String {
    let missing: Vec<&i32> = table
        .keys()
        .filter(|id| !index.contains_key(id))
        .take(10)
        .collect();
    let extra: Vec<&i32> = index
        .keys()
        .filter(|id| !table.contains_key(id))
        .take(10)
        .collect();
    let stale: Vec<(&i32, &Option<String>, &Option<String>)> = table
        .iter()
        .filter_map(|(id, name)| {
            index
                .get(id)
                .filter(|held| *held != name)
                .map(|held| (id, held, name))
        })
        .take(10)
        .collect();
    if missing.is_empty() && extra.is_empty() && stale.is_empty() {
        return String::new();
    }
    format!(
        "index {} documents, table {} rows; missing {missing:?}; extra {extra:?}; stale (id, index, table) {stale:?}",
        index.len(),
        table.len()
    )
}

/// Fail unless what the alias `target` serves equals the table.
pub(crate) async fn assert_index_equals_table(os_url: &str, target: &str, pool: &PgPool) {
    let differ = differences(&documents(os_url, target).await, &table(pool).await);
    assert!(
        differ.is_empty(),
        "the index differs from the table: {differ}"
    );
}

/// The table as the index should hold it.
pub(crate) async fn table(pool: &PgPool) -> std::collections::BTreeMap<i32, Option<String>> {
    sqlx::query_as::<_, (i32, Option<String>)>("SELECT id, name FROM t")
        .fetch_all(pool)
        .await
        .unwrap()
        .into_iter()
        .collect()
}

/// A meta-index document's source, or `None`.
pub(crate) async fn meta_doc(os_url: &str, id: &str) -> Option<Value> {
    let response = reqwest::get(format!("{os_url}/flusso_meta/_doc/{id}"))
        .await
        .unwrap();
    if response.status().as_u16() == 404 {
        return None;
    }
    let body: Value = response.json().await.unwrap();
    (body["found"] == true).then(|| body["_source"].clone())
}

/// Write a meta-index document by hand, refreshed.
pub(crate) async fn put_meta(os_url: &str, id: &str, doc: &Value) {
    let response = reqwest::Client::new()
        .put(format!("{os_url}/flusso_meta/_doc/{id}?refresh=true"))
        .json(doc)
        .send()
        .await
        .unwrap();
    assert!(
        response.status().is_success(),
        "{id}: {:?}",
        response.text().await
    );
}

/// Whether `t`'s detach record is gone: absent, or a forgotten tombstone.
pub(crate) async fn record_gone(os_url: &str, hash_alias: &str) -> bool {
    meta_doc(os_url, &format!("detach:{hash_alias}"))
        .await
        .is_none_or(|record| record["state"] == "forgotten")
}

/// A `detached` record as a hand would write it, the marker left where it
/// is: flusso detaches the lane itself (`DetachAgain`) once it holds the
/// slot, or, `live_only`, takes it as detached as it stands.
pub(crate) fn hand_record(serving: u64, live_only: bool) -> Value {
    json!({
        "state": "detached",
        "live_only": live_only,
        "serving": serving,
        "why": "written by the test",
        "attempts": 0,
    })
}

/// Detach `t`'s lane in a running process that holds the slot: a record
/// written by hand beside the marker on the serving generation (the
/// process adopts it: no other process writes while it holds the slot),
/// then one write answered as if the target were gone, so the lane restages,
/// reads the record and detaches itself (`DetachAgain`) where it stands.
/// Returns the position of that write.
pub(crate) async fn detach_while_running(
    flusso: &mut Flusso,
    proxy: &OsProxy,
    os_url: &str,
    pool: &PgPool,
    id: i32,
) -> String {
    let alias = hash_alias(os_url).await;
    put_meta(os_url, &format!("detach:{alias}"), &hand_record(1, false)).await;
    let once = Arc::new(AtomicBool::new(true));
    let target = alias.clone();
    proxy.rule(move |seen| {
        if seen.path.starts_with("/_bulk")
            && seen.body.contains(&target)
            && once.swap(false, Ordering::SeqCst)
        {
            Act::Missing
        } else {
            Act::Pass
        }
    });
    let position = commit(pool, &format!("INSERT INTO t VALUES ({id}, 'detaching')")).await;
    flusso.until_lane("detached").await;
    proxy.heal();
    position
}

// ---------------------------------------------------------------------------
// Postgres

pub(crate) async fn current_lsn(pool: &PgPool) -> String {
    sqlx::query_scalar("SELECT pg_current_wal_lsn()::text")
        .fetch_one(pool)
        .await
        .unwrap()
}

pub(crate) async fn insert(pool: &PgPool, ids: std::ops::RangeInclusive<i32>, name: &str) {
    sqlx::query("INSERT INTO t SELECT g, $1 FROM generate_series($2::int, $3::int) g")
        .bind(name)
        .bind(*ids.start())
        .bind(*ids.end())
        .execute(pool)
        .await
        .unwrap();
}

/// Run `statement`, commit, and return the position after it.
pub(crate) async fn commit(pool: &PgPool, statement: &str) -> String {
    sqlx::query(sqlx::AssertSqlSafe(statement.to_owned()))
        .execute(pool)
        .await
        .unwrap();
    current_lsn(pool).await
}

/// The slot's `confirmed_flush_lsn` as a number.
pub(crate) async fn confirmed_flush(pool: &PgPool, slot: &str) -> u64 {
    let text: Option<String> = sqlx::query_scalar(
        "SELECT confirmed_flush_lsn::text FROM pg_replication_slots WHERE slot_name = $1",
    )
    .bind(slot)
    .fetch_one(pool)
    .await
    .unwrap();
    lsn_value(&text.unwrap())
}

pub(crate) fn lsn_value(text: &str) -> u64 {
    let (high, low) = text.split_once('/').unwrap();
    (u64::from_str_radix(high, 16).unwrap() << 32) | u64::from_str_radix(low, 16).unwrap()
}

/// The pid of the walsender serving `slot`, if one does.
pub(crate) async fn walsender(pool: &PgPool, slot: &str) -> Option<i32> {
    sqlx::query_scalar("SELECT active_pid FROM pg_replication_slots WHERE slot_name = $1")
        .bind(slot)
        .fetch_optional(pool)
        .await
        .unwrap()
        .flatten()
}

/// Terminate the walsender serving `slot`; whether there was one.
pub(crate) async fn terminate_walsender(pool: &PgPool, slot: &str) -> bool {
    let Some(pid) = walsender(pool, slot).await else {
        return false;
    };
    sqlx::query_scalar::<_, bool>("SELECT pg_terminate_backend($1)")
        .bind(pid)
        .fetch_one(pool)
        .await
        .unwrap()
}

/// Wait until `check` holds, polling every 200 ms, or fail naming `what`.
pub(crate) async fn eventually<F, Fut>(what: &str, limit: Duration, check: F)
where
    F: Fn() -> Fut,
    Fut: std::future::Future<Output = bool>,
{
    let deadline = Instant::now() + limit;
    while !check().await {
        assert!(Instant::now() < deadline, "never {what}");
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
}

/// The folder a project lives in.
pub(crate) fn dir_of(project: &Project) -> &Path {
    &project.dir
}
