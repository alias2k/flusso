//! The filesystem, one counted step at a time.
//!
//! Every step that changes the disk (create a folder, write a file, keep a
//! backup, rename, remove) goes through [`Disk`], so a test can make any step
//! fail, stop the process dead after any step (as a kill would: nothing runs
//! after it), or abort the process for real. Durability (`fsync` of written
//! files and of the folders a rename changed) is on by default and off in
//! tests, which only care about the order of the steps.

use std::fs::{File, OpenOptions};
use std::io::{self, Write as _};
use std::path::Path;

use super::journal::Digest;

/// `options` set to refuse a symbolic link at the path's last component,
/// instead of opening (or creating) whatever it points at. It sets the
/// custom flags, replacing any set before.
pub(crate) fn no_follow(options: &mut OpenOptions) -> &mut OpenOptions {
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::custom_flags(options, libc::O_NOFOLLOW);
    options
}

/// Refuse a link at `path`: what [`no_follow`] does on Unix, checked
/// beforehand everywhere (Windows has no `O_NOFOLLOW`).
pub(crate) fn not_a_link(path: &Path) -> io::Result<()> {
    match path.symlink_metadata() {
        Ok(meta) if meta.file_type().is_symlink() => Err(io::Error::other(format!(
            "{} is a symbolic link",
            path.display()
        ))),
        _ => Ok(()),
    }
}

/// The filesystem, one counted step at a time.
#[derive(Debug)]
pub(crate) struct Disk {
    /// Steps taken so far.
    steps: usize,
    /// The step numbers (1-based) that fail instead of running.
    fail: Vec<usize>,
    /// The number of steps after which every step fails and nothing else may
    /// run, as when the process is killed.
    stop_after: Option<usize>,
    /// The step after which the process aborts.
    abort_after: Option<usize>,
    /// Whether written files and changed folders are flushed to the device.
    sync: bool,
    /// The number of steps from which every folder flush fails.
    fail_sync_from: Option<usize>,
    /// Whether writing a private file fails once it is created.
    fail_private_write: bool,
    /// Checks made so far ([`Disk::holds`]).
    checks: usize,
    /// The check (1-based) right before which the file it checks is given
    /// these bytes (`None`: deleted), as another program would.
    edit_at_check: Option<(usize, Option<Vec<u8>>)>,
    /// Whether that edit lands right after the check looked, not before:
    /// the instant between the last look and the rename.
    #[cfg(test)]
    edit_after_look: bool,
    /// Writes another program makes right after a step (1-based).
    #[cfg(test)]
    edits_after: Vec<Edit>,
    /// Every change and folder flush made, in order.
    #[cfg(test)]
    calls: Vec<Call>,
}

/// One change or folder flush a [`Disk`] made, for a test of their order.
#[cfg(test)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Call {
    /// A file or folder made, or a file replaced in one rename, at the path.
    Made(std::path::PathBuf),
    Renamed(std::path::PathBuf, std::path::PathBuf),
    Removed(std::path::PathBuf),
    /// A folder flushed.
    Flushed(std::path::PathBuf),
}

/// A write another program makes to a file, for a test.
#[cfg(test)]
#[derive(Debug, Clone)]
pub(crate) struct Edit {
    pub(crate) after_step: usize,
    pub(crate) path: std::path::PathBuf,
    pub(crate) bytes: Vec<u8>,
    /// Write a temp file beside it and rename it over, as most editors do;
    /// else truncate and write in place (`>`), which a hard link shares.
    pub(crate) by_rename: bool,
    /// Set once it is written (a write into a missing folder fails).
    pub(crate) done: bool,
}

impl Default for Disk {
    fn default() -> Self {
        Self {
            steps: 0,
            fail: Vec::new(),
            stop_after: None,
            abort_after: None,
            sync: true,
            fail_sync_from: None,
            fail_private_write: false,
            checks: 0,
            edit_at_check: None,
            #[cfg(test)]
            edit_after_look: false,
            #[cfg(test)]
            edits_after: Vec::new(),
            #[cfg(test)]
            calls: Vec::new(),
        }
    }
}

