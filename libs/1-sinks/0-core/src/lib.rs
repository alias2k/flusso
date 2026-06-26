#![doc = include_str!("../README.md")]

mod error;
mod fan_out;
mod json;
mod sink;

pub use error::*;
pub use fan_out::*;
pub use json::to_json;
pub use sink::*;
