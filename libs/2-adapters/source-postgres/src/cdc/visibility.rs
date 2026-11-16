//! When a decoded transaction can be read back (issue #343, ADR 0010).
//!
//! Postgres commits in three steps: it flushes the commit record, waits for
//! synchronous standbys when `synchronous_commit` asks it to, and only then
//! takes the transaction out of the proc array, which is when new snapshots
//! see it. The walsender decodes up to the flushed WAL, so a transaction can
//! reach the stream while every query flusso makes still reads the state
//! before it. The stream therefore holds a committed transaction back until
//! [`Snapshot::sees`] says a new snapshot includes it.
//!
//! - **The check runs in Rust** over `pg_current_snapshot()`'s text, the same
//!   rule as `pg_visible_in_snapshot`: below `xmin` visible, at or past `xmax`
//!   not, in between visible unless listed as running.
//! - **Widening.** pgoutput's `Begin` carries a 32-bit xid; the snapshot is in
//!   64-bit `xid8`s. A decoded transaction is within 2^31 xids of the
//!   snapshot's `xmax` (it is running or recently finished, and the server
//!   stops assigning xids long before the oldest running one is 2^31 behind),
//!   so its full xid is `xmax` moved by the **signed** 32-bit distance between
//!   the two. `xmax` is the last finished xid + 1, not the next xid: a held
//!   transaction is often at or past it.
//! - **Polling** is [`poll_delay`]: every 0.5 ms for the first 20 ms, then
//!   doubling up to 50 ms. tokio's timer ticks in milliseconds, so the fast
//!   phase waits about 1 ms in practice.
//!
//! ```text
//! snapshot 4294968030:4294968030:   (epoch 1)
//!   xid 734 → 4294968030 = xmax  → not visible (the held transaction)
//!   xid 733 → 4294968029 < xmin  → visible
//! snapshot 100:110:105
//!   xid 105 → running             → not visible
//!   xid 106 → between, not running → visible
//! ```

use std::future::Future;
use std::time::Duration;

use source::{Result, SourceError};
use sqlx::PgPool;

/// A snapshot as `pg_current_snapshot()` prints it: `xmin:xmax:xip,…`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Snapshot {
    xmin: u64,
    xmax: u64,
    running: Vec<u64>,
}

impl Snapshot {
    /// Parse the text form, `xmin:xmax:` followed by the running xids.
    pub(crate) fn parse(text: &str) -> Result<Self> {
        let invalid = || SourceError::Decode(format!("`{text}` is not a pg_snapshot"));
        let mut parts = text.trim().splitn(3, ':');
        let number = |part: Option<&str>| {
            part.and_then(|part| part.parse::<u64>().ok())
                .ok_or_else(invalid)
        };
        let xmin = number(parts.next())?;
        let xmax = number(parts.next())?;
        let running = match parts.next() {
            None => return Err(invalid()),
            Some("") => Vec::new(),
            Some(list) => list
                .split(',')
                .map(|xid| xid.parse::<u64>().map_err(|_| invalid()))
                .collect::<Result<_>>()?,
        };
        Ok(Self {
            xmin,
            xmax,
            running,
        })
    }

    /// Whether the transaction with 32-bit `xid` is visible to this snapshot.
    pub(crate) fn sees(&self, xid: u32) -> bool {
        let full = widen(xid, self.xmax);
        if full < self.xmin {
            return true;
        }
        full < self.xmax && !self.running.contains(&full)
    }
}

/// The full xid of 32-bit `xid`, within 2^31 of `xmax`.
pub(crate) fn widen(xid: u32, xmax: u64) -> u64 {
    // The low 32 bits of `xmax`: a truncating cast on purpose.
    #[allow(clippy::cast_possible_truncation)]
    let distance = xid.wrapping_sub(xmax as u32) as i32;
    xmax.wrapping_add_signed(i64::from(distance))
}

