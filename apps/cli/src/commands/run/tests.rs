use super::{ConfigPlan, lay_backoff, plan_config};
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

fn with_backoff(table: &str) -> config::Config {
    let toml = format!("[source]\ntype = \"postgres\"\n\n[backoff]\n{table}\n");
    config::Config::from(
        <config::toml::ConfigToml as config::ParseFrom<_>>::try_parse(toml).unwrap(),
    )
}

fn ms(value: u64) -> Option<std::num::NonZeroU64> {
    std::num::NonZeroU64::new(value)
}

#[test]
fn a_backoff_flag_wins_over_the_file_and_the_file_over_the_default() {
    let mut config = with_backoff("initial_ms = 200\nmax_ms = 800");
    lay_backoff(&mut config, None, None).unwrap();
    assert_eq!(
        daemon::backoff(&config).unwrap(),
        engine_backoff(200, 800),
        "the file over the default"
    );
    lay_backoff(&mut config, ms(50), None).unwrap();
    assert_eq!(daemon::backoff(&config).unwrap(), engine_backoff(50, 800));
    lay_backoff(&mut config, None, ms(50)).unwrap();
    assert_eq!(
        daemon::backoff(&config).unwrap(),
        engine_backoff(50, 50),
        "equal values: a constant interval"
    );

    let mut config = with_backoff("");
    lay_backoff(&mut config, None, None).unwrap();
    assert_eq!(
        daemon::backoff(&config).unwrap(),
        engine_backoff(1_000, 60_000)
    );
}

#[test]
fn a_backoff_cap_below_its_first_wait_is_refused_naming_both_keys() {
    let mut config = with_backoff("initial_ms = 200");
    let error = lay_backoff(&mut config, None, ms(100))
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("max_ms = 100 is below initial_ms = 200"),
        "{error}"
    );
    let mut config = with_backoff("");
    let error = lay_backoff(&mut config, ms(90_000), None)
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("max_ms = 60000 is below initial_ms = 90000"),
        "{error}"
    );
}

fn engine_backoff(initial_ms: u64, max_ms: u64) -> daemon::Backoff {
    daemon::Backoff::new(
        std::time::Duration::from_millis(initial_ms),
        std::time::Duration::from_millis(max_ms),
    )
    .unwrap()
}
