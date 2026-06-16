//! Worked `flusso-query` consumer: typed document structs + queries against a
//! `users` index. Self-contained illustration — adapt the index name and fields
//! to your own `flusso.toml`. Requires `flusso-query` with the `derive` feature
//! (and `time` for the date leaf types used below).
//!
//! Cargo.toml:
//!   flusso-query = { version = "*", features = ["derive", "time"] }
//!   serde = { version = "1", features = ["derive"] }
//!   tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
//!   time = "0.3"

use flusso_query::{Client, FlussoDocument, FlussoMultiDocument, FlussoValue, Search};

// ── Custom value type: a closed enum stored as a keyword ────────────────────
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize, FlussoValue)]
#[serde(rename_all = "lowercase")]
#[flusso(keyword)] // kind: keyword (default) | text | number | date
pub enum Tier {
    Free,
    Pro,
    Enterprise,
}

// ── Root document: a PROJECTION of the `users` index ────────────────────────
// The derive validates every field against the schema and generates the query
// surface for the WHOLE schema (so you can filter on fields not declared here).
#[derive(Debug, Clone, serde::Deserialize, FlussoDocument)]
#[flusso(index = "users")] // only input: which index. flusso.toml is auto-discovered.
pub struct User {
    pub id: i32,                    // primary key (integer) → never null
    pub email: String,              // keyword, required → never null
    #[serde(rename = "fullName")]
    pub full_name: Option<String>,  // text, not required → nullable
    pub tier: Tier,                 // enum (keyword) via FlussoValue
    pub account: Account,           // object (same-row group) → never null
    pub orders: Vec<Order>,         // has_many → nested array, never null
    #[serde(rename = "orderCount")]
    pub order_count: i64,           // count aggregate → long, never null
    #[serde(rename = "lifetimeValue")]
    pub lifetime_value: Option<f64>, // sum aggregate → nullable double
}

#[derive(Debug, Clone, serde::Deserialize, FlussoDocument)]
#[flusso(index = "users", path = "account")] // a child struct names its dotted path
pub struct Account {
    pub tier: Tier,
    pub country: Option<String>,
    #[serde(rename = "createdAt")]
    pub created_at: time::OffsetDateTime,
}

#[derive(Debug, Clone, serde::Deserialize, FlussoDocument)]
#[flusso(index = "users", path = "orders")]
pub struct Order {
    pub status: String,
    pub total: f64,
    #[serde(rename = "placedAt")]
    pub placed_at: time::OffsetDateTime,
}

// ── Reusable, client-free query value ───────────────────────────────────────
fn busy_users() -> Search<User> {
    User::query().filter(User::order_count().gte(5))
}

// Optional inputs: `Option<Query>` is itself a Query — None contributes nothing.
fn search_users(name: Option<String>, tier: Option<Tier>) -> Search<User> {
    User::query()
        .query(name.map(|n| User::full_name().matches(n)))
        .filter(tier.map(|t| Account::tier().eq(t))) // object sub-field via child handle
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Reads go straight to OpenSearch, not to flusso.
    let client = Client::connect("https://localhost:9200")?
        .basic_auth("admin", std::env::var("OS_PASSWORD")?);

    // get by primary key.
    if let Some(u) = User::get(&client, 42).await? {
        println!("{} — {} orders", u.email, u.order_count);
    }

    // A typed search:
    //  - filter BY nested (which users) — lifted from Query<Order> to Query<Root>
    //  - filter OF nested (shape each user's orders array)
    let page = User::query()
        .filter(User::email().eq("ada@example.com")) // keyword → exact
        .filter(User::order_count().gte(5))           // long → range
        .filter(User::tier().eq(Tier::Pro))           // custom keyword value
        .query(User::full_name().matches("ada lovelace")) // text → analyzed
        .filter(User::orders().any(Order::status().eq("delivered"))) // BY
        .filter_nested(
            User::orders()
                .matching(Order::status().eq("delivered"))
                .sort(Order::placed_at().desc())
                .size(5),
        ) // OF
        .sort(User::order_count().desc())
        .from(0)
        .size(20)
        .send(&client)
        .await?;

    println!("{} total", page.total);
    for hit in &page.hits {
        let u = &hit.source; // fully-typed User
        println!("{:.3}  {}", hit.score, u.email);
        for o in &u.orders {
            // already the filtered subset
            println!("    {} — {}", o.total, o.status);
        }
    }

    // Cheap variants: ids only, or a count.
    let _ids: Vec<String> = busy_users().size(100).ids(&client).await?;
    let _n: u64 = busy_users().count(&client).await?;

    // Reuse the helper.
    let _ = search_users(Some("ada".into()), Some(Tier::Pro)).send(&client).await?;

    Ok(())
}

// ── Multi-index blended search ──────────────────────────────────────────────
#[derive(Debug, FlussoMultiDocument)]
#[serde(tag = "type")]
pub enum SearchItem {
    User(User),
    Order(Order),
}
// SearchItem::query()…send(&client) ranks User + Order hits in one list;
// match on hit.source to dispatch. (Purely syntactic — no schema resolution.)
