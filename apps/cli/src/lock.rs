//! Where a command's config comes from, and whether the `flusso.lock` beside
//! it is the one that config compiles to.
//!
//! `run` and `check` decide alike which file they read ([`plan_config`]):
//! a `flusso.toml` when one is there (or named), else the compiled lock.
//! `check` then compares the lock with what the config compiles to
//! ([`LockState::of`]), byte for byte, as `run` does before it rewrites it: a
//! lock that holds another config is stale, and `run --locked`, or an image
//! baked from it, would run the older config. `check --fix` writes it again
//! ([`LockState::fixed`]) through `flusso build`'s own write.
//!
//! ```text
//! ✓ flusso.lock is current
//! [error] flusso.lock  lock_stale: `flusso.lock` was compiled from another config …
//! ```

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use anyhow::Context;
use config::Compiled;

/// What a command does to obtain its [`Config`](config::Config), decided
/// purely from the flags plus whether the candidate config file exists, so
/// the branching is unit-testable without a filesystem.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum ConfigPlan {
    /// `--locked`: load the existing lock as-is, no compile, no write.
    UseLock,
    /// Compile this config path.
    Compile(PathBuf),
    /// No config to compile: fall back to loading the existing lock.
    Fallback,
    /// An explicit `--config` was given but does not exist — fatal.
    Missing(PathBuf),
}

/// Decide the [`ConfigPlan`] cargo-style: `--locked` wins; otherwise an
/// explicit `--config` is compiled when it exists and is an error when it
/// doesn't, while a missing default `flusso.toml` is allowed and falls back
/// to the lock.
pub(crate) fn plan_config(
    locked: bool,
    config: Option<&Path>,
    default_config: &Path,
    exists: impl Fn(&Path) -> bool,
) -> ConfigPlan {
    if locked {
        return ConfigPlan::UseLock;
    }
    match config {
        Some(path) if exists(path) => ConfigPlan::Compile(path.to_path_buf()),
        Some(path) => ConfigPlan::Missing(path.to_path_buf()),
        None if exists(default_config) => ConfigPlan::Compile(default_config.to_path_buf()),
        None => ConfigPlan::Fallback,
    }
}

/// How the lock at a path stands against the config.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum LockState {
    /// It holds exactly what the config compiles to.
    Current,
    /// It holds the same config in other bytes (line endings, formatting):
    /// `run --locked` runs this config, and `run` would rewrite it.
    Respelled,
    /// It holds another config; `differs` names what, at the top level.
    Stale { differs: Vec<String> },
    /// No file there: `run` with the config writes one.
    Missing,
    /// It can't be read as a lock; `reason` says why.
    Unreadable { reason: String },
    /// There is no config to compare it with: the lock is what is checked.
    NotCompared,
    /// `check --fix` wrote what the config compiles to over a lock that was
    /// `was`.
    Rewritten { was: Box<LockState> },
}

impl LockState {
    /// Compare the lock at `path` with `compiled`, the config compiled as
    /// `run` compiles it. Reads, never writes.
    pub(crate) fn of(compiled: &Compiled, path: &Path) -> Self {
        let existing = match std::fs::read(path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Self::Missing,
            Err(error) => {
                return Self::Unreadable {
                    reason: error.to_string(),
                };
            }
        };
        match config::to_bytes(compiled) {
            Ok(expected) if expected == existing => return Self::Current,
            Ok(_) => {}
            Err(error) => {
                return Self::Unreadable {
                    reason: format!("the config does not compile to a lock: {error}"),
                };
            }
        }
        match config::from_bytes(&existing) {
            Ok(old) if same(&old, &compiled.config) => Self::Respelled,
            Ok(old) => Self::Stale {
                differs: differences(&old, &compiled.config),
            },
            Err(config::CompileError::VersionMismatch { got, expected }) => Self::Unreadable {
                reason: format!("it is format {got}, and this flusso reads format {expected}"),
            },
            Err(config::CompileError::LegacyFormat) => Self::Unreadable {
                reason: "it is not a text lock (the binary format older flusso wrote)".to_owned(),
            },
            Err(error) => Self::Unreadable {
                reason: error.to_string(),
            },
        }
    }

