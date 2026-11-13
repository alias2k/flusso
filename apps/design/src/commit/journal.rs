//! The journal: every step a save will take, written before the first one.
//!
//! It lives in the config's folder as `.flusso-save.journal`, and is renamed
//! to `.flusso-save.done` once the save is committed. The file is JSON:
//!
//! ```text
//! {
//!   "format": 2,
//!   "started_at_ms": 1763030000000,
//!   "pid": 4242,
//!   "token": "4242-1763030000000000000",
//!   "dirs": ["new"],
//!   "writes": [{
//!     "dest": "new/users.schema.yml",
//!     "before": "<sha-256 of the file before>" | null,
//!     "after": "<sha-256 of the new text>"
//!   }],
//!   "removals": [{ "path": "old.schema.yml", "before": "<sha-256>" }]
//! }
//! ```
//!
//! **A journal is input from disk**: it may be stale, copied with its
//! project, corrupt or planted. [`read`] reads at most [`MAX_JOURNAL`] bytes;
//! recovery trusts them only when the user's record of a save in flight
//! names their digest and this folder ([`super::state`]), and parses nothing
//! before that. Then [`Journal::steps`] checks it is one a save could have
//! written, and resolves it against the folder being opened:
//!
//! - every path is relative to the config's folder, `/`-separated, made of
//!   plain names; a file outside the folder (which a save may write, with a
//!   warning) starts with `..` and never comes back into it;
//! - no name is a save file of its own (`.flusso-save…`, or a temp or backup);
//! - no path leads through a link, none is named twice, the config file is
//!   only ever the last write, and a removal is inside the folder;
//! - every folder the save created holds one of its files;
//! - at most [`MAX_ENTRIES`] paths of at most [`MAX_PATH`] bytes.
//!
//! The temp file and backup of each file are not written down: they are
//! named from its path, the journal's `token` and its position
//! (`.<name>.flusso-save.<token>.<i>.tmp|bak` beside the file, `r<i>` for a
//! removal).
//!
//! The digests are what lets recovery tell the text it put in a file from
//! one someone typed there since.

use std::io::{self, Read as _};
use std::path::{Component, Path, PathBuf};

use anyhow::Result;
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};

use super::disk::not_a_link;
use crate::fspath::{MAX_PATH, on_disk};

/// The journal of a save that can still be undone.
pub(crate) const JOURNAL: &str = ".flusso-save.journal";
/// The journal of a save whose files are all in place: only cleanup is left.
pub(crate) const DONE: &str = ".flusso-save.done";
/// The journal while it is written; it means nothing was done yet.
pub(crate) const STAGING: &str = ".flusso-save.journal.tmp";
/// The journal format this designer writes and reads.
const FORMAT: u32 = 2;
/// The largest journal read: far beyond any real save's.
pub(crate) const MAX_JOURNAL: u64 = 1024 * 1024;
/// The most paths one journal names.
pub(crate) const MAX_ENTRIES: usize = 4096;

/// What a save does, as written to disk: paths relative to the config's
/// folder.
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Journal {
    format: u32,
    /// When the save started, in milliseconds since the Unix epoch.
    pub(crate) started_at_ms: u64,
    pid: u32,
    /// What names the save's temp files and backups.
    pub(crate) token: String,
    /// The folders it creates, outermost first.
    dirs: Vec<String>,
    /// The files it writes, in order (`flusso.toml` last).
    writes: Vec<FileEntry>,
    /// The files it removes.
    removals: Vec<RemovalEntry>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct FileEntry {
    dest: String,
    /// `None` for a file the save creates.
    before: Option<Digest>,
    after: Digest,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RemovalEntry {
    path: String,
    before: Digest,
}

/// A journal resolved against the project's folder: every path absolute,
/// every temp file and backup named.
#[derive(Debug)]
pub(crate) struct Steps {
    pub(crate) started_at_ms: u64,
    pub(crate) token: String,
    /// The folders the save creates, outermost first.
    pub(crate) dirs: Vec<PathBuf>,
    pub(crate) writes: Vec<FileWrite>,
    pub(crate) removals: Vec<Removal>,
}

/// One file written: its text goes to `temp`, the file is kept at `backup`,
/// then `temp` is renamed over it.
#[derive(Debug)]
pub(crate) struct FileWrite {
    pub(crate) dest: PathBuf,
    pub(crate) temp: PathBuf,
    /// `None` for a file the save creates.
    pub(crate) backup: Option<PathBuf>,
    /// `None` for a file the save creates.
    pub(crate) before: Option<Digest>,
    pub(crate) after: Digest,
}

/// One file removed, by renaming it to `backup`.
#[derive(Debug)]
pub(crate) struct Removal {
    pub(crate) path: PathBuf,
    pub(crate) backup: PathBuf,
    pub(crate) before: Digest,
}

/// The SHA-256 of a file's bytes, in hex.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub(crate) struct Digest(String);

impl Digest {
    pub(crate) fn of(bytes: &[u8]) -> Self {
        Self(hex(&Sha256::digest(bytes)))
    }

    /// The digest of the file at `path`, `None` when there is none.
    pub(crate) fn at(path: &Path) -> io::Result<Option<Self>> {
        match std::fs::read(path) {
            Ok(bytes) => Ok(Some(Self::of(&bytes))),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e),
        }
    }

    /// The digest in hex.
    pub(crate) fn hex(&self) -> &str {
        &self.0
    }

    /// Whether this is a digest a save writes: 64 lowercase hex digits.
    fn well_formed(&self) -> bool {
        self.0.len() == 64
            && self
                .0
                .bytes()
                .all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
    }
}

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        let _ = write!(out, "{byte:02x}");
    }
    out
}

