//! Putting a checked save on disk so that a failure at any step leaves the
//! project as it was.
//!
//! A commit takes the files to write (schema files first, `flusso.toml` last)
//! and the files to remove (the sources of moves), and runs in four phases:
//!
//! 1. **stage**: create missing folders, write every file's new text to a temp
//!    file beside it;
//! 2. **keep**: hard-link every file that will be replaced to a backup beside
//!    it (same folder, so same filesystem);
//! 3. **swap**: rename each temp file over its destination, then rename each
//!    removed file to its backup;
//! 4. **clean**: delete the backups.
//!
//! When a step of 1–3 fails, every step taken is undone in reverse: a replaced
//! file gets its backup back, a created file and folder go, temp files and
//! backups go. The project is then byte for byte what it was. When an undo
//! step fails too, the error names the backups still on disk.
//!
//! Every filesystem step goes through [`Disk`], which a test can tell to fail
//! at any step.
//!
//! ```ignore
//! let writes = vec![Write { dest: dir.join("users.schema.yml"), text: "…".into() }];
//! commit(&writes, &[dir.join("old.schema.yml")], &mut Disk::default())?;
//! ```

use std::io;
use std::path::{Path, PathBuf};

use anyhow::{Result, anyhow};

/// One file a commit writes.
#[derive(Debug)]
pub(crate) struct Write {
    /// The file's resolved path.
    pub(crate) dest: PathBuf,
    /// Its new text.
    pub(crate) text: String,
}

/// The filesystem, one counted step at a time.
#[derive(Debug, Default)]
pub(crate) struct Disk {
    /// Steps taken so far.
    steps: usize,
    /// The step numbers (1-based) that fail instead of running.
    fail: Vec<usize>,
}

impl Disk {
    /// A disk on which the steps numbered in `fail` fail.
    #[cfg(test)]
    pub(crate) fn failing(fail: Vec<usize>) -> Self {
        Self { steps: 0, fail }
    }

    fn step(&mut self, run: impl FnOnce() -> io::Result<()>) -> io::Result<()> {
        self.steps += 1;
        if self.fail.contains(&self.steps) {
            return Err(io::Error::other(format!("step {} failed", self.steps)));
        }
        run()
    }

    fn create_dir(&mut self, dir: &Path) -> io::Result<()> {
        self.step(|| std::fs::create_dir(dir))
    }

    fn write(&mut self, path: &Path, text: &str) -> io::Result<()> {
        self.step(|| std::fs::write(path, text))
    }

    fn keep(&mut self, path: &Path, backup: &Path) -> io::Result<()> {
        self.step(|| {
            std::fs::hard_link(path, backup).or_else(|_| std::fs::copy(path, backup).map(|_| ()))
        })
    }

    fn rename(&mut self, from: &Path, to: &Path) -> io::Result<()> {
        self.step(|| std::fs::rename(from, to))
    }

    /// Remove a file; one already gone counts as removed.
    fn remove_file(&mut self, path: &Path) -> io::Result<()> {
        self.step(|| match std::fs::remove_file(path) {
            Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e),
            _ => Ok(()),
        })
    }

    fn remove_dir(&mut self, dir: &Path) -> io::Result<()> {
        self.step(|| std::fs::remove_dir(dir))
    }
}

/// A step taken, and so a step to undo.
#[derive(Debug)]
enum Done {
    Dir(PathBuf),
    Temp(PathBuf),
    Backup(PathBuf),
    Replaced {
        dest: PathBuf,
        backup: Option<PathBuf>,
    },
    Removed {
        path: PathBuf,
        backup: PathBuf,
    },
}

/// Write `writes` in order and remove `removals`, all or nothing (see the
/// module docs).
pub(crate) fn commit(writes: &[Write], removals: &[PathBuf], disk: &mut Disk) -> Result<()> {
    let token = token();
    let mut done: Vec<Done> = Vec::new();
    match run(writes, removals, &token, disk, &mut done) {
        Ok(()) => {
            for step in &done {
                if let Done::Backup(backup) = step {
                    std::fs::remove_file(backup).ok();
                }
            }
            Ok(())
        }
        Err(failed) => Err(match undo(&done, disk) {
            Ok(()) => failed,
            Err(left) => anyhow!(
                "{failed:#}; restoring the project failed too ({}), the previous files are kept at: {}",
                left.errors.join("; "),
                left.backups.join(", ")
            ),
        }),
    }
}

