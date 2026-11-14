#![doc = include_str!("../README.md")]

mod duplicates;
mod error;
mod introspection;
mod provisioning;
mod row_key;
mod snapshot_table;
mod spec;
mod timestamp_format;
mod validation;

pub mod cdc;
pub mod document;

pub use duplicates::{
    DuplicateOutcome, NeverIndexed, Rejection, Text, duplicate_outcome, never_indexed,
};
pub use error::*;
pub use introspection::*;
pub use provisioning::*;
pub use row_key::*;
pub use snapshot_table::*;
pub use spec::*;
pub use timestamp_format::reads_iso_timestamps;
pub use validation::*;
