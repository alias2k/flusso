use super::{
    ConfigPlan, bind_then_compare_clusters, lay_backoff, lay_index_budget_share,
    lay_max_pending_documents, lay_max_waiting, plan_config,
};
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

fn waiting_laid(
    table: &str,
    per_index: Option<u64>,
    total: Option<u64>,
    env: &[(&str, &str)],
) -> anyhow::Result<config::Config> {
    let mut config = with_concurrency(table);
    lay_max_waiting(&mut config, per_index, total, vars(env))?;
    Ok(config)
}

fn waiting(
    config: &config::Config,
) -> (Option<(u64, config::Origin)>, Option<(u64, config::Origin)>) {
    let pair = |value: Option<config::CountValue>| value.map(|value| (value.count, value.origin));
    (
        pair(config.concurrency.max_waiting_per_index),
        pair(config.concurrency.max_waiting),
    )
}

#[test]
fn the_waiting_bounds_take_the_flag_over_the_variable_over_the_file() {
    use config::Origin::{Env, File, Flag};
    let file = "max_waiting_per_index = 100\nmax_waiting = 400";
    assert_eq!(
        waiting(&waiting_laid(file, None, None, &[]).unwrap()),
        (Some((100, File)), Some((400, File)))
    );
    let vars = [
        ("FLUSSO_MAX_WAITING_PER_INDEX", "200"),
        ("FLUSSO_MAX_WAITING", "800"),
    ];
    assert_eq!(
        waiting(&waiting_laid(file, None, None, &vars).unwrap()),
        (Some((200, Env)), Some((800, Env)))
    );
    assert_eq!(
        waiting(&waiting_laid(file, Some(300), Some(900), &vars).unwrap()),
        (Some((300, Flag)), Some((900, Flag)))
    );
    let config = waiting_laid("", None, None, &[]).unwrap();
    assert_eq!(waiting(&config), (None, None));
    assert_eq!(
        daemon::max_waiting(&config).unwrap(),
        daemon::MaxWaiting {
            per_index: 512,
            total: 2048
        },
        "derived from the batch and the builds"
    );
}

#[test]
fn a_waiting_bound_of_zero_or_a_total_below_one_index_is_refused_naming_where_each_came_from() {
    for (table, per_index, total, env, said) in [
        (
            "max_waiting_per_index = 0",
            None,
            None,
            &[][..],
            "max_waiting_per_index = 0 (set in flusso.toml's [concurrency] max_waiting_per_index)",
        ),
        (
            "",
            None,
            Some(0),
            &[][..],
            "max_waiting = 0 (set by --max-waiting)",
        ),
        (
            "",
            None,
            None,
            &[("FLUSSO_MAX_WAITING_PER_INDEX", "0")][..],
            "max_waiting_per_index = 0 (set by FLUSSO_MAX_WAITING_PER_INDEX)",
        ),
        (
            "max_waiting = 100",
            Some(200),
            None,
            &[][..],
            "max_waiting = 100 (set in flusso.toml's [concurrency] max_waiting) is below \
             max_waiting_per_index = 200 (set by --max-waiting-per-index)",
        ),
        (
            "max_waiting_per_index = 300",
            None,
            None,
            &[("FLUSSO_MAX_WAITING", "200")][..],
            "max_waiting = 200 (set by FLUSSO_MAX_WAITING) is below max_waiting_per_index = 300 \
             (set in flusso.toml's [concurrency] max_waiting_per_index)",
        ),
    ] {
        let refused = waiting_laid(table, per_index, total, env).unwrap_err();
        assert!(refused.to_string().contains(said), "{refused}");
    }
    let refused = waiting_laid("", None, None, &[("FLUSSO_MAX_WAITING", "many")]).unwrap_err();
    assert!(
        refused
            .to_string()
            .contains("FLUSSO_MAX_WAITING: `many` is not a whole number"),
        "{refused}"
    );
}

