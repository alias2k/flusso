//! Resolving a path the way the filesystem does, shared by the save's
//! checks (`api`) and the recovery of an interrupted save (`commit`), so both
//! agree on where a path leads.

use std::path::{Component, Path, PathBuf};

use anyhow::{Result, bail};

/// The longest path, in bytes, a save takes or a journal names.
pub(crate) const MAX_PATH: usize = 4096;

/// `path` (absolute) as the filesystem resolves it: its deepest existing
/// ancestor canonicalized (symlinks and `..` followed), then the rest
/// appended. A path that doesn't exist yet still resolves through the links
/// above it, so `sub/x.yml` with `sub -> .` is `x.yml`. Refused: a broken
/// link, a `..` in the part that doesn't exist (there is nothing to go up
/// from), and a path through a file.
///
/// Takes a number of `canonicalize` calls logarithmic in the number of
/// components: a prefix resolves only when the one before it does, so the
/// deepest one that resolves is found by bisection.
pub(crate) fn on_disk(path: &Path) -> Result<PathBuf> {
    let parts: Vec<Component<'_>> = path.components().collect();
    let prefix = |n: usize| -> PathBuf { parts.iter().take(n).collect() };
    let resolves = |n: usize| std::fs::canonicalize(prefix(n)).ok();

    let (mut deepest, mut real) = match resolves(parts.len()) {
        Some(real) => return Ok(real),
        None => match resolves(1) {
            Some(real) => (1, real),
            None => bail!("{} cannot be resolved", path.display()),
        },
    };
    // `deepest` resolves and `missing` does not.
    let mut missing = parts.len();
    while missing - deepest > 1 {
        let mid = deepest + (missing - deepest) / 2;
        match resolves(mid) {
            Some(found) => {
                deepest = mid;
                real = found;
            }
            None => missing = mid,
        }
    }

    let next = prefix(deepest + 1);
    if next.symlink_metadata().is_ok() {
        if !real.is_dir() {
            bail!(
                "{} goes through {}, which is a file",
                path.display(),
                real.display()
            );
        }
        if let Err(e) = std::fs::canonicalize(&next) {
            bail!("{} cannot be resolved: {e}", next.display());
        }
    }
    if !real.is_dir() {
        bail!(
            "{} goes through {}, which is a file",
            path.display(),
            real.display()
        );
    }
    for part in parts.iter().skip(deepest) {
        match part {
            Component::Normal(name) => real.push(name),
            Component::CurDir => {}
            _ => bail!(
                "{} goes up from a folder that does not exist",
                path.display()
            ),
        }
    }
    Ok(real)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    fn dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("flusso-fspath-{}-{tag}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::canonicalize(&dir).unwrap()
    }

    #[test]
    fn a_missing_tail_is_appended_to_the_deepest_existing_folder() {
        let base = dir("tail");
        std::fs::create_dir_all(base.join("a/b")).unwrap();
        assert_eq!(
            on_disk(&base.join("a/b/c/d.yml")).unwrap(),
            base.join("a/b/c/d.yml")
        );
        assert_eq!(on_disk(&base.join("a/./b/x")).unwrap(), base.join("a/b/x"));
        assert!(on_disk(&base.join("a/nope/../x")).is_err());
        std::fs::write(base.join("f"), "").unwrap();
        assert!(on_disk(&base.join("f/x")).is_err());
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(&base, base.join("a/up")).unwrap();
            assert_eq!(
                on_disk(&base.join("a/up/x.yml")).unwrap(),
                base.join("x.yml")
            );
            std::os::unix::fs::symlink(base.join("gone"), base.join("broken")).unwrap();
            assert!(on_disk(&base.join("broken/x")).is_err());
        }
        std::fs::remove_dir_all(&base).unwrap();
    }

    /// The reviewer's case: a path of many components took minutes when
    /// every prefix was canonicalized.
    #[test]
    fn a_path_of_twenty_thousand_components_resolves_at_once() {
        let base = dir("long");
        let long = base.join("a/".repeat(20_000) + "x.yml");
        let started = std::time::Instant::now();
        let resolved = on_disk(&long).unwrap();
        assert!(
            started.elapsed() < std::time::Duration::from_millis(500),
            "{:?}",
            started.elapsed()
        );
        assert_eq!(resolved, long);
        std::fs::remove_dir_all(&base).unwrap();
    }
}
