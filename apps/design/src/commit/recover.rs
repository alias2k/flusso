//! Settling a save a process left half done: undo it, or finish it.
//!
//! Run under the project's lock, so any journal found belongs to a dead
//! process. **Recovery acts only on a journal this user's designer has a
//! record of** ([`super::state`]): a record of a save in flight whose digest
//! is the journal's exact bytes and whose folder is the one being opened.
//! The record, not the journal's file name, says how far the save got:
//!
//! - not committed: recovery puts every file back and removes what the save
//!   created (a rollback);
//! - committed: every file was in place; recovery checks that each still
//!   holds the text the save wrote, then deletes the backups (it finishes).
//!
//! Either way it then deletes the journal and removes the record, so a
//! journal is acted on once. `.flusso-save.journal.tmp` with a record of
//! this folder and no journal means nothing was done yet: it is deleted. A
//! record of this folder with no journal left at all is removed.
//!
//! Any other journal (no record, a record of another folder, too large,
//! unreadable, damaged, naming what no save writes, or `.journal` and
//! `.done` both there) changes nothing: recovery blocks with a [`Report`].
//! The report looks only inside the config's folder ([`Inside`]): a path
//! outside it is shown as text and never opened, statted or listed. It
//! gives the steps by hand ([`HandStep`]) that take the project back to
//! before the save, or forward to after it: each file, then every temp file
//! and backup the journal implies, then the folders it created, then the
//! journal.
//!
//! **What a blocked page tells is checked against the disk when it is
//! reported.** A blocked recorded save carries its [`Direction`], and per
//! file the one step that gives it a text recovery accepts, from a copy that
//! holds that text by digest right now (a hard-linked backup edited in place
//! holds it no longer). Where every problem is a file that changed, it also
//! carries the way to keep those files as they are: rolling back, the
//! other files put back by hand; finishing, nothing but the leftovers, since
//! every other file already holds what the save wrote.
//!
//! Acting on a recorded journal, every step is gated on what the disk holds:
//!
//! - a temp file is deleted when it holds the save's text for it; an empty
//!   one (a write cut short before its first byte) only while rolling back,
//!   and only when its file was not replaced yet;
//! - a backup is deleted, or put back, only when it holds the text from
//!   before the save;
//! - a file is replaced only when it holds the text from before or after the
//!   save, and removed only when it holds the text the save created it with;
//! - a folder the journal says the save created is removed only when empty.
//!
//! Anything else is left as it is, and listed: a temp file or backup that
//! holds other bytes stays as a leftover the designer reports, and a file
//! that holds neither text stops the rollback, or the finish, before it
//! changes anything. Each step looks at the disk before it acts, so running
//! recovery again after it was itself cut short carries on from where it
//! stopped.

use std::io;
use std::path::{Component, Path, PathBuf};

use super::disk::Disk;
use super::journal::{
    self, DONE, Described, Digest, FileWrite, JOURNAL, Journal, Refused, Removal, STAGING, Steps,
    Unusable, is_save_file, read_capped,
};
use super::lock::ProjectLock;
use super::state::State;

/// What recovery found and did.
#[derive(Debug, Default)]
pub(crate) struct Recovery {
    /// Saves undone or finished.
    pub(crate) settled: Vec<Settled>,
    /// A save that could not be settled; its files are left as they are.
    pub(crate) blocked: Option<Blocked>,
}

/// A save recovery settled.
#[derive(Debug)]
pub(crate) struct Settled {
    /// Every folder the save wrote in: where a leftover it kept can be.
    pub(crate) folders: Vec<PathBuf>,
    pub(crate) outcome: Outcome,
    pub(crate) started_at_ms: u64,
    /// The save's token, which tells one save from another.
    pub(crate) token: String,
    /// The files the save changed, each now as it was before the save
    /// (rolled back) or after it (completed).
    pub(crate) files: Vec<PathBuf>,
}

/// How a save was settled.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Outcome {
    RolledBack,
    Completed,
}

/// A save recovery could not settle.
#[derive(Debug)]
pub(crate) struct Blocked {
    pub(crate) problems: Vec<Problem>,
    /// The journal, temp files and backups on disk.
    pub(crate) leftovers: Vec<PathBuf>,
    /// For a journal recovery does not act on: what each file it names
    /// holds now, and the way out by hand.
    pub(crate) report: Option<Box<Report>>,
    /// For a recorded save: which way recovery was taking it.
    pub(crate) direction: Option<Direction>,
    /// For a recorded save whose every problem is a file that changed: the
    /// steps by hand that keep those files as they are and settle the rest,
    /// the journal last.
    pub(crate) keep: Option<Vec<HandStep>>,
}

