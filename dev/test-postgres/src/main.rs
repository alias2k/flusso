//! Prints the Postgres image this run's tests and benches start, e.g. `postgres:16-alpine`.
//!
//! The benchmark job records it with the component points, so a tag change
//! restarts their regression window instead of reading as a regression.

use std::io::Write;

use testcontainers_modules as _;

fn main() -> std::io::Result<()> {
    writeln!(std::io::stdout(), "{}", test_postgres::image_name())
}
