//! An HTTP/1.1 proxy in front of OpenSearch that records every request and,
//! once armed, answers some of them `503` instead of forwarding them: a crash
//! or a failure between two of the sink's requests, placed exactly.
//!
//! Enough HTTP for the sink's client: one request at a time per connection,
//! bodies sized by `Content-Length` (no chunked bodies), keep-alive.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

/// One request as the proxy saw it.
#[derive(Debug, Clone)]
pub(crate) struct Seen {
    pub(crate) method: String,
    pub(crate) path: String,
    pub(crate) body: String,
    /// Whether the proxy refused it instead of forwarding it.
    pub(crate) refused: bool,
}

impl Seen {
    pub(crate) fn line(&self) -> String {
        format!("{} {}", self.method, self.path)
    }
}

type Rule = Box<dyn Fn(&Seen, usize) -> bool + Send + Sync>;

#[derive(Default)]
struct State {
    seen: Mutex<Vec<Seen>>,
    rule: Mutex<Option<Rule>>,
    /// Requests counted since the rule was set.
    counted: AtomicUsize,
}

/// The proxy; its URL stands in for OpenSearch's.
#[derive(Clone)]
pub(crate) struct FaultProxy {
    pub(crate) url: String,
    state: Arc<State>,
}

impl FaultProxy {
    /// Start a proxy forwarding to `target` (`http://host:port`).
    pub(crate) async fn start(target: &str) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let state = Arc::new(State::default());
        let client = reqwest::Client::new();
        let target = target.to_owned();
        let accepting = Arc::clone(&state);
        tokio::spawn(async move {
            loop {
                let Ok((stream, _)) = listener.accept().await else {
                    return;
                };
                let state = Arc::clone(&accepting);
                let client = client.clone();
                let target = target.clone();
                tokio::spawn(async move {
                    let _ = serve(stream, &state, &client, &target).await;
                });
            }
        });
        Self { url, state }
    }

    /// Refuse every request `rule` picks from now on; it gets the request and
    /// how many requests came before it since this call.
    pub(crate) fn refuse(&self, rule: impl Fn(&Seen, usize) -> bool + Send + Sync + 'static) {
        self.state.counted.store(0, Ordering::SeqCst);
        *self.state.rule.lock().unwrap() = Some(Box::new(rule));
    }

    /// Let `n` more requests through, then refuse every one after them: a
    /// sink that dies after its `n`th request.
    pub(crate) fn crash_after(&self, n: usize) {
        self.refuse(move |_, before| before >= n);
    }

    /// Forward everything again.
    pub(crate) fn heal(&self) {
        *self.state.rule.lock().unwrap() = None;
    }

    /// Every request seen so far, and forget them.
    pub(crate) fn take(&self) -> Vec<Seen> {
        std::mem::take(&mut *self.state.seen.lock().unwrap())
    }
}

async fn serve(
    mut stream: TcpStream,
    state: &State,
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
            buffer.extend_from_slice(&chunk[..n]);
        };
        let head = String::from_utf8_lossy(&buffer[..head_end]).into_owned();
        let mut lines = head.split("\r\n");
        let mut request_line = lines.next().unwrap_or_default().split(' ');
        let method = request_line.next().unwrap_or_default().to_owned();
        let path = request_line.next().unwrap_or_default().to_owned();
        let mut length = 0usize;
        let mut content_type = None;
        for line in lines {
            if let Some((name, value)) = line.split_once(':') {
                match name.trim().to_ascii_lowercase().as_str() {
                    "content-length" => length = value.trim().parse().unwrap_or(0),
                    "content-type" => content_type = Some(value.trim().to_owned()),
                    _ => {}
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
            buffer.extend_from_slice(&chunk[..n]);
        }
        let body = buffer[body_start..body_start + length].to_vec();
        buffer.drain(..body_start + length);

        let mut seen = Seen {
            method: method.clone(),
            path: path.clone(),
            body: String::from_utf8_lossy(&body).into_owned(),
            refused: false,
        };
        seen.refused = {
            let rule = state.rule.lock().unwrap();
            match rule.as_ref() {
                Some(rule) => {
                    let before = state.counted.fetch_add(1, Ordering::SeqCst);
                    rule(&seen, before)
                }
                None => false,
            }
        };
        state.seen.lock().unwrap().push(seen.clone());

        let (status, reply_type, reply) = if seen.refused {
            (
                503u16,
                "application/json".to_owned(),
                br#"{"error":"refused by the test's fault proxy"}"#.to_vec(),
            )
        } else {
            let method = reqwest::Method::from_bytes(method.as_bytes()).unwrap();
            let mut forward = client.request(method, format!("{target}{path}")).body(body);
            if let Some(content_type) = content_type {
                forward = forward.header("Content-Type", content_type);
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

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}