/// Why a journal is not acted on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Unusable {
    /// This user's designer has no record of it: written by another user, on
    /// another machine or by hand, in a project copied or moved since, by a
    /// save that could not be recorded (the state folder was unusable) or
    /// whose record was pruned, or a journal of a save already settled.
    NoRecord,
    /// This user's designer wrote it, for another folder: the project was
    /// copied or moved, or the journal was.
    OtherFolder,
    /// The user's records folder can't be used, so it can't be checked.
    NoState,
    /// It is larger than any save writes.
    TooLarge,
    /// It can't be read (a link, a folder, no permission).
    Unreadable,
    /// It is recorded, but not a journal this designer reads.
    Malformed,
    /// It is recorded, but names what no save of this folder writes.
    NotASave,
    /// Both `.flusso-save.journal` and `.flusso-save.done` are there: no save
    /// leaves both.
    Conflict,
}

/// A journal that is not acted on: why, in words, and its body as far as it
/// could be read, for the report.
#[derive(Debug)]
pub(crate) struct Refused {
    pub(crate) cause: Unusable,
    pub(crate) reason: String,
    /// The journal's body parsed without trusting it: only ever described.
    pub(crate) unverified: Option<Box<Journal>>,
}

impl Refused {
    pub(crate) fn new(cause: Unusable, reason: impl Into<String>) -> Self {
        Self {
            cause,
            reason: reason.into(),
            unverified: None,
        }
    }

    /// With the body of `bytes` parsed untrusted, for the report.
    pub(crate) fn describing(mut self, bytes: &[u8]) -> Self {
        self.unverified = serde_json::from_slice(bytes).ok().map(Box::new);
        self
    }
}

/// The bytes of the journal at `path`, or why they can't be read.
pub(crate) fn read(path: &Path) -> Result<Vec<u8>, Refused> {
    match read_capped(path, MAX_JOURNAL) {
        Ok(Some(bytes)) => Ok(bytes),
        Ok(None) => Err(Refused::new(
            Unusable::TooLarge,
            format!("it is larger than {MAX_JOURNAL} bytes, more than any save writes"),
        )),
        Err(e) => Err(Refused::new(Unusable::Unreadable, e.to_string())),
    }
}

/// The bytes of the regular file at `path`, never following a link at its
/// end and never reading more than `cap` bytes; `Ok(None)` when larger.
pub(crate) fn read_capped(path: &Path, cap: u64) -> io::Result<Option<Vec<u8>>> {
    let mut options = std::fs::OpenOptions::new();
    options.read(true);
    // A FIFO planted at the path must not block the open.
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
    if meta.len() > cap {
        return Ok(None);
    }
    read_at_most(file, cap)
}

/// At most `cap` bytes of `reader`; `Ok(None)` when it holds more. Reads
/// `cap + 1` bytes at the most.
pub(crate) fn read_at_most(reader: impl io::Read, cap: u64) -> io::Result<Option<Vec<u8>>> {
    let mut bytes = Vec::new();
    reader.take(cap.saturating_add(1)).read_to_end(&mut bytes)?;
    Ok((bytes.len() as u64 <= cap).then_some(bytes))
}

