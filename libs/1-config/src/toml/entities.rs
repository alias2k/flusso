//! The parsed `flusso.toml` entities — neutral types that mirror the file. They
//! are public so the `From<ConfigToml>` conversion beside `Config` can lift
//! them into the assembled `Config`. The port entries themselves are the
//! kernel's [`PortEntry`](kernel::PortEntry): a `type` plus uninterpreted
//! options.

mod index_entry;
mod server;

pub use index_entry::*;
pub use server::*;