impl Disk {
    /// A disk that counts its steps and flushes nothing.
    #[cfg(test)]
    pub(crate) fn counting() -> Self {
        Self {
            sync: false,
            ..Self::default()
        }
    }

    /// A disk on which the steps numbered in `fail` fail.
    #[cfg(test)]
    pub(crate) fn failing(fail: Vec<usize>) -> Self {
        Self {
            fail,
            ..Self::counting()
        }
    }

    /// A disk that stops dead once `steps` steps ran: as a process killed
    /// right after that step.
    #[cfg(test)]
    pub(crate) fn stopping_after(steps: usize) -> Self {
        Self {
            stop_after: Some(steps),
            ..Self::counting()
        }
    }

    /// A disk on which every folder flush fails once `steps` steps ran.
    #[cfg(test)]
    pub(crate) fn failing_sync_from(steps: usize) -> Self {
        Self {
            fail_sync_from: Some(steps),
            ..Self::counting()
        }
    }

    /// A disk on which writing a private file fails once it is created, as
    /// a full disk would.
    #[cfg(test)]
    pub(crate) fn failing_private_writes() -> Self {
        Self {
            fail_private_write: true,
            ..Self::counting()
        }
    }

    /// A disk on which another program writes `bytes` to (or, with `None`,
    /// deletes) the file the `check`-th check ([`Disk::holds`]) looks at,
    /// right before it looks.
    #[cfg(test)]
    pub(crate) fn editing_at_check(check: usize, bytes: Option<&str>) -> Self {
        Self {
            edit_at_check: Some((check, bytes.map(|b| b.as_bytes().to_vec()))),
            ..Self::counting()
        }
    }

    /// A disk on which another program writes `bytes` to the file the
    /// `check`-th check looks at, in place (`>`), right after it looked: in
    /// the instant before the rename that replaces it.
    #[cfg(test)]
    pub(crate) fn editing_after_look(check: usize, bytes: &str) -> Self {
        Self {
            edit_after_look: true,
            ..Self::editing_at_check(check, Some(bytes))
        }
    }

    /// This disk, on which another program also makes `edit`.
    #[cfg(test)]
    pub(crate) fn with_edit(mut self, edit: Edit) -> Self {
        self.edits_after.push(edit);
        self
    }

    /// This disk, stopping dead once `steps` steps ran as well.
    #[cfg(test)]
    pub(crate) fn and_stopping_after(mut self, steps: usize) -> Self {
        self.stop_after = Some(steps);
        self
    }

    /// The edits another program made, as they went.
    #[cfg(test)]
    pub(crate) fn edits(&self) -> &[Edit] {
        &self.edits_after
    }

    /// A disk that aborts the process right after step `step`.
    #[cfg(test)]
    pub(crate) fn aborting_after(step: usize) -> Self {
        Self {
            abort_after: Some(step),
            ..Self::default()
        }
    }

    /// Every change and folder flush made so far, in order.
    #[cfg(test)]
    pub(crate) fn calls(&self) -> &[Call] {
        &self.calls
    }

    #[cfg(test)]
    fn log(&mut self, result: &io::Result<()>, call: Call) {
        if result.is_ok() {
            self.calls.push(call);
        }
    }

    /// Steps taken so far.
    #[cfg(test)]
    pub(crate) fn steps(&self) -> usize {
        self.steps
    }

    /// Whether the disk stopped: the caller must return at once, undoing
    /// nothing, as a killed process would.
    pub(crate) fn stopped(&self) -> bool {
        self.stop_after.is_some_and(|n| self.steps >= n)
    }

    fn step(&mut self, run: impl FnOnce() -> io::Result<()>) -> io::Result<()> {
        if self.stopped() {
            return Err(io::Error::other("stopped"));
        }
        self.steps += 1;
        if self.fail.contains(&self.steps) {
            return Err(io::Error::other(format!("step {} failed", self.steps)));
        }
        let result = run();
        #[cfg(test)]
        for edit in &mut self.edits_after {
            if edit.after_step == self.steps {
                edit.done = if edit.by_rename {
                    let temp = edit.path.with_extension("editor~");
                    std::fs::write(&temp, &edit.bytes)
                        .and_then(|()| std::fs::rename(&temp, &edit.path))
                        .is_ok()
                } else {
                    std::fs::write(&edit.path, &edit.bytes).is_ok()
                };
            }
        }
        if self.abort_after == Some(self.steps) {
            std::process::abort();
        }
        result
    }