/// Which way recovery takes a recorded save.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Direction {
    /// Not committed: every file goes back to before the save.
    RollBack,
    /// Committed: every file holds what the save wrote; only cleanup is left.
    Finish,
}

/// One step by hand, as the blocked page lists it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum HandStep {
    /// Copy `from` to `to`, replacing what is there.
    Copy { from: PathBuf, to: PathBuf },
    /// Delete the file.
    Delete(PathBuf),
    /// Remove the folder, when it is empty.
    RemoveDir(PathBuf),
}

/// What a journal recovery will not act on says, checked read-only against
/// the disk inside the config's folder.
#[derive(Debug)]
pub(crate) struct Report {
    /// Why the journal is not acted on.
    pub(crate) cause: Unusable,
    /// The journal files found: every one of them is to be deleted.
    pub(crate) journals: Vec<PathBuf>,
    /// For a journal recorded for another folder: that folder.
    pub(crate) recorded_for: Option<PathBuf>,
    pub(crate) files: Vec<ReportedFile>,
    /// How many more files it names, not checked.
    pub(crate) more: usize,
    /// Every temp file and backup the journal implies that is a file inside
    /// the folder now, whatever it holds.
    pub(crate) leftovers: Vec<PathBuf>,
    /// The folders it says the save created that are folders inside the
    /// folder now, outermost first.
    pub(crate) dirs: Vec<PathBuf>,
}

impl Report {
    /// Whether every file checked can be taken `back` to before the save
    /// (else forward to after it) by hand.
    pub(crate) fn possible(&self, back: bool) -> bool {
        self.more == 0
            && self.files.iter().any(|f| !f.outside)
            && self
                .files
                .iter()
                .filter(|f| !f.outside)
                .all(|f| if back { &f.back } else { &f.forward } != &Fix::Unavailable)
    }

    /// Every step by hand that takes the project `back` to before the save
    /// (else forward to after it), the journal last; `None` when not every
    /// file can be taken there.
    pub(crate) fn route(&self, back: bool) -> Option<Vec<HandStep>> {
        if !self.possible(back) {
            return None;
        }
        let mut steps = Vec::new();
        for file in self.files.iter().filter(|f| !f.outside) {
            let path = file.path.clone()?;
            match if back { &file.back } else { &file.forward } {
                Fix::Nothing => {}
                Fix::CopyFrom(from) => steps.push(HandStep::Copy {
                    from: from.clone(),
                    to: path,
                }),
                Fix::Delete => steps.push(HandStep::Delete(path)),
                Fix::Unavailable => return None,
            }
        }
        steps.extend(self.leftovers.iter().cloned().map(HandStep::Delete));
        if back {
            steps.extend(self.dirs.iter().rev().cloned().map(HandStep::RemoveDir));
        }
        steps.extend(self.journals.iter().cloned().map(HandStep::Delete));
        Some(steps)
    }
}

/// One file an unacted journal names.
#[derive(Debug)]
pub(crate) struct ReportedFile {
    /// The file, when inside the config's folder and a path a save writes
    /// (only then is it looked at).
    pub(crate) path: Option<PathBuf>,
    /// The path as the journal writes it.
    pub(crate) text: String,
    /// It is outside the config's folder: never looked at.
    pub(crate) outside: bool,
    pub(crate) holds: Holds,
    /// The save's backup of it, when it holds the text from before the save.
    pub(crate) backup: Option<PathBuf>,
    /// The save's temp file for it, when it holds the text the save wrote.
    pub(crate) temp: Option<PathBuf>,
    /// What to do to it to get back to before the save.
    pub(crate) back: Fix,
    /// What to do to it to get forward to after the save.
    pub(crate) forward: Fix,
}

/// What a file a journal names holds now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Holds {
    /// The text from before the save (or, for a file it creates, nothing).
    Before,
    /// The text the save wrote (or, for a file it removes, nothing).
    After,
    /// Something else.
    Neither,
    /// Not looked at: not a path a save writes, a link on the way, not a
    /// plain file, or too large.
    Unchecked,
    /// Outside the config's folder: never looked at.
    Outside,
}

/// One step by hand that gets a file to a given side of the save.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Fix {
    /// It is there already.
    Nothing,
    /// Copy this file over it.
    CopyFrom(PathBuf),
    /// Delete it.
    Delete,
    /// No file holds the text it needs, or it changed since.
    Unavailable,
}

/// The most files a report checks.
const REPORTED: usize = 50;
/// The largest file a report reads.
const LARGEST_CHECKED: u64 = 16 * 1024 * 1024;

