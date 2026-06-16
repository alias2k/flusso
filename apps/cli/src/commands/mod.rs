//! The CLI subcommands and the shared human-facing printer.
//!
//! `main` dispatches to one `execute` per subcommand; [`print`](mod@print) is the pretty
//! output helper they share.

pub(crate) mod admin;
pub(crate) mod build;
pub(crate) mod check;
pub(crate) mod print;
pub(crate) mod run;
pub(crate) mod schema_cmd;
