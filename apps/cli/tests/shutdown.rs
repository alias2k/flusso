//! `flusso run` stops when it is told to, and a second start says what holds
//! the first one's place (issue #371).
//!
//! Each test starts the real binary over a Postgres container, puts it in the
//! state a laptop wakes into (a half-open HTTP request, an open long-poll, a
//! dead parent), signals it, and times the exit.
//!
//! Requires Docker. Ignored by default; run with:
//!
//! ```text
//! cargo nextest run -p flusso-cli --run-ignored all -E 'binary(shutdown)'
//! ```

#![cfg(unix)]
#![allow(clippy::unwrap_used, clippy::expect_used, unused_crate_dependencies)]

mod common;

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::time::{Duration, Instant};

/// The exit a `flusso run` must reach after its stop, past which the test
/// calls it stuck: the default deadline plus room for a slow runner.
const DEFAULT_DEADLINE: Duration = Duration::from_secs(5);
const MARGIN: Duration = Duration::from_secs(5);

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires docker"]
async fn a_half_open_http_request_does_not_hold_the_stop_past_the_deadline() {
    let (_postgres, url) = common::start_postgres(TABLE).await;
    let dir = project(&url, "half_open");
    let mut flusso = Flusso::start(&dir, &[]);

    let _client = half_open_request(flusso.public);
    let started = Instant::now();
    flusso.signal("TERM");
    let status = flusso.exit_within(DEFAULT_DEADLINE + MARGIN);

    assert!(
        started.elapsed() >= DEFAULT_DEADLINE - Duration::from_millis(500),
        "it waited for the request up to the deadline: {:?}",
        started.elapsed()
    );
    assert_eq!(status.code(), Some(1), "{}", flusso.stderr());
    let stderr = flusso.stderr();
    assert!(
        stderr.contains("shutdown deadline of 5s passed")
            && stderr.contains("closing the HTTP surfaces"),
        "the log says what it gave up on: {stderr}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires docker"]
async fn a_second_signal_stops_at_once() {
    let (_postgres, url) = common::start_postgres(TABLE).await;
    let dir = project(&url, "second");
    let mut flusso = Flusso::start(&dir, &["--shutdown-timeout-secs", "120"]);

    let _client = half_open_request(flusso.public);
    flusso.signal("TERM");
    std::thread::sleep(Duration::from_secs(1));
    assert!(flusso.running(), "the first signal waits on the request");
    flusso.signal("INT");
    let status = flusso.exit_within(Duration::from_secs(3));

    assert_eq!(
        status.code(),
        Some(130),
        "128 + SIGINT: {}",
        flusso.stderr()
    );
    assert!(
        flusso.stderr().contains("second signal"),
        "{}",
        flusso.stderr()
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires docker"]
async fn an_open_wait_is_answered_and_does_not_delay_the_stop() {
    let (_postgres, url) = common::start_postgres(TABLE).await;
    let dir = project(&url, "wait");
    let mut flusso = Flusso::start(&dir, &[]);

    let public = flusso.public;
    let waiter = std::thread::spawn(move || {
        let mut stream = TcpStream::connect(public).unwrap();
        stream
            .write_all(
                b"GET /wait?position=FFFFFFFF/0&timeout_ms=60000 HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n",
            )
            .unwrap();
        let mut answer = String::new();
        stream.read_to_string(&mut answer).unwrap();
        answer
    });
    std::thread::sleep(Duration::from_millis(500));
    let started = Instant::now();
    flusso.signal("TERM");
    let status = flusso.exit_within(Duration::from_secs(4));

    assert!(
        started.elapsed() < Duration::from_secs(3),
        "{:?}",
        started.elapsed()
    );
    assert_eq!(status.code(), Some(0), "{}", flusso.stderr());
    let answer = waiter.join().unwrap();
    assert!(
        answer.starts_with("HTTP/1.1 503") && answer.contains("shutting down"),
        "the waiter is told flusso is stopping: {answer}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires docker"]
async fn sighup_stops_like_sigterm() {
    let (_postgres, url) = common::start_postgres(TABLE).await;
    let dir = project(&url, "hup");
    let mut flusso = Flusso::start(&dir, &[]);

    flusso.signal("HUP");
    let status = flusso.exit_within(Duration::from_secs(4));

    assert_eq!(
        status.code(),
        Some(0),
        "a clean stop, not a kill: {status:?}"
    );
    assert!(flusso.stderr().contains("SIGHUP"), "{}", flusso.stderr());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires docker"]
async fn exit_with_parent_stops_a_run_whose_parent_died() {
    let (_postgres, url) = common::start_postgres(TABLE).await;
    let dir = project(&url, "orphan");

    let watched = Orphan::start(&dir, &["--exit-with-parent"]);
    let unwatched = Orphan::start(&dir, &["--slot", "s_other", "--publication", "p_other"]);
    watched.kill_parent();
    unwatched.kill_parent();

    let deadline = Instant::now() + Duration::from_secs(5);
    while watched.alive() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(100));
    }
    let watched_alive = watched.alive();
    let unwatched_alive = unwatched.alive();
    watched.kill();
    unwatched.kill();

    assert!(
        !watched_alive,
        "--exit-with-parent stops the orphan: {}",
        watched.stderr()
    );
    assert!(
        watched.stderr().contains("parent process") && watched.stderr().contains("exited"),
        "{}",
        watched.stderr()
    );
    assert!(
        unwatched_alive,
        "without the flag an orphan keeps running, like any daemon"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires docker"]
async fn a_second_start_names_the_flusso_that_holds_its_place() {
    let (_postgres, url) = common::start_postgres(TABLE).await;
    let dir = project(&url, "twice");
    let mut first = Flusso::start(&dir, &[]);

    let same_address = Command::new(env!("CARGO_BIN_EXE_flusso"))
        .current_dir(&dir)
        .env("NO_COLOR", "1")
        .args([
            "run",
            "--config",
            "flusso.toml",
            "--public-address",
            &first.public.to_string(),
            "--private-address",
            "127.0.0.1:0",
        ])
        .stdin(Stdio::null())
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&same_address.stderr);
    assert!(!same_address.status.success(), "{text}");
    assert!(
        text.contains(&format!("another flusso (pid {})", first.child.id())),
        "the address error names the running flusso: {text}"
    );

    let mut second = Flusso::spawn(&dir, &[]);
    let deadline = Instant::now() + Duration::from_secs(30);
    let named = loop {
        let stderr = second.stderr();
        if stderr.contains("replication slot \"s_twice\" is in use") {
            break stderr;
        }
        assert!(
            Instant::now() < deadline,
            "the second run never named the slot's holder: {stderr}"
        );
        std::thread::sleep(Duration::from_millis(200));
    };
    second.signal("TERM");
    second.exit_within(Duration::from_secs(10));
    first.signal("TERM");
    first.exit_within(Duration::from_secs(10));
    assert!(
        named.contains("application_name \"flusso\"") && named.contains("pg_terminate_backend"),
        "the slot error names the holder and the way out: {named}"
    );
}

const TABLE: &str = "CREATE TABLE t (id int PRIMARY KEY, name text)";

/// A project of one index over `t`, writing to stdout.
fn project(url: &str, name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
        .join(format!("flusso-shutdown-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("t.schema.yml"),
        "version: 1\ntable: t\nprimary_key: id\nfields:\n  - integer: id\n    required: true\n  - keyword: name\n    required: false\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("flusso.toml"),
        format!(
            "[source]\ntype = \"postgres\"\nconnection_url = \"{url}\"\nslot = \"s_{name}\"\n\
             publication = \"p_{name}\"\n\n[sinks.out]\ntype = \"stdout\"\n\n[[index]]\n\
             name = \"t\"\nschema = \"t.schema.yml\"\nenabled = true\n"
        ),
    )
    .unwrap();
    dir
}

/// A `flusso run` child of the test, logging to a file.
struct Flusso {
    child: Child,
    log: PathBuf,
    public: SocketAddr,
}

impl Flusso {
    /// Start `flusso run` and wait until it follows live changes.
    fn start(dir: &Path, extra: &[&str]) -> Self {
        let mut flusso = Self::spawn(dir, extra);
        let deadline = Instant::now() + Duration::from_secs(60);
        loop {
            let stderr = flusso.stderr();
            if stderr.contains("following live changes") {
                flusso.public = listening(&stderr, "public").unwrap();
                return flusso;
            }
            assert!(flusso.running(), "flusso exited at start: {stderr}");
            assert!(
                Instant::now() < deadline,
                "flusso never went live: {stderr}"
            );
            std::thread::sleep(Duration::from_millis(100));
        }
    }

    fn spawn(dir: &Path, extra: &[&str]) -> Self {
        let log = dir.join(format!("stderr-{}.log", unique()));
        let child = Command::new(env!("CARGO_BIN_EXE_flusso"))
            .current_dir(dir)
            .env("NO_COLOR", "1")
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
        Self {
            child,
            log,
            public: SocketAddr::from(([127, 0, 0, 1], 0)),
        }
    }

    fn stderr(&self) -> String {
        std::fs::read_to_string(&self.log).unwrap_or_default()
    }

    fn running(&mut self) -> bool {
        self.child.try_wait().unwrap().is_none()
    }

    fn signal(&self, name: &str) {
        send(self.child.id(), name);
    }

    /// The exit status, or a failed test (after a SIGKILL) when it takes
    /// longer than `limit`.
    fn exit_within(&mut self, limit: Duration) -> ExitStatus {
        let deadline = Instant::now() + limit;
        loop {
            if let Some(status) = self.child.try_wait().unwrap() {
                return status;
            }
            if Instant::now() > deadline {
                let _ = self.child.kill();
                let _ = self.child.wait();
                panic!(
                    "flusso was still running {limit:?} after the stop: {}",
                    self.stderr()
                );
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }
}

impl Drop for Flusso {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// A `flusso run` started by a shell that the test then kills with SIGKILL,
/// the way a task runner dies without passing anything on.
struct Orphan {
    parent: Child,
    pid: u32,
    log: PathBuf,
}

impl Orphan {
    fn start(dir: &Path, extra: &[&str]) -> Self {
        let log = dir.join(format!("orphan-{}.log", unique()));
        let pid_file = dir.join(format!("orphan-{}.pid", unique()));
        let mut parent = Command::new("sh")
            .current_dir(dir)
            .env("NO_COLOR", "1")
            .arg("-c")
            .arg(r#"bin="$1"; log="$2"; pidfile="$3"; shift 3; "$bin" "$@" 2>"$log" >/dev/null & echo $! > "$pidfile"; wait"#)
            .arg("sh")
            .arg(env!("CARGO_BIN_EXE_flusso"))
            .arg(&log)
            .arg(&pid_file)
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
            .spawn()
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(60);
        let pid = loop {
            let log_text = std::fs::read_to_string(&log).unwrap_or_default();
            if log_text.contains("following live changes")
                && let Ok(pid) = std::fs::read_to_string(&pid_file)
            {
                break pid.trim().parse().unwrap();
            }
            assert!(
                parent.try_wait().unwrap().is_none(),
                "the shell exited: {log_text}"
            );
            assert!(
                Instant::now() < deadline,
                "flusso never went live: {log_text}"
            );
            std::thread::sleep(Duration::from_millis(100));
        };
        Self { parent, pid, log }
    }

    fn kill_parent(&self) {
        send(self.parent.id(), "KILL");
    }

    fn alive(&self) -> bool {
        Command::new("kill")
            .args(["-0", &self.pid.to_string()])
            .stderr(Stdio::null())
            .status()
            .unwrap()
            .success()
    }

    fn stderr(&self) -> String {
        std::fs::read_to_string(&self.log).unwrap_or_default()
    }

    fn kill(&self) {
        let _ = Command::new("kill")
            .args(["-KILL", &self.pid.to_string()])
            .stderr(Stdio::null())
            .status();
    }
}

impl Drop for Orphan {
    fn drop(&mut self) {
        self.kill();
        let _ = self.parent.kill();
        let _ = self.parent.wait();
    }
}

fn send(pid: u32, signal: &str) {
    let sent = Command::new("kill")
        .args([&format!("-{signal}"), &pid.to_string()])
        .status()
        .unwrap();
    assert!(sent.success(), "kill -{signal} {pid}");
}

/// A request whose headers never end: what a client that vanished mid-request
/// (a laptop gone to sleep) leaves on the server.
fn half_open_request(address: SocketAddr) -> TcpStream {
    let mut stream = TcpStream::connect(address).unwrap();
    stream
        .write_all(b"GET /status HTTP/1.1\r\nHost: x\r\n")
        .unwrap();
    std::thread::sleep(Duration::from_millis(200));
    stream
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

fn unique() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos()
}