    pub(crate) fn create_dir(&mut self, dir: &Path) -> io::Result<()> {
        let result = self.step(|| std::fs::create_dir(dir));
        #[cfg(test)]
        self.log(&result, Call::Made(dir.to_path_buf()));
        result
    }

    /// Write `bytes` to a new file at `path`, with the permissions of `like`
    /// when it exists, and flush it. A link at `path` is refused, never
    /// written through.
    pub(crate) fn write(
        &mut self,
        path: &Path,
        bytes: &[u8],
        like: Option<&Path>,
    ) -> io::Result<()> {
        let sync = self.sync;
        let result = self.step(|| {
            not_a_link(path)?;
            let mut file =
                no_follow(OpenOptions::new().write(true).create(true).truncate(true)).open(path)?;
            file.write_all(bytes)?;
            if let Some(meta) = like.and_then(|like| std::fs::metadata(like).ok()) {
                file.set_permissions(meta.permissions())?;
            }
            if sync {
                file.sync_all()?;
            }
            Ok(())
        });
        #[cfg(test)]
        self.log(&result, Call::Made(path.to_path_buf()));
        result
    }

    /// Keep `path` at `backup`: a hard link, else a flushed copy.
    pub(crate) fn keep(&mut self, path: &Path, backup: &Path) -> io::Result<()> {
        let sync = self.sync;
        let result = self.step(|| {
            if std::fs::hard_link(path, backup).is_ok() {
                return Ok(());
            }
            std::fs::copy(path, backup)?;
            if sync {
                File::open(backup)?.sync_all()?;
            }
            Ok(())
        });
        #[cfg(test)]
        self.log(&result, Call::Made(backup.to_path_buf()));
        result
    }

    /// Create `path` holding `bytes`, private to the user (mode 0600 on
    /// Unix), flushed with its folder, and never replacing a file already
    /// there: written to a temp file and hard-linked into place, or, where
    /// links don't work, created with `create_new` at `path` itself.
    pub(crate) fn create_private(&mut self, path: &Path, bytes: &[u8]) -> io::Result<()> {
        let (sync, fail) = (self.sync, self.fail_private_write);
        let result = self.step(|| {
            let temp = private_temp(path);
            write_new_private(&temp, bytes, sync, fail)?;
            let linked = std::fs::hard_link(&temp, path);
            std::fs::remove_file(&temp).ok();
            match linked {
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists => return Err(e),
                Err(_) => write_new_private(path, bytes, sync, fail)?,
                Ok(()) => {}
            }
            match path.parent() {
                Some(dir) if sync => sync_dir(dir),
                _ => Ok(()),
            }
        });
        #[cfg(test)]
        self.log(&result, Call::Made(path.to_path_buf()));
        result
    }

    /// Replace `path` with a private file holding `bytes`, in one rename.
    /// The caller flushes the folder.
    pub(crate) fn replace_private(&mut self, path: &Path, bytes: &[u8]) -> io::Result<()> {
        let (sync, fail) = (self.sync, self.fail_private_write);
        let result = self.step(|| {
            let temp = private_temp(path);
            write_new_private(&temp, bytes, sync, fail)?;
            std::fs::rename(&temp, path).inspect_err(|_| {
                std::fs::remove_file(&temp).ok();
            })
        });
        #[cfg(test)]
        self.log(&result, Call::Made(path.to_path_buf()));
        result
    }

    /// Whether `path` holds exactly `expected` (`None`: no file) now. Not a
    /// step: it changes nothing. The last look before a rename replaces or
    /// removes a file another program may have written meanwhile.
    pub(crate) fn holds(&mut self, path: &Path, expected: Option<&Digest>) -> io::Result<bool> {
        self.checks += 1;
        let edit = match &self.edit_at_check {
            Some((at, bytes)) if *at == self.checks => Some(bytes.clone()),
            _ => None,
        };
        #[cfg(test)]
        let after = self.edit_after_look;
        #[cfg(not(test))]
        let after = false;
        let write = |bytes: &Option<Vec<u8>>| match bytes {
            Some(bytes) => std::fs::write(path, bytes),
            None => std::fs::remove_file(path),
        };
        if let Some(bytes) = &edit
            && !after
        {
            write(bytes)?;
        }
        let holds = Digest::at(path)?.as_ref() == expected;
        if let Some(bytes) = &edit
            && after
        {
            write(bytes)?;
        }
        Ok(holds)
    }

