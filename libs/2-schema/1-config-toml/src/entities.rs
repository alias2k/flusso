//! The parsed `flusso.toml` entities — neutral types that mirror the file. They
//! are public so the `schema` crate's `From<ConfigToml>` conversion can lift
//! them into the assembled `Config`.

mod index_entry;
mod sink;
mod source;

pub use index_entry::*;
pub use sink::*;
pub use source::*;
