//! Geographic field handles: a [`GeoPoint`] and the [`Geo`] field with the
//! `within` query family (`within` distance / `within_box` / `within_polygon`)
//! plus sort-by-distance.

use std::marker::PhantomData;

use serde_json::{Map, Value};

use super::{Common, DistanceType, Sort, SortOrder, ValidationMethod, common_opts, exists_q, wrap};
use crate::query::{AsQuery, Query, Root};

/// A geographic point — latitude/longitude in degrees.
#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize)]
pub struct GeoPoint {
    /// Latitude in degrees.
    pub lat: f64,
    /// Longitude in degrees.
    pub lon: f64,
}

impl GeoPoint {
    /// A point at `lat`/`lon` degrees.
    pub fn new(lat: f64, lon: f64) -> Self {
        Self { lat, lon }
    }

    /// `{ "lat": …, "lon": … }`.
    fn to_value(self) -> Value {
        let mut point = Map::new();
        point.insert("lat".to_string(), Value::from(self.lat));
        point.insert("lon".to_string(), Value::from(self.lon));
        Value::Object(point)
    }
}

/// A distance unit OpenSearch accepts in a `geo_distance` query or
/// `_geo_distance` sort.
#[derive(Debug, Clone, Copy)]
pub enum DistanceUnit {
    /// Kilometers (`km`).
    Kilometers,
    /// Meters (`m`).
    Meters,
    /// Centimeters (`cm`).
    Centimeters,
    /// Millimeters (`mm`).
    Millimeters,
    /// Miles (`mi`).
    Miles,
    /// Yards (`yd`).
    Yards,
    /// Feet (`ft`).
    Feet,
    /// Nautical miles (`nmi`).
    NauticalMiles,
}

impl DistanceUnit {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            DistanceUnit::Kilometers => "km",
            DistanceUnit::Meters => "m",
            DistanceUnit::Centimeters => "cm",
            DistanceUnit::Millimeters => "mm",
            DistanceUnit::Miles => "mi",
            DistanceUnit::Yards => "yd",
            DistanceUnit::Feet => "ft",
            DistanceUnit::NauticalMiles => "nmi",
        }
    }
}

/// A distance with an explicit unit — e.g. `Distance::km(12.0)`. Renders to the
/// OpenSearch distance string (`"12km"`), so a malformed radius (`"12 km"`, a
/// typo'd unit) can't reach the query.
#[derive(Debug, Clone, Copy)]
pub struct Distance {
    value: f64,
    unit: DistanceUnit,
}

impl Distance {
    /// `value` in `unit`.
    pub fn new(value: f64, unit: DistanceUnit) -> Self {
        Self { value, unit }
    }

    /// Kilometers.
    pub fn km(value: f64) -> Self {
        Self::new(value, DistanceUnit::Kilometers)
    }

    /// Meters.
    pub fn meters(value: f64) -> Self {
        Self::new(value, DistanceUnit::Meters)
    }

    /// Centimeters.
    pub fn centimeters(value: f64) -> Self {
        Self::new(value, DistanceUnit::Centimeters)
    }

    /// Millimeters.
    pub fn millimeters(value: f64) -> Self {
        Self::new(value, DistanceUnit::Millimeters)
    }

    /// Miles.
    pub fn miles(value: f64) -> Self {
        Self::new(value, DistanceUnit::Miles)
    }

    /// Yards.
    pub fn yards(value: f64) -> Self {
        Self::new(value, DistanceUnit::Yards)
    }

    /// Feet.
    pub fn feet(value: f64) -> Self {
        Self::new(value, DistanceUnit::Feet)
    }

    /// Nautical miles.
    pub fn nautical_miles(value: f64) -> Self {
        Self::new(value, DistanceUnit::NauticalMiles)
    }

    fn to_query_string(self) -> String {
        format!("{}{}", self.value, self.unit.as_str())
    }
}

/// A `geo_point` field — the `within` query family (distance / box / polygon),
/// plus sort-by-distance.
#[derive(Debug, Clone)]
pub struct Geo<S = Root> {
    path: String,
    _scope: PhantomData<fn() -> S>,
}

