//! Putting a checked save on disk so that it lands whole or not at all, even
//! when the process is killed or the power goes in the middle.
//!
//! **The invariant: once [`commit`] has started, the project is, or after
//! [`recover_telling`] becomes, byte for byte either what it was before the save or
//! what the save wrote: never a mix.** It holds for a kill or a power loss at
//! any step, including during the journal's own writing, during an undo and
//! during a recovery, which can itself be cut short and run again.
//!
//! A commit takes the files to write (schema files first, `flusso.toml` last)
//! and the files to remove (the sources of moves), holds the project's lock
//! ([`ProjectLock`]) and runs:
//!
//! 1. **record**: write a record of the save to the user's state folder
//!    ([`state`]): its token, the digest of the journal's exact bytes, the
//!    config folder's canonical path, not committed; flushed;
//! 2. **journal**: write every step it will take, with the digest of each
//!    file before and after, to `.flusso-save.journal.tmp`, flush it, and
//!    rename it to `.flusso-save.journal` in the config's folder (the
//!    [`journal`] module has the format);
//! 3. **stage**: create missing folders, write every file's new text to a
//!    flushed temp file beside it;
//! 4. **keep**: hard-link every file that will be replaced to a backup beside
//!    it (a flushed copy where links don't work);
//! 5. **swap**: rename each temp file over its destination, then rename each
//!    removed file to its backup; flush the folders. Right before each of
//!    these renames the file is checked to still hold the journal's `before`
//!    digest, which is the one the caller expects ([`Expect`]): a program
//!    that wrote it without the project's lock stops the commit with
//!    [`Changed`], and everything already done is undone. Only a write
//!    landing between that check and the rename is still replaced;
//! 6. **commit point**: mark the record committed (one rename in the state
//!    folder), and flush the state folder;
//! 7. rename the journal to `.flusso-save.done` (for the reader: recovery
//!    goes by the record);
//! 8. **clean**: once the project folder is flushed, delete the backups, then
//!    `.flusso-save.done`, then the record. When a flush fails the save
//!    stands as it is, and the next recovery cleans up.
//!
//! When a step of 1–6 fails, the commit undoes it at once, the way recovery
//! would ([`recover::roll_back`]), and removes the record. When the process
//! dies instead, whatever opens the project next ([`recover_telling`], under the
//! same lock) finds the journal and its record: not committed, it is rolled
//! back; committed, it is finished. Either way the record goes last, so a
//! journal is acted on once.
//!
//! **A file a rollback has settled is never settled again** (issue #302),
//! whatever the user does to it before a rollback cut short runs again. A
//! rollback decides every file first (put back, remove, leave, keep:
//! [`state::Act`]) and writes that into the record, flushed, before it
//! changes one. A decision that needs no step (leave, keep) settles its file
//! there. Each step (put back: the backup renamed over the file; remove: the
//! file the save created renamed to its temp file, deleted once every file
//! is settled) is one rename, then, once its folder is flushed, **its mark
//! in the record**, flushed, before the next step. The record and the project
//! are often on different filesystems, and even on one, flushing the state
//! folder does not flush a rename in the project: so every project change is
//! flushed before the record says anything more (the plan, a mark, `settled`,
//! the record removed), and a power cut never keeps a mark without its
//! rename. Completion is recorded, never read off the disk: a
//! backup gone or a temp file there proves nothing, since anyone can delete
//! or write one. A rollback running again never looks at a file whose
//! decision needs no step or whose step is marked; every other file it
//! decides again from what the disk holds now, exactly as a fresh rollback
//! would (an edit to a file not put back yet is kept, as always), recording
//! any changed decision before it acts. Once every file is settled and the
//! project's folders are flushed, it marks the record `settled`; from then on
//! only the save's own files are deleted. Finishing a committed save never
//! changes a project file, so it needs no marks.
//!
//! **The promise: recovery reports a save rolled back only when every file
//! it names holds its bytes from before the save (a created file: gone), or
//! is told kept as another program's, or was settled and then changed by the
//! user.** The one step a kill can catch between its rename and its mark is
//! told apart by **the file it moves**: the plan records each step's file
//! identity (device and inode, Unix only: [`state::FileId`]), and that file
//! at the step's destination name proves the rename ran. The step is then
//! done, and whatever the user put since at the name it left (a file the save
//! created, made again; a file put back, written to) is kept and named. With
//! no proof (another platform, or the file deleted or replaced by a new one
//! since) the step is decided again: done, it needs nothing now; its file
//! changed since (given the save's text, or deleted where the save had
//! removed it), with its backup gone, the rollback blocks as any rollback
//! without a backup does, and changes nothing. An inode reused in that
//! window can only make a step read as done whose file then holds its text
//! from before (nothing to do) or another text (kept and named): never a
//! silent mix.
//!
//! What a kill leaves, at any point, and what the next start does (the
//! record in the state folder, the rest in the config's folder):
//!
//! | Left by the kill | Next start |
//! | --- | --- |
//! | nothing of the save | nothing |
//! | a record, no journal | removes the record: nothing was changed, or the save stood, was undone in its process, or was recovered and told |
//! | a record, `.flusso-save.journal.tmp` | deletes it (nothing was changed), then removes the record |
//! | a journal or `.done`, no record of it for this folder | changes nothing; reports it, with the way out by hand |
//! | a journal, a record without `undo` (a save cut short; or a rollback cut short before it recorded, or by a designer from before `undo`) | rolls back from scratch: decides every file from what it holds, records, settles |
//! | a journal, a record with `undo`, not settled: every step marked or none begun | takes the unmarked steps, each decided again from what the disk holds now |
//! | the same, a step's rename done and its mark not written | the step's file is at its destination name (Unix): marks it done, keeps and names what the user put since at the name it left; else decides that file again: it holds its text from before (or a created file is gone), so it needs nothing |
//! | the same, no proof, and that file changed since, its backup gone (or a created file's temp name taken) | blocks as a fresh rollback would (backup missing; a file in the way), changes nothing |
//! | a journal, a record with `undo`, settled | deletes the save's temp files and empty folders, tells the save, deletes the journal, removes the record: no project file is looked at |
//! | a journal or `.done`, a committed record | finishes: deletes backups and temp files holding the save's text, tells the save, deletes the journal, removes the record; keeps any file holding another text |
//! | no journal, a stored notice, a record | removes the record; the notice is told until dismissed |
//!
//! A rollback that can't write its record (a full disk) carries on without
//! it, as before records kept a rollback: cut short, it decides again. So
//! does one an older designer cut short (its record has no `undo`): the
//! files it had settled are told apart from the rest by what they hold, as
//! that designer did, so an edit made between its kill and this start is
//! handled as that designer would have. The other way round, an older
//! designer can't read a record with `undo`: it only reports the journal,
//! and prunes the record a day later, after which this designer can only
//! report it too (see the manual).
//!
//! **A journal is acted on only when this user's designer has a record of
//! it, for this folder** ([`state`]): it is a file in the project, and
//! anyone who can put files there could write one, or bring a copy of one
//! the user's designer wrote. Any other journal is only reported (the
//! `recover` module says what it may touch). A rollback first checks every
//! file against the journal's digests, and touches nothing when one holds
//! neither text, or when the backup it needs is gone or damaged: it reports
//! the files instead. A finish first checks that every file holds the text
//! the save wrote.
//!
//! Without a usable state folder a save still works, unrecorded: a failure
//! in the process is undone from its journal, and the rename to
//! `.flusso-save.done` is its commit point; a journal a killed save leaves
//! is only reported.
//!
//! Every filesystem step goes through [`Disk`], which a test can tell to fail
//! at any step, or to stop dead after one, as a kill would.
//!
//! ```ignore
//! let lock = ProjectLock::acquire(&dir, Duration::from_secs(5))?;
//! let state = State::open();
//! recover(&lock, &dir.join("flusso.toml"), state.as_ref().map_err(|e| e.to_string().as_str()), &mut Disk::default());
//! let writes = vec![Write { dest: dir.join("users.schema.yml"), text: "…".into(), expect: Expect::AsFound }];
//! let removals = vec![Remove { path: dir.join("old.schema.yml"), expect: Expect::AsFound }];
//! commit(&lock, &writes, &removals, state.as_ref().ok(), &mut Disk::default())?;
//! ```

