//! One writer per project: an advisory lock on `.flusso-save.lock` in the
//! config's folder.
//!
//! A save holds it from before its journal is written until its last backup
//! is gone, and recovery holds it while it reads and settles a journal. So a
//! journal found under the lock belongs to a process that died: the operating
//! system drops a lock with the process that held it, a kill included.
//!
//! On Unix the lock file is removed on release. A process that was waiting on
//! the removed file then finds it gone or replaced, and locks the file at the
//! path again. Elsewhere the file stays (an empty `.flusso-save.lock`), since
//! std can't tell whether the path still names the file it locked.
//!
//! The lock file is never followed: a link (or anything but a regular file)
//! at its path is refused, so the lock can't create or open a file
//! elsewhere.
//!
//! Where the platform has no file locks (std reports `Unsupported`), the lock
//! is not taken: a warning is logged, and two designers saving one project at
//! the same moment are not kept apart.

use std::fs::{File, OpenOptions, TryLockError};
use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use super::disk::no_follow;

/// The lock file, in the config's folder.
pub(crate) const LOCK: &str = ".flusso-save.lock";

/// The project's lock, held until dropped.
#[derive(Debug)]
pub(crate) struct ProjectLock {
    base: PathBuf,
    /// `None` where the platform has no file locks.
    file: Option<File>,
}

/// Why the lock was not taken.
#[derive(Debug)]
pub(crate) enum LockError {
    /// Another process held it for the whole wait.
    Busy,
    /// The lock file could not be opened or locked.
    Io(io::Error),
    /// The lock file's path holds a link, a folder or another non-file,
    /// which is never followed or opened.
    NotAFile(PathBuf),
}

/// The error opening the lock file gave: a link at its path (`ELOOP` with
/// `O_NOFOLLOW`) is [`LockError::NotAFile`].
fn refused(path: &Path, error: io::Error) -> LockError {
    match path.symlink_metadata() {
        Ok(meta) if !meta.is_file() => LockError::NotAFile(path.to_path_buf()),
        _ => LockError::Io(error),
    }
}

impl std::fmt::Display for LockError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Busy => f.write_str("another designer is saving this project"),
            Self::Io(e) => write!(f, "locking the project: {e}"),
            Self::NotAFile(path) => write!(
                f,
                "the project can't be locked: {} is not a regular file (a link, say); the \
                 designer never follows or opens it. Delete it and reload",
                path.display()
            ),
        }
    }
}

impl ProjectLock {
    /// Take the lock of the project in `base`, waiting up to `wait` for
    /// another process to release it.
    pub(crate) fn acquire(base: &Path, wait: Duration) -> Result<Self, LockError> {
        let base = &std::fs::canonicalize(base).map_err(LockError::Io)?;
        let path = base.join(LOCK);
        let deadline = Instant::now() + wait;
        loop {
            if let Ok(meta) = path.symlink_metadata()
                && !meta.is_file()
            {
                return Err(LockError::NotAFile(path));
            }
            let file = no_follow(
                OpenOptions::new()
                    .read(true)
                    .write(true)
                    .create(true)
                    .truncate(false),
            )
            .open(&path)
            .map_err(|e| refused(&path, e))?;
            if !file.metadata().map_err(LockError::Io)?.is_file() {
                return Err(LockError::NotAFile(path));
            }
            match file.try_lock() {
                Ok(()) if still_at(&file, &path) => {
                    return Ok(Self {
                        base: base.to_path_buf(),
                        file: Some(file),
                    });
                }
                Ok(()) => continue,
                Err(TryLockError::WouldBlock) if Instant::now() < deadline => {
                    std::thread::sleep(Duration::from_millis(20));
                }
                Err(TryLockError::WouldBlock) => return Err(LockError::Busy),
                Err(TryLockError::Error(e)) if e.kind() == io::ErrorKind::Unsupported => {
                    tracing::warn!(
                        "this platform has no file locks: two designers saving {} at once are not kept apart",
                        base.display()
                    );
                    return Ok(Self {
                        base: base.to_path_buf(),
                        file: None,
                    });
                }
                Err(TryLockError::Error(e)) => return Err(LockError::Io(e)),
            }
        }
    }

    /// The config's folder, as the filesystem resolves it.
    pub(crate) fn base(&self) -> &Path {
        &self.base
    }
}

impl Drop for ProjectLock {
    fn drop(&mut self) {
        if cfg!(unix) && self.file.is_some() {
            std::fs::remove_file(self.base.join(LOCK)).ok();
        }
    }
}

/// Whether `path` still names the file `file` has open: the holder before
/// may have removed it while this process waited.
#[cfg(unix)]
fn still_at(file: &File, path: &Path) -> bool {
    use std::os::unix::fs::MetadataExt;
    match (file.metadata(), std::fs::metadata(path)) {
        (Ok(open), Ok(named)) => open.dev() == named.dev() && open.ino() == named.ino(),
        _ => false,
    }
}

#[cfg(not(unix))]
fn still_at(_file: &File, _path: &Path) -> bool {
    true
}
