//! Geographic field handles: a [`GeoPoint`] and the [`Geo`] field with the
//! `within` query family (`within` distance / `within_box` / `within_polygon`)
//! plus sort-by-distance.
//!
//! A `GeoPoint` reads a point in every form OpenSearch indexes as a
//! `geo_point`, because a one-column `geo` field stores what its column holds.
//! Coordinates land where OpenSearch puts them, whatever the order in the form:
//!
//! ```
//! use flusso_query::GeoPoint;
//!
//! for written in [
//!     r#"{ "lat": 1.5, "lon": 2.5 }"#,
//!     "[2.5, 1.5]",
//!     r#""1.5,2.5""#,
//!     r#""POINT (2.5 1.5)""#,
//!     r#"{ "type": "Point", "coordinates": [2.5, 1.5] }"#,
//! ] {
//!     let point: GeoPoint = serde_json::from_str(written).unwrap();
//!     assert_eq!((point.lat, point.lon), (1.5, 2.5));
//! }
//!
//! // A query always sends the object form.
//! let sent = serde_json::to_string(&GeoPoint::new(1.5, 2.5)).unwrap();
//! assert_eq!(sent, r#"{"lat":1.5,"lon":2.5}"#);
//! ```

use std::marker::PhantomData;

use serde_json::{Map, Value};

use super::{Common, DistanceType, Sort, SortOrder, ValidationMethod, common_opts, exists_q, wrap};
use crate::query::{AsQuery, Query, Root};

/// A geographic point — latitude/longitude in degrees.
///
/// Serializes as `{"lat": …, "lon": …}`. Deserializes from every form a `geo`
/// field reaches `_source` in, with the coordinates OpenSearch indexed:
///
/// - an object `{"lat": …, "lon": …}` (numbers or numeric strings; other keys
///   are ignored, so a flattened point reads beside its siblings);
/// - an array `[lon, lat]` or `[lon, lat, z]`, GeoJSON order;
/// - a string `"lat,lon"` or `"lat,lon,z"`;
/// - a WKT string `"POINT (lon lat)"`;
/// - a GeoJSON object `{"type": "Point", "coordinates": [lon, lat]}`;
/// - a geohash, as a string or `{"geohash": …}`, read as its cell's
///   south-west corner, where OpenSearch indexes it.
///
/// Anything else, or a latitude outside ±90 or a longitude outside ±180, is a
/// deserialize error; an altitude is ignored.
///
/// A format that is not human-readable (MessagePack, postcard, bincode) reads
/// the `lat`, `lon` fields as `Serialize` wrote them.
#[derive(Debug, Clone, Copy, serde::Serialize)]
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

impl<'de> serde::Deserialize<'de> for GeoPoint {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        // A format that is not self-describing writes the struct's fields in
        // order and cannot be read as a `Value`; an array there is not `[lon, lat]`.
        if !deserializer.is_human_readable() {
            #[derive(serde::Deserialize)]
            #[serde(rename = "GeoPoint")]
            struct Fields {
                lat: f64,
                lon: f64,
            }
            let Fields { lat, lon } = Fields::deserialize(deserializer)?;
            return Ok(Self { lat, lon });
        }
        let written = Value::deserialize(deserializer)?;
        read_point(&written).map_err(serde::de::Error::custom)
    }
}

fn read_point(written: &Value) -> Result<GeoPoint, String> {
    let point = match written {
        Value::Object(fields) => read_object(fields)?,
        Value::Array(coordinates) => lon_lat(coordinates, "an array")?,
        Value::String(text) => read_string(text)?,
        other => return Err(format!("expected a geo point, found `{other}`")),
    };
    in_range(point)
}

fn read_object(fields: &Map<String, Value>) -> Result<GeoPoint, String> {
    if fields.contains_key("lat") || fields.contains_key("lon") {
        let coordinate = |name: &str| match fields.get(name) {
            Some(Value::Number(n)) => n.as_f64().ok_or_else(|| format!("`{name}` is `{n}`")),
            Some(Value::String(text)) => number(text, name),
            Some(other) => Err(format!("`{name}` must be a number, found `{other}`")),
            None => Err(format!("a geo point object is missing `{name}`")),
        };
        return Ok(GeoPoint::new(coordinate("lat")?, coordinate("lon")?));
    }
    if let Some(kind) = fields.get("type") {
        if !kind
            .as_str()
            .is_some_and(|kind| kind.eq_ignore_ascii_case("point"))
        {
            return Err(format!(
                "a GeoJSON geo point has type `Point`, found `{kind}`"
            ));
        }
        return match fields.get("coordinates") {
            Some(Value::Array(coordinates)) => lon_lat(coordinates, "GeoJSON `coordinates`"),
            _ => Err("a GeoJSON point needs a `coordinates` array".to_owned()),
        };
    }
    match fields.get("geohash") {
        Some(Value::String(hash)) => geohash(hash),
        _ => Err(
            "a geo point object needs `lat` and `lon`, `type` and `coordinates`, or \
                  `geohash`"
                .to_owned(),
        ),
    }
}