/// One file that stops recovery.
#[derive(Debug)]
pub(crate) struct Problem {
    pub(crate) path: PathBuf,
    pub(crate) kind: ProblemKind,
    /// The system's message, or why a journal can't be used.
    pub(crate) detail: Option<String>,
    /// The backup the save keeps of the file, by name, whatever it holds.
    pub(crate) backup: Option<PathBuf>,
    /// The one step that gives the file a text recovery accepts, going its
    /// [`Direction`]: from a copy that holds it now, or deleting a file the
    /// save created (rolling back) or removed (finishing). `None`: no copy of
    /// that text is left.
    pub(crate) fix: Option<Box<HandStep>>,
    /// A file that holds the file's text from before the save now.
    pub(crate) before_copy: Option<PathBuf>,
}

/// Why a file stops recovery.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProblemKind {
    /// The backup holding the text before the save is gone or damaged.
    BackupMissing,
    /// The file holds neither the text before the save nor after it.
    Edited,
    /// The file could not be read.
    Unreadable,
    /// The journal is not one recovery acts on.
    JournalUnusable,
    /// A step failed.
    Failed,
}

impl Problem {
    pub(crate) fn new(path: &Path, kind: ProblemKind, detail: Option<String>) -> Self {
        Self {
            path: path.to_path_buf(),
            kind,
            detail,
            backup: None,
            fix: None,
            before_copy: None,
        }
    }
}

impl Blocked {
    pub(crate) fn new(problems: Vec<Problem>, leftovers: Vec<PathBuf>) -> Self {
        Self {
            problems,
            leftovers,
            report: None,
            direction: None,
            keep: None,
        }
    }

    fn going(mut self, direction: Direction) -> Self {
        self.direction = Some(direction);
        self
    }
}

/// Settle whatever save a dead process left in `lock`'s project, whose
/// config file is `config`, acting only on a journal `state` holds a record
/// of (`Err` says why there are no records to check).
pub(crate) fn recover(
    lock: &ProjectLock,
    config: &Path,
    state: Result<&State, &str>,
    disk: &mut Disk,
) -> Recovery {
    let base = lock.base();
    let mut recovery = Recovery::default();
    let journals: Vec<PathBuf> = [JOURNAL, DONE]
        .into_iter()
        .map(|name| base.join(name))
        .filter(|path| path.symlink_metadata().is_ok())
        .collect();
    let staging = base.join(STAGING);

    match journals.as_slice() {
        [] => {}
        [file] => {
            match settle(base, config, file, state, disk) {
                Ok(settled) => recovery.settled.push(settled),
                Err(blocked) => {
                    recovery.blocked = Some(blocked);
                    return recovery;
                }
            }
            if disk.stopped() {
                return recovery;
            }
        }
        _ => {
            let bytes = journal::read(&base.join(JOURNAL)).unwrap_or_default();
            recovery.blocked = Some(report(
                base,
                journals.clone(),
                None,
                Refused::new(
                    Unusable::Conflict,
                    format!("both {JOURNAL} and {DONE} are there, which no save leaves"),
                )
                .describing(&bytes),
            ));
            return recovery;
        }
    }

    match staging.symlink_metadata() {
        Ok(meta) if !meta.is_file() => {
            recovery.blocked = Some(report(
                base,
                vec![staging],
                None,
                Refused::new(Unusable::Unreadable, "it is not a regular file"),
            ));
            return recovery;
        }
        Ok(_) => {
            let pending: Vec<_> = state
                .ok()
                .map(|s| s.for_dir(base))
                .unwrap_or_default()
                .into_iter()
                .filter(|r| !r.committed)
                .collect();
            if pending.is_empty() {
                let refused = match state {
                    Ok(_) => Refused::new(
                        Unusable::NoRecord,
                        "this designer has no record of a save of this folder that wrote it",
                    ),
                    Err(why) => Refused::new(Unusable::NoState, why),
                };
                let bytes = journal::read(&staging).unwrap_or_default();
                recovery.blocked = Some(report(
                    base,
                    vec![staging],
                    None,
                    refused.describing(&bytes),
                ));
                return recovery;
            }
            if let Err(e) = disk.remove_file(&staging) {
                recovery.blocked = Some(
                    Blocked::new(
                        vec![Problem::new(
                            &staging,
                            ProblemKind::Failed,
                            Some(e.to_string()),
                        )],
                        vec![staging],
                    )
                    .going(Direction::RollBack),
                );
                return recovery;
            }
        }
        Err(_) => {}
    }
    if let Ok(state) = state {
        for record in state.for_dir(base) {
            if disk.stopped() || state.remove(&record.token, disk).is_err() {
                break;
            }
        }
    }
    recovery
}