impl Journal {
    /// Plan a save of `writes` and `removals` in the project at `base`
    /// (resolved): name every temp file and backup, and take the digest of
    /// every file it touches.
    pub(crate) fn plan(
        base: &Path,
        writes: &[super::Write],
        removals: &[PathBuf],
    ) -> Result<(Self, Steps)> {
        let started = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default();
        let token = format!("{}-{}", std::process::id(), started.as_nanos());
        let started_at_ms = u64::try_from(started.as_millis()).unwrap_or(u64::MAX);

        let mut dirs: Vec<PathBuf> = Vec::new();
        let mut planned = Vec::with_capacity(writes.len());
        for (i, write) in writes.iter().enumerate() {
            if let Some(parent) = write.dest.parent() {
                let missing: Vec<&Path> = parent.ancestors().take_while(|a| !a.exists()).collect();
                for dir in missing.into_iter().rev() {
                    if !dirs.iter().any(|d| d == dir) {
                        dirs.push(dir.to_path_buf());
                    }
                }
            }
            let before = Digest::at(&write.dest)?;
            planned.push(FileWrite {
                temp: temp(&write.dest, &token, i),
                backup: before.is_some().then(|| backup(&write.dest, &token, i)),
                dest: write.dest.clone(),
                before,
                after: Digest::of(write.text.as_bytes()),
            });
        }
        let mut planned_removals = Vec::with_capacity(removals.len());
        for (i, path) in removals.iter().enumerate() {
            planned_removals.push(Removal {
                backup: removal_backup(path, &token, i),
                path: path.clone(),
                before: Digest::at(path)?
                    .ok_or_else(|| anyhow::anyhow!("{} does not exist", path.display()))?,
            });
        }

        let journal = Self {
            format: FORMAT,
            started_at_ms,
            pid: std::process::id(),
            token: token.clone(),
            dirs: dirs.iter().map(|d| relative(base, d)).collect(),
            writes: planned
                .iter()
                .map(|w| FileEntry {
                    dest: relative(base, &w.dest),
                    before: w.before.clone(),
                    after: w.after.clone(),
                })
                .collect(),
            removals: planned_removals
                .iter()
                .map(|r| RemovalEntry {
                    path: relative(base, &r.path),
                    before: r.before.clone(),
                })
                .collect(),
        };
        let steps = Steps {
            started_at_ms,
            token,
            dirs,
            writes: planned,
            removals: planned_removals,
        };
        Ok((journal, steps))
    }

    /// The journal as written to disk.
    pub(crate) fn encode(&self) -> Result<Vec<u8>> {
        Ok(serde_json::to_vec_pretty(self)?)
    }

    /// The journal in `bytes`, already known to be one this user's designer
    /// wrote; or why it is not one this designer reads.
    pub(crate) fn parse(bytes: &[u8]) -> Result<Self, Refused> {
        let journal: Self = serde_json::from_slice(bytes).map_err(|e| {
            Refused::new(
                Unusable::Malformed,
                format!("it is incomplete or damaged ({e})"),
            )
        })?;
        if journal.format != FORMAT {
            return Err(Refused::new(
                Unusable::Malformed,
                format!("its format {} is not known", journal.format),
            ));
        }
        Ok(journal)
    }