/// `[lon, lat]` or `[lon, lat, z]`: the GeoJSON order an array and WKT use.
fn lon_lat(coordinates: &[Value], form: &str) -> Result<GeoPoint, String> {
    let numbers = coordinates
        .iter()
        .map(|value| {
            value
                .as_f64()
                .ok_or_else(|| format!("{form} holds `{value}` where a coordinate goes"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    match numbers.as_slice() {
        [lon, lat] | [lon, lat, _] => Ok(GeoPoint::new(*lat, *lon)),
        _ => Err(format!(
            "{form} holds a point as [lon, lat] or [lon, lat, z], found {} values",
            numbers.len()
        )),
    }
}

/// The string forms, told apart as OpenSearch does: WKT names `point`, a
/// coordinate pair has a comma, anything else is a geohash.
fn read_string(text: &str) -> Result<GeoPoint, String> {
    if text.to_ascii_lowercase().contains("point") {
        return wkt(text);
    }
    if text.contains(',') {
        let parts: Vec<&str> = text.split(',').collect();
        return match parts.as_slice() {
            [lat, lon] | [lat, lon, _] => {
                Ok(GeoPoint::new(number(lat, "lat")?, number(lon, "lon")?))
            }
            _ => Err(format!(
                "`{text}` holds a point as \"lat,lon\" or \"lat,lon,z\", found {} values",
                parts.len()
            )),
        };
    }
    geohash(text)
}

/// `POINT (lon lat)` or `POINT (lon lat z)`, any case.
fn wkt(text: &str) -> Result<GeoPoint, String> {
    let invalid = || format!("`{text}` is not a WKT point `POINT (lon lat)`");
    let trimmed = text.trim();
    let body = trimmed
        .get(..5)
        .filter(|keyword| keyword.eq_ignore_ascii_case("point"))
        .and_then(|_| trimmed.get(5..))
        .ok_or_else(invalid)?
        .trim_start();
    let inner = body
        .strip_prefix('(')
        .and_then(|rest| rest.strip_suffix(')'))
        .ok_or_else(invalid)?;
    let numbers = inner
        .split_whitespace()
        .map(|part| number(part, "a WKT coordinate"))
        .collect::<Result<Vec<_>, _>>()?;
    match numbers.as_slice() {
        [lon, lat] | [lon, lat, _] => Ok(GeoPoint::new(*lat, *lon)),
        _ => Err(invalid()),
    }
}

const GEOHASH_ALPHABET: &[u8; 32] = b"0123456789bcdefghjkmnpqrstuvwxyz";

/// OpenSearch reads at most this many characters of a geohash.
const GEOHASH_PRECISION: usize = 12;

/// The south-west corner of the geohash's cell: the point OpenSearch indexes.
fn geohash(hash: &str) -> Result<GeoPoint, String> {
    if hash.is_empty() {
        return Err("expected a geo point, found an empty string".to_owned());
    }
    let mut lat = (-90.0_f64, 90.0_f64);
    let mut lon = (-180.0_f64, 180.0_f64);
    let mut even = true;
    for (position, symbol) in hash.bytes().enumerate() {
        let index = GEOHASH_ALPHABET
            .iter()
            .position(|&valid| valid == symbol)
            .ok_or_else(|| {
                format!("`{hash}` is not a geo point: not a coordinate pair, WKT or a geohash")
            })?;
        if position >= GEOHASH_PRECISION {
            continue;
        }
        for bit in (0..5).rev() {
            let range = if even { &mut lon } else { &mut lat };
            let middle = (range.0 + range.1) / 2.0;
            if index >> bit & 1 == 1 {
                range.0 = middle;
            } else {
                range.1 = middle;
            }
            even = !even;
        }
    }
    Ok(GeoPoint::new(lat.0, lon.0))
}

fn number(text: &str, name: &str) -> Result<f64, String> {
    text.trim()
        .parse::<f64>()
        .ok()
        .filter(|value| value.is_finite())
        .ok_or_else(|| format!("{name} must be a number, found `{}`", text.trim()))
}

fn in_range(point: GeoPoint) -> Result<GeoPoint, String> {
    if !(-90.0..=90.0).contains(&point.lat) {
        return Err(format!("latitude {} is outside -90..=90", point.lat));
    }
    if !(-180.0..=180.0).contains(&point.lon) {
        return Err(format!("longitude {} is outside -180..=180", point.lon));
    }
    Ok(point)
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
