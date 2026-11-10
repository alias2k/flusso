//! Reading the source's current position: the token a fence waits on.
//!
//! After a commit, ask the database where its change log stands. Every commit
//! made before the question lies at or before the answer, so once flusso has
//! reached it, your write is in the index. Hand it to
//! [`Client::after`](crate::Client::after) or
//! [`Client::wait`](crate::Client::wait).
//!
//! Two rules, both about asking the right server at the right time:
//!
//! - **After the commit, outside the transaction.** A position read inside the
//!   transaction lies before its commit, so a fence on it can pass too early.
//! - **On the primary.** A replica reports its own replay position, which is
//!   not the one flusso follows.
//!
//! [`CURRENT_SQL`] is the statement, for any driver. The `diesel` and
//! `diesel-async` features add a `current` helper for each.
//!
//! ```
//! assert_eq!(
//!     flusso_query::position::CURRENT_SQL,
//!     "SELECT pg_current_wal_lsn()::text"
//! );
//! ```

/// The statement that reads the current position from Postgres: one row, one
/// `text` column (an LSN such as `0/16B3748`).
pub const CURRENT_SQL: &str = "SELECT pg_current_wal_lsn()::text";

/// The expression [`CURRENT_SQL`] selects.
#[cfg(any(feature = "diesel", feature = "diesel-async"))]
const CURRENT_EXPRESSION: &str = "pg_current_wal_lsn()::text";

/// The helper for [Diesel](https://diesel.rs)'s blocking connections.
#[cfg(feature = "diesel")]
pub mod diesel {
    use ::diesel::connection::LoadConnection;
    use ::diesel::pg::Pg;
    use ::diesel::sql_types::Text;
    use ::diesel::{Connection, QueryResult, RunQueryDsl};

    /// The source's current position. Call it after the commit, on a
    /// connection to the primary that is not inside the transaction.
    pub fn current<C>(connection: &mut C) -> QueryResult<String>
    where
        C: Connection<Backend = Pg> + LoadConnection,
    {
        ::diesel::select(::diesel::dsl::sql::<Text>(super::CURRENT_EXPRESSION))
            .get_result(connection)
    }
}

/// The helper for [`diesel-async`](https://docs.rs/diesel-async) connections.
#[cfg(feature = "diesel-async")]
pub mod diesel_async {
    use ::diesel::QueryResult;
    use ::diesel::pg::Pg;
    use ::diesel::sql_types::Text;
    use ::diesel_async::{AsyncConnection, RunQueryDsl};

    /// The source's current position. Call it after the commit, on a
    /// connection to the primary that is not inside the transaction.
    pub async fn current<C>(connection: &mut C) -> QueryResult<String>
    where
        C: AsyncConnection<Backend = Pg> + Send,
    {
        ::diesel::select(::diesel::dsl::sql::<Text>(super::CURRENT_EXPRESSION))
            .get_result(connection)
            .await
    }
}
