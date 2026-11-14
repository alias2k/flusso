// A tagged `FlussoValue` newtype reads one string, so a type the derive knows
// by name to read something else is refused: a number behind a std wrapper,
// a duration, flusso's geo point, a slice behind `Cow` (issue #294).
use std::borrow::Cow;
use std::cell::RefCell;
use std::collections::HashMap;
use std::num::{NonZeroU32, Wrapping};
use std::sync::Mutex;
use std::time::Duration;

use flusso_query::{FlussoValue, GeoPoint};

#[derive(serde::Serialize, serde::Deserialize, FlussoValue)]
#[flusso(keyword)]
struct A(NonZeroU32);

#[derive(serde::Serialize, serde::Deserialize, FlussoValue)]
#[flusso(keyword)]
struct B(Wrapping<i64>);

#[derive(serde::Serialize, serde::Deserialize, FlussoValue)]
#[flusso(keyword)]
struct C(Duration);

#[derive(serde::Serialize, serde::Deserialize, FlussoValue)]
#[flusso(keyword)]
struct D(GeoPoint);

#[derive(serde::Serialize, serde::Deserialize, FlussoValue)]
#[flusso(keyword)]
struct E(Cow<'static, [String]>);

#[derive(serde::Serialize, serde::Deserialize, FlussoValue)]
#[flusso(keyword)]
struct F(RefCell<HashMap<String, String>>);

#[derive(serde::Serialize, serde::Deserialize, FlussoValue)]
#[flusso(keyword)]
struct G(Mutex<HashMap<String, String>>);

fn main() {}