/// Settle the journal `file`, when `state` holds its record for `base`.
fn settle(
    base: &Path,
    config: &Path,
    file: &Path,
    state: Result<&State, &str>,
    disk: &mut Disk,
) -> Result<Settled, Blocked> {
    let refuse = |refused: Refused, recorded_for: Option<PathBuf>| {
        report(base, vec![file.to_path_buf()], recorded_for, refused)
    };
    let bytes = journal::read(file).map_err(|r| refuse(r, None))?;
    let state = state.map_err(|why| {
        refuse(
            Refused::new(Unusable::NoState, why).describing(&bytes),
            None,
        )
    })?;
    let Some(record) = state.find(&Digest::of(&bytes)) else {
        return Err(refuse(
            Refused::new(
                Unusable::NoRecord,
                "this designer has no record of a save in flight that wrote it",
            )
            .describing(&bytes),
            None,
        ));
    };
    if record.dir != base {
        return Err(refuse(
            Refused::new(
                Unusable::OtherFolder,
                format!(
                    "this designer wrote it while saving {}, not this folder",
                    record.dir.display()
                ),
            )
            .describing(&bytes),
            Some(record.dir),
        ));
    }
    let journal = Journal::parse(&bytes).map_err(|r| refuse(r.describing(&bytes), None))?;
    let steps = match journal.steps(base, config) {
        Ok(steps) if steps.token == record.token => steps,
        Ok(_) => {
            return Err(refuse(
                Refused::new(Unusable::Malformed, "its token is not its record's")
                    .describing(&bytes),
                None,
            ));
        }
        Err(reason) => {
            return Err(refuse(
                Refused::new(Unusable::NotASave, reason).describing(&bytes),
                None,
            ));
        }
    };
    let outcome = if record.committed {
        finish(&steps, file, disk)?;
        Outcome::Completed
    } else {
        roll_back(&steps, file, disk)?;
        Outcome::RolledBack
    };
    if !disk.stopped()
        && let Err(e) = state.remove(&record.token, disk)
    {
        tracing::warn!(error = %e, "could not remove the record of a recovered save");
    }
    let mut folders = steps.folders();
    folders.extend(steps.dirs.iter().cloned());
    Ok(Settled {
        folders,
        outcome,
        started_at_ms: steps.started_at_ms,
        files: steps.files(),
        token: steps.token,
    })
}

/// Read-only looks at the files inside the config's folder: a path outside
/// it, or through a link on the way, is never opened, statted or listed.
#[derive(Debug)]
pub(crate) struct Inside<'a> {
    base: &'a Path,
}

/// What is at a path, as [`Inside::kind`] finds it.
#[derive(Debug, PartialEq, Eq)]
enum Kind {
    Missing,
    File,
    Dir,
    /// A link, anything but a file or folder, or an error.
    Other,
}

/// What [`Inside::look`] saw at a path.
#[derive(Debug)]
enum Seen {
    /// Nothing is there (or a folder on the way is missing).
    Missing,
    /// A regular file with this digest.
    File(Digest),
    /// Something it does not read: a link on the way, a folder, a file too
    /// large, an error.
    Other,
}

#[cfg(test)]
thread_local! {
    /// Every path a report looked at, for a test.
    pub(crate) static LOOKED_AT: std::cell::RefCell<Vec<PathBuf>> = const { std::cell::RefCell::new(Vec::new()) };
}

impl<'a> Inside<'a> {
    pub(crate) fn new(base: &'a Path) -> Self {
        Self { base }
    }

    /// Whether `path` is inside the folder, written as plain names.
    fn contains(&self, path: &Path) -> bool {
        path.strip_prefix(self.base).is_ok_and(|rel| {
            rel.components().next().is_some()
                && rel.components().all(|c| matches!(c, Component::Normal(_)))
        })
    }

    fn lstat(path: &Path) -> io::Result<std::fs::Metadata> {
        #[cfg(test)]
        LOOKED_AT.with(|l| l.borrow_mut().push(path.to_path_buf()));
        path.symlink_metadata()
    }

