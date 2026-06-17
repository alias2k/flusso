//! Geographic field handles: a [`GeoPoint`] and the [`Geo`] field with distance,
//! bounding-box, and polygon queries plus sort-by-distance.

use std::marker::PhantomData;

use serde_json::{Map, Value};

use super::{Sort, SortOrder, exists_q};
use crate::query::{Query, Root};

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

/// A `geo_point` field — distance, bounding-box, and polygon queries, plus
/// sort-by-distance.
#[derive(Debug, Clone)]
pub struct Geo<S = Root> {
    path: String,
    _scope: PhantomData<fn() -> S>,
}

impl<S> Geo<S> {
    /// Build a handle for the field at `path`.
    pub fn at(path: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            _scope: PhantomData,
        }
    }

    /// Points within `distance` (e.g. `"12km"`, `"5mi"`) of `center`.
    pub fn within(&self, distance: impl Into<String>, center: GeoPoint) -> Query<S> {
        let mut body = Map::new();
        body.insert("distance".to_string(), Value::String(distance.into()));
        body.insert(self.path.clone(), center.to_value());
        wrap_object("geo_distance", body)
    }

    /// Points inside the axis-aligned box with the given corners.
    pub fn in_bounding_box(&self, top_left: GeoPoint, bottom_right: GeoPoint) -> Query<S> {
        let mut corners = Map::new();
        corners.insert("top_left".to_string(), top_left.to_value());
        corners.insert("bottom_right".to_string(), bottom_right.to_value());
        let mut body = Map::new();
        body.insert(self.path.clone(), Value::Object(corners));
        wrap_object("geo_bounding_box", body)
    }

    /// Points inside the polygon described by `points` (three or more vertices).
    pub fn in_polygon(&self, points: impl IntoIterator<Item = GeoPoint>) -> Query<S> {
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

    /// Sort by distance from `center`, measured in `unit` (e.g. `"km"`).
    pub fn distance_sort(
        &self,
        center: GeoPoint,
        order: SortOrder,
        unit: impl Into<String>,
    ) -> Sort {
        let mut body = Map::new();
        body.insert(self.path.clone(), center.to_value());
        body.insert(
            "order".to_string(),
            Value::String(order.as_str().to_string()),
        );
        body.insert("unit".to_string(), Value::String(unit.into()));
        let mut outer = Map::new();
        outer.insert("_geo_distance".to_string(), Value::Object(body));
        Sort::raw(Value::Object(outer))
    }
}

/// `{ "<name>": { <body> } }` as a scope-`S` query.
fn wrap_object<S>(name: &str, body: Map<String, Value>) -> Query<S> {
    let mut outer = Map::new();
    outer.insert(name.to_string(), Value::Object(body));
    Query::leaf(Value::Object(outer))
}
