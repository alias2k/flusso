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
//! - committed: every file was in place, so only the cleanup is left:
//!   recovery deletes the backups and temp files (it finishes).
//!
//! **A file holding a third text is kept as it is**: neither the text from
//! before the save nor the text it wrote (or deleted, where the save left a
//! file). Something other than the save wrote it, during the save or after
//! it, and that is the user's newest text for it: [`Kept`] lists it. A
//! committed save then finishes around it. A rollback puts every other file
//! back, and of the kept file's backup: a recovery keeps the one holding its
//! text from before and names it (the user decides), the save's own undo
//! drops it (the page still shows that text). A backup holding the kept
//! file's own bytes (a hard link edited in place) is dropped either way.
//! The result is the project before the save, or after it, plus the other
//! program's writes: never a mix of the save's text and the text before.
//!
//! Either way it then hands what it did to the caller ([`recover_telling`]:
//! the designer keeps it as a notice), deletes the journal, and removes the
//! record last, so a journal is acted on once. A kill before the journal is
//! deleted makes the next start settle the save again, which finds the files
//! already settled and tells the same thing (a notice is kept per save, so it
//! is replaced, never doubled); a kill after it leaves the notice told. `.flusso-save.journal.tmp` with a record of
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
//! that holds neither text is kept (above). A file that can't be read, or a
//! rollback with no copy of a file's text from before, stops it before it
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
    /// (rolled back) or after it (completed), except those in `kept`.
    pub(crate) files: Vec<PathBuf>,
    /// The files another program wrote meanwhile, left as they are.
    pub(crate) kept: Vec<Kept>,
    /// Backups holding neither text of their file, kept (see [`Finished`]).
    pub(crate) displaced: Vec<Displaced>,
}

/// What finishing a committed save left as another program wrote it.
#[derive(Debug, Default)]
pub(crate) struct Finished {
    /// Files written since they were put in place.
    pub(crate) kept: Vec<Kept>,
    /// Files whose backup holds neither their text from before the save nor
    /// the text it wrote: what a program wrote to the file in place (through
    /// the hard link the backup is) in the instant the save replaced it. The
    /// file holds the save's text; the backup is kept, the only copy of the
    /// other bytes.
    pub(crate) displaced: Vec<Displaced>,
}

/// A file whose other bytes are in `copy` (see [`Finished::displaced`]).
#[derive(Debug, Clone)]
pub(crate) struct Displaced {
    pub(crate) path: PathBuf,
    pub(crate) copy: PathBuf,
}

