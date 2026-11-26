#![doc = include_str!("../README.md")]

mod duplicates;
mod error;
mod introspection;
mod provisioning;
mod row_key;
mod spec;
mod validation;

pub mod cdc;
pub mod document;

pub use duplicates::{
    DuplicateOutcome, NeverIndexed, Reach, Rejection, Text, duplicate_is_wrong, duplicate_outcome,
    never_indexed,
};
pub use error::*;
pub use introspection::*;
pub use provisioning::*;
pub use row_key::*;
pub use spec::*;
pub use validation::*;