fn run(
    writes: &[Write],
    removals: &[PathBuf],
    token: &str,
    disk: &mut Disk,
    done: &mut Vec<Done>,
) -> Result<()> {
    let mut temps = Vec::with_capacity(writes.len());
    for (i, write) in writes.iter().enumerate() {
        if let Some(parent) = write.dest.parent() {
            let missing: Vec<&Path> = parent.ancestors().take_while(|a| !a.exists()).collect();
            for dir in missing.into_iter().rev() {
                disk.create_dir(dir)
                    .map_err(|e| anyhow!("creating {}: {e}", dir.display()))?;
                done.push(Done::Dir(dir.to_path_buf()));
            }
        }
        let temp = sibling(&write.dest, &format!("{token}.{i}.tmp"));
        done.push(Done::Temp(temp.clone()));
        disk.write(&temp, &write.text)
            .map_err(|e| anyhow!("staging {}: {e}", write.dest.display()))?;
        if let Ok(meta) = std::fs::metadata(&write.dest) {
            std::fs::set_permissions(&temp, meta.permissions())
                .map_err(|e| anyhow!("staging {}: {e}", write.dest.display()))?;
        }
        temps.push(temp);
    }

    let mut backups = Vec::with_capacity(writes.len());
    for (i, write) in writes.iter().enumerate() {
        let backup = write
            .dest
            .is_file()
            .then(|| sibling(&write.dest, &format!("{token}.{i}.bak")));
        if let Some(backup) = &backup {
            done.push(Done::Backup(backup.clone()));
            disk.keep(&write.dest, backup)
                .map_err(|e| anyhow!("keeping a copy of {}: {e}", write.dest.display()))?;
        }
        backups.push(backup);
    }

    for ((write, temp), backup) in writes.iter().zip(&temps).zip(backups) {
        disk.rename(temp, &write.dest)
            .map_err(|e| anyhow!("writing {}: {e}", write.dest.display()))?;
        done.push(Done::Replaced {
            dest: write.dest.clone(),
            backup,
        });
    }

    for (i, path) in removals.iter().enumerate() {
        let backup = sibling(path, &format!("{token}.r{i}.bak"));
        disk.rename(path, &backup)
            .map_err(|e| anyhow!("removing {}: {e}", path.display()))?;
        done.push(Done::Removed {
            path: path.clone(),
            backup: backup.clone(),
        });
        done.push(Done::Backup(backup));
    }
    Ok(())
}

/// What an undo could not put back.
struct Left {
    errors: Vec<String>,
    backups: Vec<String>,
}

/// Undo `done` in reverse; keep going past a failure, and report it. A
/// backup that could not be put back is never removed.
fn undo(done: &[Done], disk: &mut Disk) -> std::result::Result<(), Left> {
    let mut errors = Vec::new();
    let mut kept: Vec<&Path> = Vec::new();
    for step in done.iter().rev() {
        let (result, what) = match step {
            Done::Removed { path, backup }
            | Done::Replaced {
                dest: path,
                backup: Some(backup),
            } => {
                let result = disk.rename(backup, path);
                if result.is_err() {
                    kept.push(backup);
                }
                (result, path)
            }
            Done::Replaced { dest, backup: None } => (disk.remove_file(dest), dest),
            Done::Backup(backup) if kept.contains(&backup.as_path()) => continue,
            Done::Backup(backup) => (disk.remove_file(backup), backup),
            Done::Temp(temp) => (disk.remove_file(temp), temp),
            Done::Dir(dir) => (disk.remove_dir(dir), dir),
        };
        if let Err(e) = result {
            errors.push(format!("{}: {e}", what.display()));
        }
    }
    if errors.is_empty() {
        return Ok(());
    }
    let backups = done
        .iter()
        .filter_map(|step| match step {
            Done::Backup(backup) if backup.exists() => Some(backup.display().to_string()),
            _ => None,
        })
        .collect();
    Err(Left { errors, backups })
}

/// A name unique to this save, so its temp files and backups never meet a
/// file an earlier save left behind.
fn token() -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or_default();
    format!("{}-{nanos}", std::process::id())
}