/// How long to wait before checking again, once a transaction has been
/// waited on for `waited` and the last wait was `previous`.
pub(crate) fn poll_delay(waited: Duration, previous: Duration) -> Duration {
    const FAST: Duration = Duration::from_micros(500);
    if waited < Duration::from_millis(20) {
        FAST
    } else {
        (previous * 2).clamp(FAST, Duration::from_millis(50))
    }
}

/// How `synchronous_standby_names` takes in an `application_name`: by its
/// name, by `*`, or both.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct Listing {
    pub(crate) by_name: bool,
    pub(crate) by_wildcard: bool,
}

/// Read `synchronous_standby_names` in any of its forms (`a, b`,
/// `2 (a, b)`, `FIRST 1 (a)`, `ANY 1 (a)`, double-quoted names) for
/// `application_name`. Postgres compares names case-insensitively, and `*`
/// matches every walsender.
pub(crate) fn standby_listing(names: &str, application_name: &str) -> Listing {
    let tokens = standby_tokens(names);
    let listed: Vec<&Token> = match tokens.iter().position(|token| *token == Token::Open) {
        Some(open) => tokens.iter().skip(open + 1).collect(),
        None => tokens.iter().collect(),
    };
    let mut listing = Listing::default();
    for token in listed {
        if let Token::Name(name) = token {
            listing.by_wildcard |= name == "*";
            listing.by_name |= name.eq_ignore_ascii_case(application_name);
        }
    }
    listing
}

/// The refusal for a server whose `synchronous_standby_names` takes in flusso,
/// or `None`. Every commit would wait for flusso to confirm it while flusso
/// waits for the commit to become visible: neither finishes.
pub(crate) fn refuse_standby(names: &str, application_name: &str) -> Option<SourceError> {
    let listing = standby_listing(names, application_name);
    let how = match (listing.by_name, listing.by_wildcard) {
        (false, false) => return None,
        (true, false) => format!("lists '{application_name}'"),
        (false, true) => "takes in every walsender with '*', flusso's included".to_owned(),
        (true, true) => format!("lists '{application_name}' and '*'"),
    };
    let remedy = if listing.by_wildcard {
        let also = if listing.by_name {
            format!(", and take '{application_name}' out")
        } else {
            String::new()
        };
        format!(
            "Replace '*' with the names of the synchronous standbys{also}, then reload. '*' \
             matches any application_name, so renaming flusso does not help, and flusso counts \
             among the candidates even where the other standbys could fill the quorum"
        )
    } else {
        format!(
            "Take '{application_name}' out of synchronous_standby_names and reload, or give \
             flusso another application_name"
        )
    };
    Some(SourceError::Setup(format!(
        "the source would make flusso a synchronous standby: synchronous_standby_names = \
         '{names}' {how}. A commit would wait for flusso to confirm it while flusso waits for \
         the commit to become visible, so neither would finish. {remedy}"
    )))
}

#[derive(Debug, PartialEq, Eq)]
enum Token {
    Name(String),
    Open,
    Close,
}

fn standby_tokens(names: &str) -> Vec<Token> {
    let mut tokens = Vec::new();
    let mut chars = names.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '(' => tokens.push(Token::Open),
            ')' => tokens.push(Token::Close),
            ',' => {}
            c if c.is_whitespace() => {}
            '"' => {
                let mut name = String::new();
                while let Some(c) = chars.next() {
                    if c == '"' {
                        if chars.peek() == Some(&'"') {
                            chars.next();
                            name.push('"');
                        } else {
                            break;
                        }
                    } else {
                        name.push(c);
                    }
                }
                tokens.push(Token::Name(name));
            }
            c => {
                let mut word = String::from(c);
                while let Some(&next) = chars.peek() {
                    if next == ','
                        || next == '('
                        || next == ')'
                        || next == '"'
                        || next.is_whitespace()
                    {
                        break;
                    }
                    word.push(next);
                    chars.next();
                }
                tokens.push(Token::Name(word));
            }
        }
    }
    tokens
}