#[test]
fn an_unset_waiting_bound_follows_the_one_set() {
    let config = waiting_laid("", Some(4096), None, &[]).unwrap();
    assert_eq!(
        daemon::max_waiting(&config).unwrap(),
        daemon::MaxWaiting {
            per_index: 4096,
            total: 4096
        },
        "max_waiting, unset, rises to the per-index bound"
    );
    let config = waiting_laid("max_waiting = 100", None, None, &[]).unwrap();
    assert_eq!(
        daemon::max_waiting(&config).unwrap(),
        daemon::MaxWaiting {
            per_index: 100,
            total: 100
        },
        "max_waiting_per_index, unset, falls to the total"
    );
    let config = waiting_laid("max_waiting = 4000", None, None, &[]).unwrap();
    assert_eq!(
        daemon::max_waiting(&config).unwrap(),
        daemon::MaxWaiting {
            per_index: 512,
            total: 4000
        },
        "each keeps its default while the other leaves room"
    );
}

fn pending_laid(
    table: &str,
    flag: Option<u64>,
    env: &[(&str, &str)],
) -> anyhow::Result<Option<(u64, config::Origin)>> {
    let mut config = with_concurrency(table);
    lay_max_pending_documents(&mut config, flag, vars(env))?;
    Ok(config
        .concurrency
        .max_pending_documents
        .map(|value| (value.count, value.origin)))
}

/// `max_pending_documents` (6b-2): the flag over the variable over the file,
/// 1,000,000 unset.
#[test]
fn max_pending_documents_takes_the_flag_over_the_variable_over_the_file() {
    use config::Origin::{Env, File, Flag};
    let file = "max_pending_documents = 5000";
    let env = [("FLUSSO_MAX_PENDING_DOCUMENTS", "6000")];
    assert_eq!(pending_laid(file, None, &[]).unwrap(), Some((5000, File)));
    assert_eq!(pending_laid(file, None, &env).unwrap(), Some((6000, Env)));
    assert_eq!(
        pending_laid(file, Some(7000), &env).unwrap(),
        Some((7000, Flag))
    );
    let config = with_concurrency("");
    assert_eq!(daemon::max_pending_documents(&config).unwrap(), 1_000_000);
}

/// 0 is refused, naming where it came from; a word is no number.
#[test]
fn max_pending_documents_of_zero_is_refused_naming_where_it_came_from() {
    for (table, flag, env, said) in [
        (
            "max_pending_documents = 0",
            None,
            &[][..],
            "max_pending_documents = 0 (set in flusso.toml's [concurrency] max_pending_documents)",
        ),
        (
            "",
            Some(0),
            &[][..],
            "max_pending_documents = 0 (set by --max-pending-documents)",
        ),
        (
            "",
            None,
            &[("FLUSSO_MAX_PENDING_DOCUMENTS", "0")][..],
            "max_pending_documents = 0 (set by FLUSSO_MAX_PENDING_DOCUMENTS)",
        ),
    ] {
        let refused = pending_laid(table, flag, env).unwrap_err();
        assert!(refused.to_string().contains(said), "{refused}");
    }
    let refused = pending_laid("", None, &[("FLUSSO_MAX_PENDING_DOCUMENTS", "lots")]).unwrap_err();
    assert!(
        refused
            .to_string()
            .contains("FLUSSO_MAX_PENDING_DOCUMENTS: `lots` is not a whole number"),
        "{refused}"
    );
}

/// The review of round 13, m-5: the listeners are bound before any cluster is
/// asked its `cluster_uuid`: a taken public port fails with no `GET /` sent
/// to either cluster.
#[tokio::test]
async fn a_taken_port_fails_before_any_cluster_is_asked() {
    use config::ParseFrom;
    use std::sync::atomic::{AtomicUsize, Ordering};
    let asked = std::sync::Arc::new(AtomicUsize::new(0));
    let cluster = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", cluster.local_addr().unwrap());
    let counted = std::sync::Arc::clone(&asked);
    tokio::spawn(async move {
        while let Ok((_socket, _)) = cluster.accept().await {
            counted.fetch_add(1, Ordering::SeqCst);
        }
    });
    let config = config::Config::from(
        config::toml::ConfigToml::try_parse(&format!(
            r#"
            [source]
            type = "postgres"

            [sinks.primary]
            type = "opensearch"
            url = "{url}"

            [sinks.audit]
            type = "opensearch"
            url = "{url}/other"
            "#
        ))
        .unwrap(),
    );
    let taken = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let public = taken.local_addr().unwrap();
    let private: std::net::SocketAddr = "127.0.0.1:0".parse().unwrap();
    assert!(
        bind_then_compare_clusters(&config, public, private)
            .await
            .is_err()
    );
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    assert_eq!(asked.load(Ordering::SeqCst), 0, "a cluster was asked");
}
