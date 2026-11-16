#![doc = include_str!("../README.md")]

use testcontainers_modules::postgres::Postgres;
use testcontainers_modules::testcontainers::{ContainerAsync, ContainerRequest, ImageExt};

/// A started Postgres container; the type callers name, since only this crate
/// may import `testcontainers_modules::postgres`.
pub type Container = ContainerAsync<Postgres>;

/// The environment variable that overrides [`DEFAULT_TAG`] for a whole run.
pub const TAG_VAR: &str = "FLUSSO_TEST_PG_TAG";

/// The tag of the official `postgres` image a run starts when [`TAG_VAR`] is unset.
pub const DEFAULT_TAG: &str = "16-alpine";

/// A version a test needs whatever the run's tag; each variant says why.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pinned {
    /// Postgres 10: it has no `jit` parameter, so `SET jit = off` fails (issue #335).
    NoJitParameter,
    /// Postgres 18: the first with `min`/`max` over `bytea`.
    ByteaMinMax,
}

impl Pinned {
    /// The image tag this exception starts.
    pub const fn tag(self) -> &'static str {
        match self {
            Self::NoJitParameter => "10-alpine",
            Self::ByteaMinMax => "18-alpine",
        }
    }

    /// Why this test needs its version.
    pub const fn reason(self) -> &'static str {
        match self {
            Self::NoJitParameter => {
                "Postgres 10 has no `jit` parameter, which this test needs refused"
            }
            Self::ByteaMinMax => "Postgres 18 is the first with `min`/`max` over `bytea`",
        }
    }
}

/// The tag this run starts: [`TAG_VAR`] when set and not empty, else [`DEFAULT_TAG`].
pub fn tag() -> String {
    resolve(std::env::var(TAG_VAR).ok())
}

/// The full image name this run starts, e.g. `postgres:16-alpine`.
pub fn image_name() -> String {
    format!("postgres:{}", tag())
}

/// The Postgres image this run starts, ready for `with_cmd`, `with_copy_to` or `start`.
pub fn image() -> ContainerRequest<Postgres> {
    Postgres::default().with_tag(tag())
}

/// The Postgres image of a named exception, whatever the run's tag.
///
/// Prints one line to stderr naming the image and why it ignores [`TAG_VAR`],
/// so a run on another tag says which tests did not follow it.
// A test's stderr is what the runner shows; tests install no tracing subscriber.
#[allow(clippy::print_stderr)]
pub fn pinned(version: Pinned) -> ContainerRequest<Postgres> {
    eprintln!("{}", pinned_note(version, &tag()));
    Postgres::default().with_tag(version.tag())
}

fn pinned_note(version: Pinned, run_tag: &str) -> String {
    format!(
        "test-postgres: starting postgres:{} instead of the run's postgres:{run_tag} ({TAG_VAR} ignored): {}",
        version.tag(),
        version.reason()
    )
}

fn resolve(value: Option<String>) -> String {
    value
        .map(|tag| tag.trim().to_owned())
        .filter(|tag| !tag.is_empty())
        .unwrap_or_else(|| DEFAULT_TAG.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_unset_or_empty_variable_is_the_default() {
        assert_eq!(resolve(None), DEFAULT_TAG);
        assert_eq!(resolve(Some(String::new())), DEFAULT_TAG);
        assert_eq!(resolve(Some("  ".to_owned())), DEFAULT_TAG);
    }

    #[test]
    fn a_pinned_start_names_its_image_the_run_and_the_reason() {
        let note = pinned_note(Pinned::NoJitParameter, "18-alpine");
        assert!(note.contains("postgres:10-alpine"), "{note}");
        assert!(note.contains("postgres:18-alpine"), "{note}");
        assert!(note.contains(TAG_VAR), "{note}");
        assert!(note.contains("jit"), "{note}");
    }

    #[test]
    fn a_set_variable_names_the_tag() {
        assert_eq!(resolve(Some("14-alpine".to_owned())), "14-alpine");
        assert_eq!(resolve(Some(" 18-alpine\n".to_owned())), "18-alpine");
    }
}