/// What a stall check found about a transaction held past the warning.
#[derive(Debug, Default)]
pub(crate) struct Stall {
    /// The server now names flusso a synchronous standby: stop with this.
    pub(crate) refusal: Option<SourceError>,
    /// The sessions waiting on synchronous replication, described.
    pub(crate) waiting: Vec<String>,
}

/// Where the stream reads visibility from: the server, or a test double.
pub(crate) trait Visibility: Clone + Send + Sync + 'static {
    /// A new snapshot.
    fn snapshot(&self) -> impl Future<Output = Result<Snapshot>> + Send;

    /// Why the transaction with `xid` is still not visible, as far as the
    /// server tells.
    fn stall(&self, xid: u32) -> impl Future<Output = Result<Stall>> + Send;
}

/// A session waiting on synchronous replication: pid, user, application, xid,
/// seconds waited (`NULL`s where the role may not see another's session).
type WaitingSession = (
    i32,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<f64>,
);

/// The source's own visibility, read through the capture's SQL pool.
#[derive(Debug, Clone)]
pub(crate) struct PgVisibility {
    pub(crate) pool: PgPool,
    pub(crate) application_name: String,
    pub(crate) slot: String,
}

impl Visibility for PgVisibility {
    async fn snapshot(&self) -> Result<Snapshot> {
        let text: String = sqlx::query_scalar("SELECT pg_current_snapshot()::text")
            .fetch_one(&self.pool)
            .await
            .map_err(|e| SourceError::Query(format!("reading a snapshot: {e}")))?;
        Snapshot::parse(&text)
    }

    async fn stall(&self, xid: u32) -> Result<Stall> {
        let query = |e: sqlx::Error| SourceError::Query(format!("inspecting a held commit: {e}"));
        let (names, sync_state): (String, Option<String>) = sqlx::query_as(
            "SELECT current_setting('synchronous_standby_names'), \
             (SELECT r.sync_state FROM pg_replication_slots s \
              JOIN pg_stat_replication r ON r.pid = s.active_pid WHERE s.slot_name = $1)",
        )
        .bind(&self.slot)
        .fetch_one(&self.pool)
        .await
        .map_err(query)?;
        // The setting decides it for any role. The walsender's row is a
        // cross-check: without `pg_read_all_stats` its `sync_state` is NULL.
        let refusal = refuse_standby(&names, &self.application_name).or_else(|| {
            sync_state.filter(|state| state != "async").map(|state| {
                SourceError::Setup(format!(
                    "flusso's walsender is a synchronous standby (sync_state = '{state}', \
                         synchronous_standby_names = '{names}'): a commit would wait for \
                         flusso to confirm it while flusso waits for the commit to become \
                         visible. Change synchronous_standby_names so it no longer takes in \
                         flusso, then reload"
                ))
            })
        });
        let rows: Vec<WaitingSession> = sqlx::query_as(
            "SELECT pid, usename::text, application_name, backend_xid::text, \
                 extract(epoch FROM clock_timestamp() - state_change)::float8 \
                 FROM pg_stat_activity WHERE wait_event = 'SyncRep' ORDER BY pid",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(query)?;
        let held = xid.to_string();
        let waiting = rows
            .into_iter()
            .map(|(pid, user, application, backend_xid, seconds)| {
                let holds = if backend_xid.as_deref() == Some(held.as_str()) {
                    ", the transaction flusso waits for"
                } else {
                    ""
                };
                format!(
                    "pid {pid} (user {}, application '{}', xid {}, waiting {:.0} s{holds})",
                    user.as_deref().unwrap_or("?"),
                    application.as_deref().unwrap_or(""),
                    backend_xid.as_deref().unwrap_or("?"),
                    seconds.unwrap_or_default(),
                )
            })
            .collect();
        Ok(Stall { refusal, waiting })
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests;