    /// What is at `path`, looking at each folder on the way from the base
    /// without following a link.
    fn kind(&self, path: &Path) -> Kind {
        if !self.contains(path) {
            return Kind::Other;
        }
        let Ok(rel) = path.strip_prefix(self.base) else {
            return Kind::Other;
        };
        let mut at = self.base.to_path_buf();
        let parts: Vec<_> = rel.components().collect();
        for (i, part) in parts.iter().enumerate() {
            at.push(part);
            let meta = match Self::lstat(&at) {
                Ok(meta) => meta,
                Err(e) if e.kind() == io::ErrorKind::NotFound => return Kind::Missing,
                Err(_) => return Kind::Other,
            };
            if i + 1 < parts.len() {
                if !meta.is_dir() {
                    return if meta.is_file() {
                        Kind::Missing
                    } else {
                        Kind::Other
                    };
                }
            } else if meta.is_file() {
                return Kind::File;
            } else if meta.is_dir() {
                return Kind::Dir;
            }
        }
        Kind::Other
    }

    /// What is at `path`, read when it is a file.
    fn look(&self, path: &Path) -> Seen {
        match self.kind(path) {
            Kind::Missing => return Seen::Missing,
            Kind::File => {}
            Kind::Dir | Kind::Other => return Seen::Other,
        }
        #[cfg(test)]
        LOOKED_AT.with(|l| l.borrow_mut().push(path.to_path_buf()));
        match read_capped(path, LARGEST_CHECKED) {
            Ok(Some(bytes)) => Seen::File(Digest::of(&bytes)),
            _ => Seen::Other,
        }
    }

    /// `path` when it is a regular file inside holding `digest`.
    fn holding(&self, path: Option<&PathBuf>, digest: Option<&Digest>) -> Option<PathBuf> {
        let (path, digest) = (path?, digest?);
        matches!(self.look(path), Seen::File(d) if d == *digest).then(|| path.clone())
    }
}

/// The blocked recovery of the journal `journals` that is not acted on:
/// what each file it names holds now and the way out, read-only and inside
/// the config's folder only.
fn report(
    base: &Path,
    journals: Vec<PathBuf>,
    recorded_for: Option<PathBuf>,
    refused: Refused,
) -> Blocked {
    let inside = Inside::new(base);
    let described = refused
        .unverified
        .as_ref()
        .map(|j| j.described(base))
        .unwrap_or_default();
    let implied: Vec<PathBuf> = described
        .iter()
        .filter(|d| d.path.as_ref().is_some_and(|p| inside.contains(p)))
        .flat_map(|d| [d.temp.clone(), d.backup.clone()])
        .flatten()
        .filter(|p| inside.kind(p) == Kind::File)
        .collect();
    let dirs: Vec<PathBuf> = refused
        .unverified
        .as_ref()
        .map(|j| j.described_dirs(base))
        .unwrap_or_default()
        .into_iter()
        .filter(|d| inside.kind(d) == Kind::Dir)
        .collect();
    let more = described.len().saturating_sub(REPORTED);
    let files: Vec<ReportedFile> = described
        .into_iter()
        .take(REPORTED)
        .map(|d| reported(&inside, d))
        .collect();
    let mut leftovers = journals.clone();
    leftovers.extend(implied.iter().cloned());
    let report = Report {
        cause: refused.cause,
        journals,
        recorded_for,
        files,
        more,
        leftovers: implied,
        dirs,
    };
    let first = report.journals.first().cloned().unwrap_or_default();
    Blocked {
        report: Some(Box::new(report)),
        ..Blocked::new(
            vec![Problem::new(
                &first,
                ProblemKind::JournalUnusable,
                Some(refused.reason),
            )],
            leftovers,
        )
    }
}

fn reported(inside: &Inside<'_>, described: Described) -> ReportedFile {
    let path = described.path.filter(|p| inside.contains(p));
    let outside = path.is_none() && described.outside;
    let seen = path.as_ref().map_or(Seen::Other, |p| inside.look(p));
    let (before, after) = (described.before.as_ref(), described.after.as_ref());
    let holds = match (&path, &seen) {
        _ if outside => Holds::Outside,
        (None, _) | (_, Seen::Other) => Holds::Unchecked,
        (_, Seen::Missing) => match (before, after) {
            (None, _) => Holds::Before,
            (_, None) => Holds::After,
            _ => Holds::Neither,
        },
        (_, Seen::File(d)) if Some(d) == before => Holds::Before,
        (_, Seen::File(d)) if Some(d) == after => Holds::After,
        _ => Holds::Neither,
    };
    let missing = matches!(seen, Seen::Missing);
    let backup = path
        .as_ref()
        .and_then(|_| inside.holding(described.backup.as_ref(), before));
    let temp = path
        .as_ref()
        .and_then(|_| inside.holding(described.temp.as_ref(), after));
    let copy = |from: &Option<PathBuf>| from.clone().map_or(Fix::Unavailable, Fix::CopyFrom);
    let (back, forward) = match holds {
        Holds::Before => (
            Fix::Nothing,
            if after.is_none() {
                Fix::Delete
            } else {
                copy(&temp)
            },
        ),
        Holds::After => (
            if before.is_none() {
                Fix::Delete
            } else {
                copy(&backup)
            },
            Fix::Nothing,
        ),
        Holds::Neither if missing => (copy(&backup), copy(&temp)),
        _ => (Fix::Unavailable, Fix::Unavailable),
    };
    ReportedFile {
        path,
        text: described.text,
        outside,
        holds,
        backup,
        temp,
        back,
        forward,
    }
}

