//! The user's record of saves in flight: what tells a journal this user's
//! designer wrote, for this folder, from any other.
//!
//! A journal is a file in the project, so anyone who can put a file there
//! (a repository, an archive, a shared folder) can write one, or bring a
//! copy of one the user's designer really wrote. So recovery acts only on a
//! journal this user's designer has a **record** of, kept where a project
//! never travels: one small file per save in flight in the user's own state
//! folder ([`dir`]):
//!
//! - Linux and other Unix: `$XDG_STATE_HOME/flusso/design-saves/`, else
//!   `~/.local/state/flusso/design-saves/`;
//! - macOS: `~/Library/Application Support/io.github.alias2k.flusso/design-saves/`;
//! - Windows: `%APPDATA%\alias2k\flusso\data\design-saves\`.
//!
//! A record is `<token>.json`:
//!
//! ```text
//! {
//!   "format": 1,
//!   "token": "4242-1763030000000000000",
//!   "journal": "<sha-256 of the journal's exact bytes>",
//!   "dir": "/home/me/project",
//!   "committed": false,
//!   "created_at_ms": 1763030000000
//! }
//! ```
//!
//! - **Bound to its project**: `dir` is the config folder's canonical path.
//!   A journal found in another folder (a copied or moved project, a sibling
//!   an attacker planted it in) is only reported.
//! - **Single use**: a record is written before its journal appears and
//!   removed once the save is settled (cleaned up, undone in the process, or
//!   recovered). An old journal, replayed, has no record.
//! - **The commit point**: `committed` turns true, durably, once every file is
//!   in place. Recovery reads roll back or finish from the record, never from
//!   the journal's file name.
//!
//! The folder is created mode 0700, a record mode 0600 with `create_new`
//! (never replacing a file), and neither is ever opened through a link or
//! with a blocking open. A folder or record with wider permissions or
//! another owner is not used. Without a usable folder a save still works:
//! its journal undoes a failure in the process, but a journal a killed save
//! leaves is only reported. When the designer starts ([`State::prune`]) it
//! prunes records older than [`MAX_AGE`], and records whose folder is gone
//! once they are older than [`GONE_AGE`] too (a folder moved away and back,
//! or on a drive not mounted, keeps its record for a while). A temp file a
//! killed write left goes once its process is gone ([`TEMP_AGE`] where that
//! can't be checked).
//!
//! Beside the records, `notices/` keeps what the designer is to tell the user
//! about saves it settled ([`Notices`]): one `<token>.json` per save, written
//! before the save's record is removed, so a recovery the user was never told
//! about outlives the designer. The API owns what one says; it is pruned
//! once older than [`MAX_AGE`].
//!
//! ```ignore
//! let state = State::open()?;
//! let record = Record::new(&token, &journal_bytes, base);
//! state.create(&record, &mut disk)?;
//! assert_eq!(state.find(&Digest::of(&journal_bytes)).unwrap().dir, base);
//! ```

use std::fs::OpenOptions;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Once;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use super::disk::{Disk, not_a_link, sync_dir};
use super::journal::{Digest, read_at_most, token_ok};

/// The folder that holds the records, in the state folder.
#[cfg(not(test))]
const RECORDS: &str = "design-saves";
/// The folder that holds the notices, in the records folder.
const NOTICES: &str = "notices";
/// The largest notice read.
const MAX_NOTICE: u64 = 1024 * 1024;
/// The record format this designer writes and reads.
const FORMAT: u32 = 1;
/// The largest record read: far beyond any real one.
const MAX_RECORD: u64 = 64 * 1024;
/// A record older than this is pruned: the save it names was never
/// recovered, and its journal will only be reported.
pub(crate) const MAX_AGE: Duration = Duration::from_secs(180 * 24 * 60 * 60);
/// A record whose project folder is missing is pruned once this old: a
/// folder can be away for a while (moved and back, a drive not mounted).
pub(crate) const GONE_AGE: Duration = Duration::from_secs(7 * 24 * 60 * 60);
/// A file in the folder that is no record is pruned once this old; so is a
/// temp file whose process looks alive (its pid may have been reused).
const STRAY_AGE: Duration = Duration::from_secs(24 * 60 * 60);
/// A temp file a killed write left, whose process can't be checked, is
/// pruned once this old.
pub(crate) const TEMP_AGE: Duration = Duration::from_secs(60 * 60);

/// One save in flight.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Record {
    format: u32,
    /// The save's token.
    pub(crate) token: String,
    /// The SHA-256 of the journal's exact bytes.
    pub(crate) journal: Digest,
    /// The config's folder, canonical.
    pub(crate) dir: PathBuf,
    /// Whether every file was in place: the commit point.
    pub(crate) committed: bool,
    created_at_ms: u64,
}

