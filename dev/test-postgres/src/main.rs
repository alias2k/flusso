//! Prints the Postgres image this run's tests and benches start, e.g. `postgres:16-alpine`.
//!
//! The benchmark job records it with the component points, so a tag change
//! restarts their regression window instead of reading as a regression.
//!
//! `matrix` prints instead the tested tags other than the default as a JSON
//! array (`["14-alpine","18-alpine"]`): the legs of the `postgres-matrix` workflow.

use std::io::Write;

use testcontainers_modules as _;

fn main() -> std::io::Result<()> {
    let mut out = std::io::stdout();
    match std::env::args().nth(1).as_deref() {
        None => writeln!(out, "{}", test_postgres::image_name()),
        Some("matrix") => {
            let tags: Vec<String> = test_postgres::matrix_tags()
                .map(|tag| format!("\"{tag}\""))
                .collect();
            writeln!(out, "[{}]", tags.join(","))
        }
        Some(other) => Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("unknown argument `{other}`; expected none or `matrix`"),
        )),
    }
}
