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

    /// A disk that aborts the process right after step `step`.
    #[cfg(test)]
    pub(crate) fn aborting_after(step: usize) -> Self {
        Self {
            abort_after: Some(step),
            ..Self::default()
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
        if self.abort_after == Some(self.steps) {
            std::process::abort();
        }
        result
    }

    pub(crate) fn create_dir(&mut self, dir: &Path) -> io::Result<()> {
        self.step(|| std::fs::create_dir(dir))
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
        self.step(|| {
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
        })
    }

    /// Keep `path` at `backup`: a hard link, else a flushed copy.
    pub(crate) fn keep(&mut self, path: &Path, backup: &Path) -> io::Result<()> {
        let sync = self.sync;
        self.step(|| {
            if std::fs::hard_link(path, backup).is_ok() {
                return Ok(());
            }
            std::fs::copy(path, backup)?;
            if sync {
                File::open(backup)?.sync_all()?;
            }
            Ok(())
        })
    }

    /// Create `path` holding `bytes`, private to the user (mode 0600 on
    /// Unix), flushed with its folder, and never replacing a file already
    /// there: written to a temp file and hard-linked into place, or, where
    /// links don't work, created with `create_new` at `path` itself.
    pub(crate) fn create_private(&mut self, path: &Path, bytes: &[u8]) -> io::Result<()> {
        let (sync, fail) = (self.sync, self.fail_private_write);
        self.step(|| {
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
        })
    }

    /// Replace `path` with a private file holding `bytes`, in one rename.
    /// The caller flushes the folder.
    pub(crate) fn replace_private(&mut self, path: &Path, bytes: &[u8]) -> io::Result<()> {
        let (sync, fail) = (self.sync, self.fail_private_write);
        self.step(|| {
            let temp = private_temp(path);
            write_new_private(&temp, bytes, sync, fail)?;
            std::fs::rename(&temp, path).inspect_err(|_| {
                std::fs::remove_file(&temp).ok();
            })
        })
    }

    pub(crate) fn rename(&mut self, from: &Path, to: &Path) -> io::Result<()> {
        self.step(|| std::fs::rename(from, to))
    }

    /// Remove a file; one already gone counts as removed.
    pub(crate) fn remove_file(&mut self, path: &Path) -> io::Result<()> {
        self.step(|| match std::fs::remove_file(path) {
            Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e),
            _ => Ok(()),
        })
    }

    /// Remove an empty folder; one already gone, or holding a file someone
    /// else put there, is left as it is.
    pub(crate) fn remove_dir(&mut self, dir: &Path) -> io::Result<()> {
        self.step(|| match std::fs::remove_dir(dir) {
            Err(e)
                if !matches!(
                    e.kind(),
                    io::ErrorKind::NotFound | io::ErrorKind::DirectoryNotEmpty
                ) =>
            {
                Err(e)
            }
            _ => Ok(()),
        })
    }

    /// Flush a folder's entries, so the renames in it survive a power loss.
    /// Not a step. A platform or filesystem that can't flush a folder is
    /// skipped; a folder already gone has nothing to flush.
    pub(crate) fn sync_dir(&self, dir: &Path) -> io::Result<()> {
        if self.fail_sync_from.is_some_and(|n| self.steps >= n) {
            return Err(io::Error::other(format!(
                "flushing {} failed after step {}",
                dir.display(),
                self.steps
            )));
        }
        if !self.sync {
            return Ok(());
        }
        sync_dir(dir)
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
