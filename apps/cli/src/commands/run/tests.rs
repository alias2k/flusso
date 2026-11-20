use super::{ConfigPlan, lay_backoff, lay_index_budget_share, plan_config};
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

/// A lookup of only the given variables.
fn vars<'a>(set: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
    move |name| {
        set.iter()
            .find(|(var, _)| *var == name)
            .map(|(_, value)| (*value).to_owned())
    }
}

fn laid(
    table: &str,
    initial: Option<std::num::NonZeroU64>,
    max: Option<std::num::NonZeroU64>,
    env: &[(&str, &str)],
) -> anyhow::Result<config::Config> {
    let mut config = with_backoff(table);
    lay_backoff(&mut config, initial, max, vars(env))?;
    Ok(config)
}

fn origin(value: Option<config::BackoffValue>) -> Option<(u64, config::Origin)> {
    value.map(|value| (value.ms.get(), value.origin))
}

#[test]
fn each_backoff_key_takes_the_flag_over_the_variable_over_the_file() {
    use config::Origin::{Env, File, Flag};
    let file = "initial_ms = 200\nmax_ms = 800";
    let both_vars = [
        ("FLUSSO_BACKOFF_INITIAL_MS", "300"),
        ("FLUSSO_BACKOFF_MAX_MS", "900"),
    ];

    let config = laid(file, None, None, &[]).unwrap();
    assert_eq!(origin(config.backoff.initial_ms), Some((200, File)));
    assert_eq!(origin(config.backoff.max_ms), Some((800, File)));

    let config = laid(file, None, None, &both_vars).unwrap();
    assert_eq!(origin(config.backoff.initial_ms), Some((300, Env)));
    assert_eq!(origin(config.backoff.max_ms), Some((900, Env)));

    let config = laid(file, ms(50), ms(400), &both_vars).unwrap();
    assert_eq!(origin(config.backoff.initial_ms), Some((50, Flag)));
    assert_eq!(origin(config.backoff.max_ms), Some((400, Flag)));

    let config = laid("", None, None, &[]).unwrap();
    assert_eq!(
        (config.backoff.initial_ms, config.backoff.max_ms),
        (None, None)
    );
    assert_eq!(
        daemon::backoff(&config).unwrap(),
        engine_backoff(1_000, 60_000)
    );

    let config = laid("", None, None, &[("FLUSSO_BACKOFF_MAX_MS", "")]).unwrap();
    assert_eq!(config.backoff.max_ms, None, "an empty variable is unset");
}

/// `--backoff-initial-ms 90000` with nothing else is valid: the cap rises
/// with it.
#[test]
fn raising_the_first_wait_alone_is_valid_from_any_source() {
    for config in [
        laid("initial_ms = 90000", None, None, &[]).unwrap(),
        laid("", ms(90_000), None, &[]).unwrap(),
        laid("", None, None, &[("FLUSSO_BACKOFF_INITIAL_MS", "90000")]).unwrap(),
    ] {
        assert_eq!(
            daemon::backoff(&config).unwrap(),
            engine_backoff(90_000, 90_000)
        );
    }
}

#[test]
fn a_backoff_cap_below_its_first_wait_is_refused_naming_where_each_came_from() {
    let error = laid("initial_ms = 200", None, ms(100), &[])
        .unwrap_err()
        .to_string();
    assert!(
        error.contains(
            "max_ms = 100 (set by --backoff-max-ms) is below initial_ms = 200 \
             (set in flusso.toml's [backoff] initial_ms)"
        ),
        "{error}"
    );
    let error = laid("", None, None, &[("FLUSSO_BACKOFF_MAX_MS", "100")])
        .unwrap_err()
        .to_string();
    assert!(
        error.contains(
            "max_ms = 100 (set by FLUSSO_BACKOFF_MAX_MS) is below initial_ms = 1000 (the default)"
        ),
        "{error}"
    );
    let error = laid("", None, None, &[("FLUSSO_BACKOFF_MAX_MS", "soon")])
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("FLUSSO_BACKOFF_MAX_MS: `soon` is not a whole number of milliseconds"),
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

fn with_concurrency(table: &str) -> config::Config {
    let toml = format!("[source]\ntype = \"postgres\"\n\n[concurrency]\n{table}\n");
    config::Config::from(
        <config::toml::ConfigToml as config::ParseFrom<_>>::try_parse(toml).unwrap(),
    )
}

fn share_laid(
    table: &str,
    flag: Option<f64>,
    env: &[(&str, &str)],
) -> anyhow::Result<config::Config> {
    let mut config = with_concurrency(table);
    lay_index_budget_share(&mut config, flag, vars(env))?;
    Ok(config)
}

fn share(config: &config::Config) -> Option<(f64, config::Origin)> {
    config
        .concurrency
        .index_budget_share
        .map(|value| (value.share, value.origin))
}

#[test]
fn the_index_budget_share_takes_the_flag_over_the_variable_over_the_file() {
    use config::Origin::{Env, File, Flag};
    let file = "index_budget_share = 0.25";
    let var = [("FLUSSO_INDEX_BUDGET_SHARE", "0.4")];
    assert_eq!(
        share(&share_laid(file, None, &[]).unwrap()),
        Some((0.25, File))
    );
    assert_eq!(
        share(&share_laid(file, None, &var).unwrap()),
        Some((0.4, Env))
    );
    assert_eq!(
        share(&share_laid(file, Some(0.75), &var).unwrap()),
        Some((0.75, Flag))
    );
    let config = share_laid("", None, &[]).unwrap();
    assert_eq!(share(&config), None);
    assert_eq!(daemon::index_budget_share(&config).unwrap(), 0.5);
    assert_eq!(
        share(&share_laid("index_budget_share = 1", None, &[]).unwrap()),
        Some((1.0, File)),
        "a whole 1 is the whole budget"
    );
    assert_eq!(
        share(&share_laid("", None, &[("FLUSSO_INDEX_BUDGET_SHARE", "")]).unwrap()),
        None,
        "an empty variable is unset"
    );
}

#[test]
fn an_index_budget_share_outside_zero_to_one_is_refused_naming_where_it_came_from() {
    for (table, flag, env, said) in [
        (
            "index_budget_share = 0",
            None,
            &[][..],
            "index_budget_share = 0 (set in flusso.toml's [concurrency] index_budget_share)",
        ),
        (
            "",
            Some(1.5),
            &[][..],
            "index_budget_share = 1.5 (set by --index-budget-share)",
        ),
        (
            "",
            None,
            &[("FLUSSO_INDEX_BUDGET_SHARE", "-0.5")][..],
            "index_budget_share = -0.5 (set by FLUSSO_INDEX_BUDGET_SHARE)",
        ),
        (
            "index_budget_share = nan",
            None,
            &[][..],
            "index_budget_share = NaN (set in flusso.toml's [concurrency] index_budget_share)",
        ),
    ] {
        let error = share_laid(table, flag, env).unwrap_err().to_string();
        assert!(error.contains(said), "{error}");
        assert!(error.contains("is outside (0, 1]"), "{error}");
    }
    let error = share_laid("", None, &[("FLUSSO_INDEX_BUDGET_SHARE", "half")])
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("FLUSSO_INDEX_BUDGET_SHARE: `half` is not a number"),
        "{error}"
    );
}