    /// The journal resolved against `base`, the project's folder as the
    /// filesystem resolves it, with `config` the config file in it; or why
    /// no save of that project could have written it (see the module docs).
    pub(crate) fn steps(&self, base: &Path, config: &Path) -> Result<Steps, String> {
        if !token_ok(&self.token) {
            return Err(format!(
                "its token `{}` is not one a save makes",
                self.token
            ));
        }
        if self.writes.len() + self.removals.len() + self.dirs.len() > MAX_ENTRIES {
            return Err(format!("it names more than {MAX_ENTRIES} paths"));
        }
        if let Some(bad) = self
            .writes
            .iter()
            .flat_map(|w| w.before.iter().chain([&w.after]))
            .chain(self.removals.iter().map(|r| &r.before))
            .find(|d| !d.well_formed())
        {
            return Err(format!("its digest `{}` is not a SHA-256", bad.0));
        }

        let mut seen: Vec<PathBuf> = Vec::new();
        let mut file = |rel: &str, outside: bool| -> Result<PathBuf, String> {
            let path = resolved(base, rel, outside)?;
            if seen.contains(&path) {
                return Err(format!("it names {rel} twice"));
            }
            seen.push(path.clone());
            Ok(path)
        };
        let mut writes = Vec::with_capacity(self.writes.len());
        for (i, write) in self.writes.iter().enumerate() {
            let dest = file(&write.dest, true)?;
            if dest == config && i + 1 != self.writes.len() {
                return Err(format!(
                    "it writes {} before other files, where a save writes it last",
                    write.dest
                ));
            }
            writes.push(FileWrite {
                temp: temp(&dest, &self.token, i),
                backup: write
                    .before
                    .is_some()
                    .then(|| backup(&dest, &self.token, i)),
                dest,
                before: write.before.clone(),
                after: write.after.clone(),
            });
        }
        let mut removals = Vec::with_capacity(self.removals.len());
        for (i, removal) in self.removals.iter().enumerate() {
            let path = file(&removal.path, false)?;
            if path == config {
                return Err(format!(
                    "it removes {}, which a save never does",
                    removal.path
                ));
            }
            removals.push(Removal {
                backup: removal_backup(&path, &self.token, i),
                path,
                before: removal.before.clone(),
            });
        }
        let mut dirs: Vec<PathBuf> = Vec::with_capacity(self.dirs.len());
        for rel in &self.dirs {
            let dir = resolved(base, rel, true)?;
            if dirs.contains(&dir) || !writes.iter().any(|w| w.dest.starts_with(&dir)) {
                return Err(format!(
                    "it creates the folder {rel}, which holds none of its files"
                ));
            }
            dirs.push(dir);
        }
        Ok(Steps {
            started_at_ms: self.started_at_ms,
            token: self.token.clone(),
            dirs,
            writes,
            removals,
        })
    }

    /// Every file the journal names, each with its temp file and backup
    /// where its path is one a save could write; else the path as text.
    /// Never trusted: only ever described.
    pub(crate) fn described(&self, base: &Path) -> Vec<Described> {
        let token = token_ok(&self.token).then_some(self.token.as_str());
        let write = self.writes.iter().enumerate().map(|(i, w)| {
            let path = lexical(base, &w.dest, true).ok();
            let outside = path.as_ref().is_some_and(|p| !p.starts_with(base));
            let path = path.filter(|p| reportable(base, p, true));
            Described {
                outside,
                text: w.dest.clone(),
                temp: path.as_ref().zip(token).map(|(p, t)| temp(p, t, i)),
                backup: path
                    .as_ref()
                    .zip(token)
                    .filter(|_| w.before.is_some())
                    .map(|(p, t)| backup(p, t, i)),
                path,
                before: w.before.clone(),
                after: Some(w.after.clone()),
            }
        });
        let removal = self.removals.iter().enumerate().map(|(i, r)| {
            let path = lexical(base, &r.path, false)
                .ok()
                .filter(|p| reportable(base, p, false));
            Described {
                outside: false,
                text: r.path.clone(),
                temp: None,
                backup: path
                    .as_ref()
                    .zip(token)
                    .map(|(p, t)| removal_backup(p, t, i)),
                path,
                before: Some(r.before.clone()),
                after: None,
            }
        });
        write.chain(removal).collect()
    }

    /// The folders the journal says the save created, outermost first, where
    /// each is one a save could create. Never trusted: only ever described.
    pub(crate) fn described_dirs(&self, base: &Path) -> Vec<PathBuf> {
        self.dirs
            .iter()
            .filter_map(|rel| lexical(base, rel, true).ok())
            .filter(|dir| inside_and_plain(base, dir))
            .collect()
    }
}

/// Whether a report may give steps for `path`: a schema file (`.yml`,
/// `.yaml`) in the project's folder under no hidden folder, or, for a write,
/// a `.toml` file directly in it (the config). A report's steps are carried
/// out by hand from a journal nobody vouches for, so they never name a
/// dotfile, a file of another kind, or the removal of a config.
fn reportable(base: &Path, path: &Path, write: bool) -> bool {
    if !inside_and_plain(base, path) {
        return false;
    }
    match path.extension().and_then(|e| e.to_str()) {
        Some("yml" | "yaml") => true,
        Some("toml") => write && path.parent() == Some(base),
        _ => false,
    }
}