mod disk;
pub(crate) mod journal;
mod lock;
mod recover;
pub(crate) mod state;
#[cfg(test)]
mod tests;

use std::path::PathBuf;

use anyhow::{Result, anyhow};

pub(crate) use disk::Disk;
#[cfg(test)]
pub(crate) use disk::Edit;
pub(crate) use journal::Unusable;
use journal::{DONE, JOURNAL, Journal, STAGING, Steps};
pub(crate) use lock::{LockError, ProjectLock};
#[cfg(test)]
pub(crate) use recover::recover;
use recover::{Backups, JournalThen, Rollback};
pub(crate) use recover::{
    Blocked, Direction, Fix, HandStep, Holds, Outcome, ProblemKind, Report, Settled,
    recover_telling, strays,
};
use state::Record;
pub(crate) use state::{State, use_dir as use_state_dir};

/// One file a commit writes.
#[derive(Debug)]
pub(crate) struct Write {
    /// The file's resolved path.
    pub(crate) dest: PathBuf,
    /// Its new text.
    pub(crate) text: String,
    /// What it must hold for the commit to replace it.
    pub(crate) expect: Expect,
}

/// One file a commit removes.
#[derive(Debug)]
pub(crate) struct Remove {
    /// The file's resolved path.
    pub(crate) path: PathBuf,
    /// What it must hold for the commit to remove it.
    pub(crate) expect: Expect,
}

