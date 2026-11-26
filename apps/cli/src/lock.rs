//! Where a command's config comes from, and whether the `flusso.lock` beside
//! it is the one that config compiles to.
//!
//! `run` and `check` decide alike which file they read ([`plan_config`]):
//! a `flusso.toml` when one is there (or named), else the compiled lock.
//! `check` then compares the lock with what the config compiles to
//! ([`LockState::of`]), byte for byte, as `run` does before it rewrites it: a
//! lock that differs is stale, and `run --locked`, or an image baked from it,
//! would run the older config.
//!
//! ```text
//! ✓ flusso.lock is current
//! [error] flusso.lock  lock_stale: `flusso.lock` was compiled from another config …
//! ```

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

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
    /// It can't be read as a lock; `reason` carries the regenerate hint.
    Unreadable { reason: String },
    /// There is no config to compare it with: the lock is what is checked.
    NotCompared,
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
                    reason: format!("{error}; {REGENERATE}"),
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
            Err(
                error @ (config::CompileError::VersionMismatch { .. }
                | config::CompileError::LegacyFormat),
            ) => Self::Unreadable {
                reason: error.to_string(),
            },
            Err(error) => Self::Unreadable {
                reason: format!("{error}; {REGENERATE}"),
            },
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
        }
    }

    /// What it means, in a sentence, for the lock at `path`.
    pub(crate) fn message(&self, path: &Path) -> String {
        let lock = path.display();
        match self {
            Self::Current => format!("{lock} is current"),
            Self::Respelled => format!(
                "{lock} holds this config in other bytes than flusso writes (line endings or \
                 formatting); `flusso run` would rewrite it"
            ),
            Self::Missing => format!("{lock} not found; flusso build writes it"),
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
                     --locked`, or an image baked from this lock, would run the older config. \
                     Run `flusso build` (or `flusso run`, which rewrites it) and commit the result"
                )
            }
            Self::Unreadable { reason } => {
                format!(
                    "`{lock}` can't be read as a lock, so `flusso run --locked` would fail on it: \
                     {reason}"
                )
            }
        }
    }
}

const REGENERATE: &str =
    "re-run `flusso build` (or `flusso run` without `--locked`) to regenerate the lock";

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
        assert!(message.contains("Run `flusso build`"), "{message}");
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
            (&b"format_version = 2\n[config]\n"[..], "format version 2"),
            (&b"\xff\x00binary"[..], "not a text lock"),
            (&b"this is = not [a lock"[..], "flusso build"),
        ] {
            std::fs::write(dir.join("flusso.lock"), written).unwrap();
            let state = LockState::of(&compiled(&dir), &dir.join("flusso.lock"));
            assert!(
                matches!(&state, LockState::Unreadable { reason } if reason.contains(says)),
                "{state:?}"
            );
            assert!(state.is_error());
            assert!(
                state
                    .message(Path::new("flusso.lock"))
                    .contains("regenerate")
                    || state
                        .message(Path::new("flusso.lock"))
                        .contains("flusso build")
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
}