    /// Write what the config compiles to over this lock when the config can
    /// repair it (stale, unreadable, respelled, missing), through the write
    /// `flusso build` uses: a temporary file beside it, renamed over it. A
    /// lock already current, or one with no config to write from, stays.
    pub(crate) fn fixed(self, compiled: &Compiled, path: &Path) -> anyhow::Result<Self> {
        match self {
            Self::Current | Self::NotCompared | Self::Rewritten { .. } => Ok(self),
            was => {
                config::write(compiled, path)
                    .with_context(|| format!("`check --fix` could not write {}", path.display()))?;
                Ok(Self::Rewritten { was: Box::new(was) })
            }
        }
    }

    /// Whether `check` fails on it.
    pub(crate) fn is_error(&self) -> bool {
        matches!(self, Self::Stale { .. } | Self::Unreadable { .. })
    }

    /// Its stable `snake_case` code.
    pub(crate) fn as_str(&self) -> &'static str {
        match self {
            Self::Current => "current",
            Self::Respelled => "same_config",
            Self::Stale { .. } => "lock_stale",
            Self::Missing => "missing",
            Self::Unreadable { .. } => "lock_unreadable",
            Self::NotCompared => "not_compared",
            Self::Rewritten { was } => was.as_str(),
        }
    }

    /// What it means, in a sentence, for the lock at `path`.
    pub(crate) fn message(&self, path: &Path) -> String {
        let lock = path.display();
        match self {
            Self::Current => format!("{lock} is current"),
            Self::Rewritten { .. } => format!(
                "{} rewritten: {lock}",
                path.file_name().map_or_else(
                    || lock.to_string(),
                    |name| name.to_string_lossy().into_owned()
                )
            ),
            Self::Respelled => format!(
                "{lock} holds this config in other bytes than flusso writes (line endings or \
                 formatting); `flusso run` would rewrite it"
            ),
            Self::Missing if is_default(path) => {
                format!("{lock} not found; flusso build writes it")
            }
            Self::Missing => format!("{lock} not found; `{}` writes it", build(path)),
            Self::NotCompared => {
                format!("no config to compare {lock} with: the lock is what was checked")
            }
            Self::Stale { differs } => {
                let what = if differs.is_empty() {
                    String::new()
                } else {
                    format!(" ({})", differs.join("; "))
                };
                format!(
                    "`{lock}` was compiled from another config than this one{what}: `flusso run \
                     --locked`, or an image baked from this lock, would run the older config. {} \
                     and commit the result",
                    rewrite(path)
                )
            }
            Self::Unreadable { reason } => {
                format!(
                    "`{lock}` can't be read as a lock, so `flusso run --locked` would fail on it: \
                     {reason}. {} to regenerate it",
                    rewrite(path)
                )
            }
        }
    }
}

/// Whether `path` is the lock `build` and `run` write when not told another.
fn is_default(path: &Path) -> bool {
    path == Path::new(crate::DEFAULT_LOCK)
}

/// The `build` that writes the lock at `path`.
fn build(path: &Path) -> String {
    if is_default(path) {
        "flusso build".to_owned()
    } else {
        format!("flusso build --out {}", path.display())
    }
}

/// How to write the lock at `path` again: `check --fix`, `build` or `run`.
fn rewrite(path: &Path) -> String {
    let (fix, run) = if is_default(path) {
        ("flusso check --fix".to_owned(), "flusso run".to_owned())
    } else {
        (
            format!("flusso check --fix --lock {}", path.display()),
            format!("flusso run --lock {}", path.display()),
        )
    };
    format!(
        "Run `{fix}` or `{}` (or `{run}`, which rewrites it)",
        build(path)
    )
}

/// Whether the config a lock holds is the config now, value for value.
fn same(old: &config::Config, new: &config::Config) -> bool {
    match (serde_json::to_value(old), serde_json::to_value(new)) {
        (Ok(old), Ok(new)) => old == new,
        _ => false,
    }
}

