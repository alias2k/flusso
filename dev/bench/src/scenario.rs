//! The two scenarios: their store, deterministic seed, and change mix.
//!
//! A scenario owns a directory under `scenarios/` holding the `flusso.toml`
//! the child runs with; the schema and seed SQL are embedded here. Every
//! change the writer issues is a self-contained SQL string (values inlined —
//! they come from a seeded generator, never from outside), optionally carrying
//! a [`Probe`] when it stamped a root row the latency phase can time.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicI64, Ordering};

use rand::Rng;
use rand::rngs::StdRng;

use crate::scale::Scale;

/// Which scenario to run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum ScenarioName {
    /// The dev store's three indexes: the everyday, mid-complexity shape.
    Reference,
    /// One worst-case `users` document: every assembly feature at once.
    Complex,
}

impl ScenarioName {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Reference => "reference",
            Self::Complex => "complex",
        }
    }
}

/// A root-row write the latency phase can time: the document `id` in `index`
/// carries `updatedAt == stamp` once this change is visible.
#[derive(Debug, Clone)]
pub(crate) struct Probe {
    pub(crate) index: &'static str,
    pub(crate) id: i64,
    pub(crate) stamp: String,
}

/// One change to commit.
#[derive(Debug)]
pub(crate) struct Change {
    pub(crate) sql: String,
    pub(crate) probe: Option<Probe>,
}

/// Fresh ids for inserts, shared by every writer.
#[derive(Debug)]
pub(crate) struct IdCounters {
    next_order: AtomicI64,
    next_item: AtomicI64,
    next_review: AtomicI64,
}

impl IdCounters {
    fn take(counter: &AtomicI64, n: i64) -> i64 {
        counter.fetch_add(n, Ordering::Relaxed)
    }
}

#[derive(Debug)]
pub(crate) struct Scenario {
    pub(crate) name: ScenarioName,
    pub(crate) dir: PathBuf,
    schema_sql: &'static str,
    seed_template: &'static str,
}

impl Scenario {
    pub(crate) fn new(name: ScenarioName) -> Self {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("scenarios")
            .join(name.as_str());
        match name {
            ScenarioName::Reference => Self {
                name,
                dir,
                schema_sql: include_str!("../../postgres/init/01_schema.sql"),
                seed_template: include_str!("../scenarios/reference/seed.sql"),
            },
            ScenarioName::Complex => Self {
                name,
                dir,
                schema_sql: include_str!("../scenarios/complex/schema.sql"),
                seed_template: include_str!("../scenarios/complex/seed.sql"),
            },
        }
    }

    pub(crate) fn config_path(&self) -> PathBuf {
        self.dir.join("flusso.toml")
    }

    pub(crate) fn schema_sql(&self) -> &'static str {
        self.schema_sql
    }

    pub(crate) fn seed_sql(&self, scale: &Scale) -> String {
        self.seed_template
            .replace("{users}", &scale.users.to_string())
            .replace("{products}", &scale.products.to_string())
            .replace("{orders}", &scale.orders.to_string())
            .replace("{items_per_order}", &scale.items_per_order.to_string())
            .replace(
                "{reviews_per_product}",
                &scale.reviews_per_product.to_string(),
            )
            .replace("{orders_per_user}", &scale.orders_per_user.to_string())
            .replace("{tags}", &scale.tags.to_string())
            .replace("{tags_per_user}", &scale.tags_per_user.to_string())
    }

    /// Documents the backfill seeds, across every index.
    pub(crate) fn root_documents(&self, scale: &Scale) -> u64 {
        match self.name {
            ScenarioName::Reference => (scale.users + scale.products + scale.orders) as u64,
            ScenarioName::Complex => scale.users as u64,
        }
    }

    pub(crate) fn id_counters(&self, scale: &Scale) -> IdCounters {
        IdCounters {
            next_order: AtomicI64::new(scale.orders + 1),
            next_item: AtomicI64::new(scale.orders * scale.items_per_order + 1),
            next_review: AtomicI64::new(scale.products * scale.reviews_per_product + 1),
        }
    }

    /// The next change in the scenario's mix.
    pub(crate) fn change(&self, rng: &mut StdRng, scale: &Scale, ids: &IdCounters) -> Change {
        match self.name {
            ScenarioName::Reference => reference_change(rng, scale, ids),
            ScenarioName::Complex => complex_change(rng, scale),
        }
    }
}

/// Now, at millisecond precision, as both the SQL literal and the value the
/// document will carry — OpenSearch dates are millisecond-precise, so a finer
/// stamp would never match its own document.
fn stamp() -> String {
    chrono::Utc::now()
        .format("%Y-%m-%dT%H:%M:%S%.3fZ")
        .to_string()
}

fn pick(rng: &mut StdRng, upper: i64) -> i64 {
    rng.random_range(1..=upper.max(1))
}

/// Users below this id are never soft-deleted, so a probe on one of them can
/// always become visible; the top slice is where the soft-delete toggles churn.
fn stable_users(scale: &Scale) -> i64 {
    (scale.users * 95 / 100).max(1)
}