impl Record {
    /// The record of a save with `token` whose journal is `journal`, in the
    /// project at `dir`.
    pub(crate) fn new(token: &str, journal: &[u8], dir: &Path) -> Self {
        Self {
            format: FORMAT,
            token: token.to_owned(),
            journal: Digest::of(journal),
            dir: dir.to_path_buf(),
            committed: false,
            created_at_ms: now_ms(),
        }
    }

    fn encode(&self) -> io::Result<Vec<u8>> {
        serde_json::to_vec_pretty(self).map_err(io::Error::other)
    }
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
}

/// The user's records folder, checked usable.
#[derive(Debug)]
pub(crate) struct State {
    dir: PathBuf,
}

/// Why there is no usable records folder.
#[derive(Debug)]
pub(crate) enum StateError {
    /// No state folder could be found (no home folder).
    NoFolder,
    /// The folder is there but can't be used.
    Unusable(PathBuf, io::Error),
}

impl std::fmt::Display for StateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoFolder => {
                f.write_str("there is no folder for the designer's save records (no home folder)")
            }
            Self::Unusable(path, e) => write!(
                f,
                "the designer's save records folder {} can't be used: {e}",
                path.display()
            ),
        }
    }
}

impl State {
    /// The records folder, made when missing.
    pub(crate) fn open() -> Result<Self, StateError> {
        Self::at(dir().ok_or(StateError::NoFolder)?)
    }

    /// The records folder at `dir`, made when missing.
    pub(crate) fn at(dir: PathBuf) -> Result<Self, StateError> {
        private_dir(&dir).map_err(|e| StateError::Unusable(dir.clone(), e))?;
        Ok(Self { dir })
    }

    /// The records folder for a save: `None`, logged once per process, when
    /// it can't be used.
    pub(crate) fn for_saving() -> Option<Self> {
        Self::open().map_err(|e| warn_once(&e.to_string())).ok()
    }

    /// The folder.
    pub(crate) fn path(&self) -> &Path {
        &self.dir
    }

    /// The notices of settled saves, in their folder, made when missing.
    pub(crate) fn notices(&self) -> io::Result<Notices> {
        let dir = self.dir.join(NOTICES);
        private_dir(&dir)?;
        Ok(Notices { dir })
    }

    fn file(&self, token: &str) -> PathBuf {
        self.dir.join(format!("{token}.json"))
    }

    /// Write `record`, durably, never replacing a file.
    pub(crate) fn create(&self, record: &Record, disk: &mut Disk) -> io::Result<()> {
        if !token_ok(&record.token) {
            return Err(io::Error::other("not a save's token"));
        }
        disk.create_private(&self.file(&record.token), &record.encode()?)
    }

    /// Mark `record` committed: the commit point. The caller flushes the
    /// folder ([`State::path`]).
    pub(crate) fn commit(&self, record: &Record, disk: &mut Disk) -> io::Result<()> {
        let committed = Record {
            committed: true,
            ..record.clone()
        };
        disk.replace_private(&self.file(&record.token), &committed.encode()?)
    }

    /// Remove the record of the save `token`.
    pub(crate) fn remove(&self, token: &str, disk: &mut Disk) -> io::Result<()> {
        if !token_ok(token) {
            return Ok(());
        }
        disk.remove_file(&self.file(token))
    }

    /// Every readable record.
    pub(crate) fn records(&self) -> Vec<Record> {
        self.entries()
            .into_iter()
            .filter_map(|(_, record)| record)
            .collect()
    }

    /// Every file in the folder, with its record when it is a readable one.
    fn entries(&self) -> Vec<(PathBuf, Option<Record>)> {
        let Ok(entries) = std::fs::read_dir(&self.dir) else {
            return Vec::new();
        };
        entries
            .flatten()
            .map(|entry| {
                let path = entry.path();
                let token = entry
                    .file_name()
                    .to_str()
                    .and_then(|n| n.strip_suffix(".json"))
                    .filter(|t| token_ok(t))
                    .map(str::to_owned);
                let record = token.and_then(|token| {
                    let record = read_private(&path, MAX_RECORD)
                        .map_err(|e| {
                            warn_once(&format!("the record {} is not used: {e}", path.display()))
                        })
                        .ok()
                        .flatten()
                        .and_then(|bytes| serde_json::from_slice::<Record>(&bytes).ok())?;
                    (record.format == FORMAT && record.token == token).then_some(record)
                });
                (path, record)
            })
            .collect()
    }

    /// The record of the journal whose bytes have the digest `journal`.
    pub(crate) fn find(&self, journal: &Digest) -> Option<Record> {
        self.records().into_iter().find(|r| r.journal == *journal)
    }

