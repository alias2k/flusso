//! Scenario sizes. A scale is part of every series name, so a CI-sized run is
//! never compared to a default-sized one.

use serde::Serialize;

use crate::scenario::ScenarioName;

/// Which preset to run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum, Serialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum ScaleName {
    /// Fits the CI time budget on a shared runner.
    Ci,
    /// The size to compare release to release, locally.
    Default,
}

impl ScaleName {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Ci => "ci",
            Self::Default => "default",
        }
    }
}

/// Every knob a scenario reads. Fields a scenario doesn't use stay at zero.
#[derive(Debug, Clone, Serialize)]
pub(crate) struct Scale {
    pub(crate) name: ScaleName,
    pub(crate) users: i64,
    pub(crate) products: i64,
    pub(crate) orders: i64,
    pub(crate) items_per_order: i64,
    pub(crate) reviews_per_product: i64,
    pub(crate) orders_per_user: i64,
    pub(crate) tags: i64,
    pub(crate) tags_per_user: i64,
    /// Changes committed as fast as Postgres accepts them in the drain phase.
    pub(crate) burst: u64,
    /// Concurrent writer sessions in the drain phase.
    pub(crate) writers: u64,
    /// Root-row updates timed to visibility in the latency phase.
    pub(crate) probes: u64,
    /// Latency-phase pacing, changes per second — a trickle, so latency is the
    /// unloaded pipeline's.
    pub(crate) probe_rate_per_s: u64,
    /// The whole scenario must finish within this, or the run fails.
    pub(crate) wall_cap_secs: u64,
    /// The child's resident set must stay under this, or the run fails.
    pub(crate) rss_cap_mib: u64,
}

impl Scale {
    pub(crate) fn for_scenario(scenario: ScenarioName, name: ScaleName) -> Self {
        match (scenario, name) {
            (ScenarioName::Reference, ScaleName::Default) => Self {
                name,
                users: 10_000,
                products: 2_000,
                orders: 50_000,
                items_per_order: 3,
                reviews_per_product: 3,
                orders_per_user: 0,
                tags: 0,
                tags_per_user: 0,
                burst: 20_000,
                writers: 8,
                probes: 300,
                probe_rate_per_s: 20,
                wall_cap_secs: 1_800,
                rss_cap_mib: 2_048,
            },
            (ScenarioName::Reference, ScaleName::Ci) => Self {
                name,
                users: 5_000,
                products: 1_000,
                orders: 25_000,
                items_per_order: 3,
                reviews_per_product: 2,
                orders_per_user: 0,
                tags: 0,
                tags_per_user: 0,
                burst: 10_000,
                writers: 8,
                probes: 200,
                probe_rate_per_s: 20,
                wall_cap_secs: 900,
                rss_cap_mib: 2_048,
            },
            (ScenarioName::Complex, ScaleName::Default) => Self {
                name,
                users: 5_000,
                products: 0,
                orders: 0,
                items_per_order: 4,
                reviews_per_product: 0,
                orders_per_user: 5,
                tags: 8,
                tags_per_user: 4,
                burst: 10_000,
                writers: 8,
                probes: 200,
                probe_rate_per_s: 20,
                wall_cap_secs: 1_800,
                rss_cap_mib: 2_048,
            },
            (ScenarioName::Complex, ScaleName::Ci) => Self {
                name,
                users: 2_000,
                products: 0,
                orders: 0,
                items_per_order: 4,
                reviews_per_product: 0,
                orders_per_user: 5,
                tags: 8,
                tags_per_user: 4,
                burst: 4_000,
                writers: 8,
                probes: 150,
                probe_rate_per_s: 20,
                wall_cap_secs: 900,
                rss_cap_mib: 2_048,
            },
        }
    }
}