/// Every temp file or backup a save left in `folders` (not recursing) that no
/// journal names: left by a designer from before journals, by hand, or by
/// a recovery that found it holding something other than the save's text.
pub(crate) fn strays(folders: &[PathBuf]) -> Vec<PathBuf> {
    let mut found = Vec::new();
    for folder in folders {
        let Ok(entries) = std::fs::read_dir(folder) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if entry.file_name().to_str().is_some_and(is_save_file)
                && path.is_file()
                && !found.contains(&path)
            {
                found.push(path);
            }
        }
    }
    found.sort();
    found
}

fn failed(
    steps: &Steps,
    journal: &Path,
    direction: Direction,
    at: &Path,
    error: &io::Error,
) -> Blocked {
    Blocked::new(
        vec![Problem::new(
            at,
            ProblemKind::Failed,
            Some(error.to_string()),
        )],
        on_disk(steps, journal),
    )
    .going(direction)
}

/// The save's temp files and backups on disk, and its journal.
fn on_disk(steps: &Steps, journal: &Path) -> Vec<PathBuf> {
    let mut leftovers = steps.leftovers();
    if journal.symlink_metadata().is_ok() {
        leftovers.push(journal.to_path_buf());
    }
    leftovers
}

/// The steps by hand that end a save once its files are as wanted: delete
/// every temp file and backup it left, remove the folders it created when
/// empty, then delete its journal.
fn clear_by_hand(steps: &Steps, journal: &Path) -> Vec<HandStep> {
    let mut hand: Vec<HandStep> = steps
        .leftovers()
        .into_iter()
        .map(HandStep::Delete)
        .collect();
    hand.extend(
        steps
            .dirs
            .iter()
            .rev()
            .filter(|d| d.symlink_metadata().is_ok_and(|m| m.is_dir()))
            .cloned()
            .map(HandStep::RemoveDir),
    );
    hand.push(HandStep::Delete(journal.to_path_buf()));
    hand
}

/// Whether every problem is a file that changed, which the user can keep.
fn keepable(problems: &[Problem]) -> bool {
    problems
        .iter()
        .all(|p| matches!(p.kind, ProblemKind::Edited | ProblemKind::BackupMissing))
}

/// Whether `path` is a regular file (not a link, not a folder) holding
/// exactly `digest`.
fn holds(path: &Path, digest: &Digest) -> bool {
    regular(path) && matches!(Digest::at(path), Ok(Some(d)) if d == *digest)
}

fn regular(path: &Path) -> bool {
    path.symlink_metadata().is_ok_and(|m| m.is_file())
}

/// Whether `write`'s temp file is the save's own, to delete while rolling
/// back: it holds the save's text, or it is empty (a write cut short before
/// its first byte) while its file was not replaced yet, holding its text from
/// before the save or, for a file the save creates, not there.
fn own_temp_before_commit(write: &FileWrite) -> bool {
    let empty = regular(&write.temp) && write.temp.metadata().is_ok_and(|m| m.len() == 0);
    let unreplaced = match &write.before {
        Some(before) => holds(&write.dest, before),
        None => write.dest.symlink_metadata().is_err(),
    };
    holds(&write.temp, &write.after) || (empty && unreplaced)
}

