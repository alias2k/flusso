//! Document construction: *what to build?*
//!
//! A [`DocumentBuilder`] turns a changed row — named only by its table and
//! [`RowKey`](crate::RowKey) — into the target documents it affects, then
//! assembles each one. This module is self-contained: it takes neutral
//! primitives, not a [`cdc::Change`](crate::cdc::Change), so it has no
//! dependency on how changes are captured. The engine reads a change from the
//! capture stream and passes its table and key here.

mod builder;

pub use builder::*;