/// `path` lies in `base` and no part of it below `base` is hidden.
fn inside_and_plain(base: &Path, path: &Path) -> bool {
    path.strip_prefix(base).is_ok_and(|rest| {
        rest.components()
            .all(|c| !c.as_os_str().to_string_lossy().starts_with('.'))
    })
}

/// One file an untrusted journal names, for the report.
#[derive(Debug)]
pub(crate) struct Described {
    /// The path as the journal writes it.
    pub(crate) text: String,
    /// The path, when it is one a save could write.
    pub(crate) path: Option<PathBuf>,
    /// The path is one a save writes outside the config's folder.
    pub(crate) outside: bool,
    pub(crate) temp: Option<PathBuf>,
    pub(crate) backup: Option<PathBuf>,
    /// What it held before the save (`None`: it did not exist).
    pub(crate) before: Option<Digest>,
    /// What the save leaves in it (`None`: it is removed).
    pub(crate) after: Option<Digest>,
}

/// Whether `token` is one a save makes: `<pid>-<nanoseconds>`.
pub(crate) fn token_ok(token: &str) -> bool {
    let parts: Vec<&str> = token.split('-').collect();
    parts.len() == 2
        && parts
            .iter()
            .all(|p| !p.is_empty() && p.len() <= 40 && p.bytes().all(|b| b.is_ascii_digit()))
}

/// `rel`, a journal's `/`-separated path, joined to `base` without looking
/// at the disk, or why a save could not have written it. Each part is a
/// plain name that is no save file; only `outside` paths may start with
/// `..`, and those never lead back into `base`. Every path a save records
/// passes (see [`relative`]).
pub(crate) fn lexical(base: &Path, rel: &str, outside: bool) -> Result<PathBuf, String> {
    let refused = |why: &str| format!("it names {}, {why}", shown(rel));
    if rel.len() > MAX_PATH {
        return Err(refused("which is longer than any path a save writes"));
    }
    if rel.contains('\0') {
        return Err(refused("which holds a NUL"));
    }
    let plain = |name: &std::ffi::OsStr| -> Result<(), String> {
        let name = name.to_str().unwrap_or("");
        if name.starts_with(".flusso-save") || is_save_file(name) {
            return Err(refused("a file of the save's own"));
        }
        Ok(())
    };
    let given = Path::new(rel);
    if given.is_absolute() {
        // A save writes a path as absolute only when it shares no root with
        // the project (another drive).
        if !outside || given.components().next() == base.components().next() {
            return Err(refused("which is not a path inside the project's folder"));
        }
        let mut names = 0;
        for component in given.components() {
            match component {
                Component::Prefix(_) | Component::RootDir => {}
                Component::Normal(name) => {
                    plain(name)?;
                    names += 1;
                }
                _ => return Err(refused("which is not a path a save writes")),
            }
        }
        if names == 0 {
            return Err(refused("which names no file"));
        }
        return Ok(given.to_path_buf());
    }
    let mut path = base.to_path_buf();
    let mut ups = 0;
    let mut names = 0;
    for part in rel.split('/') {
        if part == ".." && names == 0 && outside {
            if !path.pop() {
                return Err(refused("which goes above the filesystem's root"));
            }
            ups += 1;
            continue;
        }
        let mut components = Path::new(part).components();
        match (components.next(), components.next()) {
            (Some(Component::Normal(name)), None) if name == part => plain(name)?,
            _ => return Err(refused("which is not a path a save writes")),
        }
        path.push(part);
        names += 1;
    }
    if names == 0 {
        return Err(refused("which names no file"));
    }
    if ups > 0 && path.starts_with(base) {
        return Err(refused("which is not a path a save writes"));
    }
    Ok(path)
}

/// [`lexical`], then checked on disk: refused when a link on the way, or
/// at the end, leads elsewhere.
fn resolved(base: &Path, rel: &str, outside: bool) -> Result<PathBuf, String> {
    let path = lexical(base, rel, outside)?;
    match on_disk(&path) {
        Ok(real) if real == path => Ok(path),
        Ok(_) => Err(format!(
            "it names {}, which leads through a link out of the path it names",
            shown(rel)
        )),
        Err(e) => Err(format!(
            "it names {}, which cannot be resolved ({e:#})",
            shown(rel)
        )),
    }
}