    /// The records of saves in the project at `dir`.
    pub(crate) fn for_dir(&self, dir: &Path) -> Vec<Record> {
        self.records()
            .into_iter()
            .filter(|r| r.dir == dir)
            .collect()
    }

    /// Remove the records older than [`MAX_AGE`], or whose project folder
    /// is gone and that are older than [`GONE_AGE`], and the stray files a
    /// killed write left (see the module docs).
    pub(crate) fn prune(&self) {
        let now = now_ms();
        let age = |ms: u64| Duration::from_millis(now.saturating_sub(ms));
        for (path, record) in self.entries() {
            let stale = match &record {
                Some(r) => {
                    let age = age(r.created_at_ms);
                    age > MAX_AGE || (age > GONE_AGE && r.dir.symlink_metadata().is_err())
                }
                None => {
                    let elapsed = path
                        .symlink_metadata()
                        .ok()
                        .filter(|m| m.is_file())
                        .and_then(|m| m.modified().ok())
                        .and_then(|t| t.elapsed().ok());
                    match (elapsed, temp_owner(&path).map(alive)) {
                        (None, _) => false,
                        (Some(_), Some(Some(false))) => true,
                        (Some(elapsed), Some(None)) => elapsed > TEMP_AGE,
                        (Some(elapsed), _) => elapsed > STRAY_AGE,
                    }
                }
            };
            if stale {
                tracing::debug!(path = %path.display(), "pruning a save record");
                std::fs::remove_file(&path).ok();
            }
        }
        if let Ok(notices) = self.notices() {
            notices.prune();
        }
    }
}

/// What the designer is to tell the user about saves it settled, one
/// private file per save (see the module docs). Its bytes are the API's.
#[derive(Debug)]
pub(crate) struct Notices {
    dir: PathBuf,
}

impl Notices {
    fn file(&self, token: &str) -> Option<PathBuf> {
        token_ok(token).then(|| self.dir.join(format!("{token}.json")))
    }

    /// Write the notice of the save `token`, replacing any earlier one.
    pub(crate) fn put(&self, token: &str, bytes: &[u8], disk: &mut Disk) -> io::Result<()> {
        let file = self
            .file(token)
            .ok_or_else(|| io::Error::other("not a save's token"))?;
        disk.replace_private(&file, bytes)
    }

    /// Every readable notice, by its save's token.
    pub(crate) fn all(&self) -> Vec<(String, Vec<u8>)> {
        let Ok(entries) = std::fs::read_dir(&self.dir) else {
            return Vec::new();
        };
        entries
            .flatten()
            .filter_map(|entry| {
                let token = entry
                    .file_name()
                    .to_str()?
                    .strip_suffix(".json")
                    .filter(|t| token_ok(t))?
                    .to_owned();
                let bytes = read_private(&entry.path(), MAX_NOTICE).ok().flatten()?;
                Some((token, bytes))
            })
            .collect()
    }

    /// Remove the notice of the save `token`.
    pub(crate) fn remove(&self, token: &str, disk: &mut Disk) -> io::Result<()> {
        match self.file(token) {
            Some(file) => disk.remove_file(&file),
            None => Ok(()),
        }
    }

    /// Remove the notices older than [`MAX_AGE`], and the temp files a killed
    /// write left once older than [`TEMP_AGE`].
    fn prune(&self) {
        let Ok(entries) = std::fs::read_dir(&self.dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let temp = path.extension().is_some_and(|e| e == "tmp");
            let elapsed = path
                .symlink_metadata()
                .ok()
                .filter(|m| m.is_file())
                .and_then(|m| m.modified().ok())
                .and_then(|t| t.elapsed().ok());
            if elapsed.is_some_and(|e| e > if temp { TEMP_AGE } else { MAX_AGE }) {
                tracing::debug!(path = %path.display(), "pruning a save notice");
                std::fs::remove_file(&path).ok();
            }
        }
    }
}

/// The pid of the process that wrote the temp file at `path`, named
/// `.<record>.json.<pid>-<nanos>.tmp`.
fn temp_owner(path: &Path) -> Option<u32> {
    let name = path.file_name()?.to_str()?;
    let stem = name.strip_prefix('.')?.strip_suffix(".tmp")?;
    let (record, stamp) = stem.rsplit_once('.')?;
    record.strip_suffix(".json")?;
    let (pid, nanos) = stamp.split_once('-')?;
    nanos.bytes().all(|b| b.is_ascii_digit()).then_some(())?;
    pid.parse().ok()
}