/// Delete what a committed save left: its backups and temp files, each only
/// when it holds the save's text, then its journal `file`. First checks that
/// every file holds the text the save wrote and every removed file is gone:
/// when one doesn't, nothing is deleted and the files are reported.
pub(crate) fn finish(steps: &Steps, file: &Path, disk: &mut Disk) -> Result<(), Blocked> {
    let mut problems = Vec::new();
    for write in &steps.writes {
        match state(&write.dest) {
            Ok(Some(d)) if d == write.after => {}
            Ok(_) => problems.push(Problem {
                backup: write.backup.clone(),
                fix: holds(&write.temp, &write.after).then(|| {
                    Box::new(HandStep::Copy {
                        from: write.temp.clone(),
                        to: write.dest.clone(),
                    })
                }),
                before_copy: write
                    .backup
                    .clone()
                    .zip(write.before.as_ref())
                    .filter(|(backup, before)| holds(backup, before))
                    .map(|(backup, _)| backup),
                ..Problem::new(&write.dest, ProblemKind::Edited, None)
            }),
            Err(problem) => problems.push(problem),
        }
    }
    for removal in &steps.removals {
        if removal.path.symlink_metadata().is_ok() {
            problems.push(Problem {
                backup: Some(removal.backup.clone()),
                fix: Some(Box::new(HandStep::Delete(removal.path.clone()))),
                before_copy: holds(&removal.backup, &removal.before)
                    .then(|| removal.backup.clone()),
                ..Problem::new(&removal.path, ProblemKind::Edited, None)
            });
        }
    }
    if !problems.is_empty() {
        let keep = keepable(&problems).then(|| {
            let mut hand: Vec<HandStep> = steps
                .leftovers()
                .into_iter()
                .map(HandStep::Delete)
                .collect();
            hand.push(HandStep::Delete(file.to_path_buf()));
            hand
        });
        return Err(Blocked {
            keep,
            ..Blocked::new(problems, on_disk(steps, file)).going(Direction::Finish)
        });
    }
    clean(steps, file, disk).map_err(|(path, e)| failed(steps, file, Direction::Finish, &path, &e))
}

fn clean(steps: &Steps, file: &Path, disk: &mut Disk) -> Result<(), (PathBuf, io::Error)> {
    for write in &steps.writes {
        if let (Some(backup), Some(before)) = (&write.backup, &write.before)
            && holds(backup, before)
        {
            disk.remove_file(backup).map_err(at(backup))?;
        }
        if holds(&write.temp, &write.after) {
            disk.remove_file(&write.temp).map_err(at(&write.temp))?;
        }
    }
    for removal in &steps.removals {
        if holds(&removal.backup, &removal.before) {
            disk.remove_file(&removal.backup)
                .map_err(at(&removal.backup))?;
        }
    }
    disk.remove_file(file).map_err(at(file))
}

/// What a file holds, against what the save expects of it.
fn state(path: &Path) -> Result<Option<Digest>, Problem> {
    Digest::at(path).map_err(|e| Problem::new(path, ProblemKind::Unreadable, Some(e.to_string())))
}

/// What undoing `write` needs to do, or why it can't.
enum WriteUndo {
    Nothing,
    /// Put the backup back over the file.
    Restore,
    /// A backup beside a file that was never replaced: drop it.
    DropBackup,
    /// Remove a file the save created.
    Remove,
}

fn plan_write(write: &FileWrite) -> Result<WriteUndo, Problem> {
    let now = state(&write.dest)?;
    let backed_up = write
        .backup
        .as_ref()
        .zip(write.before.as_ref())
        .is_some_and(|(backup, before)| holds(backup, before));
    let stuck = |kind: ProblemKind| {
        let before_copy = write.backup.clone().filter(|_| backed_up);
        Problem {
            backup: write.backup.clone(),
            fix: match &before_copy {
                Some(from) => Some(HandStep::Copy {
                    from: from.clone(),
                    to: write.dest.clone(),
                }),
                None if write.before.is_none() => Some(HandStep::Delete(write.dest.clone())),
                None => None,
            }
            .map(Box::new),
            before_copy,
            ..Problem::new(&write.dest, kind, None)
        }
    };
    let Some(before) = &write.before else {
        return match now {
            None => Ok(WriteUndo::Nothing),
            Some(d) if d == write.after => Ok(WriteUndo::Remove),
            Some(_) => Err(stuck(ProblemKind::Edited)),
        };
    };
    match now {
        // Never renamed over, or already put back. A hard-linked backup is the
        // same file, and renaming one link over the other does nothing, so
        // the backup is removed instead. One that holds anything else stays.
        Some(d) if d == *before => Ok(if backed_up {
            WriteUndo::DropBackup
        } else {
            WriteUndo::Nothing
        }),
        Some(d) if d != write.after => Err(stuck(ProblemKind::Edited)),
        _ if backed_up => Ok(WriteUndo::Restore),
        _ => Err(stuck(ProblemKind::BackupMissing)),
    }
}

/// What undoing `removal` needs to do.
enum RemovalUndo {
    Nothing,
    Restore,
    DropBackup,
}