/// 70% root updates (users 40 / products 20 / orders 40), 20% related-table
/// updates, 10% inserts and deletes. Only root updates carry a probe, and only
/// on users outside the soft-delete churn slice.
fn reference_change(rng: &mut StdRng, scale: &Scale, ids: &IdCounters) -> Change {
    let roll = rng.random_range(0..100);
    if roll < 70 {
        let stamp = stamp();
        let which = rng.random_range(0..100);
        if which < 40 {
            let id = pick(rng, stable_users(scale));
            let tier = ["free", "pro", "enterprise"]
                .get(rng.random_range(0..3usize))
                .copied()
                .unwrap_or("pro");
            Change {
                sql: format!(
                    "UPDATE users SET tier = '{tier}', updated_at = '{stamp}'::timestamptz WHERE id = {id}"
                ),
                probe: Some(Probe {
                    index: "users",
                    id,
                    stamp,
                }),
            }
        } else if which < 60 {
            let id = pick(rng, scale.products);
            let price = 10 + rng.random_range(0..990);
            let in_stock = rng.random_range(0..10) != 0;
            Change {
                sql: format!(
                    "UPDATE products SET price = {price}.99, in_stock = {in_stock}, updated_at = '{stamp}'::timestamptz WHERE id = {id}"
                ),
                probe: Some(Probe {
                    index: "products",
                    id,
                    stamp,
                }),
            }
        } else {
            let id = pick(rng, scale.orders);
            let status = ["pending", "paid", "shipped", "delivered"]
                .get(rng.random_range(0..4usize))
                .copied()
                .unwrap_or("paid");
            Change {
                sql: format!(
                    "UPDATE orders SET status = '{status}', updated_at = '{stamp}'::timestamptz WHERE id = {id}"
                ),
                probe: Some(Probe {
                    index: "orders",
                    id,
                    stamp,
                }),
            }
        }
    } else if roll < 90 {
        let which = rng.random_range(0..100);
        let sql = if which < 40 {
            let id = pick(rng, scale.orders * scale.items_per_order);
            let qty = rng.random_range(1..=5);
            format!("UPDATE order_items SET quantity = {qty} WHERE id = {id}")
        } else if which < 65 {
            let id = pick(rng, scale.products * scale.reviews_per_product);
            let rating = rng.random_range(1..=5);
            format!("UPDATE reviews SET rating = {rating} WHERE id = {id}")
        } else if which < 85 {
            let id = pick(rng, scale.users * 2);
            let n = rng.random_range(0..1000);
            format!("UPDATE addresses SET city = 'City {n}' WHERE id = {id}")
        } else {
            let id = pick(rng, scale.users);
            let n = rng.random_range(0..1000);
            format!("UPDATE profiles SET bio = 'Bio revision {n}' WHERE user_id = {id}")
        };
        Change { sql, probe: None }
    } else {
        let which = rng.random_range(0..100);
        let sql = if which < 40 {
            let user = pick(rng, scale.users);
            let order = IdCounters::take(&ids.next_order, 1);
            let item = IdCounters::take(&ids.next_item, 2);
            let p1 = pick(rng, scale.products);
            let p2 = pick(rng, scale.products);
            format!(
                "INSERT INTO orders (id, user_id, status, total, placed_at) VALUES ({order}, {user}, 'pending', 0, now()); \
                 INSERT INTO order_items (id, order_id, product_id, quantity, unit_price) VALUES \
                   ({item}, {order}, {p1}, 1, 19.99), ({}, {order}, {p2}, 2, 9.50); \
                 UPDATE orders SET total = 38.99 WHERE id = {order}",
                item + 1
            )
        } else if which < 65 {
            let review = IdCounters::take(&ids.next_review, 1);
            let product = pick(rng, scale.products);
            let user = pick(rng, scale.users);
            let rating = rng.random_range(1..=5);
            format!(
                "INSERT INTO reviews (id, product_id, user_id, rating, body) VALUES ({review}, {product}, {user}, {rating}, 'benchmark review')"
            )
        } else if which < 80 {
            let id = pick(rng, scale.products * scale.reviews_per_product);
            format!("DELETE FROM reviews WHERE id = {id}")
        } else {
            let id = rng.random_range(stable_users(scale) + 1..=scale.users.max(2));
            format!("UPDATE users SET deleted = NOT deleted, updated_at = now() WHERE id = {id}")
        };
        Change { sql, probe: None }
    }
}

/// 40% `order_items` updates (the multi-hop resolve), 30% `orders`, 20% `users`
/// (the only probed change), 10% junction inserts and deletes.
fn complex_change(rng: &mut StdRng, scale: &Scale) -> Change {
    let roll = rng.random_range(0..100);
    let user = pick(rng, scale.users);
    let order = user * 1000 + rng.random_range(0..scale.orders_per_user.max(1));
    if roll < 40 {
        let item = order * 100 + rng.random_range(0..scale.items_per_order.max(1));
        let qty = rng.random_range(1..=9);
        Change {
            sql: format!("UPDATE order_items SET qty = {qty} WHERE id = {item}"),
            probe: None,
        }
    } else if roll < 70 {
        let status = if rng.random_range(0..2) == 0 {
            "fulfilled"
        } else {
            "pending"
        };
        let total = rng.random_range(10..500);
        Change {
            sql: format!(
                "UPDATE orders SET status = '{status}', total = {total}.50 WHERE id = {order}"
            ),
            probe: None,
        }
    } else if roll < 90 {
        let stamp = stamp();
        let n = rng.random_range(0..1000);
        Change {
            sql: format!(
                "UPDATE users SET name = 'Customer {user} rev {n}', updated_at = '{stamp}'::timestamptz WHERE id = {user}"
            ),
            probe: Some(Probe {
                index: "users",
                id: user,
                stamp,
            }),
        }
    } else {
        let tag = pick(rng, scale.tags);
        let sql = if rng.random_range(0..2) == 0 {
            format!(
                "INSERT INTO user_tags (user_id, tag_id) VALUES ({user}, {tag}) ON CONFLICT DO NOTHING"
            )
        } else {
            format!("DELETE FROM user_tags WHERE user_id = {user} AND tag_id = {tag}")
        };
        Change { sql, probe: None }
    }
}