/// Whether the process `pid` is running; `None` where that can't be told.
fn alive(pid: u32) -> Option<bool> {
    #[cfg(unix)]
    {
        let pid = rustix::process::Pid::from_raw(i32::try_from(pid).ok()?)?;
        match rustix::process::test_kill_process(pid) {
            Ok(()) => Some(true),
            Err(rustix::io::Errno::SRCH) => Some(false),
            Err(rustix::io::Errno::PERM) => Some(true),
            Err(_) => None,
        }
    }
    #[cfg(not(unix))]
    {
        let _ = pid;
        None
    }
}

fn warn_once(error: &str) {
    static WARNED: Once = Once::new();
    WARNED.call_once(|| {
        tracing::warn!(
            "{error}. A save still lands whole, but a save the designer is killed in the middle \
             of is only reported on the next start, not recovered"
        );
    });
}

/// `dir` made private: created with mode 0700 on Unix, refused when it is a
/// link, not a folder, or (on Unix) open to others or owned by another user.
fn private_dir(dir: &Path) -> io::Result<()> {
    let missing: Vec<PathBuf> = dir
        .ancestors()
        .take_while(|a| a.symlink_metadata().is_err())
        .map(Path::to_path_buf)
        .collect();
    let mut builder = std::fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    std::os::unix::fs::DirBuilderExt::mode(&mut builder, 0o700);
    builder.create(dir)?;
    // A record flushed into a folder whose own entry was never flushed is
    // lost with it on a power cut: flush each folder made into its parent.
    for made in missing.iter().rev() {
        if let Some(parent) = made.parent() {
            sync_dir(parent)?;
        }
    }
    let meta = dir.symlink_metadata()?;
    if !meta.is_dir() {
        return Err(io::Error::other("it is not a folder"));
    }
    private(&meta)
}

/// Refuse what others can read or write, or another user owns.
#[cfg(unix)]
fn private(meta: &std::fs::Metadata) -> io::Result<()> {
    use std::os::unix::fs::MetadataExt as _;
    if meta.uid() != rustix::process::geteuid().as_raw() {
        return Err(io::Error::other("it belongs to another user"));
    }
    if meta.mode() & 0o077 != 0 {
        return Err(io::Error::other(format!(
            "others may read or write it (mode {:o}); it must be private to you",
            meta.mode() & 0o777
        )));
    }
    Ok(())
}

#[cfg(not(unix))]
fn private(_meta: &std::fs::Metadata) -> io::Result<()> {
    Ok(())
}

/// The bytes of the private regular file at `path`, never following a link,
/// never blocking on the open, never reading more than `cap` bytes
/// (`Ok(None)` when larger).
fn read_private(path: &Path, cap: u64) -> io::Result<Option<Vec<u8>>> {
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::custom_flags(
        &mut options,
        libc::O_NOFOLLOW | libc::O_NONBLOCK,
    );
    not_a_link(path)?;
    let file = options.open(path)?;
    let meta = file.metadata()?;
    if !meta.is_file() {
        return Err(io::Error::other("it is not a regular file"));
    }
    private(&meta)?;
    if meta.len() > cap {
        return Ok(None);
    }
    read_at_most(file, cap)
}

/// Where the records are kept, when not in the platform's state folder.
static OVERRIDE: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();

/// Keep the records in `dir` instead of the platform's state folder, for
/// this whole process. The first call wins.
pub(crate) fn use_dir(dir: PathBuf) {
    OVERRIDE.set(dir).ok();
}

#[cfg(test)]
thread_local! {
    static TEST_DIR: std::cell::RefCell<Option<PathBuf>> = const { std::cell::RefCell::new(None) };
}

/// Keep this thread's records in `dir` (`None`: no folder), for a test.
#[cfg(test)]
pub(crate) fn use_test_dir(dir: Option<PathBuf>) {
    TEST_DIR.with(|d| *d.borrow_mut() = dir);
}

/// The records folder (see the module docs); `None` with no home folder. A
/// unit test sees only the folder its thread set.
pub(crate) fn dir() -> Option<PathBuf> {
    #[cfg(test)]
    {
        TEST_DIR.with(|d| d.borrow().clone())
    }
    #[cfg(not(test))]
    {
        if let Some(dir) = OVERRIDE.get() {
            return Some(dir.clone());
        }
        platform_dir().map(|d| d.join(RECORDS))
    }
}

#[cfg(not(test))]
fn platform_dir() -> Option<PathBuf> {
    use etcetera::app_strategy::{AppStrategy as _, AppStrategyArgs, choose_native_strategy};
    let strategy = choose_native_strategy(AppStrategyArgs {
        top_level_domain: "io.github".to_owned(),
        author: "alias2k".to_owned(),
        app_name: "flusso".to_owned(),
    })
    .ok()?;
    Some(strategy.state_dir().unwrap_or_else(|| strategy.data_dir()))
}