    pub(crate) fn rename(&mut self, from: &Path, to: &Path) -> io::Result<()> {
        let result = self.step(|| std::fs::rename(from, to));
        #[cfg(test)]
        self.log(&result, Call::Renamed(from.to_path_buf(), to.to_path_buf()));
        result
    }

    /// Remove a file; one already gone counts as removed.
    pub(crate) fn remove_file(&mut self, path: &Path) -> io::Result<()> {
        let result = self.step(|| match std::fs::remove_file(path) {
            Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e),
            _ => Ok(()),
        });
        #[cfg(test)]
        self.log(&result, Call::Removed(path.to_path_buf()));
        result
    }

    /// Remove an empty folder; one already gone, or holding a file someone
    /// else put there, is left as it is.
    pub(crate) fn remove_dir(&mut self, dir: &Path) -> io::Result<()> {
        let result = self.step(|| match std::fs::remove_dir(dir) {
            Err(e)
                if !matches!(
                    e.kind(),
                    io::ErrorKind::NotFound | io::ErrorKind::DirectoryNotEmpty
                ) =>
            {
                Err(e)
            }
            _ => Ok(()),
        });
        #[cfg(test)]
        self.log(&result, Call::Removed(dir.to_path_buf()));
        result
    }

    /// Flush a folder's entries, so the renames in it survive a power loss.
    /// Not a step. A platform or filesystem that can't flush a folder is
    /// skipped; a folder already gone has nothing to flush.
    pub(crate) fn sync_dir(&mut self, dir: &Path) -> io::Result<()> {
        if self.fail_sync_from.is_some_and(|n| self.steps >= n) {
            return Err(io::Error::other(format!(
                "flushing {} failed after step {}",
                dir.display(),
                self.steps
            )));
        }
        let result = if self.sync { sync_dir(dir) } else { Ok(()) };
        #[cfg(test)]
        self.log(&result, Call::Flushed(dir.to_path_buf()));
        result
    }
}

/// A fresh temp file name beside `path`: `.<name>.<pid>-<nanos>.tmp`
/// (the state folder's pruning reads the pid back).
fn private_temp(path: &Path) -> std::path::PathBuf {
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("state");
    path.with_file_name(format!(
        ".{name}.{}-{}.tmp",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ))
}

/// Create `path` (never one already there, never through a link), mode
/// 0600 on Unix, holding `bytes`. A write that fails once the file is
/// created removes it again.
fn write_new_private(path: &Path, bytes: &[u8], sync: bool, fail: bool) -> io::Result<()> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    let mut file = no_follow(&mut options).open(path)?;
    let written = (|| {
        if fail {
            return Err(io::Error::other("no space left on the device"));
        }
        file.write_all(bytes)?;
        if sync {
            file.sync_all()?;
        }
        Ok(())
    })();
    if written.is_err() {
        drop(file);
        std::fs::remove_file(path).ok();
    }
    written
}

/// Flush a folder's entries; a filesystem that can't, or a folder already
/// gone, counts as flushed.
#[cfg(unix)]
pub(crate) fn sync_dir(dir: &Path) -> io::Result<()> {
    let result = File::open(dir).and_then(|d| d.sync_all());
    match result {
        Err(e)
            if matches!(
                e.kind(),
                io::ErrorKind::NotFound | io::ErrorKind::Unsupported | io::ErrorKind::InvalidInput
            ) =>
        {
            Ok(())
        }
        other => other,
    }
}

/// Windows can't open a folder as a file without a flag std doesn't set; its
/// renames are journaled by NTFS.
#[cfg(not(unix))]
pub(crate) fn sync_dir(_dir: &Path) -> io::Result<()> {
    Ok(())
}
