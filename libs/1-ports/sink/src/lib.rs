#![doc = include_str!("../README.md")]

mod error;
mod json;
mod sink;

pub use error::*;
pub use json::{Json, to_json};
pub use sink::*;