/// A file a settled save left as it found it, holding neither its text from
/// before the save nor the text the save wrote (see the module docs).
#[derive(Debug, Clone)]
pub(crate) struct Kept {
    pub(crate) path: PathBuf,
    /// What the file held before the save: `None` for one it created.
    pub(crate) before: Option<Digest>,
    /// The save's backup of the file, holding its text from before, kept
    /// for the user to decide on.
    pub(crate) backup: Option<PathBuf>,
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
#[cfg(test)]
pub(crate) fn recover(
    lock: &ProjectLock,
    config: &Path,
    state: Result<&State, &str>,
    disk: &mut Disk,
) -> Recovery {
    recover_telling(lock, config, state, disk, &mut |_, _| {})
}

/// Settle whatever save a dead process left in `lock`'s project, whose
/// config file is `config`, acting only on a journal `state` holds a record
/// of (`Err` says why there are no records to check). Each save it settles
/// is handed to `tell` once its files are settled and before its journal is
/// deleted: a kill before the journal goes leaves it and its record, so the
/// next start settles the save again from the state the kill left (its files
/// are settled already, so it tells the same thing). The journal is deleted
/// in the same recovery whatever `tell` managed to keep, so a save is never
/// settled twice over files the user may have changed since.
///
/// A record of this folder with no journal left is removed, and logged: no
/// save it names is untold. Such a record is left by a kill before the save
/// wrote its journal (nothing was changed), after the save's own cleanup or
/// undo deleted the journal (the save stood, or was undone in the process
/// that made it), or after a recovery deleted it (told already).
pub(crate) fn recover_telling(
    lock: &ProjectLock,
    config: &Path,
    state: Result<&State, &str>,
    disk: &mut Disk,
    tell: &mut dyn FnMut(&Settled, &mut Disk),
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
            match settle(base, config, file, state, disk, tell) {
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
            tracing::info!(
                token = %record.token,
                committed = record.committed,
                "removing the record of a save whose journal is gone: it changed nothing, \
                 stood or was undone by itself, or was recovered and told already"
            );
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
    tell: &mut dyn FnMut(&Settled, &mut Disk),
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
    let (outcome, direction, Finished { kept, displaced }) = if record.committed {
        (
            Outcome::Completed,
            Direction::Finish,
            finish_with(&steps, file, JournalThen::Keep, disk)?,
        )
    } else {
        (
            Outcome::RolledBack,
            Direction::RollBack,
            Finished {
                kept: roll_back_with(&steps, file, Backups::Keep, JournalThen::Keep, disk)?,
                displaced: Vec::new(),
            },
        )
    };
    let mut folders = steps.folders();
    folders.extend(steps.dirs.iter().cloned());
    let settled = Settled {
        folders,
        outcome,
        started_at_ms: steps.started_at_ms,
        files: steps.files(),
        token: steps.token.clone(),
        kept,
        displaced,
    };
    if disk.stopped() {
        return Ok(settled);
    }
    tell(&settled, disk);
    if disk.stopped() {
        return Ok(settled);
    }
    disk.remove_file(file)
        .map_err(|e| failed(&steps, file, direction, file, &e))?;
    if !disk.stopped()
        && let Err(e) = state.remove(&record.token, disk)
    {
        tracing::warn!(error = %e, "could not remove the record of a recovered save");
    }
    Ok(settled)
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
        .all(|p| p.kind == ProblemKind::BackupMissing)
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
    holds(&write.temp, &write.after) || (empty && (unreplaced || !holds(&write.dest, &write.after)))
}

/// Delete what a committed save left: its backups and temp files, each only
/// when it holds the save's text, then its journal `file`. Every file was in
/// place at the commit point, so one holding another text now, or a removed
/// one made again, was written since: it is kept as it is, and listed.
pub(crate) fn finish(steps: &Steps, file: &Path, disk: &mut Disk) -> Result<Finished, Blocked> {
    finish_with(steps, file, JournalThen::Delete, disk)
}

/// What settling a save does with its journal once the files are settled.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum JournalThen {
    /// Delete it: the save is over.
    Delete,
    /// Leave it to the caller, which tells about the save first: a kill
    /// before the journal goes makes the next start settle the save again.
    Keep,
}

/// [`finish`], deleting the journal or not as `journal` says.
fn finish_with(
    steps: &Steps,
    file: &Path,
    journal: JournalThen,
    disk: &mut Disk,
) -> Result<Finished, Blocked> {
    let mut kept = Vec::new();
    let mut displaced = Vec::new();
    let mut unreadable = Vec::new();
    for write in &steps.writes {
        match state(&write.dest) {
            Ok(Some(d)) if d == write.after => {}
            Ok(_) => kept.push(Kept {
                path: write.dest.clone(),
                before: write.before.clone(),
                backup: None,
            }),
            Err(problem) => unreadable.push(problem),
        }
        if let (Some(backup), Some(before)) = (&write.backup, &write.before)
            && regular(backup)
            && !holds(backup, before)
            && !holds(backup, &write.after)
        {
            displaced.push(Displaced {
                path: write.dest.clone(),
                copy: backup.clone(),
            });
        }
    }
    for removal in &steps.removals {
        if removal.path.symlink_metadata().is_ok() {
            kept.push(Kept {
                path: removal.path.clone(),
                before: Some(removal.before.clone()),
                backup: None,
            });
        }
    }
    if !unreadable.is_empty() {
        return Err(Blocked::new(unreadable, on_disk(steps, file)).going(Direction::Finish));
    }
    clean(steps, file, journal, disk)
        .map_err(|(path, e)| failed(steps, file, Direction::Finish, &path, &e))?;
    Ok(Finished { kept, displaced })
}

fn clean(
    steps: &Steps,
    file: &Path,
    journal: JournalThen,
    disk: &mut Disk,
) -> Result<(), (PathBuf, io::Error)> {
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
    if journal == JournalThen::Delete {
        disk.remove_file(file).map_err(at(file))?;
    }
    Ok(())
}

/// What a file holds, against what the save expects of it.
fn state(path: &Path) -> Result<Option<Digest>, Problem> {
    Digest::at(path).map_err(|e| Problem::new(path, ProblemKind::Unreadable, Some(e.to_string())))
}

/// What a rollback does with the backup of a file it keeps.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Backups {
    /// Keep one holding the text from before, and name it: a recovery,
    /// where the user decides.
    Keep,
    /// Drop it: the save's own undo, whose page still shows that text.
    Drop,
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
    /// The file holds a third text: leave it, and drop its backup when
    /// `drop_backup`.
    Keep {
        drop_backup: bool,
        kept: Kept,
    },
}