/// What differs between the config a lock holds and the config now, at the
/// top level: each index's schema (added, removed, changed), then the
/// deployment settings by key.
fn differences(old: &config::Config, new: &config::Config) -> Vec<String> {
    let (Ok(old), Ok(new)) = (serde_json::to_value(old), serde_json::to_value(new)) else {
        return Vec::new();
    };
    let (Some(old), Some(new)) = (old.as_object(), new.as_object()) else {
        return Vec::new();
    };
    let indexes = |value: Option<&serde_json::Value>| {
        value
            .and_then(serde_json::Value::as_object)
            .cloned()
            .unwrap_or_default()
    };
    let (old_indexes, new_indexes) = (indexes(old.get("indexes")), indexes(new.get("indexes")));
    let listed = |names: Vec<&String>| {
        names
            .iter()
            .map(|name| format!("`{name}`"))
            .collect::<Vec<_>>()
            .join(", ")
    };
    let mut differs = Vec::new();
    let added: Vec<&String> = new_indexes
        .keys()
        .filter(|name| !old_indexes.contains_key(*name))
        .collect();
    let removed: Vec<&String> = old_indexes
        .keys()
        .filter(|name| !new_indexes.contains_key(*name))
        .collect();
    let changed: Vec<&String> = new_indexes
        .iter()
        .filter(|(name, index)| old_indexes.get(*name).is_some_and(|old| old != *index))
        .map(|(name, _)| name)
        .collect();
    if !changed.is_empty() {
        differs.push(format!("the index {} changed", listed(changed)));
    }
    if !added.is_empty() {
        differs.push(format!("the index {} is new", listed(added)));
    }
    if !removed.is_empty() {
        differs.push(format!("the index {} is gone", listed(removed)));
    }
    let keys: BTreeSet<&String> = old.keys().chain(new.keys()).collect();
    let settings: Vec<String> = keys
        .into_iter()
        .filter(|key| *key != "indexes" && old.get(*key) != new.get(*key))
        .map(|key| format!("`{key}`"))
        .collect();
    if !settings.is_empty() {
        differs.push(format!("the deployment settings {}", settings.join(", ")));
    }
    differs
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    /// A project folder of its own: a config with the indexes `names`, each
    /// over a table of its name, and a stdout sink.
    fn project(name: &str, indexes: &[&str], sink: &str) -> PathBuf {
        let dir = std::env::temp_dir()
            .join("flusso-lock-tests")
            .join(format!("{name}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut toml =
            format!("[source]\ntype = \"postgres\"\n\n[sinks.{sink}]\ntype = \"stdout\"\n");
        for index in indexes {
            std::fs::write(
                dir.join(format!("{index}.schema.yml")),
                format!(
                    "version: 1\ntable: {index}\nprimary_key: id\nfields:\n  - integer: id\n    required: true\n"
                ),
            )
            .unwrap();
            toml.push_str(&format!(
                "\n[[index]]\nname = \"{index}\"\nschema = \"{index}.schema.yml\"\nenabled = true\n"
            ));
        }
        std::fs::write(dir.join("flusso.toml"), toml).unwrap();
        dir
    }

    fn compiled(dir: &Path) -> Compiled {
        config::compile(dir.join("flusso.toml")).unwrap()
    }

    #[test]
    fn a_lock_written_from_the_config_is_current() {
        let dir = project("current", &["users"], "out");
        config::write(&compiled(&dir), dir.join("flusso.lock")).unwrap();
        let state = LockState::of(&compiled(&dir), &dir.join("flusso.lock"));
        assert_eq!(state, LockState::Current);
        assert!(!state.is_error());
    }

    #[test]
    fn a_lock_of_another_config_is_stale_and_says_what_differs() {
        let dir = project("stale", &["users", "orders"], "out");
        config::write(&compiled(&dir), dir.join("flusso.lock")).unwrap();
        let now = project("stale", &["users", "items"], "printed");
        std::fs::write(
            now.join("users.schema.yml"),
            "version: 1\ntable: users\nprimary_key: id\nfields:\n  - integer: id\n    required: true\n  - keyword: name\n    required: false\n",
        )
        .unwrap();
        let state = LockState::of(&compiled(&now), &now.join("flusso.lock"));
        let LockState::Stale { differs } = &state else {
            panic!("{state:?}");
        };
        assert_eq!(
            differs,
            &[
                "the index `users` changed".to_owned(),
                "the index `items` is new".to_owned(),
                "the index `orders` is gone".to_owned(),
                "the deployment settings `sinks`".to_owned(),
            ]
        );
        assert!(state.is_error());
        assert_eq!(state.as_str(), "lock_stale");
        let message = state.message(Path::new("flusso.lock"));
        assert!(message.contains("would run the older config"), "{message}");
        assert!(
            message.contains("Run `flusso check --fix` or `flusso build`"),
            "{message}"
        );
    }

    #[test]
    fn no_lock_is_missing_and_no_error() {
        let dir = project("missing", &["users"], "out");
        let state = LockState::of(&compiled(&dir), &dir.join("flusso.lock"));
        assert_eq!(state, LockState::Missing);
        assert!(!state.is_error());
        assert_eq!(
            state.message(Path::new("flusso.lock")),
            "flusso.lock not found; flusso build writes it"
        );
    }

    #[test]
    fn a_lock_of_an_older_format_or_garbage_is_unreadable_with_the_regenerate_hint() {
        let dir = project("unreadable", &["users"], "out");
        for (written, says) in [
            (&b"format_version = 2\n[config]\n"[..], "format 2"),
            (&b"\xff\x00binary"[..], "not a text lock"),
            (&b"this is = not [a lock"[..], "line 1"),
        ] {
            std::fs::write(dir.join("flusso.lock"), written).unwrap();
            let state = LockState::of(&compiled(&dir), &dir.join("flusso.lock"));
            assert!(
                matches!(&state, LockState::Unreadable { reason } if reason.contains(says)),
                "{state:?}"
            );
            assert!(state.is_error());
            let message = state.message(Path::new("flusso.lock"));
            assert!(
                message.contains(
                    "Run `flusso check --fix` or `flusso build` (or `flusso run`, which rewrites \
                     it)"
                ),
                "{message}"
            );
        }
    }

    /// A lock holding this config in other bytes (CRLF line endings from a
    /// Windows checkout, another formatting) runs exactly this config, so it
    /// is no error: a note that `run` would rewrite it (review 3 of #486).
    #[test]
    fn a_lock_with_crlf_line_endings_holds_the_same_config_and_is_no_error() {
        let dir = project("crlf", &["users"], "out");
        let bytes = config::to_bytes(&compiled(&dir)).unwrap();
        let crlf = String::from_utf8(bytes).unwrap().replace('\n', "\r\n");
        std::fs::write(dir.join("flusso.lock"), crlf).unwrap();
        let state = LockState::of(&compiled(&dir), &dir.join("flusso.lock"));
        assert_eq!(state, LockState::Respelled);
        assert!(!state.is_error());
        assert_eq!(state.as_str(), "same_config");
        assert_eq!(
            state.message(Path::new("flusso.lock")),
            "flusso.lock holds this config in other bytes than flusso writes (line endings or \
             formatting); `flusso run` would rewrite it"
        );
    }

    /// A lock that fails to parse names where (line and column) and why, never
    /// the text there: a literal secret may sit on that line.
    #[test]
    fn an_unreadable_lock_never_echoes_its_content() {
        let dir = project("echo", &["users"], "out");
        let mut bytes = config::to_bytes(&compiled(&dir)).unwrap();
        bytes.extend_from_slice(b"\n[config.sinks.out.extra]\npassword = \"OsSecretBBB\" typo\n");
        std::fs::write(dir.join("flusso.lock"), &bytes).unwrap();
        let state = LockState::of(&compiled(&dir), &dir.join("flusso.lock"));
        let LockState::Unreadable { reason } = &state else {
            panic!("{state:?}");
        };
        let message = state.message(Path::new("flusso.lock"));
        for said in [reason, &message] {
            assert!(!said.contains("OsSecretBBB"), "{said}");
            assert!(!said.contains("typo"), "{said}");
            assert!(said.contains("line "), "{said}");
        }
        let loaded = config::load_compiled(dir.join("flusso.lock"))
            .unwrap_err()
            .to_string();
        assert!(!loaded.contains("OsSecretBBB"), "{loaded}");
        assert!(loaded.contains("line "), "{loaded}");
    }

    /// The advice names the command that writes the lock `check` compared:
    /// `flusso build` writes `--out` (default `flusso.lock`), `flusso run`
    /// writes `--lock`.
    #[test]
    fn the_advice_names_a_lock_path_other_than_the_default() {
        let elsewhere = Path::new("deploy/flusso.lock");
        let stale = LockState::Stale {
            differs: Vec::new(),
        };
        let unreadable = LockState::Unreadable {
            reason: "line 1, column 1: invalid key".to_owned(),
        };
        for state in [&stale, &unreadable] {
            let message = state.message(elsewhere);
            assert!(
                message.contains(
                    "Run `flusso check --fix --lock deploy/flusso.lock` or `flusso build --out \
                     deploy/flusso.lock` (or `flusso run --lock deploy/flusso.lock`, which \
                     rewrites it)"
                ),
                "{message}"
            );
            assert!(state.message(Path::new(crate::DEFAULT_LOCK)).contains(
                "Run `flusso check --fix` or `flusso build` (or `flusso run`, which \
                         rewrites it)"
            ));
        }
        assert_eq!(
            LockState::Missing.message(elsewhere),
            "deploy/flusso.lock not found; `flusso build --out deploy/flusso.lock` writes it"
        );
    }

    /// A lock whose config differs by a value of the same length is stale:
    /// the bytes are compared, not their count.
    #[test]
    fn a_lock_differing_by_a_value_of_the_same_length_is_stale() {
        let dir = project("same-length", &["users"], "out");
        config::write(&compiled(&dir), dir.join("flusso.lock")).unwrap();
        let before = std::fs::read(dir.join("flusso.lock")).unwrap();
        let now = project("same-length", &["users"], "abc");
        let after = config::to_bytes(&compiled(&now)).unwrap();
        assert_eq!(before.len(), after.len());
        let state = LockState::of(&compiled(&now), &now.join("flusso.lock"));
        assert_eq!(
            state,
            LockState::Stale {
                differs: vec!["the deployment settings `sinks`".to_owned()],
            }
        );
    }

    /// `check --fix` writes what `flusso build` writes over every lock the
    /// config can repair (stale, missing, CRLF, unreadable), and a plain
    /// `check` then finds it current.
    #[test]
    fn fix_rewrites_every_lock_the_config_can_repair() {
        let dir = project("fix", &["users"], "out");
        let lock = dir.join("flusso.lock");
        let expected = config::to_bytes(&compiled(&dir)).unwrap();
        let crlf = String::from_utf8(expected.clone())
            .unwrap()
            .replace('\n', "\r\n");
        let older = project("fix-older", &["users", "orders"], "out");
        let stale = config::to_bytes(&compiled(&older)).unwrap();
        for (case, written) in [
            ("stale", Some(stale)),
            ("missing", None),
            ("crlf", Some(crlf.into_bytes())),
            ("unreadable", Some(b"format_version = 2\n".to_vec())),
        ] {
            let _ = std::fs::remove_file(&lock);
            if let Some(bytes) = written {
                std::fs::write(&lock, bytes).unwrap();
            }
            let before = LockState::of(&compiled(&dir), &lock);
            let after = before.clone().fixed(&compiled(&dir), &lock).unwrap();
            assert_eq!(
                after,
                LockState::Rewritten {
                    was: Box::new(before)
                },
                "{case}"
            );
            assert!(!after.is_error(), "{case}");
            assert_eq!(std::fs::read(&lock).unwrap(), expected, "{case}");
            assert_eq!(
                LockState::of(&compiled(&dir), &lock),
                LockState::Current,
                "{case}"
            );
            assert_eq!(
                after.message(&lock),
                format!("flusso.lock rewritten: {}", lock.display()),
                "{case}"
            );
        }
    }

    /// A current lock, and a lock with no config to write it from, are left
    /// as they are by `--fix`.
    #[test]
    fn fix_leaves_a_current_lock_and_a_lock_alone_as_they_are() {
        let dir = project("fix-current", &["users"], "out");
        let lock = dir.join("flusso.lock");
        config::write(&compiled(&dir), &lock).unwrap();
        let modified = std::fs::metadata(&lock).unwrap().modified().unwrap();
        for state in [LockState::Current, LockState::NotCompared] {
            let fixed = state.clone().fixed(&compiled(&dir), &lock).unwrap();
            assert_eq!(fixed, state);
        }
        assert_eq!(
            std::fs::metadata(&lock).unwrap().modified().unwrap(),
            modified
        );
    }

    /// A lock `--fix` can't write is an error naming the path and the cause,
    /// and the lock keeps its bytes.
    #[cfg(unix)]
    #[test]
    fn fix_that_cannot_write_names_the_path_and_leaves_the_lock_whole() {
        use std::os::unix::fs::PermissionsExt;
        let dir = project("fix-readonly", &["users"], "out");
        let lock = dir.join("flusso.lock");
        std::fs::write(&lock, b"format_version = 2\n").unwrap();
        let mode = |path: &Path, bits| {
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(bits)).unwrap();
        };
        mode(&lock, 0o444);
        mode(&dir, 0o555);
        let writable = std::fs::OpenOptions::new().write(true).open(&lock).is_ok();
        let state = LockState::of(&compiled(&dir), &lock);
        let result = state.fixed(&compiled(&dir), &lock);
        let bytes = std::fs::read(&lock).unwrap();
        mode(&dir, 0o755);
        mode(&lock, 0o644);
        if writable {
            return; // run as root: permissions don't stop a write
        }
        let error = format!("{:#}", result.unwrap_err());
        assert!(error.contains(&lock.display().to_string()), "{error}");
        assert!(error.contains("ermission denied"), "{error}");
        assert_eq!(bytes, b"format_version = 2\n");
    }
}