impl<S> Geo<S> {
    pub fn at(path: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            _scope: PhantomData,
        }
    }

    /// Points within `distance` (e.g. `Distance::km(12.0)`) of `center`. Returns
    /// a [`GeoDistanceQuery`] builder for `distance_type` / `validation_method`
    /// plus `boost` / `name`.
    pub fn within(&self, distance: Distance, center: GeoPoint) -> GeoDistanceQuery<S> {
        GeoDistanceQuery {
            path: self.path.clone(),
            distance: distance.to_query_string(),
            center,
            opts: Map::new(),
            common: Common::default(),
            _scope: PhantomData,
        }
    }

    /// Points inside the axis-aligned box with the given corners.
    pub fn within_box(&self, top_left: GeoPoint, bottom_right: GeoPoint) -> Query<S> {
        let mut corners = Map::new();
        corners.insert("top_left".to_string(), top_left.to_value());
        corners.insert("bottom_right".to_string(), bottom_right.to_value());
        let mut body = Map::new();
        body.insert(self.path.clone(), Value::Object(corners));
        wrap_object("geo_bounding_box", body)
    }

    /// Points inside the polygon described by `points` (three or more vertices).
    pub fn within_polygon(&self, points: impl IntoIterator<Item = GeoPoint>) -> Query<S> {
        let vertices = points.into_iter().map(GeoPoint::to_value).collect();
        let mut inner = Map::new();
        inner.insert("points".to_string(), Value::Array(vertices));
        let mut body = Map::new();
        body.insert(self.path.clone(), Value::Object(inner));
        wrap_object("geo_polygon", body)
    }

    /// The field has a value.
    pub fn exists(&self) -> Query<S> {
        exists_q(&self.path)
    }

    /// Sort by distance from `center`, nearest first. Sugar for the common
    /// `_geo_distance` sort: ascending, OpenSearch's default unit (meters);
    /// chain `.desc()` to flip it. For an explicit unit or order use
    /// [`distance_sort`](Self::distance_sort).
    pub fn distance_from(&self, center: GeoPoint) -> Sort {
        let mut body = Map::new();
        body.insert(self.path.clone(), center.to_value());
        body.insert("order".to_string(), Value::String("asc".to_string()));
        Sort::from_parts("_geo_distance".to_string(), body)
    }

    /// Sort by distance from `center`, measured in `unit`.
    pub fn distance_sort(&self, center: GeoPoint, order: SortOrder, unit: DistanceUnit) -> Sort {
        let mut body = Map::new();
        body.insert(self.path.clone(), center.to_value());
        body.insert(
            "order".to_string(),
            Value::String(order.as_str().to_string()),
        );
        body.insert("unit".to_string(), Value::String(unit.as_str().to_string()));
        Sort::from_parts("_geo_distance".to_string(), body)
    }
}

/// `{ "<name>": { <body> } }` as a scope-`S` query.
fn wrap_object<S>(name: &str, body: Map<String, Value>) -> Query<S> {
    wrap(name, body)
}

/// A `geo_distance` clause: points within a radius of a center, with the
/// `distance_type` / `validation_method` options plus `boost` / `name`.
#[derive(Debug, Clone)]
pub struct GeoDistanceQuery<S = Root> {
    path: String,
    distance: String,
    center: GeoPoint,
    opts: Map<String, Value>,
    common: Common,
    _scope: PhantomData<fn() -> S>,
}

impl<S> GeoDistanceQuery<S> {
    /// How distance is computed ([`DistanceType::Arc`] is the default).
    #[must_use]
    pub fn distance_type(mut self, distance_type: DistanceType) -> Self {
        self.opts.insert(
            "distance_type".to_string(),
            Value::String(distance_type.as_str().to_string()),
        );
        self
    }

    /// How malformed coordinates are handled ([`ValidationMethod::Strict`] is
    /// the default).
    #[must_use]
    pub fn validation_method(mut self, validation_method: ValidationMethod) -> Self {
        self.opts.insert(
            "validation_method".to_string(),
            Value::String(validation_method.as_str().to_string()),
        );
        self
    }

    common_opts!(common);
}

impl<S> AsQuery<S> for GeoDistanceQuery<S> {
    fn into_query(self) -> Option<Query<S>> {
        let mut body = self.opts;
        body.insert("distance".to_string(), Value::String(self.distance));
        body.insert(self.path, self.center.to_value());
        self.common.write(&mut body);
        Some(wrap("geo_distance", body))
    }
}