/// `rel` cut short for a message.
fn shown(rel: &str) -> String {
    const MAX: usize = 120;
    if rel.len() <= MAX {
        return rel.escape_debug().to_string();
    }
    let cut = (0..=MAX)
        .rev()
        .find(|&i| rel.is_char_boundary(i))
        .unwrap_or(0);
    format!("{}…", rel.get(..cut).unwrap_or("").escape_debug())
}

impl Steps {
    /// Every file the save changes, in order.
    pub(crate) fn files(&self) -> Vec<PathBuf> {
        self.writes
            .iter()
            .map(|w| w.dest.clone())
            .chain(self.removals.iter().map(|r| r.path.clone()))
            .collect()
    }

    /// Every temp file and backup of the save that is on disk.
    pub(crate) fn leftovers(&self) -> Vec<PathBuf> {
        self.writes
            .iter()
            .flat_map(|w| [Some(&w.temp), w.backup.as_ref()])
            .flatten()
            .chain(self.removals.iter().map(|r| &r.backup))
            .filter(|p| p.symlink_metadata().is_ok())
            .cloned()
            .collect()
    }

    /// Every folder whose entries the save changes.
    pub(crate) fn folders(&self) -> Vec<PathBuf> {
        let mut folders: Vec<PathBuf> = Vec::new();
        let parents = self
            .writes
            .iter()
            .map(|w| &w.dest)
            .chain(self.removals.iter().map(|r| &r.path))
            .chain(&self.dirs)
            .filter_map(|p| p.parent());
        for parent in parents {
            if !folders.iter().any(|f| f == parent) {
                folders.push(parent.to_path_buf());
            }
        }
        folders
    }
}

/// `path` relative to `base`, `/`-separated: with `..` when it lies outside,
/// as given when the two share no root.
fn relative(base: &Path, path: &Path) -> String {
    let base_parts: Vec<Component<'_>> = base.components().collect();
    let parts: Vec<Component<'_>> = path.components().collect();
    let common = base_parts
        .iter()
        .zip(&parts)
        .take_while(|(a, b)| a == b)
        .count();
    if common == 0 {
        return path.display().to_string();
    }
    let up = base_parts.len().saturating_sub(common);
    std::iter::repeat_n("..".to_owned(), up)
        .chain(
            parts
                .iter()
                .skip(common)
                .map(|c| c.as_os_str().to_string_lossy().into_owned()),
        )
        .collect::<Vec<_>>()
        .join("/")
}

fn temp(dest: &Path, token: &str, i: usize) -> PathBuf {
    sibling(dest, &format!("{token}.{i}.tmp"))
}

fn backup(dest: &Path, token: &str, i: usize) -> PathBuf {
    sibling(dest, &format!("{token}.{i}.bak"))
}

fn removal_backup(path: &Path, token: &str, i: usize) -> PathBuf {
    sibling(path, &format!("{token}.r{i}.bak"))
}

/// A hidden file beside `path`: `.<name>.flusso-save.<suffix>`.
fn sibling(path: &Path, suffix: &str) -> PathBuf {
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("out");
    let dir = path.parent().unwrap_or(Path::new("."));
    dir.join(format!(".{name}.flusso-save.{suffix}"))
}

/// Whether `name` is a temp file or backup some save left: `.<name>.flusso-save.<…>.tmp|bak`.
pub(crate) fn is_save_file(name: &str) -> bool {
    name.starts_with('.')
        && name.contains(".flusso-save.")
        && (name.ends_with(".tmp") || name.ends_with(".bak"))
}

#[cfg(test)]
mod report_tests {
    use super::*;

    #[test]
    fn a_report_gives_steps_only_for_schema_and_config_files() {
        let base = Path::new("/p");
        let yes = |rel: &str, write: bool| reportable(base, &base.join(rel), write);
        assert!(yes("users.schema.yml", true) && yes("users.schema.yml", false));
        assert!(yes("schemas/orders.yaml", true));
        assert!(yes("flusso.toml", true));
        assert!(!yes("flusso.toml", false), "a config is never removed");
        assert!(!yes("sub/other.toml", true));
        for rel in [
            ".envrc",
            ".git/HEAD",
            ".cargo/config.toml",
            "README.md",
            "a/.b/c.yml",
        ] {
            assert!(!yes(rel, true) && !yes(rel, false), "{rel}");
        }
        assert!(!reportable(base, Path::new("/q/users.schema.yml"), true));
    }
}