fn plan_removal(removal: &Removal) -> Result<RemovalUndo, Problem> {
    let backed_up = holds(&removal.backup, &removal.before);
    let stuck = |kind: ProblemKind| Problem {
        backup: Some(removal.backup.clone()),
        fix: backed_up.then(|| {
            Box::new(HandStep::Copy {
                from: removal.backup.clone(),
                to: removal.path.clone(),
            })
        }),
        before_copy: backed_up.then(|| removal.backup.clone()),
        ..Problem::new(&removal.path, kind, None)
    };
    match state(&removal.path)? {
        Some(d) if d == removal.before => Ok(if backed_up {
            RemovalUndo::DropBackup
        } else {
            RemovalUndo::Nothing
        }),
        Some(_) => Err(stuck(ProblemKind::Edited)),
        None if backed_up => Ok(RemovalUndo::Restore),
        None => Err(stuck(ProblemKind::BackupMissing)),
    }
}

/// Undo the save `steps` describes and delete `file`, its journal. Every
/// file is checked before anything changes.
pub(crate) fn roll_back(steps: &Steps, file: &Path, disk: &mut Disk) -> Result<(), Blocked> {
    let mut problems = Vec::new();
    let removals: Vec<(&Removal, RemovalUndo)> = steps
        .removals
        .iter()
        .filter_map(|r| {
            plan_removal(r)
                .map(|u| (r, u))
                .map_err(|p| problems.push(p))
                .ok()
        })
        .collect();
    let writes: Vec<(&FileWrite, WriteUndo)> = steps
        .writes
        .iter()
        .filter_map(|w| {
            plan_write(w)
                .map(|u| (w, u))
                .map_err(|p| problems.push(p))
                .ok()
        })
        .collect();
    if !problems.is_empty() {
        let keep = keepable(&problems).then(|| {
            let mut hand = Vec::new();
            for (removal, undo) in &removals {
                if let RemovalUndo::Restore = undo {
                    hand.push(HandStep::Copy {
                        from: removal.backup.clone(),
                        to: removal.path.clone(),
                    });
                }
            }
            for (write, undo) in &writes {
                match (undo, &write.backup) {
                    (WriteUndo::Restore, Some(backup)) => hand.push(HandStep::Copy {
                        from: backup.clone(),
                        to: write.dest.clone(),
                    }),
                    (WriteUndo::Remove, _) => hand.push(HandStep::Delete(write.dest.clone())),
                    _ => {}
                }
            }
            hand.extend(clear_by_hand(steps, file));
            hand
        });
        return Err(Blocked {
            keep,
            ..Blocked::new(problems, on_disk(steps, file)).going(Direction::RollBack)
        });
    }
    let temps: Vec<&Path> = steps
        .writes
        .iter()
        .rev()
        .filter(|w| own_temp_before_commit(w))
        .map(|w| w.temp.as_path())
        .collect();

    undo(steps, &removals, &writes, &temps, file, disk)
        .map_err(|(path, e)| failed(steps, file, Direction::RollBack, &path, &e))
}

fn undo(
    steps: &Steps,
    removals: &[(&Removal, RemovalUndo)],
    writes: &[(&FileWrite, WriteUndo)],
    temps: &[&Path],
    file: &Path,
    disk: &mut Disk,
) -> Result<(), (PathBuf, io::Error)> {
    for (removal, undo) in removals.iter().rev() {
        match undo {
            RemovalUndo::Nothing => Ok(()),
            RemovalUndo::Restore => disk.rename(&removal.backup, &removal.path),
            RemovalUndo::DropBackup => disk.remove_file(&removal.backup),
        }
        .map_err(at(&removal.path))?;
    }
    for (write, undo) in writes.iter().rev() {
        match (undo, &write.backup) {
            (WriteUndo::Restore, Some(backup)) => disk.rename(backup, &write.dest),
            (WriteUndo::DropBackup, Some(backup)) => disk.remove_file(backup),
            (WriteUndo::Remove, _) => disk.remove_file(&write.dest),
            _ => Ok(()),
        }
        .map_err(at(&write.dest))?;
    }
    for temp in temps {
        disk.remove_file(temp).map_err(at(temp))?;
    }
    for dir in steps.dirs.iter().rev() {
        if dir.symlink_metadata().is_ok_and(|m| m.is_dir()) {
            disk.remove_dir(dir).map_err(at(dir))?;
        }
    }
    for folder in steps.folders() {
        disk.sync_dir(&folder).map_err(at(&folder))?;
    }
    disk.remove_file(file).map_err(at(file))
}

/// Tag an error with the file it happened on.
fn at(path: &Path) -> impl FnOnce(io::Error) -> (PathBuf, io::Error) {
    let path = path.to_path_buf();
    move |e| (path, e)
}
