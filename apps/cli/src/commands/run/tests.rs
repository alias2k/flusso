use super::{ConfigPlan, plan_config};
use std::path::{Path, PathBuf};

const DEFAULT: &str = "flusso.toml";

/// Helper: an `exists` predicate matching exactly the given paths.
fn present<'a>(paths: &'a [&'a str]) -> impl Fn(&Path) -> bool + 'a {
    move |p| paths.iter().any(|present| Path::new(present) == p)
}

#[test]
fn locked_uses_the_lock_and_ignores_the_config() {
    // Even with a config present, `--locked` short-circuits to the lock.
    let plan = plan_config(
        true,
        Some(Path::new("flusso.toml")),
        Path::new(DEFAULT),
        present(&["flusso.toml"]),
    );
    assert_eq!(plan, ConfigPlan::UseLock);
}

#[test]
fn explicit_config_present_is_compiled() {
    let plan = plan_config(
        false,
        Some(Path::new("dev/flusso.toml")),
        Path::new(DEFAULT),
        present(&["dev/flusso.toml"]),
    );
    assert_eq!(plan, ConfigPlan::Compile(PathBuf::from("dev/flusso.toml")));
}

#[test]
fn explicit_config_missing_is_fatal() {
    let plan = plan_config(
        false,
        Some(Path::new("nope.toml")),
        Path::new(DEFAULT),
        present(&[]),
    );
    assert_eq!(plan, ConfigPlan::Missing(PathBuf::from("nope.toml")));
}

#[test]
fn default_config_present_is_compiled() {
    let plan = plan_config(false, None, Path::new(DEFAULT), present(&[DEFAULT]));
    assert_eq!(plan, ConfigPlan::Compile(PathBuf::from(DEFAULT)));
}

#[test]
fn default_config_absent_falls_back_to_the_lock() {
    let plan = plan_config(false, None, Path::new(DEFAULT), present(&[]));
    assert_eq!(plan, ConfigPlan::Fallback);
}