/// The [`Kept`] record of `path`, and whether its backup goes: one holding
/// the kept file's own bytes always, one holding the text from before
/// unless `backups` keeps it, anything else never (a leftover reported).
fn keep(
    path: &Path,
    now: Option<&Digest>,
    before: Option<&Digest>,
    backup: Option<&PathBuf>,
    backups: Backups,
) -> (bool, Kept) {
    let at = |b: &PathBuf| Digest::at(b).ok().flatten();
    let backup_holds = backup.and_then(at);
    let redundant = backup_holds.is_some() && backup_holds.as_ref() == now;
    let before_text = backup_holds.is_some() && backup_holds.as_ref() == before;
    let drop_backup = redundant || (before_text && backups == Backups::Drop);
    let named = (before_text && !drop_backup)
        .then(|| backup.cloned())
        .flatten();
    (
        drop_backup,
        Kept {
            path: path.to_path_buf(),
            before: before.cloned(),
            backup: named,
        },
    )
}

fn plan_write(write: &FileWrite, backups: Backups) -> Result<WriteUndo, Problem> {
    let now = state(&write.dest)?;
    let backed_up = write
        .backup
        .as_ref()
        .zip(write.before.as_ref())
        .is_some_and(|(backup, before)| holds(backup, before));
    let third = || {
        let (drop_backup, kept) = keep(
            &write.dest,
            now.as_ref(),
            write.before.as_ref(),
            write.backup.as_ref(),
            backups,
        );
        Ok(WriteUndo::Keep { drop_backup, kept })
    };
    let Some(before) = &write.before else {
        return match now {
            None => Ok(WriteUndo::Nothing),
            Some(d) if d == write.after => Ok(WriteUndo::Remove),
            Some(_) => third(),
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
        Some(d) if d == write.after && backed_up => Ok(WriteUndo::Restore),
        Some(d) if d == write.after => Err(Problem {
            backup: write.backup.clone(),
            ..Problem::new(&write.dest, ProblemKind::BackupMissing, None)
        }),
        _ => third(),
    }
}

/// What undoing `removal` needs to do.
enum RemovalUndo {
    Nothing,
    Restore,
    DropBackup,
    /// Made again with another text: leave it, as [`WriteUndo::Keep`].
    Keep {
        drop_backup: bool,
        kept: Kept,
    },
}

fn plan_removal(removal: &Removal, backups: Backups) -> Result<RemovalUndo, Problem> {
    let backed_up = holds(&removal.backup, &removal.before);
    match state(&removal.path)? {
        Some(d) if d == removal.before => Ok(if backed_up {
            RemovalUndo::DropBackup
        } else {
            RemovalUndo::Nothing
        }),
        Some(d) => {
            let (drop_backup, kept) = keep(
                &removal.path,
                Some(&d),
                Some(&removal.before),
                Some(&removal.backup),
                backups,
            );
            Ok(RemovalUndo::Keep { drop_backup, kept })
        }
        None if backed_up => Ok(RemovalUndo::Restore),
        None => Err(Problem {
            backup: Some(removal.backup.clone()),
            ..Problem::new(&removal.path, ProblemKind::BackupMissing, None)
        }),
    }
}

/// Undo the save `steps` describes and delete `file`, its journal. Every
/// file is checked before anything changes; one holding a third text is
/// kept (see the module docs), its backup as `backups` says.
pub(crate) fn roll_back(
    steps: &Steps,
    file: &Path,
    backups: Backups,
    disk: &mut Disk,
) -> Result<Vec<Kept>, Blocked> {
    roll_back_with(steps, file, backups, JournalThen::Delete, disk)
}

/// [`roll_back`], deleting the journal or not as `journal` says.
fn roll_back_with(
    steps: &Steps,
    file: &Path,
    backups: Backups,
    journal: JournalThen,
    disk: &mut Disk,
) -> Result<Vec<Kept>, Blocked> {
    let mut problems = Vec::new();
    let removals: Vec<(&Removal, RemovalUndo)> = steps
        .removals
        .iter()
        .filter_map(|r| {
            plan_removal(r, backups)
                .map(|u| (r, u))
                .map_err(|p| problems.push(p))
                .ok()
        })
        .collect();
    let writes: Vec<(&FileWrite, WriteUndo)> = steps
        .writes
        .iter()
        .filter_map(|w| {
            plan_write(w, backups)
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
    let kept: Vec<Kept> = removals
        .iter()
        .filter_map(|(_, u)| match u {
            RemovalUndo::Keep { kept, .. } => Some(kept.clone()),
            _ => None,
        })
        .chain(writes.iter().filter_map(|(_, u)| match u {
            WriteUndo::Keep { kept, .. } => Some(kept.clone()),
            _ => None,
        }))
        .collect();

    undo(steps, &removals, &writes, &temps, file, journal, disk)
        .map_err(|(path, e)| failed(steps, file, Direction::RollBack, &path, &e))?;
    Ok(kept)
}

fn undo(
    steps: &Steps,
    removals: &[(&Removal, RemovalUndo)],
    writes: &[(&FileWrite, WriteUndo)],
    temps: &[&Path],
    file: &Path,
    journal: JournalThen,
    disk: &mut Disk,
) -> Result<(), (PathBuf, io::Error)> {
    for (removal, undo) in removals.iter().rev() {
        match undo {
            RemovalUndo::Nothing
            | RemovalUndo::Keep {
                drop_backup: false, ..
            } => Ok(()),
            RemovalUndo::Restore => disk.rename(&removal.backup, &removal.path),
            RemovalUndo::DropBackup
            | RemovalUndo::Keep {
                drop_backup: true, ..
            } => disk.remove_file(&removal.backup),
        }
        .map_err(at(&removal.path))?;
    }
    for (write, undo) in writes.iter().rev() {
        match (undo, &write.backup) {
            (WriteUndo::Restore, Some(backup)) => disk.rename(backup, &write.dest),
            (
                WriteUndo::DropBackup
                | WriteUndo::Keep {
                    drop_backup: true, ..
                },
                Some(backup),
            ) => disk.remove_file(backup),
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
    if journal == JournalThen::Delete {
        disk.remove_file(file).map_err(at(file))?;
    }
    Ok(())
}

/// Tag an error with the file it happened on.
fn at(path: &Path) -> impl FnOnce(io::Error) -> (PathBuf, io::Error) {
    let path = path.to_path_buf();
    move |e| (path, e)
}