/// A hidden file beside `path`: `.<name>.flusso-save.<suffix>`.
fn sibling(path: &Path, suffix: &str) -> PathBuf {
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("out");
    let dir = path.parent().unwrap_or(Path::new("."));
    dir.join(format!(".{name}.flusso-save.{suffix}"))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::indexing_slicing)]

    use std::collections::BTreeMap;

    use super::*;

    /// Every folder and file under `dir`, with each file's bytes.
    fn snapshot(dir: &Path) -> BTreeMap<PathBuf, Option<Vec<u8>>> {
        let mut out = BTreeMap::new();
        let mut stack = vec![dir.to_path_buf()];
        while let Some(at) = stack.pop() {
            for entry in std::fs::read_dir(&at).unwrap().flatten() {
                let path = entry.path();
                if path.is_dir() {
                    out.insert(path.clone(), None);
                    stack.push(path);
                } else {
                    out.insert(path.clone(), Some(std::fs::read(&path).unwrap()));
                }
            }
        }
        out
    }

    /// A project with three files, and a commit that overwrites two of them
    /// (the config last), creates one in a new folder, and removes the third.
    fn project(tag: &str) -> (PathBuf, Vec<Write>, Vec<PathBuf>) {
        let dir =
            std::env::temp_dir().join(format!("flusso-design-commit-{}-{tag}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("flusso.toml"), "config\n").unwrap();
        std::fs::write(dir.join("users.schema.yml"), "users\n").unwrap();
        std::fs::write(dir.join("orders.schema.yml"), "orders\n").unwrap();
        let writes = vec![
            Write {
                dest: dir.join("users.schema.yml"),
                text: "users 2\n".into(),
            },
            Write {
                dest: dir.join("new/deep/people.schema.yml"),
                text: "people\n".into(),
            },
            Write {
                dest: dir.join("flusso.toml"),
                text: "config 2\n".into(),
            },
        ];
        (dir.clone(), writes, vec![dir.join("orders.schema.yml")])
    }

    #[test]
    fn a_commit_writes_removes_and_leaves_nothing_behind() {
        let (dir, writes, removals) = project("ok");
        let mut disk = Disk::default();
        commit(&writes, &removals, &mut disk).unwrap();
        let after = snapshot(&dir);
        let names: Vec<String> = after
            .keys()
            .map(|p| p.strip_prefix(&dir).unwrap().display().to_string())
            .collect();
        assert_eq!(
            names,
            [
                "flusso.toml",
                "new",
                "new/deep",
                "new/deep/people.schema.yml",
                "users.schema.yml"
            ]
        );
        assert_eq!(
            std::fs::read_to_string(dir.join("flusso.toml")).unwrap(),
            "config 2\n"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    /// A failure at any step of the commit leaves the project byte for byte
    /// as it was: no temp file, no backup, no new folder.
    #[cfg(unix)]
    #[test]
    fn a_replaced_file_keeps_its_permissions() {
        use std::os::unix::fs::PermissionsExt;
        let (dir, writes, removals) = project("mode");
        let config = dir.join("flusso.toml");
        std::fs::set_permissions(&config, std::fs::Permissions::from_mode(0o600)).unwrap();
        commit(&writes, &removals, &mut Disk::default()).unwrap();
        let mode = std::fs::metadata(&config).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_failure_at_any_step_restores_the_project() {
        let (dir, writes, removals) = project("count");
        let mut counted = Disk::default();
        commit(&writes, &removals, &mut counted).unwrap();
        let steps = counted.steps;
        assert!(steps >= 10, "{steps}");
        std::fs::remove_dir_all(&dir).ok();

        for n in 1..=steps {
            let (dir, writes, removals) = project(&format!("fail-{n}"));
            let before = snapshot(&dir);
            let failed = commit(&writes, &removals, &mut Disk::failing(vec![n]));
            assert!(failed.is_err(), "step {n} did not fail the commit");
            assert_eq!(snapshot(&dir), before, "after a failure at step {n}");
            std::fs::remove_dir_all(&dir).ok();
        }
    }

    /// When putting a file back fails too, the error names the backup that
    /// still holds the previous text.
    #[test]
    fn a_failed_restore_names_the_backups_left() {
        let (dir, writes, removals) = project("restore");
        let mut counted = Disk::default();
        commit(&writes, &removals, &mut counted).unwrap();
        let steps = counted.steps;
        std::fs::remove_dir_all(&dir).ok();

        // Fail the last step (removing orders.schema.yml); the first undo step
        // then puts flusso.toml back, and fails too.
        let (dir, writes, removals) = project("restore-2");
        let err = commit(
            &writes,
            &removals,
            &mut Disk::failing(vec![steps, steps + 1]),
        )
        .unwrap_err()
        .to_string();
        assert!(err.contains("restoring the project failed"), "{err}");
        assert!(err.contains(".flusso.toml.flusso-save."), "{err}");
        assert!(err.contains(".bak"), "{err}");
        std::fs::remove_dir_all(&dir).ok();
    }
}