/// What a file must hold for a commit to replace or remove it: checked when
/// the commit plans (it becomes the journal's `before`), and again right
/// before the rename that replaces or removes the file.
#[derive(Debug, Clone)]
pub(crate) enum Expect {
    /// Whatever it holds when the commit plans: for tests of the commit alone.
    #[cfg(test)]
    AsFound,
    /// Exactly this: `None` for no file.
    Holds(Option<journal::Digest>),
}

impl Expect {
    /// The digest `path` must hold, refusing with [`Changed`] when it holds
    /// another.
    fn check(&self, path: &std::path::Path) -> Result<Option<journal::Digest>> {
        let now = journal::Digest::at(path)?;
        match self {
            Self::Holds(expected) if *expected != now => {
                Err(Changed(vec![(path.to_path_buf(), expected.clone())]).into())
            }
            #[cfg(test)]
            Self::AsFound => Ok(now),
            Self::Holds(_) => Ok(now),
        }
    }
}

/// A commit refused because files it replaces or removes no longer hold
/// what it expects: written by someone who doesn't take the project's lock.
/// Each with what the save expected it to hold (`None`: no file). Nothing
/// the commit did is left, and each of these files keeps what it holds.
#[derive(Debug)]
pub(crate) struct Changed(pub(crate) Vec<(PathBuf, Option<journal::Digest>)>);

impl std::fmt::Display for Changed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let files: Vec<String> = self
            .0
            .iter()
            .map(|(p, _)| p.display().to_string())
            .collect();
        write!(f, "{} changed during the save", files.join(", "))
    }
}

impl std::error::Error for Changed {}

/// How far [`run`] got: the writes renamed into place and the files removed.
#[derive(Debug, Default)]
struct Progress {
    renamed: usize,
    removed: usize,
}

/// A save that stands.
#[derive(Debug)]
pub(crate) struct Committed {
    /// The save's token.
    pub(crate) token: String,
    /// When it started, in milliseconds since the Unix epoch.
    pub(crate) started_at_ms: u64,
    /// The save's token, when its cleanup is left to the next recovery
    /// (its commit mark could not be flushed): that recovery finishes this
    /// save, not an interrupted one.
    pub(crate) cleanup_left: Option<String>,
    /// Backups kept because they hold what another program wrote to their
    /// file as the save replaced it ([`recover::Finished::displaced`]).
    pub(crate) displaced: Vec<recover::Displaced>,
}

