//! Prints the Postgres image this run's tests and benches start, e.g. `postgres:16-alpine`.
//!
//! The benchmark job records it with the component points, so a tag change
//! restarts their regression window instead of reading as a regression.
//!
//! `matrix` prints instead the tested tags other than the default as a JSON
//! array (`["14-alpine","18-alpine"]`): the legs of the `postgres-matrix` workflow.

use std::io::Write;
use std::process::ExitCode;

use testcontainers_modules as _;

fn main() -> ExitCode {
    let line = match std::env::args().nth(1).as_deref() {
        None => test_postgres::image_name(),
        Some("matrix") => {
            let tags: Vec<String> = test_postgres::matrix_tags()
                .map(|tag| format!("\"{tag}\""))
                .collect();
            format!("[{}]", tags.join(","))
        }
        Some(other) => {
            let _ = writeln!(
                std::io::stderr(),
                "flusso-test-postgres: unknown argument `{other}`; expected none or `matrix`"
            );
            return ExitCode::from(2);
        }
    };
    match writeln!(std::io::stdout(), "{line}") {
        Ok(()) => ExitCode::SUCCESS,
        Err(_) => ExitCode::FAILURE,
    }
}