/// Write `writes` in order and remove `removals`, all or nothing (see the
/// module docs). `lock` is the project's: the journal goes in its folder,
/// recorded in `state` when there is one.
pub(crate) fn commit(
    lock: &ProjectLock,
    writes: &[Write],
    removals: &[Remove],
    state: Option<&State>,
    disk: &mut Disk,
) -> Result<Committed> {
    let base = lock.base();
    let (journal, steps) = Journal::plan(base, writes, removals)?;
    let text = journal.encode()?;
    let journal_path = base.join(JOURNAL);
    let done = base.join(DONE);
    let record = Record::new(&steps.token, &text, base);

    let state = match state.map(|s| s.create(&record, disk)) {
        Some(Ok(())) => state,
        _ if disk.stopped() => return Err(anyhow!("stopped")),
        Some(Err(e)) => {
            tracing::warn!(
                error = %e,
                "could not record the save in the designer's state folder; if the designer is \
                 killed in the middle of it, the save is only reported on the next start"
            );
            None
        }
        None => None,
    };

    let mut progress = Progress::default();
    let ran = run(&steps, &text, writes, lock, disk, &mut progress).and_then(|()| match state {
        Some(state) => state
            .commit(&record, disk)
            .map_err(|e| anyhow!("completing the save: {e}")),
        None => disk
            .rename(&journal_path, &done)
            .map_err(|e| anyhow!("completing the save: {e}")),
    });
    if let Err(failed) = ran {
        if disk.stopped() {
            return Err(failed);
        }
        let staging = base.join(STAGING);
        let reached = failed
            .is::<Changed>()
            .then_some((progress.renamed, progress.removed));
        let undone = recover::roll_back(
            &steps,
            &journal_path,
            &Rollback {
                backups: Backups::Drop,
                ledger: state.map(|s| (s, &record)),
                reached,
                journal: JournalThen::Delete,
            },
            disk,
        );
        let undone = undone.and_then(|kept| {
            disk.remove_file(&staging)
                .and_then(|()| disk.sync_dir(base))
                .map_err(|e| {
                    Blocked::new(Vec::new(), vec![staging.clone()]).with_error(&staging, e)
                })
                .map(|()| kept)
        });
        if undone.is_ok()
            && let Some(state) = state
        {
            state.remove(&steps.token, disk).ok();
        }
        return Err(match undone {
            Ok(kept) => with_kept(failed, kept),
            Err(left) => anyhow!(
                "{failed:#}; restoring the project failed too ({}); the previous files are kept at: {}. \
                 The designer finishes restoring them the next time it opens this project",
                left.problems
                    .iter()
                    .map(|p| format!(
                        "{}: {}",
                        p.path.display(),
                        p.detail.as_deref().unwrap_or("not restored")
                    ))
                    .collect::<Vec<_>>()
                    .join("; "),
                left.leftovers
                    .iter()
                    .map(|p| p.display().to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        });
    }

    // Past the commit point a failure no longer undoes the save. The backups
    // go only once the commit is known to be on the device: until then a
    // power cut can bring back an uncommitted record, and rolling it back
    // needs them. Unflushed, the save is left for recovery to finish.
    let left = |e: &dyn std::fmt::Display| {
        tracing::warn!(
            error = %e,
            "the save is complete, but cleaning up after it failed; its backups are kept until \
             the designer next opens this project"
        );
        Ok(Committed {
            token: steps.token.clone(),
            started_at_ms: steps.started_at_ms,
            cleanup_left: Some(steps.token.clone()),
            displaced: Vec::new(),
        })
    };
    if let Some(state) = state {
        if let Err(e) = disk.sync_dir(state.path()) {
            return left(&e);
        }
        if let Err(e) = disk.rename(&journal_path, &done) {
            return left(&e);
        }
    }
    if let Err(e) = disk.sync_dir(base) {
        return left(&e);
    }
    let finished = match recover::finish(&steps, &done, disk) {
        Err(blocked) => return left(&format!("{blocked:?}")),
        Ok(finished) => finished,
    };
    if !finished.kept.is_empty() {
        tracing::debug!(
            files = ?finished.kept.iter().map(|k| &k.path).collect::<Vec<_>>(),
            "files written by another program right after the save are kept as they are"
        );
    }
    if let Some(state) = state
        && let Err(e) = state.remove(&steps.token, disk)
    {
        tracing::debug!(error = %e, "could not remove the record of a finished save");
    }
    Ok(Committed {
        token: steps.token.clone(),
        started_at_ms: steps.started_at_ms,
        cleanup_left: None,
        displaced: finished.displaced,
    })
}

/// `failed`, naming the files another program wrote meanwhile, which the undo
/// kept: a [`Changed`] grows by them, any other failure says so.
fn with_kept(failed: anyhow::Error, kept: Vec<recover::Kept>) -> anyhow::Error {
    if kept.is_empty() {
        return failed;
    }
    match failed.downcast::<Changed>() {
        Ok(Changed(mut files)) => {
            for k in kept {
                if !files.iter().any(|(p, _)| *p == k.path) {
                    files.push((k.path, k.before));
                }
            }
            Changed(files).into()
        }
        Err(failed) => anyhow!(
            "{failed:#}; nothing was saved, and {} changed meanwhile and kept what it holds",
            kept.iter()
                .map(|k| k.path.display().to_string())
                .collect::<Vec<_>>()
                .join(", ")
        ),
    }
}

/// Steps 2–5: everything up to the commit point. Right before each rename
/// that replaces or removes a file, the file is checked to still hold what
/// the journal says it held ([`Changed`] otherwise).
fn run(
    steps: &Steps,
    text: &[u8],
    writes: &[Write],
    lock: &ProjectLock,
    disk: &mut Disk,
    progress: &mut Progress,
) -> Result<()> {
    let base = lock.base();
    let staging = base.join(STAGING);
    disk.write(&staging, text, None)
        .map_err(|e| anyhow!("writing the save's journal: {e}"))?;
    disk.rename(&staging, &base.join(JOURNAL))
        .map_err(|e| anyhow!("writing the save's journal: {e}"))?;
    disk.sync_dir(base)
        .map_err(|e| anyhow!("writing the save's journal: {e}"))?;

    for dir in &steps.dirs {
        disk.create_dir(dir)
            .map_err(|e| anyhow!("creating {}: {e}", dir.display()))?;
    }
    for (planned, write) in steps.writes.iter().zip(writes) {
        disk.write(&planned.temp, write.text.as_bytes(), Some(&planned.dest))
            .map_err(|e| anyhow!("staging {}: {e}", planned.dest.display()))?;
    }
    for planned in &steps.writes {
        if let Some(backup) = &planned.backup {
            disk.keep(&planned.dest, backup)
                .map_err(|e| anyhow!("keeping a copy of {}: {e}", planned.dest.display()))?;
        }
    }
    let folders = steps.folders();
    for folder in &folders {
        disk.sync_dir(folder)
            .map_err(|e| anyhow!("flushing {}: {e}", folder.display()))?;
    }

    for planned in &steps.writes {
        let unchanged = disk
            .holds(&planned.dest, planned.before.as_ref())
            .map_err(|e| anyhow!("reading {}: {e}", planned.dest.display()))?;
        if !unchanged {
            return Err(Changed(vec![(planned.dest.clone(), planned.before.clone())]).into());
        }
        disk.rename(&planned.temp, &planned.dest)
            .map_err(|e| anyhow!("writing {}: {e}", planned.dest.display()))?;
        progress.renamed += 1;
    }
    for removal in &steps.removals {
        let unchanged = disk
            .holds(&removal.path, Some(&removal.before))
            .map_err(|e| anyhow!("reading {}: {e}", removal.path.display()))?;
        if !unchanged {
            return Err(Changed(vec![(removal.path.clone(), Some(removal.before.clone()))]).into());
        }
        disk.rename(&removal.path, &removal.backup)
            .map_err(|e| anyhow!("removing {}: {e}", removal.path.display()))?;
        progress.removed += 1;
    }
    for folder in &folders {
        disk.sync_dir(folder)
            .map_err(|e| anyhow!("flushing {}: {e}", folder.display()))?;
    }
    Ok(())
}

impl Blocked {
    fn with_error(mut self, path: &std::path::Path, error: std::io::Error) -> Self {
        self.problems.push(recover::Problem::new(
            path,
            ProblemKind::Failed,
            Some(error.to_string()),
        ));
        self
    }
}

/// Whether a save left a journal in `base`: what makes a diff stop to
/// recover first. Reading it takes no lock.
pub(crate) fn pending(base: &std::path::Path) -> bool {
    [JOURNAL, DONE, STAGING]
        .iter()
        .any(|name| base.join(name).symlink_metadata().is_ok())
}
